"""把各模块接在一起，并提供给 HTTP 接口用的业务逻辑"""
import base64
import json
import logging
import os
import random
import sys
import time
from concurrent.futures import ThreadPoolExecutor

from db import clean_artist, norm
from lastfm import Lastfm
from monitor import Monitor
from ncm import NCM
from scrobbler import Scrobbler

log = logging.getLogger("app")


class App:
    def __init__(self, cfg, db):
        self.cfg = cfg
        self.db = db
        self.ncm = NCM(db)
        self.lf = Lastfm(cfg, db)
        self.scrobbler = Scrobbler(cfg, db, self.lf)
        self.monitor = Monitor(cfg, db, self.ncm, self.scrobbler)
        self.pool = ThreadPoolExecutor(max_workers=4)

    @property
    def base_url(self):
        return f"http://127.0.0.1:{self.cfg.get('port')}"

    # ---------- 状态 ----------
    def state(self):
        pending = self.db.q("SELECT COUNT(*) n FROM plays WHERE scrobble_state='pending'")[0]["n"]
        return {
            "now": self.monitor.snapshot(),
            "config": self.cfg.public(),
            "scrobbling": self.cfg.scrobbling,
            "pending": pending,
            "last_error": self.scrobbler.last_error,
            "last_ok": self.scrobbler.last_ok,
            "session_app": self.monitor.session_app,
        }

    # ---------- 网易云客户端开播 ----------
    @staticmethod
    def play_in_client(song_id):
        payload = {"type": "song", "id": int(song_id), "cmd": "play", "channel": "webset"}
        b64 = base64.b64encode(json.dumps(payload, separators=(",", ":")).encode()).decode()
        os.startfile(f"orpheus://{b64}")  # 由本机直接唤起，浏览器不会再弹“是否打开”确认框

    def play_artist(self, artist):
        songs = self.ncm.search(artist, 20)
        for s in songs:
            if self.ncm.artist_ok(s["artists"], artist):
                self.play_in_client(s["id"])
                return s
        return None

    # ---------- 雷达 ----------
    def radar(self, track, artist):
        seed = None
        if track:
            try:
                seed = self.ncm.match(track, artist)
            except Exception as e:
                log.warning("seed match failed: %s", e)

        futures = {}
        if track:
            futures["lf"] = self.pool.submit(self.lf.similar_tracks, track, artist, 50)
        if seed:
            futures["ncm"] = self.pool.submit(self.ncm.simi, seed["id"], 50)
        lf_tracks, ncm_tracks = [], []
        for name, fut in futures.items():
            try:
                res = fut.result(timeout=12)
            except Exception as e:
                log.warning("radar source %s failed: %s", name, e)
                res = []
            if name == "lf":
                lf_tracks = res
            else:
                ncm_tracks = res

        items = {}
        seed_key = (norm(track), norm((seed or {}).get("artist") or (artist or "").split("/")[0]))

        def add(title, art, score, source, ncm=None):
            key = (norm(title), norm(art.split("/")[0] if art else ""))
            if not key[0] or key == seed_key:
                return
            if key in items:
                it = items[key]
                if source not in it["sources"]:
                    it["sources"].append(source)
                    it["score"] += 0.2  # 两个来源都推荐 → 更可信
                if ncm and not it.get("ncm"):
                    it["ncm"] = ncm
            else:
                items[key] = {"title": title, "artist": art, "score": score,
                              "sources": [source], "ncm": ncm}

        for t in lf_tracks:
            add(t["title"], t["artist"], t["match"], "lastfm")
        for i, s in enumerate(ncm_tracks):
            add(s["title"], s["artist"], max(0.35, 0.8 - i * 0.05), "ncm",
                {"id": s["id"], "title": s["title"], "artist": s["artist"], "cover": s["cover"]})

        heard = self.db.heard_index()
        fb_map = self.db.get_feedback_map()
        for key, it in items.items():
            h = heard.get(key)
            it["base_score"] = it["score"]
            if h:
                it["heard"] = {"plays": h["n"], "completed": h["c"] or 0, "skipped": h["s"] or 0}
                it["score"] *= 0.6  # 听过的往后放，优先给新歌
                if (h["s"] or 0) >= 2 and (h["s"] or 0) > (h["c"] or 0):
                    it["score"] *= 0.3  # 累计跳过 ≥2 次且大于听完次数，才判定为不感兴趣并降权
            else:
                it["heard"] = None
            fb = fb_map.get(key)
            it["feedback"] = fb
            if fb == "like":
                it["score"] *= 1.4

        tracks = [
            it for it in sorted(items.values(), key=lambda x: x["score"], reverse=True)
            if it.get("feedback") != "dislike"
        ][:60]
        if tracks:
            self.pool.submit(self.db.log_recommendations, tracks[:20], "radar", track, artist)
        artists = []
        if not tracks and artist:
            artists = self.lf.similar_artists(artist.split("/")[0], 20)
        return {
            "seed": seed,
            "tracks": tracks,
            "artists": artists,
            "sources": {"lastfm": len(lf_tracks), "ncm": len(ncm_tracks)},
        }

    # ---------- 专属全盘漫游 ----------
    def roam(self, user=None, hide_heard=True, shuffle=False):
        user = (user or self.cfg.get("username") or "").strip()
        seeds = []
        seen_seed = set()

        def add_seed(title, artist, ncm_id=None):
            ca = clean_artist(artist)
            k = (norm(title), norm(ca))
            if not k[0] or k in seen_seed:
                return
            seen_seed.add(k)
            seeds.append({"title": title, "artist": ca, "ncm_id": ncm_id})

        # 1) 本地听完率最高的歌
        local_top = self.db.q(
            "SELECT title, artist, MAX(ncm_id) ncm_id, COUNT(*) n, SUM(completed) c, SUM(skipped) s "
            "FROM plays WHERE ended_at IS NOT NULL GROUP BY title_key, artist_key "
            "HAVING COALESCE(s,0) <= COALESCE(c,0) ORDER BY c DESC, n DESC LIMIT 12"
        )
        for r in local_top:
            add_seed(r["title"], r["artist"], r["ncm_id"])

        # 2) Last.fm 账号的常听曲目 + 最近曲目
        if user:
            top_t = self.lf.user_top_tracks(user, "1month", 15) or self.lf.user_top_tracks(user, "overall", 15)
            for t in top_t[:10]:
                add_seed(t["title"], t["artist"])
            for t in self.lf.user_recent_tracks(user, 25)[:10]:
                add_seed(t["title"], t["artist"])

        # 3) 如果正在放歌，把当前正在放的也算作候选种子
        now = self.monitor.snapshot()
        if now:
            add_seed(now["title"], now["artist"], now.get("ncm_id"))

        # 4) 用户主动点过 👍 喜欢的曲目
        liked_tracks = self.db.q("SELECT title, artist FROM feedback WHERE action='like' ORDER BY created_at DESC LIMIT 8")
        for r in liked_tracks:
            add_seed(r["title"], r["artist"])

        if shuffle and len(seeds) > 8:
            seeds = random.sample(seeds, 8)
        else:
            seeds = seeds[:8]
        if not seeds:
            return {"seeds": [], "tracks": [], "user": user}

        heard = self.db.heard_index()
        fb_map = self.db.get_feedback_map()
        if user:
            for t in self.lf.user_recent_tracks(user, 100):
                hk = (norm(t["title"]), norm(t["artist"].split("/")[0]))
                heard.setdefault(hk, {"n": 1, "c": 1, "s": 0})

        items = {}

        def fetch_one(sd):
            res_lf = self.lf.similar_tracks(sd["title"], sd["artist"], 25)
            nid = sd.get("ncm_id")
            if not nid:
                m = self.ncm.match(sd["title"], sd["artist"])
                nid = m["id"] if m else None
            res_ncm = self.ncm.simi(nid, 20) if nid else []
            return sd, res_lf, res_ncm

        futs = [self.pool.submit(fetch_one, sd) for sd in seeds]
        for fut in futs:
            try:
                sd, res_lf, res_ncm = fut.result(timeout=15)
            except Exception as e:
                log.warning("roam seed failed: %s", e)
                continue
            seed_label = f"{sd['artist']} - {sd['title']}"
            for t in res_lf:
                self._merge_roam_item(items, t["title"], t["artist"], t["match"], "lastfm", seed_label, None)
            for i, s in enumerate(res_ncm):
                self._merge_roam_item(
                    items, s["title"], s["artist"], max(0.4, 0.82 - i * 0.04), "ncm", seed_label,
                    {"id": s["id"], "title": s["title"], "artist": s["artist"], "cover": s["cover"]}
                )

        out = []
        for key, it in items.items():
            if key in seen_seed:
                continue
            fb = fb_map.get(key)
            if fb == "dislike":
                continue  # 避雷黑名单，彻底过滤
            it["feedback"] = fb
            if fb == "like":
                it["score"] *= 1.4
            h = heard.get(key)
            it["heard"] = {"plays": h["n"], "completed": h["c"] or 0, "skipped": h["s"] or 0} if h else None
            if h and hide_heard:
                continue
            if h:
                it["score"] *= 0.5
                if (h["s"] or 0) >= 2 and (h["s"] or 0) > (h["c"] or 0):
                    it["score"] *= 0.2
            out.append(it)

        out.sort(key=lambda x: x["score"], reverse=True)
        final_tracks = out[:50]
        if final_tracks:
            self.pool.submit(self.db.log_recommendations, final_tracks[:20], "roam", "全盘漫游", user or "本地")
        return {"seeds": seeds, "tracks": final_tracks, "user": user}

    @staticmethod
    def _merge_roam_item(items, title, art, score, source, reason, ncm):
        key = (norm(title), norm(art.split("/")[0] if art else ""))
        if not key[0]:
            return
        if key in items:
            it = items[key]
            it["score"] += score * 0.45
            if source not in it["sources"]:
                it["sources"].append(source)
            if reason not in it["reasons"] and len(it["reasons"]) < 3:
                it["reasons"].append(reason)
            if ncm and not it.get("ncm"):
                it["ncm"] = ncm
        else:
            items[key] = {
                "title": title,
                "artist": art,
                "score": score,
                "sources": [source],
                "reasons": [reason],
                "ncm": ncm,
            }

    # ---------- 导入 Last.fm 历史打卡 ----------
    def import_lastfm(self, user=None):
        user = (user or self.cfg.get("username") or "").strip()
        if not user:
            return {"ok": False, "error": "请先填写 Last.fm 用户名"}
        tracks = self.lf.user_recent_tracks(user, limit=200)
        added = 0
        for t in reversed(tracks):
            ts = t["timestamp"]
            tk, ak = norm(t["title"]), norm(t["artist"].split("/")[0])
            exists = self.db.q(
                "SELECT id FROM plays WHERE title_key=? AND ABS(started_at-?)<120 LIMIT 1",
                (tk, ts),
            )
            if exists:
                continue
            self.db.x(
                "INSERT INTO plays(started_at,ended_at,title,artist,artists,album,completed,scrobble_state,app,title_key,artist_key) "
                "VALUES(?,?,?,?,?,?,1,'done','last.fm',?,?)",
                (ts, ts + 200, t["title"], t["artist"].split("/")[0].strip(), t["artist"], t["album"], tk, ak),
            )
            added += 1
        return {"ok": True, "imported": added, "total_fetched": len(tracks), "user": user}


    # ---------- 手动补打卡 / 删除记录 ----------
    def manual_scrobble(self, play_id=None):
        if not self.cfg.authorized:
            return {"ok": False, "error": "请先在「⚙️ 账号与设置」完成 Last.fm 授权"}
        min_ts = int(time.time()) - 14 * 86400 + 3600
        if play_id:
            rows = self.db.q(
                "SELECT id, started_at, scrobble_state FROM plays WHERE id=?", (int(play_id),))
            if not rows:
                return {"ok": False, "error": "记录不存在"}
            r = rows[0]
            if r["scrobble_state"] in ("done", "pending"):
                return {"ok": False, "error": "这条记录已打卡或正在排队"}
            if r["started_at"] <= min_ts:
                return {"ok": False, "error": "超过 14 天的记录 Last.fm 不再接受"}
            self.db.update_play(r["id"], scrobble_state="pending")
            count = 1
        else:
            rows = self.db.q(
                "SELECT id FROM plays WHERE started_at>? AND scrobble_state IN ('eligible','none') "
                "AND (scrobble_state='eligible' OR completed=1)",
                (min_ts,),
            )
            for r in rows:
                self.db.update_play(r["id"], scrobble_state="pending")
            count = len(rows)
        if count > 0:
            self.scrobbler.flush_async()  # 后台提交，不阻塞网页请求；flush 内部有锁防重复
        return {"ok": True, "queued": count}

    def delete_play(self, play_id):
        self.db.delete_play(int(play_id))
        return {"ok": True}

    def clear_skip(self, title, artist):
        """清除某首歌的切歌跳过标记（防止误切被当作避雷）"""
        self.db.x(
            "UPDATE plays SET skipped=0 WHERE title_key=? AND artist_key=?",
            (norm(title), norm(artist)),
        )
        return {"ok": True}

    # ---------- 统计 ----------
    def stats(self, days=7):
        since = int(time.time()) - days * 86400
        tot = self.db.q(
            "SELECT COUNT(*) n, COALESCE(SUM(played_ms),0) ms, COALESCE(SUM(completed),0) c, "
            "COALESCE(SUM(skipped),0) s, COUNT(DISTINCT title_key||'|'||artist_key) uniq "
            "FROM plays WHERE started_at>=? AND ended_at IS NOT NULL", (since,))[0]
        top_artists = self.db.q(
            "SELECT artist, COUNT(*) n, SUM(played_ms) ms FROM plays WHERE started_at>=? "
            "GROUP BY artist_key ORDER BY n DESC, ms DESC LIMIT 10", (since,))
        loved = self.db.q(
            "SELECT title, artist, ncm_id, COUNT(*) n, SUM(completed) c, SUM(skipped) s FROM plays "
            "WHERE ended_at IS NOT NULL GROUP BY title_key, artist_key HAVING c>=1 "
            "ORDER BY c DESC, n DESC LIMIT 10")
        skipped = self.db.q(
            "SELECT title, artist, ncm_id, COUNT(*) n, SUM(completed) c, SUM(skipped) s FROM plays "
            "WHERE ended_at IS NOT NULL GROUP BY title_key, artist_key "
            "HAVING s>=2 AND s>COALESCE(c,0) "
            "ORDER BY s DESC, n DESC LIMIT 10")
        hours = self.db.q(
            "SELECT CAST(strftime('%H', started_at, 'unixepoch', 'localtime') AS INTEGER) h, "
            "COUNT(*) n FROM plays WHERE started_at>=? GROUP BY h", (since,))
        scrobble = self.db.q(
            "SELECT scrobble_state st, COUNT(*) n FROM plays GROUP BY scrobble_state")
        recs = self.db.recs_stats(days)
        return {
            "days": days,
            "totals": tot,
            "top_artists": top_artists,
            "loved": loved,
            "skipped": skipped,
            "hours": {r["h"]: r["n"] for r in hours},
            "scrobble": {r["st"]: r["n"] for r in scrobble},
            "recs": recs,
        }

    # ---------- 用户偏好表态 ----------
    def feedback(self, title, artist, action):
        return {"ok": self.db.set_feedback(title, artist, action)}

    def feedback_list(self, limit=100):
        return {"ok": True, "list": self.db.get_feedback_list(limit)}

    # ---------- 数据导出 ----------
    def export_csv(self):
        import csv
        import io
        output = io.StringIO()
        output.write("\ufeff")  # UTF-8 BOM 确保 Windows Excel 直接双击打开不乱码
        writer = csv.writer(output)
        writer.writerow([
            "ID", "播放开始时间", "播放结束时间", "歌名", "主歌手", "完整歌手",
            "专辑", "歌曲总时长(秒)", "实际播放(秒)", "完播率(%)",
            "完整播放", "切歌跳过", "打卡状态", "播放软件", "网易云ID"
        ])
        rows = self.db.q("SELECT * FROM plays ORDER BY started_at DESC, id DESC")
        state_map = {
            "none": "未达标",
            "eligible": "达标(未打卡)",
            "pending": "排队打卡中",
            "done": "已打卡",
            "ignored": "Last.fm忽略",
            "expired": "已过期",
        }
        for r in rows:
            st = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(r["started_at"])) if r.get("started_at") else ""
            et = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(r["ended_at"])) if r.get("ended_at") else ""
            dur_s = round((r.get("duration_ms") or 0) / 1000)
            played_s = round((r.get("played_ms") or 0) / 1000)
            ratio = round((played_s / dur_s) * 100, 1) if dur_s > 0 else 0.0
            comp = "是" if r.get("completed") else "否"
            skip = "是" if r.get("skipped") else "否"
            scrobble_desc = state_map.get(r.get("scrobble_state"), r.get("scrobble_state") or "")
            writer.writerow([
                r["id"], st, et, r.get("title", ""), r.get("artist", ""), r.get("artists", ""),
                r.get("album", ""), dur_s, played_s, f"{ratio}%",
                comp, skip, scrobble_desc, r.get("app", ""), r.get("ncm_id") or ""
            ])
        return output.getvalue().encode("utf-8-sig")

    # ---------- 启动桌面独立灵动岛悬浮挂件 ----------
    def open_mini_window(self):
        import subprocess
        port = self.cfg.get("port") or 8765
        # 1. 优先启动完全脱离浏览器的原生悬浮挂件 (pywebview)
        try:
            if getattr(sys, "frozen", False):
                subprocess.Popen([sys.executable, "--mini", f"--port={port}"])
                return True
            script = os.path.join(BASE_DIR, "mini_window.py")
            if os.path.isfile(script):
                subprocess.Popen([sys.executable, script, f"--port={port}"])
                return True
        except Exception as e:
            log.warning("launch native mini window failed: %s", e)

        # 2. 兜底方案：尝试 Edge App 模式（无地址栏精简窗口）
        edge = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
        chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        exe = edge if os.path.exists(edge) else (chrome if os.path.exists(chrome) else None)
        url = f"{self.base_url}/mini"
        if exe:
            try:
                subprocess.Popen([
                    exe,
                    f"--app={url}",
                    "--window-size=390,72",
                    "--window-position=1480,920",
                ])
                return True
            except Exception as e:
                log.warning("launch mini app mode failed: %s", e)

        # 3. 最终兜底：浏览器打开
        import webbrowser
        webbrowser.open(url)
        return True
