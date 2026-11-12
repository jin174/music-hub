use crate::normalize::norm;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS plays(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    artists TEXT,
    album TEXT,
    ncm_id INTEGER,
    cover TEXT,
    duration_ms INTEGER,
    played_ms INTEGER DEFAULT 0,
    completed INTEGER DEFAULT 0,
    skipped INTEGER DEFAULT 0,
    scrobble_state TEXT DEFAULT 'none',
    app TEXT,
    title_key TEXT,
    artist_key TEXT
);
CREATE INDEX IF NOT EXISTS idx_plays_started ON plays(started_at);
CREATE INDEX IF NOT EXISTS idx_plays_keys ON plays(title_key, artist_key);
CREATE INDEX IF NOT EXISTS idx_plays_state ON plays(scrobble_state);
CREATE TABLE IF NOT EXISTS cache(k TEXT PRIMARY KEY, v TEXT, exp INTEGER);

CREATE TABLE IF NOT EXISTS recs(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    title_key TEXT NOT NULL,
    artist_key TEXT NOT NULL,
    source TEXT,
    seed_title TEXT,
    seed_artist TEXT,
    status TEXT DEFAULT 'pending',
    hit_play_id INTEGER,
    hit_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_recs_keys ON recs(title_key, artist_key);
CREATE INDEX IF NOT EXISTS idx_recs_created ON recs(created_at);

CREATE TABLE IF NOT EXISTS feedback(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    title_key TEXT NOT NULL,
    artist_key TEXT NOT NULL,
    action TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_feedback_keys ON feedback(title_key, artist_key);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayRow {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub title: String,
    pub artist: String,
    pub artists: Option<String>,
    pub album: Option<String>,
    pub ncm_id: Option<i64>,
    pub cover: Option<String>,
    pub duration_ms: Option<i64>,
    pub played_ms: i64,
    pub completed: i64,
    pub skipped: i64,
    pub scrobble_state: String,
    pub app: Option<String>,
    pub title_key: Option<String>,
    pub artist_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecStats {
    pub days: i64,
    pub total: i64,
    pub heard: i64,
    pub completed: i64,
    pub skipped: i64,
    pub conversion_rate: f64,
    pub hit_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackRow {
    pub id: i64,
    pub created_at: i64,
    pub title: String,
    pub artist: String,
    pub title_key: String,
    pub artist_key: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeardInfo {
    pub n: i64,
    pub c: i64,
    pub s: i64,
}

pub fn now_sec() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Clone)]
pub struct DB {
    conn: Arc<Mutex<Connection>>,
    pub path: PathBuf,
}

impl DB {
    pub fn open(custom_data_dir: Option<&Path>) -> Result<Self, rusqlite::Error> {
        let base_dir = if let Some(dir) = custom_data_dir {
            dir.to_path_buf()
        } else {
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."));
            exe_dir.join("data")
        };
        let _ = std::fs::create_dir_all(&base_dir);
        let db_path = base_dir.join("hub.db");

        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "DELETE FROM plays WHERE artist='' OR title GLOB '[0-9]*-[0-9]*-[0-9]*'",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: db_path,
        })
    }

    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn checkpoint_truncate(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []);
    }

    // ---------- 播放记录 ----------
    pub fn start_play(&self, title: &str, artist: &str, app: &str) -> i64 {
        let now = now_sec();
        let tk = norm(title);
        let ak = norm(artist);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO plays(started_at,title,artist,artists,app,title_key,artist_key) VALUES(?,?,?,?,?,?,?)",
            params![now, title, artist, artist, app, tk, ak],
        ).unwrap();
        conn.last_insert_rowid()
    }

    pub fn update_play_fields(
        &self,
        play_id: i64,
        artist: Option<&str>,
        artists: Option<&str>,
        album: Option<&str>,
        ncm_id: Option<i64>,
        cover: Option<&str>,
        duration_ms: Option<i64>,
        played_ms: Option<i64>,
        completed: Option<i32>,
        skipped: Option<i32>,
        scrobble_state: Option<&str>,
        ended_at: Option<i64>,
    ) {
        let conn = self.conn.lock().unwrap();
        if let Some(a) = artist {
            let ak = norm(a);
            let _ = conn.execute(
                "UPDATE plays SET artist=?, artist_key=? WHERE id=?",
                params![a, ak, play_id],
            );
        }
        if let Some(art_all) = artists {
            let _ = conn.execute(
                "UPDATE plays SET artists=? WHERE id=?",
                params![art_all, play_id],
            );
        }
        if let Some(alb) = album {
            let _ = conn.execute(
                "UPDATE plays SET album=? WHERE id=?",
                params![alb, play_id],
            );
        }
        if let Some(nid) = ncm_id {
            let _ = conn.execute(
                "UPDATE plays SET ncm_id=? WHERE id=?",
                params![nid, play_id],
            );
        }
        if let Some(c) = cover {
            let _ = conn.execute(
                "UPDATE plays SET cover=? WHERE id=?",
                params![c, play_id],
            );
        }
        if let Some(dur) = duration_ms {
            let _ = conn.execute(
                "UPDATE plays SET duration_ms=? WHERE id=?",
                params![dur, play_id],
            );
        }
        if let Some(p) = played_ms {
            let _ = conn.execute(
                "UPDATE plays SET played_ms=? WHERE id=?",
                params![p, play_id],
            );
        }
        if let Some(c) = completed {
            let _ = conn.execute(
                "UPDATE plays SET completed=? WHERE id=?",
                params![c, play_id],
            );
        }
        if let Some(s) = skipped {
            let _ = conn.execute(
                "UPDATE plays SET skipped=? WHERE id=?",
                params![s, play_id],
            );
        }
        if let Some(st) = scrobble_state {
            let _ = conn.execute(
                "UPDATE plays SET scrobble_state=? WHERE id=?",
                params![st, play_id],
            );
        }
        if let Some(end) = ended_at {
            let _ = conn.execute(
                "UPDATE plays SET ended_at=? WHERE id=?",
                params![end, play_id],
            );
        }
    }

    pub fn delete_play(&self, play_id: i64) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM plays WHERE id=?", params![play_id]);
    }

    pub fn recent(&self, limit: i64) -> Vec<PlayRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, started_at, ended_at, title, artist, artists, album, ncm_id, cover, duration_ms, played_ms, completed, skipped, scrobble_state, app, title_key, artist_key FROM plays ORDER BY started_at DESC, id DESC LIMIT ?")
            .unwrap();
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(PlayRow {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    title: row.get(3)?,
                    artist: row.get(4)?,
                    artists: row.get(5)?,
                    album: row.get(6)?,
                    ncm_id: row.get(7)?,
                    cover: row.get(8)?,
                    duration_ms: row.get(9)?,
                    played_ms: row.get(10)?,
                    completed: row.get(11)?,
                    skipped: row.get(12)?,
                    scrobble_state: row.get(13)?,
                    app: row.get(14)?,
                    title_key: row.get(15)?,
                    artist_key: row.get(16)?,
                })
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn pending_scrobbles(&self, limit: i64) -> Vec<PlayRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, started_at, ended_at, title, artist, artists, album, ncm_id, cover, duration_ms, played_ms, completed, skipped, scrobble_state, app, title_key, artist_key FROM plays WHERE scrobble_state='pending' ORDER BY started_at LIMIT ?")
            .unwrap();
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(PlayRow {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    title: row.get(3)?,
                    artist: row.get(4)?,
                    artists: row.get(5)?,
                    album: row.get(6)?,
                    ncm_id: row.get(7)?,
                    cover: row.get(8)?,
                    duration_ms: row.get(9)?,
                    played_ms: row.get(10)?,
                    completed: row.get(11)?,
                    skipped: row.get(12)?,
                    scrobble_state: row.get(13)?,
                    app: row.get(14)?,
                    title_key: row.get(15)?,
                    artist_key: row.get(16)?,
                })
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn count_pending_scrobbles(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM plays WHERE scrobble_state='pending'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    pub fn get_play_state(&self, play_id: i64) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT scrobble_state FROM plays WHERE id=?",
            params![play_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None)
    }

    pub fn heard_index(&self) -> HashMap<(String, String), HeardInfo> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT title_key, artist_key, COUNT(*) n, SUM(completed) c, SUM(skipped) s FROM plays GROUP BY title_key, artist_key")
            .unwrap();
        let mut map = HashMap::new();
        let rows = stmt
            .query_map([], |row| {
                let tk: Option<String> = row.get(0)?;
                let ak: Option<String> = row.get(1)?;
                let n: i64 = row.get(2)?;
                let c: Option<i64> = row.get(3)?;
                let s: Option<i64> = row.get(4)?;
                Ok((
                    tk.unwrap_or_default(),
                    ak.unwrap_or_default(),
                    HeardInfo {
                        n,
                        c: c.unwrap_or(0),
                        s: s.unwrap_or(0),
                    },
                ))
            })
            .unwrap();
        for r in rows.flatten() {
            if !r.0.is_empty() {
                map.insert((r.0, r.1), r.2);
            }
        }
        map
    }

    // ---------- 缓存 ----------
    pub fn cache_get(&self, k: &str) -> Option<serde_json::Value> {
        let now = now_sec();
        let conn = self.conn.lock().unwrap();
        let res: Option<(String, i64)> = conn
            .query_row(
                "SELECT v, exp FROM cache WHERE k=?",
                params![k],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .unwrap_or(None);
        if let Some((v, exp)) = res {
            if exp > now {
                return serde_json::from_str(&v).ok();
            }
        }
        None
    }

    pub fn cache_set(&self, k: &str, v: &serde_json::Value, ttl_sec: i64) {
        let exp = now_sec() + ttl_sec;
        if let Ok(json_str) = serde_json::to_string(v) {
            let conn = self.conn.lock().unwrap();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO cache(k,v,exp) VALUES(?,?,?)",
                params![k, json_str, exp],
            );
        }
    }

    // ---------- 推荐闭环追踪与表态 ----------
    pub fn log_recommendations(
        &self,
        rec_list: &[(String, String)], // (title, artist)
        source: &str,
        seed_title: &str,
        seed_artist: &str,
    ) {
        let now = now_sec();
        let cutoff = now - 7 * 86400;
        let conn = self.conn.lock().unwrap();
        for (t, a) in rec_list {
            let title = t.trim();
            let artist = a.trim();
            if title.is_empty() || artist.is_empty() {
                continue;
            }
            let tk = norm(title);
            let first_art = artist.split('/').next().unwrap_or("").trim();
            let ak = norm(first_art);
            if tk.is_empty() {
                continue;
            }
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT id FROM recs WHERE title_key=? AND artist_key=? AND created_at>?",
                    params![tk, ak, cutoff],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            if exists.is_none() {
                let _ = conn.execute(
                    "INSERT INTO recs(created_at, title, artist, title_key, artist_key, source, seed_title, seed_artist) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![now, title, artist, tk, ak, source, seed_title, seed_artist],
                );
            }
        }
    }

    pub fn check_recommendation_hit(
        &self,
        title: &str,
        artist: &str,
        play_id: i64,
        completed: bool,
        skipped: bool,
    ) {
        let tk = norm(title);
        let first_art = artist.split('/').next().unwrap_or("").trim();
        let ak = norm(first_art);
        if tk.is_empty() {
            return;
        }
        let now = now_sec();
        let cutoff = now - 30 * 86400;
        let st = if completed {
            "completed"
        } else if skipped {
            "skipped"
        } else {
            "partial"
        };
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "UPDATE recs SET status=?, hit_play_id=?, hit_at=? WHERE title_key=? AND artist_key=? AND status='pending' AND created_at>?",
            params![st, play_id, now, tk, ak, cutoff],
        );
    }

    pub fn recs_stats(&self, days: i64) -> RecStats {
        let cutoff = if days > 0 { now_sec() - days * 86400 } else { 0 };
        let conn = self.conn.lock().unwrap();
        let res = conn.query_row(
            "SELECT COUNT(*) total, SUM(CASE WHEN status != 'pending' THEN 1 ELSE 0 END) heard, SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) completed, SUM(CASE WHEN status = 'skipped' THEN 1 ELSE 0 END) skipped FROM recs WHERE created_at>=?",
            params![cutoff],
            |r| {
                let total: i64 = r.get(0)?;
                let heard: Option<i64> = r.get(1)?;
                let completed: Option<i64> = r.get(2)?;
                let skipped: Option<i64> = r.get(3)?;
                Ok((total, heard.unwrap_or(0), completed.unwrap_or(0), skipped.unwrap_or(0)))
            },
        ).unwrap_or((0, 0, 0, 0));

        let (total, heard, completed, skipped) = res;
        let conversion_rate = if total > 0 {
            ((heard as f64 / total as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        };
        let hit_rate = if heard > 0 {
            ((completed as f64 / heard as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        };

        RecStats {
            days,
            total,
            heard,
            completed,
            skipped,
            conversion_rate,
            hit_rate,
        }
    }

    pub fn set_feedback(&self, title: &str, artist: &str, action: &str) -> bool {
        let t = title.trim();
        let a = artist.trim();
        let tk = norm(t);
        let first_art = a.split('/').next().unwrap_or("").trim();
        let ak = norm(first_art);
        if tk.is_empty() {
            return false;
        }
        let conn = self.conn.lock().unwrap();
        if action == "remove" || action == "none" || action.is_empty() {
            let _ = conn.execute(
                "DELETE FROM feedback WHERE title_key=? AND artist_key=?",
                params![tk, ak],
            );
        } else {
            let now = now_sec();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO feedback(created_at, title, artist, title_key, artist_key, action) VALUES (?, ?, ?, ?, ?, ?)",
                params![now, t, a, tk, ak, action],
            );
        }
        true
    }

    /// Current feedback action ("like" / "dislike") for a single track, if any.
    pub fn get_feedback(&self, title: &str, artist: &str) -> Option<String> {
        let tk = norm(title.trim());
        let first_art = artist.split('/').next().unwrap_or("").trim();
        let ak = norm(first_art);
        if tk.is_empty() {
            return None;
        }
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT action FROM feedback WHERE title_key=? AND artist_key=?",
            params![tk, ak],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    pub fn get_feedback_map(&self) -> HashMap<(String, String), String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT title_key, artist_key, action FROM feedback")
            .unwrap();
        let mut map = HashMap::new();
        let rows = stmt
            .query_map([], |r| {
                let tk: String = r.get(0)?;
                let ak: String = r.get(1)?;
                let act: String = r.get(2)?;
                Ok((tk, ak, act))
            })
            .unwrap();
        for r in rows.flatten() {
            map.insert((r.0, r.1), r.2);
        }
        map
    }

    pub fn get_feedback_list(&self, limit: i64) -> Vec<FeedbackRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, created_at, title, artist, title_key, artist_key, action FROM feedback ORDER BY created_at DESC LIMIT ?")
            .unwrap();
        let rows = stmt
            .query_map(params![limit], |r| {
                Ok(FeedbackRow {
                    id: r.get(0)?,
                    created_at: r.get(1)?,
                    title: r.get(2)?,
                    artist: r.get(3)?,
                    title_key: r.get(4)?,
                    artist_key: r.get(5)?,
                    action: r.get(6)?,
                })
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn clear_skip(&self, title: &str, artist: &str) {
        let tk = norm(title);
        let ak = norm(artist);
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "UPDATE plays SET skipped=0 WHERE title_key=? AND artist_key=?",
            params![tk, ak],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_lifecycle() {
        let db = DB::open_in_memory().unwrap();
        let id = db.start_play("Song A", "Artist 1", "test.exe");
        assert!(id > 0);
        db.update_play_fields(
            id,
            Some("Artist 1"),
            Some("Artist 1"),
            Some("Album 1"),
            Some(123456),
            Some("http://cover.jpg"),
            Some(200_000),
            Some(180_000),
            Some(1),
            Some(0),
            Some("pending"),
            Some(now_sec()),
        );

        let rec = db.recent(10);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].title, "Song A");
        assert_eq!(rec[0].completed, 1);
        assert_eq!(rec[0].ncm_id, Some(123456));

        assert_eq!(db.count_pending_scrobbles(), 1);

        db.set_feedback("Song A", "Artist 1", "like");
        let fb_map = db.get_feedback_map();
        assert_eq!(fb_map.get(&(norm("Song A"), norm("Artist 1"))), Some(&"like".to_string()));
    }

    #[test]
    fn test_get_feedback_single_track_lookup() {
        let db = DB::open_in_memory().unwrap();

        assert_eq!(db.get_feedback("Song B", "Artist 2"), None);

        db.set_feedback("Song B", "Artist 2", "like");
        assert_eq!(db.get_feedback("Song B", "Artist 2"), Some("like".to_string()));

        // The player bar passes the full artist string; the lookup must key on the first
        // artist only, exactly like set_feedback does.
        assert_eq!(
            db.get_feedback("Song B", "Artist 2 / Guest"),
            Some("like".to_string())
        );

        // Version suffixes stay independent: a Live take never inherits the studio decision.
        assert_eq!(db.get_feedback("Song B (Live)", "Artist 2"), None);

        db.set_feedback("Song B", "Artist 2", "remove");
        assert_eq!(db.get_feedback("Song B", "Artist 2"), None);

        db.set_feedback("Song B", "Artist 2", "dislike");
        assert_eq!(
            db.get_feedback("Song B", "Artist 2"),
            Some("dislike".to_string())
        );
    }
}
