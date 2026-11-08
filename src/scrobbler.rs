use crate::config::Config;
use crate::db::{now_sec, DB};
use crate::lastfm::{Lastfm, ScrobbleItem};
use crate::normalize::clean_artist;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::time::{sleep, Duration};

const RETRY_SEC: u64 = 60;
const MAX_AGE_SEC: i64 = 14 * 86400 - 3600;

#[derive(Clone)]
pub struct Scrobbler {
    cfg: Config,
    db: DB,
    lf: Lastfm,
    notify: Arc<Notify>,
    stop_flag: Arc<AtomicBool>,
    flush_lock: Arc<Mutex<()>>,
    pub last_error: Arc<RwLock<Option<String>>>,
    pub last_ok: Arc<RwLock<Option<i64>>>,
}

impl Scrobbler {
    pub fn new(cfg: Config, db: DB, lf: Lastfm) -> Self {
        Self {
            cfg,
            db,
            lf,
            notify: Arc::new(Notify::new()),
            stop_flag: Arc::new(AtomicBool::new(false)),
            flush_lock: Arc::new(Mutex::new(())),
            last_error: Arc::new(RwLock::new(None)),
            last_ok: Arc::new(RwLock::new(None)),
        }
    }

    pub fn kick(&self) {
        self.notify.notify_one();
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    pub fn spawn_worker(self) {
        tokio::spawn(async move {
            while !self.stop_flag.load(Ordering::SeqCst) {
                tokio::select! {
                    _ = sleep(Duration::from_secs(RETRY_SEC)) => {},
                    _ = self.notify.notified() => {},
                }
                if self.stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                if self.cfg.is_scrobbling() {
                    let _ = self.flush().await;
                }
            }
        });
    }

    pub fn flush_async(&self) {
        let sc = self.clone();
        tokio::spawn(async move {
            let _ = sc.flush().await;
        });
    }

    pub async fn flush(&self) -> Result<(), String> {
        let _guard = self.flush_lock.lock().await;
        self.flush_locked().await
    }

    async fn flush_locked(&self) -> Result<(), String> {
        loop {
            let rows = self.db.pending_scrobbles(50);
            if rows.is_empty() {
                return Ok(());
            }

            let now = now_sec();
            let mut fresh = Vec::new();
            for r in rows {
                if now - r.started_at > MAX_AGE_SEC {
                    self.db.update_play_fields(
                        r.id,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some("expired"),
                        None,
                    );
                } else {
                    fresh.push(r);
                }
            }

            if fresh.is_empty() {
                continue;
            }

            let items: Vec<ScrobbleItem> = fresh
                .iter()
                .map(|r| {
                    let ca = clean_artist(&r.artist);
                    ScrobbleItem {
                        artist: if ca.is_empty() { r.artist.clone() } else { ca },
                        track: r.title.clone(),
                        timestamp: r.started_at,
                        album: r.album.clone(),
                        duration: r.duration_ms.map(|d| d / 1000),
                    }
                })
                .collect();

            match self.lf.scrobble(&items).await {
                Ok(accepted) => {
                    for (r, ok) in fresh.iter().zip(accepted.into_iter()) {
                        let st = if ok { "done" } else { "ignored" };
                        self.db.update_play_fields(
                            r.id,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(st),
                            None,
                        );
                    }
                    *self.last_error.write().await = None;
                    *self.last_ok.write().await = Some(now_sec());
                    tracing::info!("Scrobbled {} items successfully", fresh.len());
                }
                Err(err) => {
                    tracing::warn!("Scrobble failed: {err}");
                    *self.last_error.write().await = Some(err.clone());
                    return Err(err);
                }
            }
        }
    }

    pub fn now_playing_async(
        &self,
        artist: String,
        track: String,
        album: Option<String>,
        duration_ms: Option<i64>,
    ) {
        let lf = self.lf.clone();
        tokio::spawn(async move {
            let dur_s = duration_ms.map(|d| d / 1000);
            let _ = lf
                .update_now_playing(&artist, &track, album.as_deref(), dur_s)
                .await;
        });
    }
}
