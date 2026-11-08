"""网易云官方接口（由本地后台直接请求，无 CORS 问题，不依赖第三方节点）"""
import json
import logging
import urllib.parse
import urllib.request

from db import artist_variants, clean_artist, clean_title, norm

log = logging.getLogger("ncm")

HEADERS = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36",
    "Referer": "https://music.163.com/",
}
DAY = 86400


class NCM:
    def __init__(self, db):
        self.db = db

    def _get(self, url, timeout=6):
        req = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read().decode("utf-8"))

    @staticmethod
    def _simplify(s):
        artists = [a.get("name", "") for a in s.get("artists") or s.get("ar") or []]
        album = s.get("album") or s.get("al") or {}
        return {
            "id": s.get("id"),
            "title": s.get("name"),
            "artists": artists,
            "artist": artists[0] if artists else "",
            "album": album.get("name") or "",
            "cover": album.get("picUrl") or "",
            "duration_ms": s.get("duration") or s.get("dt") or 0,
            "status": s.get("status", 0),
        }

    def search(self, keyword, limit=20):
        key = f"ncm:search:{keyword}:{limit}"
        cached = self.db.cache_get(key)
        if cached is not None:
            return cached
        q = urllib.parse.urlencode({"s": keyword, "type": 1, "limit": limit, "offset": 0})
        data = self._get("https://music.163.com/api/search/get/web?" + q)
        songs = [self._simplify(s) for s in (data.get("result") or {}).get("songs") or []]
        self.db.cache_set(key, songs, 3 * DAY)
        return songs

    def detail(self, song_id):
        key = f"ncm:detail:{song_id}"
        cached = self.db.cache_get(key)
        if cached is not None:
            return cached
        data = self._get(f"https://music.163.com/api/song/detail/?id={song_id}&ids=[{song_id}]")
        songs = data.get("songs") or []
        res = self._simplify(songs[0]) if songs else None
        if res:
            self.db.cache_set(key, res, 7 * DAY)
        return res

    def simi(self, song_id, limit=50):
        key = f"ncm:simi:{song_id}"
        cached = self.db.cache_get(key)
        if cached is not None:
            return cached
        data = self._get(
            f"https://music.163.com/api/v1/discovery/simiSong?songid={song_id}&limit={limit}&offset=0"
        )
        songs = [self._simplify(s) for s in data.get("songs") or []]
        self.db.cache_set(key, songs, DAY)
        return songs

    @staticmethod
    def artist_ok(candidates, target):
        if not target:
            return True
        targets = [norm(v) for v in artist_variants(target) if norm(v)]
        if not targets:
            return True
        for c in candidates:
            for cv in artist_variants(c):
                cn = norm(cv)
                if cn and any(cn in t or t in cn for t in targets):
                    return True
        return False

    def match(self, title, artist):
        """歌名 + 歌手双重校验，优先精确匹配当前版本；匹配不到返回 None"""
        key = f"ncm:match:{norm(title)}:{norm(clean_artist(artist))}"
        cached = self.db.cache_get(key)
        if cached is not None:
            return cached or None
        try:
            first_artist = clean_artist(artist)
            results = self.search(f"{first_artist} {title}".strip(), 20)
        except Exception as e:
            log.warning("ncm search failed: %s", e)
            return None  # 网络错误不缓存

        t = norm(title)
        tc = norm(clean_title(title))
        best, best_score = None, -1
        for i, s in enumerate(results):
            st = norm(s["title"])
            stc = norm(clean_title(s["title"]))
            if not st or not t:
                continue
            if st == t:
                title_score = 4  # 连版本都完全一致，最高优先
            elif tc and stc == tc:
                title_score = 3  # 主歌名一致，仅发行后缀不同
            elif st.startswith(t) or t.startswith(st):
                title_score = 2
            elif t in st or st in t:
                title_score = 1
            else:
                continue
            if not self.artist_ok(s["artists"], artist):
                continue
            score = title_score * 10 + (2 if s["status"] >= 0 else 0) - i * 0.1
            if score > best_score:
                best, best_score = s, score

        if best and not best.get("cover") and best.get("id"):
            try:
                det = self.detail(best["id"])
                if det and det.get("cover"):
                    best["cover"] = det["cover"]
            except Exception:
                pass

        self.db.cache_set(key, best or {}, 7 * DAY if best else DAY)
        return best
