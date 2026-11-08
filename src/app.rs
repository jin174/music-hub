use crate::config::Config;
use crate::db::{now_sec, DB};
use crate::lastfm::Lastfm;
use crate::monitor::{Monitor, PlaySnapshot};
use crate::ncm::{Ncm, NcmSong};
use crate::normalize::{clean_artist, norm};
use crate::scrobbler::Scrobbler;
use base64::Engine;
use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateResponse {
    pub now: Option<PlaySnapshot>,
    pub config: crate::config::PublicConfig,
    pub scrobbling: bool,
    pub pending: i64,
    pub last_error: Option<String>,
    pub last_ok: Option<i64>,
    pub session_app: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarTrack {
    pub title: String,
    pub artist: String,
    pub score: f64,
    pub base_score: f64,
    pub sources: Vec<String>,
    pub ncm: Option<RadarNcm>,
    pub heard: Option<RadarHeard>,
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarNcm {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub cover: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarHeard {
    pub plays: i64,
    pub completed: i64,
    pub skipped: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarResponse {
    pub seed: Option<NcmSong>,
    pub tracks: Vec<RadarTrack>,
    pub artists: Vec<crate::lastfm::SimilarArtist>,
    pub sources: RadarSourcesCount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarSourcesCount {
    pub lastfm: usize,
    pub ncm: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoamSeed {
    pub title: String,
    pub artist: String,
    pub ncm_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoamTrack {
    pub title: String,
    pub artist: String,
    pub score: f64,
    pub sources: Vec<String>,
    pub reasons: Vec<String>,
    pub ncm: Option<RadarNcm>,
    pub heard: Option<RadarHeard>,
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoamResponse {
    pub seeds: Vec<RoamSeed>,
    pub tracks: Vec<RoamTrack>,
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsTotals {
    pub n: i64,
    pub ms: i64,
    pub c: i64,
    pub s: i64,
    pub uniq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsTopArtist {
    pub artist: String,
    pub n: i64,
    pub ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsTrackItem {
    pub title: String,
    pub artist: String,
    pub ncm_id: Option<i64>,
    pub n: i64,
    pub c: i64,
    pub s: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub days: i64,
    pub totals: StatsTotals,
    pub top_artists: Vec<StatsTopArtist>,
    pub loved: Vec<StatsTrackItem>,
    pub skipped: Vec<StatsTrackItem>,
    pub hours: HashMap<i64, i64>,
    pub scrobble: HashMap<String, i64>,
    pub recs: crate::db::RecStats,
}

pub enum WidgetCommand {
    Show,
    Hide,
    Toggle,
    Quit,
}

#[derive(Clone)]
pub struct App {
    pub cfg: Config,
    pub db: DB,
    pub ncm: Ncm,
    pub lf: Lastfm,
    pub scrobbler: Scrobbler,
    pub monitor: Monitor,
    pub widget_tx: Arc<tokio::sync::RwLock<Option<tokio::sync::mpsc::UnboundedSender<WidgetCommand>>>>,
}

impl App {
    pub fn new(cfg: Config, db: DB) -> Self {
        let ncm = Ncm::new(db.clone());
        let lf = Lastfm::new(cfg.clone(), db.clone());
        let scrobbler = Scrobbler::new(cfg.clone(), db.clone(), lf.clone());
        let monitor = Monitor::new(cfg.clone(), db.clone(), ncm.clone(), scrobbler.clone());

        Self {
            cfg,
            db,
            ncm,
            lf,
            scrobbler,
            monitor,
            widget_tx: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.cfg.get_data().port)
    }

    pub async fn state(&self) -> AppStateResponse {
        let now = self.monitor.snapshot();
        let pending = self.db.count_pending_scrobbles();
        let last_error = self.scrobbler.last_error.read().await.clone();
        let last_ok = *self.scrobbler.last_ok.read().await;
        let session_app = self.monitor.session_app.read().unwrap().clone();

        AppStateResponse {
            now,
            config: self.cfg.public(),
            scrobbling: self.cfg.is_scrobbling(),
            pending,
            last_error,
            last_ok,
            session_app,
        }
    }

    pub fn play_in_client(song_id: i64) {
        let payload = serde_json::json!({
            "type": "song",
            "id": song_id,
            "cmd": "play",
            "channel": "webset"
        });
        if let Ok(json_str) = serde_json::to_string(&payload) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(json_str);
            let uri = format!("orpheus://{}", b64);
            let _ = open::that(uri);
        }
    }

    pub async fn play_artist(&self, artist: &str) -> Option<NcmSong> {
        if let Ok(songs) = self.ncm.search(artist, 20).await {
            for s in songs {
                if self.ncm.artist_ok(&s.artists, artist) {
                    Self::play_in_client(s.id);
                    return Some(s);
                }
            }
        }
        None
    }

    pub async fn open_mini_window(&self) -> bool {
        let guard = self.widget_tx.read().await;
        if let Some(ref tx) = *guard {
            let _ = tx.send(WidgetCommand::Show);
            return true;
        }
        false
    }

    pub async fn radar(&self, track: &str, artist: &str) -> RadarResponse {
        let seed = if !track.is_empty() {
            self.ncm.match_song(track, artist).await
        } else {
            None
        };

        let lf_fut = {
            let lf = self.lf.clone();
            let trk = track.to_string();
            let art = artist.to_string();
            async move {
                if !trk.is_empty() {
                    lf.similar_tracks(&trk, &art, 50).await
                } else {
                    Vec::new()
                }
            }
        };

        let ncm_fut = {
            let ncm = self.ncm.clone();
            let seed_id = seed.as_ref().map(|s| s.id);
            async move {
                if let Some(id) = seed_id {
                    ncm.simi(id, 50).await.unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
        };

        let (lf_tracks, ncm_tracks) = tokio::join!(lf_fut, ncm_fut);
        let mut items: HashMap<(String, String), RadarTrack> = HashMap::new();

        let seed_first_art = seed
            .as_ref()
            .map(|s| s.artist.as_str())
            .unwrap_or_else(|| artist.split('/').next().unwrap_or("").trim());
        let seed_key = (norm(track), norm(seed_first_art));

        for t in &lf_tracks {
            let first_art = t.artist.split('/').next().unwrap_or("").trim();
            let key = (norm(&t.title), norm(first_art));
            if key.0.is_empty() || key == seed_key {
                continue;
            }
            let entry = items.entry(key).or_insert_with(|| RadarTrack {
                title: t.title.clone(),
                artist: t.artist.clone(),
                score: t.match_score,
                base_score: t.match_score,
                sources: vec!["lastfm".to_string()],
                ncm: None,
                heard: None,
                feedback: None,
            });
            if !entry.sources.contains(&"lastfm".to_string()) {
                entry.sources.push("lastfm".to_string());
                entry.score += 0.2;
            }
        }

        for (i, s) in ncm_tracks.iter().enumerate() {
            let first_art = s.artist.split('/').next().unwrap_or("").trim();
            let key = (norm(&s.title), norm(first_art));
            if key.0.is_empty() || key == seed_key {
                continue;
            }
            let ncm_score = (0.8 - (i as f64) * 0.05).max(0.35);
            let ncm_info = RadarNcm {
                id: s.id,
                title: s.title.clone(),
                artist: s.artist.clone(),
                cover: s.cover.clone(),
            };
            let entry = items.entry(key).or_insert_with(|| RadarTrack {
                title: s.title.clone(),
                artist: s.artist.clone(),
                score: ncm_score,
                base_score: ncm_score,
                sources: vec!["ncm".to_string()],
                ncm: Some(ncm_info.clone()),
                heard: None,
                feedback: None,
            });
            if !entry.sources.contains(&"ncm".to_string()) {
                entry.sources.push("ncm".to_string());
                entry.score += 0.2;
            }
            if entry.ncm.is_none() {
                entry.ncm = Some(ncm_info);
            }
        }

        let heard_map = self.db.heard_index();
        let fb_map = self.db.get_feedback_map();

        for (key, it) in items.iter_mut() {
            it.base_score = it.score;
            if let Some(h) = heard_map.get(key) {
                it.heard = Some(RadarHeard {
                    plays: h.n,
                    completed: h.c,
                    skipped: h.s,
                });
                it.score *= 0.6;
                if h.s >= 2 && h.s > h.c {
                    it.score *= 0.3;
                }
            }
            if let Some(fb) = fb_map.get(key) {
                it.feedback = Some(fb.clone());
                if fb == "like" {
                    it.score *= 1.4;
                }
            }
        }

        let mut tracks: Vec<RadarTrack> = items
            .into_values()
            .filter(|it| it.feedback.as_deref() != Some("dislike"))
            .collect();
        tracks.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        tracks.truncate(60);

        if !tracks.is_empty() {
            let log_items: Vec<(String, String)> = tracks
                .iter()
                .take(20)
                .map(|t| (t.title.clone(), t.artist.clone()))
                .collect();
            let db = self.db.clone();
            let trk = track.to_string();
            let art = artist.to_string();
            tokio::task::spawn_blocking(move || {
                db.log_recommendations(&log_items, "radar", &trk, &art);
            });
        }

        let mut artists = Vec::new();
        if tracks.is_empty() && !artist.is_empty() {
            let first_art = artist.split('/').next().unwrap_or("").trim();
            artists = self.lf.similar_artists(first_art, 20).await;
        }

        RadarResponse {
            seed,
            tracks,
            artists,
            sources: RadarSourcesCount {
                lastfm: lf_tracks.len(),
                ncm: ncm_tracks.len(),
            },
        }
    }

    pub async fn roam(&self, user_opt: Option<&str>, hide_heard: bool, shuffle: bool) -> RoamResponse {
        let user = user_opt
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.cfg.get_data().username);

        let mut seeds: Vec<RoamSeed> = Vec::new();
        let mut seen_seed: HashSet<(String, String)> = HashSet::new();

        let mut add_seed = |title: &str, artist: &str, ncm_id: Option<i64>| {
            let ca = clean_artist(artist);
            let art = if ca.is_empty() { artist } else { &ca };
            let k = (norm(title), norm(art));
            if !k.0.is_empty() && !seen_seed.contains(&k) {
                seen_seed.insert(k);
                seeds.push(RoamSeed {
                    title: title.to_string(),
                    artist: art.to_string(),
                    ncm_id,
                });
            }
        };

        // 1) Local top tracks
        let local_rows = self.db.recent(100);
        let mut local_groups: HashMap<(String, String), (String, String, Option<i64>, i64, i64)> = HashMap::new();
        for r in local_rows {
            if r.ended_at.is_some() {
                let tk = norm(&r.title);
                let ak = norm(&clean_artist(&r.artist));
                let entry = local_groups.entry((tk, ak)).or_insert_with(|| {
                    (r.title.clone(), r.artist.clone(), r.ncm_id, 0, 0)
                });
                entry.3 += r.completed;
                entry.4 += r.skipped;
                if entry.2.is_none() && r.ncm_id.is_some() {
                    entry.2 = r.ncm_id;
                }
            }
        }
        let mut local_top: Vec<_> = local_groups.into_values().filter(|g| g.4 <= g.3).collect();
        local_top.sort_by_key(|g| std::cmp::Reverse(g.3));
        for g in local_top.into_iter().take(12) {
            add_seed(&g.0, &g.1, g.2);
        }

        // 2) Last.fm user top & recent
        if !user.is_empty() {
            let top_t = self.lf.user_top_tracks(&user, "1month", 15).await;
            for t in top_t.into_iter().take(10) {
                add_seed(&t.title, &t.artist, None);
            }
            let recent_t = self.lf.user_recent_tracks(&user, 25).await;
            for t in recent_t.into_iter().take(10) {
                add_seed(&t.title, &t.artist, None);
            }
        }

        // 3) Currently playing song
        if let Some(now) = self.monitor.snapshot() {
            add_seed(&now.title, &now.artist, now.ncm_id);
        }

        // 4) Liked tracks from feedback
        for fb in self.db.get_feedback_list(8) {
            if fb.action == "like" {
                add_seed(&fb.title, &fb.artist, None);
            }
        }

        if shuffle && seeds.len() > 8 {
            use std::collections::hash_map::RandomState;
            use std::hash::{BuildHasher, Hasher};
            let s = RandomState::new().build_hasher().finish() as usize;
            let len = seeds.len();
            if len > 0 {
                seeds.rotate_left(s % len);
            }
            seeds.truncate(8);
        } else {
            seeds.truncate(8);
        }

        if seeds.is_empty() {
            return RoamResponse {
                seeds: Vec::new(),
                tracks: Vec::new(),
                user,
            };
        }

        let mut heard_map = self.db.heard_index();
        let fb_map = self.db.get_feedback_map();

        if !user.is_empty() {
            for t in self.lf.user_recent_tracks(&user, 100).await {
                let first_art = t.artist.split('/').next().unwrap_or("").trim();
                let hk = (norm(&t.title), norm(first_art));
                heard_map.entry(hk).or_insert(crate::db::HeardInfo { n: 1, c: 1, s: 0 });
            }
        }

        let mut items: HashMap<(String, String), RoamTrack> = HashMap::new();

        for sd in &seeds {
            let lf_res = self.lf.similar_tracks(&sd.title, &sd.artist, 25).await;
            let ncm_id = match sd.ncm_id {
                Some(id) => Some(id),
                None => self.ncm.match_song(&sd.title, &sd.artist).await.map(|m| m.id),
            };
            let ncm_res = if let Some(nid) = ncm_id {
                self.ncm.simi(nid, 20).await.unwrap_or_default()
            } else {
                Vec::new()
            };

            let reason = format!("{} - {}", sd.artist, sd.title);

            for t in lf_res {
                let first_art = t.artist.split('/').next().unwrap_or("").trim();
                let key = (norm(&t.title), norm(first_art));
                if key.0.is_empty() {
                    continue;
                }
                let entry = items.entry(key).or_insert_with(|| RoamTrack {
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    score: 0.0,
                    sources: Vec::new(),
                    reasons: Vec::new(),
                    ncm: None,
                    heard: None,
                    feedback: None,
                });
                entry.score += t.match_score * 0.45;
                if !entry.sources.contains(&"lastfm".to_string()) {
                    entry.sources.push("lastfm".to_string());
                }
                if !entry.reasons.contains(&reason) && entry.reasons.len() < 3 {
                    entry.reasons.push(reason.clone());
                }
            }

            for (i, s) in ncm_res.into_iter().enumerate() {
                let first_art = s.artist.split('/').next().unwrap_or("").trim();
                let key = (norm(&s.title), norm(first_art));
                if key.0.is_empty() {
                    continue;
                }
                let score = (0.82 - (i as f64) * 0.04).max(0.4);
                let ncm_info = RadarNcm {
                    id: s.id,
                    title: s.title.clone(),
                    artist: s.artist.clone(),
                    cover: s.cover.clone(),
                };
                let entry = items.entry(key).or_insert_with(|| RoamTrack {
                    title: s.title.clone(),
                    artist: s.artist.clone(),
                    score: 0.0,
                    sources: Vec::new(),
                    reasons: Vec::new(),
                    ncm: Some(ncm_info.clone()),
                    heard: None,
                    feedback: None,
                });
                entry.score += score * 0.45;
                if !entry.sources.contains(&"ncm".to_string()) {
                    entry.sources.push("ncm".to_string());
                }
                if !entry.reasons.contains(&reason) && entry.reasons.len() < 3 {
                    entry.reasons.push(reason.clone());
                }
                if entry.ncm.is_none() {
                    entry.ncm = Some(ncm_info);
                }
            }
        }

        let mut out = Vec::new();
        for (key, mut it) in items {
            if seen_seed.contains(&key) {
                continue;
            }
            if let Some(fb) = fb_map.get(&key) {
                if fb == "dislike" {
                    continue;
                }
                it.feedback = Some(fb.clone());
                if fb == "like" {
                    it.score *= 1.4;
                }
            }
            if let Some(h) = heard_map.get(&key) {
                it.heard = Some(RadarHeard {
                    plays: h.n,
                    completed: h.c,
                    skipped: h.s,
                });
                if hide_heard {
                    continue;
                }
                it.score *= 0.5;
                if h.s >= 2 && h.s > h.c {
                    it.score *= 0.2;
                }
            }
            out.push(it);
        }

        out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(50);

        if !out.is_empty() {
            let log_items: Vec<(String, String)> = out
                .iter()
                .take(20)
                .map(|t| (t.title.clone(), t.artist.clone()))
                .collect();
            let db = self.db.clone();
            let u_label = if user.is_empty() { "本地".to_string() } else { user.clone() };
            tokio::task::spawn_blocking(move || {
                db.log_recommendations(&log_items, "roam", "全盘漫游", &u_label);
            });
        }

        RoamResponse {
            seeds,
            tracks: out,
            user,
        }
    }

    pub async fn import_lastfm(&self, user_opt: Option<&str>) -> Result<serde_json::Value, String> {
        let user = user_opt
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.cfg.get_data().username);
        if user.is_empty() {
            return Err("请先填写 Last.fm 用户名".to_string());
        }

        let tracks = self.lf.user_recent_tracks(&user, 200).await;
        let mut added = 0;

        for t in tracks.iter().rev() {
            let ts = t.timestamp;
            let tk = norm(&t.title);
            let first_art = t.artist.split('/').next().unwrap_or("").trim();
            let _ak = norm(first_art);

            let recent = self.db.recent(200);
            let exists = recent
                .iter()
                .any(|r| r.title_key.as_deref() == Some(&tk) && (r.started_at - ts).abs() < 120);
            if exists {
                continue;
            }

            let play_id = self.db.start_play(&t.title, first_art, "last.fm");
            self.db.update_play_fields(
                play_id,
                Some(first_art),
                Some(&t.artist),
                Some(&t.album),
                None,
                None,
                None,
                Some(200_000),
                Some(1),
                Some(0),
                Some("done"),
                Some(ts + 200),
            );
            added += 1;
        }

        Ok(serde_json::json!({
            "ok": true,
            "imported": added,
            "total_fetched": tracks.len(),
            "user": user
        }))
    }

    pub fn manual_scrobble(&self, play_id_opt: Option<i64>) -> Result<serde_json::Value, String> {
        if !self.cfg.is_authorized() {
            return Err("请先在「⚙️ 账号与设置」完成 Last.fm 授权".to_string());
        }
        let min_ts = now_sec() - 14 * 86400 + 3600;
        let count: usize;

        if let Some(id) = play_id_opt {
            let recent = self.db.recent(500);
            let r = recent.iter().find(|x| x.id == id).ok_or_else(|| "记录不存在".to_string())?;
            if r.scrobble_state == "done" || r.scrobble_state == "pending" {
                return Err("这条记录已打卡或正在排队".to_string());
            }
            if r.started_at <= min_ts {
                return Err("超过 14 天的记录 Last.fm 不再接受".to_string());
            }
            self.db.update_play_fields(
                r.id, None, None, None, None, None, None, None, None, None, Some("pending"), None,
            );
            count = 1;
        } else {
            let recent = self.db.recent(500);
            let mut target_ids = Vec::new();
            for r in recent {
                if r.started_at > min_ts
                    && (r.scrobble_state == "eligible" || r.completed == 1)
                    && (r.scrobble_state == "eligible" || r.scrobble_state == "none")
                {
                    target_ids.push(r.id);
                }
            }
            for id in &target_ids {
                self.db.update_play_fields(
                    *id, None, None, None, None, None, None, None, None, None, Some("pending"), None,
                );
            }
            count = target_ids.len();
        }

        if count > 0 {
            self.scrobbler.flush_async();
        }

        Ok(serde_json::json!({
            "ok": true,
            "queued": count
        }))
    }

    pub fn stats(&self, days: i64) -> StatsResponse {
        let since = now_sec() - days * 86400;
        let recent = self.db.recent(5000);
        let valid_plays: Vec<_> = recent
            .into_iter()
            .filter(|r| r.started_at >= since && r.ended_at.is_some())
            .collect();

        let n = valid_plays.len() as i64;
        let mut ms: i64 = 0;
        let mut c: i64 = 0;
        let mut s: i64 = 0;
        let mut uniq_set: HashSet<(String, String)> = HashSet::new();
        let mut artist_stats: HashMap<String, (String, i64, i64)> = HashMap::new();
        let mut track_stats: HashMap<(String, String), (String, String, Option<i64>, i64, i64, i64)> = HashMap::new();
        let mut hours: HashMap<i64, i64> = HashMap::new();
        let mut scrobble_counts: HashMap<String, i64> = HashMap::new();

        for r in valid_plays {
            ms += r.played_ms;
            c += r.completed;
            s += r.skipped;

            let tk = norm(&r.title);
            let ak = norm(&r.artist);
            uniq_set.insert((tk.clone(), ak.clone()));

            let art_entry = artist_stats.entry(ak.clone()).or_insert_with(|| (r.artist.clone(), 0, 0));
            art_entry.1 += 1;
            art_entry.2 += r.played_ms;

            let trk_entry = track_stats.entry((tk, ak)).or_insert_with(|| {
                (r.title.clone(), r.artist.clone(), r.ncm_id, 0, 0, 0)
            });
            trk_entry.3 += 1;
            trk_entry.4 += r.completed;
            trk_entry.5 += r.skipped;

            let dt = Local.timestamp_opt(r.started_at, 0).single();
            if let Some(d) = dt {
                use chrono::Timelike;
                let h = d.hour() as i64;
                *hours.entry(h).or_insert(0) += 1;
            }

            *scrobble_counts.entry(r.scrobble_state).or_insert(0) += 1;
        }

        let mut top_artists: Vec<_> = artist_stats
            .into_values()
            .map(|(artist, n, ms)| StatsTopArtist { artist, n, ms })
            .collect();
        top_artists.sort_by_key(|a| std::cmp::Reverse((a.n, a.ms)));
        top_artists.truncate(10);

        let mut loved_tracks: Vec<_> = track_stats
            .values()
            .filter(|t| t.4 >= 1)
            .map(|t| StatsTrackItem {
                title: t.0.clone(),
                artist: t.1.clone(),
                ncm_id: t.2,
                n: t.3,
                c: t.4,
                s: t.5,
            })
            .collect();
        loved_tracks.sort_by_key(|t| std::cmp::Reverse((t.c, t.n)));
        loved_tracks.truncate(10);

        let mut skipped_tracks: Vec<_> = track_stats
            .into_values()
            .filter(|t| t.5 >= 2 && t.5 > t.4)
            .map(|t| StatsTrackItem {
                title: t.0,
                artist: t.1,
                ncm_id: t.2,
                n: t.3,
                c: t.4,
                s: t.5,
            })
            .collect();
        skipped_tracks.sort_by_key(|t| std::cmp::Reverse((t.s, t.n)));
        skipped_tracks.truncate(10);

        let recs = self.db.recs_stats(days);

        StatsResponse {
            days,
            totals: StatsTotals {
                n,
                ms,
                c,
                s,
                uniq: uniq_set.len() as i64,
            },
            top_artists,
            loved: loved_tracks,
            skipped: skipped_tracks,
            hours,
            scrobble: scrobble_counts,
            recs,
        }
    }

    pub fn export_csv(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // UTF-8 BOM
        out.extend_from_slice(&[0xef, 0xbb, 0xbf]);

        let headers = "ID,播放开始时间,播放结束时间,歌名,主歌手,完整歌手,专辑,歌曲总时长(秒),实际播放(秒),完播率(%),完整播放,切歌跳过,打卡状态,播放软件,网易云ID\r\n";
        out.extend_from_slice(headers.as_bytes());

        let rows = self.db.recent(50000);
        for r in rows {
            let st_str = Local
                .timestamp_opt(r.started_at, 0)
                .single()
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();
            let et_str = r
                .ended_at
                .and_then(|et| Local.timestamp_opt(et, 0).single())
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();

            let dur_s = r.duration_ms.unwrap_or(0) / 1000;
            let played_s = r.played_ms / 1000;
            let ratio = if dur_s > 0 {
                format!("{:.1}%", (played_s as f64 / dur_s as f64) * 100.0)
            } else {
                "0.0%".to_string()
            };

            let comp = if r.completed == 1 { "是" } else { "否" };
            let skip = if r.skipped == 1 { "是" } else { "否" };

            let state_desc = match r.scrobble_state.as_str() {
                "none" => "未达标",
                "eligible" => "达标(未打卡)",
                "pending" => "排队打卡中",
                "done" => "已打卡",
                "ignored" => "Last.fm忽略",
                "expired" => "已过期",
                other => other,
            };

            let escape_csv = |s: &str| -> String {
                if s.contains(',') || s.contains('"') || s.contains('\n') {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s.to_string()
                }
            };

            let line = format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\r\n",
                r.id,
                st_str,
                et_str,
                escape_csv(&r.title),
                escape_csv(&r.artist),
                escape_csv(r.artists.as_deref().unwrap_or("")),
                escape_csv(r.album.as_deref().unwrap_or("")),
                dur_s,
                played_s,
                ratio,
                comp,
                skip,
                state_desc,
                escape_csv(r.app.as_deref().unwrap_or("")),
                r.ncm_id.map(|id| id.to_string()).unwrap_or_default()
            );
            out.extend_from_slice(line.as_bytes());
        }

        out
    }

    pub fn export_backup(&self) -> Vec<u8> {
        self.db.checkpoint_truncate();
        std::fs::read(&self.db.path).unwrap_or_default()
    }
}
