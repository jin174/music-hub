"""本地 SQLite 听歌库"""
import json
import os
import re
import sqlite3
import threading
import time

from config import DATA_DIR

DB_PATH = os.path.join(DATA_DIR, "hub.db")

SCHEMA = """
CREATE TABLE IF NOT EXISTS plays(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,          -- 打卡用的主歌手
    artists TEXT,                  -- 完整歌手名单（/ 分隔）
    album TEXT,
    ncm_id INTEGER,
    cover TEXT,
    duration_ms INTEGER,
    played_ms INTEGER DEFAULT 0,
    completed INTEGER DEFAULT 0,
    skipped INTEGER DEFAULT 0,
    -- none: 未达打卡线 | eligible: 达标但处于“只记录”模式 | pending: 等待提交
    -- done: 已打卡 | ignored: Last.fm 拒收 | expired: 超过 14 天未能提交
    scrobble_state TEXT DEFAULT 'none',
    app TEXT,
    title_key TEXT,
    artist_key TEXT
);
CREATE INDEX IF NOT EXISTS idx_plays_started ON plays(started_at);
CREATE INDEX IF NOT EXISTS idx_plays_keys ON plays(title_key, artist_key);
CREATE INDEX IF NOT EXISTS idx_plays_state ON plays(scrobble_state);
CREATE TABLE IF NOT EXISTS cache(k TEXT PRIMARY KEY, v TEXT, exp INTEGER);
"""

_NON_WORD = re.compile(r"[^0-9a-z\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af]")
_JUNK_TITLE = re.compile(r"^\d+[-_]\d+[-_]\d+$")
_NOISE_BRACKETS = re.compile(
    r"\s*[\(\[（【][^)\]）】]*(?:remaster|soundtrack|from\s+[\"“]|album\s+version|single\s+version|"
    r"deluxe|anniversary|explicit)[^)\]）】]*[\)\]）】]",
    re.IGNORECASE,
)
_NOISE_DASH = re.compile(r"\s+-\s+(?:\d{4}\s+)?remaster(?:ed)?.*$", re.IGNORECASE)
_ARTIST_ALIAS = re.compile(r"^([^\(（]+?)[\(（]([^\)）]+)[\)）]$")


def is_junk_stream(title, artist):
    """过滤网易云视频流内部编号或空歌手"""
    t = (title or "").strip()
    a = (artist or "").strip()
    if not t or not a:
        return True
    if _JUNK_TITLE.match(t):
        return True
    return False


def clean_title(title):
    """仅剥离纯发行标注后缀（如 Remaster、From "..."/Soundtrack Version），
    保留 Live / Acoustic / Remix / 伴奏 等真正不同的版本；仅在搜相似无结果时作为回退"""
    if not title:
        return ""
    s = _NOISE_BRACKETS.sub("", title.strip())
    s = _NOISE_DASH.sub("", s).strip()
    return s or title.strip()


def clean_artist(artist):
    """提取干净的主歌手名；如 '冯沁苑(买辣椒也用券)' -> '冯沁苑'"""
    if not artist:
        return ""
    first = artist.split("/")[0].strip()
    m = _ARTIST_ALIAS.match(first)
    if m:
        return m.group(1).strip()
    return first


def artist_variants(artist):
    """返回歌手的所有候选名字（如 ['冯沁苑', '买辣椒也用券']）"""
    if not artist:
        return []
    out = []
    for part in artist.split("/"):
        part = part.strip()
        if not part:
            continue
        m = _ARTIST_ALIAS.match(part)
        if m:
            out.extend([m.group(1).strip(), m.group(2).strip()])
        else:
            out.append(part)
    return [x for x in out if x]


def norm(s):
    """用于比对的归一化：小写、& → and、去掉标点空格（保留版本文字以区分不同版本）"""
    if not s:
        return ""
    return _NON_WORD.sub("", s.lower().replace("&", "and"))


class DB:
    def __init__(self, path=DB_PATH):
        self._lock = threading.RLock()
        self.conn = sqlite3.connect(path, check_same_thread=False)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.executescript(SCHEMA)
        self.conn.execute("DELETE FROM plays WHERE artist='' OR title GLOB '[0-9]*-[0-9]*-[0-9]*'")
        self.conn.commit()

    # ---------- 通用 ----------
    def q(self, sql, args=()):
        with self._lock:
            return [dict(r) for r in self.conn.execute(sql, args).fetchall()]

    def x(self, sql, args=()):
        with self._lock:
            cur = self.conn.execute(sql, args)
            self.conn.commit()
            return cur.lastrowid

    # ---------- 播放记录 ----------
    def start_play(self, title, artist, app):
        now = int(time.time())
        return self.x(
            "INSERT INTO plays(started_at,title,artist,artists,app,title_key,artist_key) VALUES(?,?,?,?,?,?,?)",
            (now, title, artist, artist, app, norm(title), norm(artist)),
        )

    def update_play(self, play_id, **fields):
        if not fields:
            return
        if "artist" in fields:
            fields["artist_key"] = norm(fields["artist"])
        if "title" in fields:
            fields["title_key"] = norm(fields["title"])
        cols = ",".join(f"{k}=?" for k in fields)
        self.x(f"UPDATE plays SET {cols} WHERE id=?", (*fields.values(), play_id))

    def delete_play(self, play_id):
        self.x("DELETE FROM plays WHERE id=?", (play_id,))

    def recent(self, limit=50):
        return self.q("SELECT * FROM plays ORDER BY started_at DESC, id DESC LIMIT ?", (limit,))

    def pending_scrobbles(self, limit=50):
        return self.q(
            "SELECT * FROM plays WHERE scrobble_state='pending' ORDER BY started_at LIMIT ?", (limit,)
        )

    def history_for(self, title, artist):
        """某首歌在本地库里的播放 / 跳过次数（用于给推荐加权）"""
        rows = self.q(
            "SELECT COUNT(*) n, SUM(completed) c, SUM(skipped) s FROM plays WHERE title_key=? AND artist_key=?",
            (norm(title), norm(artist)),
        )
        r = rows[0]
        return {"plays": r["n"] or 0, "completed": r["c"] or 0, "skipped": r["s"] or 0}

    def heard_index(self):
        """所有听过的 (title_key, artist_key) → 统计，供推荐批量过滤"""
        rows = self.q(
            "SELECT title_key, artist_key, COUNT(*) n, SUM(completed) c, SUM(skipped) s "
            "FROM plays GROUP BY title_key, artist_key"
        )
        return {(r["title_key"], r["artist_key"]): r for r in rows}

    # ---------- 缓存 ----------
    def cache_get(self, k):
        rows = self.q("SELECT v, exp FROM cache WHERE k=?", (k,))
        if rows and rows[0]["exp"] > time.time():
            return json.loads(rows[0]["v"])
        return None

    def cache_set(self, k, v, ttl):
        self.x(
            "INSERT OR REPLACE INTO cache(k,v,exp) VALUES(?,?,?)",
            (k, json.dumps(v, ensure_ascii=False), int(time.time() + ttl)),
        )
