"""Windows SMTC 监听 + 播放状态机

网易云只向系统提供“歌名 + 歌手”，不提供时长和进度，所以：
- 时长：用网易云接口查出这首歌的 ID 和准确时长
- 进度：自己计时（只累计“播放中”的时间，暂停不算）
- 单曲循环：系统不会报换歌，累计时长超过一整首 + 余量时，视为新的一遍
"""
import asyncio
import ctypes
import logging
import threading
import time
from dataclasses import dataclass, field

from winsdk.windows.media.control import (
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
)

from db import clean_artist, is_junk_stream

log = logging.getLogger("monitor")

TICK_S = 1.0
MAX_DELTA_S = 3.0          # 防止休眠 / 卡顿造成计时暴涨
MIN_KEEP_MS = 1500         # 短于此的“播放”视为切歌过程中的噪声，不入库
DEFAULT_THRESHOLD_MS = 240_000
LOOP_MARGIN_MS = 15_000    # 超过一整首多少毫秒算单曲循环（容忍网易云不同版本时长差异）
PERSIST_EVERY_S = 10


@dataclass
class Play:
    play_id: int
    title: str
    artist_raw: str
    app: str
    started_at: float
    last_tick: float
    playing: bool
    artist: str = ""                 # 打卡用主歌手
    artists: list = field(default_factory=list)
    album: str = ""
    cover: str = ""
    ncm_id: int = None
    duration_ms: int = 0
    played_ms: float = 0.0
    lookup: str = "pending"          # pending | done | failed
    threshold_reached: bool = False
    scrobble_state: str = "none"
    last_persist: float = 0.0

    @property
    def threshold_ms(self):
        if self.duration_ms:
            if self.duration_ms <= 30_000:
                return None          # Last.fm 规定：30 秒以下的歌不打卡
            return min(self.duration_ms / 2, 240_000)
        return DEFAULT_THRESHOLD_MS


class Monitor:
    def __init__(self, cfg, db, ncm, scrobbler):
        self.cfg = cfg
        self.db = db
        self.ncm = ncm
        self.scrobbler = scrobbler
        self.current = None
        self.lock = threading.RLock()
        self._stop = threading.Event()
        self.session_app = None

    # ---------- 线程入口 ----------
    def run(self):
        try:
            asyncio.run(self._main())
        except Exception:
            log.exception("monitor crashed")

    def stop(self):
        self._stop.set()

    async def _main(self):
        mgr = await SessionManager.request_async()
        log.info("SMTC monitor started, watching %s", self.cfg.get("apps"))
        while not self._stop.is_set():
            try:
                await self._tick(mgr)
            except Exception:
                log.exception("tick failed")
            await asyncio.sleep(TICK_S)
        with self.lock:
            if self.current:
                self._finalize("shutdown")

    # ---------- 状态机 ----------
    def _find_session(self, mgr):
        apps = [a.lower() for a in self.cfg.get("apps") or []]
        found = None
        for s in mgr.get_sessions():
            app_id = (s.source_app_user_model_id or "").lower()
            if any(a in app_id for a in apps):
                if s.get_playback_info().playback_status == PlaybackStatus.PLAYING:
                    return s
                found = found or s
        return found

    def _find_window_fallback(self):
        """当部分国内播放器（如未开启 SMTC 的 QQ 音乐）未出现在 SMTC 时，零开销读取其主窗口标题作为兜底"""
        apps = [a.lower() for a in self.cfg.get("apps") or []]
        if not any("qqmusic" in a for a in apps):
            return None
        try:
            u32 = ctypes.windll.user32
            hwnd = u32.FindWindowW("QQMusic_Daemon_Wnd", None)
            if not hwnd:
                return None
            buf = ctypes.create_unicode_buffer(512)
            u32.GetWindowTextW(hwnd, buf, 512)
            text = (buf.value or "").strip()
            if " - " in text and not text.startswith("QQ音乐"):
                title, artist = text.split(" - ", 1)
                return title.strip(), artist.strip(), "qqmusic.exe", True
        except Exception:
            pass
        return None

    async def _tick(self, mgr):
        s = self._find_session(mgr)
        now = time.time()
        if s is not None:
            props = await s.try_get_media_properties_async()
            title = (props.title or "").strip()
            artist = (props.artist or "").strip()
            playing = s.get_playback_info().playback_status == PlaybackStatus.PLAYING
            app = s.source_app_user_model_id
        else:
            fb = self._find_window_fallback()
            if fb is None:
                with self.lock:
                    if self.current:
                        self._finalize("closed")
                self.session_app = None
                return
            title, artist, app, playing = fb

        self.session_app = app
        if is_junk_stream(title, artist):
            return  # 切歌瞬间或视频流的空歌手/内部编号，忽略

        with self.lock:
            cur = self.current
            if cur is None or cur.title != title or cur.artist_raw != artist:
                if cur:
                    self._finalize("changed")
                self._start(title, artist, app, now, playing)
                return

            if playing:
                cur.played_ms += min(now - cur.last_tick, MAX_DELTA_S) * 1000
            cur.last_tick = now
            cur.playing = playing

            # 单曲循环：累计超过一整首 + 余量 → 结算本遍，开始新一遍
            if cur.duration_ms and cur.played_ms >= cur.duration_ms + LOOP_MARGIN_MS:
                excess = cur.played_ms - cur.duration_ms
                cur.played_ms = cur.duration_ms
                self._finalize("loop")
                new = self._start(title, artist, app, now, playing, lookup_from=cur)
                new.played_ms = excess
                cur = new

            self._check_threshold(cur)
            if now - cur.last_persist >= PERSIST_EVERY_S:
                cur.last_persist = now
                self.db.update_play(cur.play_id, played_ms=int(cur.played_ms))

    def _start(self, title, artist, app, now, playing, lookup_from=None):
        primary_artist = clean_artist(artist)
        play_id = self.db.start_play(title, primary_artist, app)
        p = Play(play_id=play_id, title=title, artist_raw=artist, app=app,
                 started_at=now, last_tick=now, playing=playing, last_persist=now)
        p.artist = primary_artist
        p.artists = [a.strip() for a in artist.split("/")] if artist else []
        self.current = p
        log.info("start: %s - %s", artist, title)
        if lookup_from and lookup_from.lookup == "done":
            # 单曲循环的新一遍：直接沿用上一遍的查询结果
            self._apply_lookup(p, {
                "id": lookup_from.ncm_id, "artist": lookup_from.artist, "artists": lookup_from.artists,
                "album": lookup_from.album, "cover": lookup_from.cover,
                "duration_ms": lookup_from.duration_ms,
            } if lookup_from.ncm_id else None)
        else:
            threading.Thread(target=self._lookup, args=(p,), daemon=True).start()
        return p

    def _lookup(self, p):
        try:
            m = self.ncm.match(p.title, p.artist_raw)
        except Exception as e:
            log.warning("lookup failed: %s", e)
            m = None
        with self.lock:
            if self.current is not p:
                return
            self._apply_lookup(p, m)
        can_scrobble = bool(p.ncm_id or self.cfg.get("scrobble_unmatched"))
        if self.cfg.scrobbling and can_scrobble:
            self.scrobbler.now_playing_async(p.artist, p.title, p.album, p.duration_ms)

    def _apply_lookup(self, p, m):
        if m:
            p.ncm_id = m["id"]
            p.artist = clean_artist(m["artist"]) or p.artist
            p.artists = m["artists"] or p.artists
            p.album = m["album"]
            p.cover = m["cover"]
            p.duration_ms = m["duration_ms"]
            p.lookup = "done"
        else:
            p.lookup = "failed"
        self.db.update_play(
            p.play_id, artist=p.artist, artists="/".join(p.artists), album=p.album,
            ncm_id=p.ncm_id, cover=p.cover, duration_ms=p.duration_ms,
        )

    def _check_threshold(self, p):
        thr = p.threshold_ms
        if p.threshold_reached or thr is None or p.played_ms < thr:
            return
        p.threshold_reached = True
        can_scrobble = bool(p.ncm_id or self.cfg.get("scrobble_unmatched"))
        p.scrobble_state = "pending" if (self.cfg.scrobbling and can_scrobble) else "eligible"
        self.db.update_play(p.play_id, scrobble_state=p.scrobble_state, played_ms=int(p.played_ms))
        log.info("threshold reached (%s): %s - %s", p.scrobble_state, p.artist, p.title)
        if p.scrobble_state == "pending":
            self.scrobbler.kick()

    def _finalize(self, reason):
        p = self.current
        self.current = None
        if p is None:
            return
        played = int(p.played_ms)
        if played < MIN_KEEP_MS and not p.threshold_reached:
            self.db.delete_play(p.play_id)
            return
        dur = p.duration_ms
        completed = 1 if dur and played >= dur * 0.85 else 0
        skipped = 1 if dur and reason == "changed" and played < dur * 0.3 else 0
        self.db.update_play(p.play_id, ended_at=int(time.time()), played_ms=played,
                            completed=completed, skipped=skipped)
        try:
            self.db.check_recommendation_hit(p.title, p.artist, p.play_id, completed, skipped)
        except Exception as e:
            log.warning("check_recommendation_hit failed: %s", e)
        log.info("end(%s): %s - %s  %ds/%ds%s%s", reason, p.artist, p.title, played // 1000,
                 dur // 1000, " completed" if completed else "", " skipped" if skipped else "")

    # ---------- 给前端 / 托盘 ----------
    def snapshot(self):
        with self.lock:
            p = self.current
            if not p:
                return None
            return {
                "play_id": p.play_id,
                "title": p.title,
                "artist": p.artist,
                "artists": p.artists,
                "album": p.album,
                "cover": p.cover,
                "ncm_id": p.ncm_id,
                "duration_ms": p.duration_ms,
                "played_ms": int(p.played_ms),
                "threshold_ms": p.threshold_ms,
                "threshold_reached": p.threshold_reached,
                "scrobble_state": self._live_state(p),
                "playing": p.playing,
                "lookup": p.lookup,
                "started_at": int(p.started_at),
                "app": p.app,
            }

    def _live_state(self, p):
        if p.scrobble_state == "pending":
            row = self.db.q("SELECT scrobble_state FROM plays WHERE id=?", (p.play_id,))
            if row:
                p.scrobble_state = row[0]["scrobble_state"]
        return p.scrobble_state
