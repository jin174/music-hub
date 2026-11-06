"""Last.fm API：签名调用（授权 / 打卡 / 红心）+ 只读查询（相似歌曲 / 歌手）"""
import hashlib
import json
import logging
import urllib.error
import urllib.parse
import urllib.request

from db import clean_artist, clean_title

log = logging.getLogger("lastfm")

API_URL = "https://ws.audioscrobbler.com/2.0/"
AUTH_URL = "https://www.last.fm/api/auth/"
DAY = 86400


class LastfmError(Exception):
    def __init__(self, code, message):
        super().__init__(f"[{code}] {message}")
        self.code = code
        self.message = message


class Lastfm:
    def __init__(self, cfg, db):
        self.cfg = cfg
        self.db = db

    # ---------- 底层 ----------
    @staticmethod
    def sign(params, secret):
        raw = "".join(f"{k}{params[k]}" for k in sorted(params) if k not in ("format", "callback"))
        return hashlib.md5((raw + secret).encode("utf-8")).hexdigest()

    def call(self, method, params=None, signed=False, post=False, timeout=8):
        p = {k: str(v) for k, v in (params or {}).items() if v not in (None, "")}
        p["method"] = method
        if signed:
            p["api_key"] = self.cfg.get("api_key")
            p["api_sig"] = self.sign(p, self.cfg.get("api_secret"))
        else:
            p.setdefault("api_key", self.cfg.read_key)
        p["format"] = "json"
        body = urllib.parse.urlencode(p).encode("utf-8")
        if post:
            req = urllib.request.Request(API_URL, data=body, method="POST")
        else:
            req = urllib.request.Request(API_URL + "?" + body.decode("utf-8"))
        req.add_header("User-Agent", "MusicHub/1.0")
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                data = json.loads(r.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            # Last.fm 出错时也会返回 JSON
            try:
                data = json.loads(e.read().decode("utf-8"))
            except Exception:
                raise
        if isinstance(data, dict) and "error" in data:
            raise LastfmError(data["error"], data.get("message", ""))
        return data

    # ---------- 授权 ----------
    def auth_url(self, callback):
        q = urllib.parse.urlencode({"api_key": self.cfg.get("api_key"), "cb": callback})
        return f"{AUTH_URL}?{q}"

    def get_session(self, token):
        data = self.call("auth.getSession", {"token": token}, signed=True)
        s = data["session"]
        return s["key"], s["name"]

    # ---------- 写入 ----------
    def now_playing(self, artist, track, album=None, duration_s=None):
        return self.call(
            "track.updateNowPlaying",
            {"artist": artist, "track": track, "album": album, "duration": duration_s,
             "sk": self.cfg.get("session_key")},
            signed=True, post=True,
        )

    def scrobble(self, items):
        """items: [{artist, track, timestamp, album, duration}]，一次最多 50 条。
        返回与 items 等长的 [True(接受)/False(被忽略)]"""
        p = {"sk": self.cfg.get("session_key")}
        for i, it in enumerate(items):
            p[f"artist[{i}]"] = it["artist"]
            p[f"track[{i}]"] = it["track"]
            p[f"timestamp[{i}]"] = it["timestamp"]
            if it.get("album"):
                p[f"album[{i}]"] = it["album"]
            if it.get("duration"):
                p[f"duration[{i}]"] = it["duration"]
        data = self.call("track.scrobble", p, signed=True, post=True)
        sc = data.get("scrobbles", {}).get("scrobble", [])
        if isinstance(sc, dict):
            sc = [sc]
        result = []
        for s in sc:
            ignored = s.get("ignoredMessage", {}).get("code", "0")
            result.append(str(ignored) == "0")
        while len(result) < len(items):
            result.append(True)
        return result

    def love(self, artist, track):
        return self.call(
            "track.love", {"artist": artist, "track": track, "sk": self.cfg.get("session_key")},
            signed=True, post=True,
        )

    # ---------- 只读（带缓存） ----------
    def _cached(self, key, ttl, fn):
        v = self.db.cache_get(key)
        if v is not None:
            return v
        v = fn()
        self.db.cache_set(key, v, ttl)
        return v

    def similar_tracks(self, track, artist, limit=50):
        def _query(trk, art):
            try:
                d = self.call("track.getSimilar",
                              {"track": trk, "artist": art, "limit": limit, "autocorrect": 1})
            except LastfmError:
                return []
            out = []
            for t in d.get("similartracks", {}).get("track", []) or []:
                out.append({
                    "title": t.get("name", ""),
                    "artist": (t.get("artist") or {}).get("name", ""),
                    "match": float(t.get("match") or 0),
                })
            return out

        def fetch():
            art = clean_artist(artist) or artist
            out = _query(track, art)
            # 若带后缀的版本在 Last.fm 查不到相似曲目，自动回退用去发行后缀的歌名再查一次
            ct = clean_title(track)
            if not out and ct and ct != track:
                out = _query(ct, art)
            return out
        return self._cached(f"lf:simtrack:{artist}:{track}:{limit}", DAY, fetch)

    def similar_artists(self, artist, limit=20):
        def fetch():
            try:
                d = self.call("artist.getSimilar", {"artist": artist, "limit": limit, "autocorrect": 1})
            except LastfmError:
                return []
            return [{"artist": a.get("name", ""), "match": float(a.get("match") or 0)}
                    for a in d.get("similarartists", {}).get("artist", []) or []]
        return self._cached(f"lf:simartist:{artist}:{limit}", DAY, fetch)

    def user_top_artists(self, user, period="1month", limit=15):
        if not user:
            return []
        def fetch():
            try:
                d = self.call("user.getTopArtists", {"user": user, "period": period, "limit": limit})
            except LastfmError:
                return []
            return [{
                "artist": a.get("name", ""),
                "playcount": int(a.get("playcount") or 0),
            } for a in d.get("topartists", {}).get("artist", []) or []]
        return self._cached(f"lf:utopart:{user}:{period}:{limit}", 1800, fetch)

    def user_top_tracks(self, user, period="1month", limit=20):
        if not user:
            return []
        def fetch():
            try:
                d = self.call("user.getTopTracks", {"user": user, "period": period, "limit": limit})
            except LastfmError:
                return []
            return [{
                "title": t.get("name", ""),
                "artist": (t.get("artist") or {}).get("name", ""),
                "playcount": int(t.get("playcount") or 0),
            } for t in d.get("toptracks", {}).get("track", []) or []]
        return self._cached(f"lf:utoptrk:{user}:{period}:{limit}", 1800, fetch)

    def user_recent_tracks(self, user, limit=100):
        if not user:
            return []
        try:
            d = self.call("user.getRecentTracks", {"user": user, "limit": limit})
        except LastfmError:
            return []
        tracks = d.get("recenttracks", {}).get("track", []) or []
        if isinstance(tracks, dict):
            tracks = [tracks]
        out = []
        for t in tracks:
            if (t.get("@attr") or {}).get("nowplaying") == "true":
                continue
            ts = int((t.get("date") or {}).get("uts") or 0)
            if not ts:
                continue
            out.append({
                "title": t.get("name", ""),
                "artist": (t.get("artist") or {}).get("#text", ""),
                "album": (t.get("album") or {}).get("#text", ""),
                "timestamp": ts,
            })
        return out

