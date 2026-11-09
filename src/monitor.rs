use crate::config::Config;
use crate::db::{now_sec, DB};
use crate::ncm::{Ncm, NcmSong};
use crate::normalize::{clean_artist, is_junk_stream};
use crate::scrobbler::Scrobbler;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::w;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowTextW};

const TICK_S: f64 = 1.0;
const MAX_DELTA_S: f64 = 3.0;
const MIN_KEEP_MS: i64 = 1500;
const DEFAULT_THRESHOLD_MS: i64 = 240_000;
const LOOP_MARGIN_MS: i64 = 15_000;
const PERSIST_EVERY_S: f64 = 10.0;

fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[derive(Debug, Clone)]
pub struct CurrentPlay {
    pub play_id: i64,
    pub title: String,
    pub artist_raw: String,
    pub app: String,
    pub started_at: f64,
    pub last_tick: f64,
    pub playing: bool,
    pub artist: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover: String,
    pub cover_bytes: Option<Arc<Vec<u8>>>,
    pub ncm_id: Option<i64>,
    pub duration_ms: i64,
    pub played_ms: f64,
    pub lookup: String,
    pub threshold_reached: bool,
    pub scrobble_state: String,
    pub last_persist: f64,
}

impl CurrentPlay {
    pub fn threshold_ms(&self) -> Option<i64> {
        if self.duration_ms > 0 {
            if self.duration_ms <= 30_000 {
                return None;
            }
            return Some((self.duration_ms / 2).min(240_000));
        }
        Some(DEFAULT_THRESHOLD_MS)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaySnapshot {
    pub play_id: i64,
    pub title: String,
    pub artist: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover: String,
    #[serde(skip)]
    pub cover_bytes: Option<Arc<Vec<u8>>>,
    pub ncm_id: Option<i64>,
    pub duration_ms: i64,
    pub played_ms: i64,
    pub threshold_ms: Option<i64>,
    pub threshold_reached: bool,
    pub scrobble_state: String,
    pub playing: bool,
    pub lookup: String,
    pub started_at: i64,
    pub app: String,
}

#[derive(Clone)]
pub struct Monitor {
    cfg: Config,
    db: DB,
    ncm: Ncm,
    scrobbler: Scrobbler,
    current: Arc<RwLock<Option<CurrentPlay>>>,
    pub session_app: Arc<RwLock<Option<String>>>,
    stop_flag: Arc<AtomicBool>,
}

impl Monitor {
    pub fn new(cfg: Config, db: DB, ncm: Ncm, scrobbler: Scrobbler) -> Self {
        Self {
            cfg,
            db,
            ncm,
            scrobbler,
            current: Arc::new(RwLock::new(None)),
            session_app: Arc::new(RwLock::new(None)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> Option<PlaySnapshot> {
        let guard = self.current.read().unwrap();
        let p = guard.as_ref()?;
        let mut st = p.scrobble_state.clone();
        if st == "pending" {
            if let Some(db_st) = self.db.get_play_state(p.play_id) {
                st = db_st;
            }
        }

        Some(PlaySnapshot {
            play_id: p.play_id,
            title: p.title.clone(),
            artist: p.artist.clone(),
            artists: p.artists.clone(),
            album: p.album.clone(),
            cover: p.cover.clone(),
            cover_bytes: p.cover_bytes.clone(),
            ncm_id: p.ncm_id,
            duration_ms: p.duration_ms,
            played_ms: p.played_ms as i64,
            threshold_ms: p.threshold_ms(),
            threshold_reached: p.threshold_reached,
            scrobble_state: st,
            playing: p.playing,
            lookup: p.lookup.clone(),
            started_at: p.started_at as i64,
            app: p.app.clone(),
        })
    }

    pub fn spawn_worker(self) {
        std::thread::Builder::new()
            .name("smtc-monitor".to_string())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }

                let mgr = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
                    Ok(op) => match op.get() {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::error!("Failed to get SMTC SessionManager: {e}");
                            return;
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to RequestAsync SMTC SessionManager: {e}");
                        return;
                    }
                };

                tracing::info!("SMTC monitor worker started");

                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async {
                    while !self.stop_flag.load(Ordering::SeqCst) {
                        let tick_start = Instant::now();
                        self.tick(&mgr).await;
                        let elapsed = tick_start.elapsed();
                        let sleep_time = Duration::from_secs_f64(TICK_S).saturating_sub(elapsed);
                        tokio::time::sleep(sleep_time).await;
                    }

                    let mut guard = self.current.write().unwrap();
                    if guard.is_some() {
                        self.finalize(&mut guard, "shutdown").await;
                    }
                });
            })
            .expect("Failed to spawn SMTC thread");
    }

    fn find_window_fallback(&self) -> Option<(String, String, String, bool)> {
        let cfg_data = self.cfg.get_data();
        let watching_qq = cfg_data
            .apps
            .iter()
            .any(|a| a.to_lowercase().contains("qqmusic"));
        if !watching_qq {
            return None;
        }

        unsafe {
            if let Ok(hwnd) = FindWindowW(w!("QQMusic_Daemon_Wnd"), None) {
                if !hwnd.is_invalid() {
                    let mut buf = [0u16; 512];
                    let len = GetWindowTextW(hwnd, &mut buf);
                    if len > 0 {
                        let text = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();
                        if text.contains(" - ") && !text.starts_with("QQ音乐") {
                            let parts: Vec<&str> = text.splitn(2, " - ").collect();
                            if parts.len() == 2 {
                                return Some((
                                    parts[0].trim().to_string(),
                                    parts[1].trim().to_string(),
                                    "qqmusic.exe".to_string(),
                                    true,
                                ));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_session(
        &self,
        mgr: &GlobalSystemMediaTransportControlsSessionManager,
    ) -> Option<GlobalSystemMediaTransportControlsSession> {
        let cfg_data = self.cfg.get_data();
        let apps: Vec<String> = cfg_data.apps.iter().map(|a| a.to_lowercase()).collect();

        let sessions = mgr.GetSessions().ok()?;
        let count = sessions.Size().ok()?;
        let mut fallback_session = None;

        for i in 0..count {
            if let Ok(s) = sessions.GetAt(i) {
                let app_id = s
                    .SourceAppUserModelId()
                    .map(|id| id.to_string().to_lowercase())
                    .unwrap_or_default();
                if apps.iter().any(|a| app_id.contains(a)) {
                    if let Ok(info) = s.GetPlaybackInfo() {
                        if let Ok(st) = info.PlaybackStatus() {
                            if st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
                            {
                                return Some(s);
                            }
                        }
                    }
                    if fallback_session.is_none() {
                        fallback_session = Some(s);
                    }
                }
            }
        }
        fallback_session
    }

    fn extract_smtc_thumbnail(
        props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
    ) -> Option<Arc<Vec<u8>>> {
        use windows::Storage::Streams::DataReader;
        let thumb_ref = props.Thumbnail().ok()?;
        let stream = thumb_ref.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()? as u32;
        if size == 0 || size > 8 * 1024 * 1024 {
            return None;
        }
        let input = stream.GetInputStreamAt(0).ok()?;
        let reader = DataReader::CreateDataReader(&input).ok()?;
        reader.LoadAsync(size).ok()?.get().ok()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf).ok()?;
        Some(Arc::new(buf))
    }

    async fn tick(&self, mgr: &GlobalSystemMediaTransportControlsSessionManager) {
        let mut smtc_session_props = None;
        let (title, artist, app, playing) = if let Some(s) = self.find_session(mgr) {
            let props = match s.TryGetMediaPropertiesAsync().ok().and_then(|op| op.get().ok()) {
                Some(p) => p,
                None => return,
            };
            let t = props.Title().map(|s| s.to_string()).unwrap_or_default().trim().to_string();
            let a = props.Artist().map(|s| s.to_string()).unwrap_or_default().trim().to_string();
            let app_id = s.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default();
            let is_playing = s
                .GetPlaybackInfo()
                .ok()
                .and_then(|i| i.PlaybackStatus().ok())
                == Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
            smtc_session_props = Some((s, props));
            (t, a, app_id, is_playing)
        } else if let Some(fb) = self.find_window_fallback() {
            fb
        } else {
            let mut guard = self.current.write().unwrap();
            if guard.is_some() {
                self.finalize(&mut guard, "closed").await;
            }
            *self.session_app.write().unwrap() = None;
            return;
        };

        *self.session_app.write().unwrap() = Some(app.clone());
        if is_junk_stream(&title, &artist) {
            return;
        }

        let now = now_f64();
        let mut guard = self.current.write().unwrap();

        let needs_start = match &*guard {
            None => true,
            Some(cur) => cur.title != title || cur.artist_raw != artist,
        };

        if needs_start {
            if guard.is_some() {
                self.finalize(&mut guard, "changed").await;
            }
            let mut new_play = self.start_play(&title, &artist, &app, now, playing, None).await;
            if let Some((ref s, ref props)) = smtc_session_props {
                new_play.cover_bytes = Self::extract_smtc_thumbnail(props);
                if new_play.duration_ms <= 0 {
                    if let Some(dur) = s.GetTimelineProperties().ok().and_then(|tl| {
                        let end = tl.EndTime().ok()?.Duration;
                        let start = tl.StartTime().ok()?.Duration;
                        let d = (end - start) / 10_000;
                        if d > 0 { Some(d) } else { None }
                    }) {
                        new_play.duration_ms = dur;
                    }
                }
            }
            *guard = Some(new_play);
            return;
        }

        if let Some(ref mut cur) = *guard {
            if cur.cover_bytes.is_none() {
                if let Some((_, ref props)) = smtc_session_props {
                    cur.cover_bytes = Self::extract_smtc_thumbnail(props);
                }
            }
            if cur.duration_ms <= 0 {
                if let Some((ref s, _)) = smtc_session_props {
                    if let Some(dur) = s.GetTimelineProperties().ok().and_then(|tl| {
                        let end = tl.EndTime().ok()?.Duration;
                        let start = tl.StartTime().ok()?.Duration;
                        let d = (end - start) / 10_000;
                        if d > 0 { Some(d) } else { None }
                    }) {
                        cur.duration_ms = dur;
                    }
                }
            }
            if playing {
                cur.played_ms += (now - cur.last_tick).min(MAX_DELTA_S) * 1000.0;
            }
            cur.last_tick = now;
            cur.playing = playing;

            // Single song loop detection
            if cur.duration_ms > 0
                && cur.played_ms >= (cur.duration_ms + LOOP_MARGIN_MS) as f64
            {
                let excess = cur.played_ms - cur.duration_ms as f64;
                cur.played_ms = cur.duration_ms as f64;
                let prev = cur.clone();
                self.finalize(&mut guard, "loop").await;
                let mut new_p = self.start_play(&title, &artist, &app, now, playing, Some(&prev)).await;
                new_p.played_ms = excess;
                *guard = Some(new_p);
                return;
            }

            self.check_threshold(cur);
            if now - cur.last_persist >= PERSIST_EVERY_S {
                cur.last_persist = now;
                self.db.update_play_fields(
                    cur.play_id,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(cur.played_ms as i64),
                    None,
                    None,
                    None,
                    None,
                );
            }
        }
    }

    async fn start_play(
        &self,
        title: &str,
        artist: &str,
        app: &str,
        now: f64,
        playing: bool,
        lookup_from: Option<&CurrentPlay>,
    ) -> CurrentPlay {
        let primary_artist = clean_artist(artist);
        let first_art = if primary_artist.is_empty() { artist } else { &primary_artist };
        let play_id = self.db.start_play(title, first_art, app);

        let artists_vec: Vec<String> = if artist.is_empty() {
            Vec::new()
        } else {
            artist.split('/').map(|s| s.trim().to_string()).collect()
        };

        let mut p = CurrentPlay {
            play_id,
            title: title.to_string(),
            artist_raw: artist.to_string(),
            app: app.to_string(),
            started_at: now,
            last_tick: now,
            playing,
            artist: first_art.to_string(),
            artists: artists_vec,
            album: String::new(),
            cover: String::new(),
            cover_bytes: None,
            ncm_id: None,
            duration_ms: 0,
            played_ms: 0.0,
            lookup: "pending".to_string(),
            threshold_reached: false,
            scrobble_state: "none".to_string(),
            last_persist: now,
        };

        tracing::info!("Start play: {} - {}", artist, title);

        if let Some(prev) = lookup_from {
            if prev.lookup == "done" {
                p.ncm_id = prev.ncm_id;
                p.artist = prev.artist.clone();
                p.artists = prev.artists.clone();
                p.album = prev.album.clone();
                p.cover = prev.cover.clone();
                p.cover_bytes = prev.cover_bytes.clone();
                p.duration_ms = prev.duration_ms;
                p.lookup = "done".to_string();
                self.db.update_play_fields(
                    p.play_id,
                    Some(&p.artist),
                    Some(&p.artists.join("/")),
                    Some(&p.album),
                    p.ncm_id,
                    Some(&p.cover),
                    Some(p.duration_ms),
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                return p;
            }
        }

        // Spawn async lookup
        let ncm = self.ncm.clone();
        let db = self.db.clone();
        let cfg = self.cfg.clone();
        let scrobbler = self.scrobbler.clone();
        let current_lock = self.current.clone();
        let play_id_val = p.play_id;
        let title_clone = p.title.clone();
        let artist_raw_clone = p.artist_raw.clone();

        tokio::spawn(async move {
            let m: Option<NcmSong> = ncm.match_song(&title_clone, &artist_raw_clone).await;

            let mut guard = current_lock.write().unwrap();
            if let Some(ref mut cur) = *guard {
                if cur.play_id == play_id_val {
                    if let Some(ref song) = m {
                        cur.ncm_id = Some(song.id);
                        let ca = clean_artist(&song.artist);
                        cur.artist = if ca.is_empty() { song.artist.clone() } else { ca };
                        cur.artists = song.artists.clone();
                        cur.album = song.album.clone();
                        cur.cover = song.cover.clone();
                        if song.duration_ms > 0 {
                            cur.duration_ms = song.duration_ms;
                        }
                        cur.lookup = "done".to_string();
                    } else {
                        cur.lookup = "failed".to_string();
                    }

                    db.update_play_fields(
                        cur.play_id,
                        Some(&cur.artist),
                        Some(&cur.artists.join("/")),
                        Some(&cur.album),
                        cur.ncm_id,
                        Some(&cur.cover),
                        Some(cur.duration_ms),
                        None,
                        None,
                        None,
                        None,
                        None,
                    );

                    let can_scrobble = cur.ncm_id.is_some() || cfg.get_data().scrobble_unmatched;
                    if cfg.is_scrobbling() && can_scrobble {
                        scrobbler.now_playing_async(
                            cur.artist.clone(),
                            cur.title.clone(),
                            if cur.album.is_empty() { None } else { Some(cur.album.clone()) },
                            Some(cur.duration_ms),
                        );
                    }
                }
            }
        });

        p
    }

    fn check_threshold(&self, cur: &mut CurrentPlay) {
        let thr = match cur.threshold_ms() {
            Some(t) => t,
            None => return,
        };

        if cur.threshold_reached || (cur.played_ms as i64) < thr {
            return;
        }

        cur.threshold_reached = true;
        let can_scrobble = cur.ncm_id.is_some() || self.cfg.get_data().scrobble_unmatched;
        cur.scrobble_state = if self.cfg.is_scrobbling() && can_scrobble {
            "pending".to_string()
        } else {
            "eligible".to_string()
        };

        self.db.update_play_fields(
            cur.play_id,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(cur.played_ms as i64),
            None,
            None,
            Some(&cur.scrobble_state),
            None,
        );

        tracing::info!(
            "Threshold reached ({}): {} - {}",
            cur.scrobble_state,
            cur.artist,
            cur.title
        );

        if cur.scrobble_state == "pending" {
            self.scrobbler.kick();
        }
    }

    async fn finalize(&self, guard: &mut Option<CurrentPlay>, reason: &str) {
        let p = match guard.take() {
            Some(p) => p,
            None => return,
        };

        let played = p.played_ms as i64;
        if played < MIN_KEEP_MS && !p.threshold_reached {
            self.db.delete_play(p.play_id);
            return;
        }

        let dur = p.duration_ms;
        let completed = if dur > 0 && played >= (dur as f64 * 0.85) as i64 {
            1
        } else {
            0
        };
        let skipped = if dur > 0 && reason == "changed" && played < (dur as f64 * 0.3) as i64 {
            1
        } else {
            0
        };

        self.db.update_play_fields(
            p.play_id,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(played),
            Some(completed),
            Some(skipped),
            None,
            Some(now_sec()),
        );

        self.db.check_recommendation_hit(
            &p.title,
            &p.artist,
            p.play_id,
            completed == 1,
            skipped == 1,
        );

        tracing::info!(
            "End play ({}): {} - {}  {}s/{}s{}{}",
            reason,
            p.artist,
            p.title,
            played / 1000,
            dur / 1000,
            if completed == 1 { " completed" } else { "" },
            if skipped == 1 { " skipped" } else { "" }
        );
    }
}
