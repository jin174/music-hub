"""本地 HTTP 服务（只监听 127.0.0.1）"""
import json
import logging
import os
import subprocess
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from config import BASE_DIR, WEB_DIR
from lastfm import LastfmError

log = logging.getLogger("server")


class Server(ThreadingHTTPServer):
    # Windows 上 SO_REUSEADDR 会允许第二个进程绑定同一端口，必须关掉才能保证单实例
    allow_reuse_address = False
    daemon_threads = True


def make_server(app, port):
    handler = type("BoundHandler", (Handler,), {"app": app})
    return Server(("127.0.0.1", port), handler)


class Handler(BaseHTTPRequestHandler):
    app = None

    def log_message(self, fmt, *args):
        pass  # 不刷屏

    # ---------- 工具 ----------
    def _json(self, obj, code=200):
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _redirect(self, url):
        self.send_response(302)
        self.send_header("Location", url)
        self.end_headers()

    def _file(self, name, ctype):
        path = os.path.join(WEB_DIR, name)
        if not os.path.isfile(path):
            return self._json({"error": "not found"}, 404)
        with open(path, "rb") as f:
            body = f.read()
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _body(self):
        n = int(self.headers.get("Content-Length") or 0)
        if not n:
            return {}
        return json.loads(self.rfile.read(n).decode("utf-8"))

    def _same_origin(self):
        """简单防护：只接受来自本地控制台页面的写操作"""
        origin = self.headers.get("Origin")
        return origin is None or origin == self.app.base_url

    # ---------- GET ----------
    def do_GET(self):
        url = urllib.parse.urlparse(self.path)
        qs = {k: v[0] for k, v in urllib.parse.parse_qs(url.query).items()}
        a = self.app
        try:
            if url.path in ("/", "/index.html"):
                return self._file("index.html", "text/html; charset=utf-8")
            if url.path in ("/mini", "/mini.html"):
                return self._file("mini.html", "text/html; charset=utf-8")
            if url.path == "/api/state":
                return self._json(a.state())
            if url.path == "/api/recent":
                return self._json(a.db.recent(int(qs.get("limit", 50))))
            if url.path == "/api/stats":
                return self._json(a.stats(int(qs.get("days", 7))))
            if url.path == "/api/radar":
                return self._json(a.radar(qs.get("track", "").strip(), qs.get("artist", "").strip()))
            if url.path == "/api/roam":
                return self._json(
                    a.roam(
                        qs.get("user", "").strip(),
                        qs.get("hide_heard", "1") != "0",
                        qs.get("shuffle") == "1",
                    )
                )
            if url.path == "/api/ncm/match":
                return self._json(a.ncm.match(qs.get("track", ""), qs.get("artist", "")) or {})
            if url.path == "/api/ncm/song":
                return self._json(a.ncm.detail(int(qs["id"])) or {})
            if url.path == "/api/recs/stats":
                return self._json(a.db.recs_stats(int(qs.get("days", 30))))
            if url.path == "/api/feedback":
                return self._json(a.feedback_list(int(qs.get("limit", 100))))
            if url.path == "/api/export/csv":
                csv_bytes = a.export_csv()
                fname = f"musichub_history_{time.strftime('%Y%m%d_%H%M%S')}.csv"
                self.send_response(200)
                self.send_header("Content-Type", "text/csv; charset=utf-8-sig")
                self.send_header("Content-Disposition", f'attachment; filename="{fname}"')
                self.send_header("Content-Length", str(len(csv_bytes)))
                self.end_headers()
                self.wfile.write(csv_bytes)
                return
            if url.path == "/api/export/backup":
                from db import DB_PATH
                with a.db._lock:
                    a.db.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
                with open(DB_PATH, "rb") as f:
                    db_bytes = f.read()
                fname = f"musichub_backup_{time.strftime('%Y%m%d_%H%M%S')}.db"
                self.send_response(200)
                self.send_header("Content-Type", "application/x-sqlite3")
                self.send_header("Content-Disposition", f'attachment; filename="{fname}"')
                self.send_header("Content-Length", str(len(db_bytes)))
                self.end_headers()
                self.wfile.write(db_bytes)
                return
            if url.path == "/api/auth/start":
                if not a.cfg.has_credentials:
                    return self._redirect("/#settings")
                return self._redirect(a.lf.auth_url(a.base_url + "/callback"))
            if url.path == "/callback":
                token = qs.get("token")
                if not token:
                    return self._redirect("/#settings")
                try:
                    sk, name = a.lf.get_session(token)
                    a.cfg.update(session_key=sk, username=name)
                    log.info("authorized as %s", name)
                    return self._redirect("/?auth=ok#settings")
                except Exception as e:
                    log.warning("auth failed: %s", e)
                    return self._redirect("/?auth=fail#settings")
            return self._json({"error": "not found"}, 404)
        except Exception as e:
            log.exception("GET %s failed", self.path)
            return self._json({"error": str(e)}, 500)

    # ---------- POST ----------
    def do_POST(self):
        if not self._same_origin():
            return self._json({"error": "forbidden"}, 403)
        url = urllib.parse.urlparse(self.path)
        a = self.app
        try:
            body = self._body()
            if url.path == "/api/play":
                a.play_in_client(int(body["id"]))
                return self._json({"ok": True})
            if url.path == "/api/play_artist":
                s = a.play_artist(body["artist"])
                return self._json({"ok": bool(s), "song": s})
            if url.path == "/api/config":
                updates = {}
                for k in ("api_key", "api_secret", "username"):
                    if k in body:
                        v = (body.get(k) or "").strip()
                        if v or k == "username":
                            updates[k] = v
                if "scrobble_unmatched" in body:
                    updates["scrobble_unmatched"] = bool(body["scrobble_unmatched"])
                if "apps" in body and isinstance(body["apps"], list):
                    updates["apps"] = [str(x).strip() for x in body["apps"] if str(x).strip()]
                if "api_key" in updates and updates["api_key"] != a.cfg.get("api_key"):
                    updates["session_key"] = ""  # 换了 Key，需要重新授权
                a.cfg.update(**updates)
                return self._json({"ok": True, "config": a.cfg.public()})
            if url.path == "/api/play/delete":
                return self._json(a.delete_play(body["id"]))
            if url.path == "/api/play/clear_skip":
                return self._json(a.clear_skip(body.get("title", ""), body.get("artist", "")))
            if url.path == "/api/feedback":
                return self._json(a.feedback(body.get("title", ""), body.get("artist", ""), body.get("action", "")))
            if url.path == "/api/open_mini":
                return self._json({"ok": a.open_mini_window()})
            if url.path == "/api/play/scrobble":
                return self._json(a.manual_scrobble(body.get("id")))
            if url.path == "/api/import_lastfm":
                user = (body.get("user") or "").strip()
                if user:
                    a.cfg.update(username=user)
                return self._json(a.import_lastfm(user))
            if url.path == "/api/mode":
                mode = body.get("mode")
                if mode == "scrobble" and not a.cfg.authorized:
                    return self._json({"ok": False, "error": "请先完成 Last.fm 授权"}, 400)
                if mode not in ("record", "scrobble"):
                    return self._json({"ok": False, "error": "bad mode"}, 400)
                a.cfg.update(mode=mode)
                if mode == "scrobble":
                    a.scrobbler.kick()
                return self._json({"ok": True})
            if url.path == "/api/love":
                if not a.cfg.authorized:
                    return self._json({"ok": False, "error": "请先完成 Last.fm 授权"}, 400)
                now = a.monitor.snapshot()
                artist = body.get("artist") or (now or {}).get("artist")
                track = body.get("track") or (now or {}).get("title")
                if not (artist and track):
                    return self._json({"ok": False, "error": "当前没有在播放"}, 400)
                a.lf.love(artist, track)
                return self._json({"ok": True})
            if url.path == "/api/logout":
                a.cfg.update(session_key="", username="", mode="record")
                return self._json({"ok": True})
            if url.path == "/api/quit":
                self._json({"ok": True})
                os._exit(0)
            if url.path == "/api/restart":
                self._json({"ok": True})
                if getattr(sys, "frozen", False):
                    cmd = f'ping -n 2 127.0.0.1 >nul && "{sys.executable}"'
                else:
                    hub_py = os.path.join(BASE_DIR, "hub.py")
                    cmd = f'ping -n 2 127.0.0.1 >nul && "{sys.executable}" "{hub_py}"'
                subprocess.Popen(cmd, shell=True, creationflags=0x08000000)
                os._exit(0)
            return self._json({"error": "not found"}, 404)
        except LastfmError as e:
            return self._json({"ok": False, "error": f"Last.fm：{e.message}"}, 400)
        except Exception as e:
            log.exception("POST %s failed", self.path)
            return self._json({"ok": False, "error": str(e)}, 500)
