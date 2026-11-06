"""打卡队列：离线排队、联网自动补交、14 天过期"""
import logging
import threading
import time

from db import clean_artist
from lastfm import LastfmError

log = logging.getLogger("scrobbler")

RETRY_S = 60
MAX_AGE_S = 14 * 86400 - 3600   # Last.fm 只接受 14 天内的打卡


class Scrobbler:
    def __init__(self, cfg, db, lastfm):
        self.cfg = cfg
        self.db = db
        self.lf = lastfm
        self._event = threading.Event()
        self._stop = threading.Event()
        self.last_error = None
        self.last_ok = None

    def kick(self):
        self._event.set()

    def stop(self):
        self._stop.set()
        self._event.set()

    def run(self):
        while not self._stop.is_set():
            self._event.wait(RETRY_S)
            self._event.clear()
            if self._stop.is_set():
                break
            if self.cfg.scrobbling:
                try:
                    self.flush()
                except Exception:
                    log.exception("flush failed")

    def flush(self):
        while True:
            rows = self.db.pending_scrobbles(50)
            if not rows:
                return
            now = time.time()
            fresh = []
            for r in rows:
                if now - r["started_at"] > MAX_AGE_S:
                    self.db.update_play(r["id"], scrobble_state="expired")
                else:
                    fresh.append(r)
            if not fresh:
                continue
            items = [{
                "artist": clean_artist(r["artist"]) or r["artist"],
                "track": r["title"],
                "timestamp": r["started_at"],
                "album": r["album"] or None,
                "duration": (r["duration_ms"] // 1000) if r["duration_ms"] else None,
            } for r in fresh]
            try:
                accepted = self.lf.scrobble(items)
            except LastfmError as e:
                self.last_error = str(e)
                log.warning("scrobble rejected: %s", e)
                return  # 授权失效 / 服务不可用：保留 pending，稍后重试
            except Exception as e:
                self.last_error = f"网络错误：{e}"
                log.warning("scrobble network error: %s", e)
                return
            for r, ok in zip(fresh, accepted):
                self.db.update_play(r["id"], scrobble_state="done" if ok else "ignored")
            self.last_error = None
            self.last_ok = int(time.time())
            log.info("scrobbled %d item(s)", len(fresh))

    def now_playing_async(self, artist, track, album, duration_ms):
        def go():
            try:
                self.lf.now_playing(artist, track, album or None,
                                    (duration_ms // 1000) if duration_ms else None)
            except Exception as e:
                log.info("now playing failed: %s", e)
        threading.Thread(target=go, daemon=True).start()
