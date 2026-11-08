use crate::config::Config;
use crate::db::DB;
use crate::normalize::{clean_artist, clean_title};
use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

pub const API_URL: &str = "https://ws.audioscrobbler.com/2.0/";
pub const AUTH_URL: &str = "https://www.last.fm/api/auth/";
const DAY_SEC: i64 = 86400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleItem {
    pub artist: String,
    pub track: String,
    pub timestamp: i64,
    pub album: Option<String>,
    pub duration: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarTrack {
    pub title: String,
    pub artist: String,
    pub match_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarArtist {
    pub artist: String,
    pub match_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTopTrack {
    pub title: String,
    pub artist: String,
    pub playcount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecentTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub timestamp: i64,
}

#[derive(Clone)]
pub struct Lastfm {
    cfg: Config,
    db: DB,
    client: reqwest::Client,
}

impl Lastfm {
    pub fn new(cfg: Config, db: DB) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("MusicHub/2.0"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self { cfg, db, client }
    }

    pub fn sign(params: &BTreeMap<String, String>, secret: &str) -> String {
        let mut raw = String::new();
        for (k, v) in params {
            if k != "format" && k != "callback" {
                raw.push_str(k);
                raw.push_str(v);
            }
        }
        raw.push_str(secret);

        let mut hasher = Md5::new();
        hasher.update(raw.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub async fn call(
        &self,
        method: &str,
        mut params: BTreeMap<String, String>,
        signed: bool,
        post: bool,
    ) -> Result<serde_json::Value, String> {
        params.insert("method".to_string(), method.to_string());
        let cfg_data = self.cfg.get_data();

        if signed {
            params.insert("api_key".to_string(), cfg_data.api_key.clone());
            let sig = Self::sign(&params, &cfg_data.api_secret);
            params.insert("api_sig".to_string(), sig);
        } else {
            params.entry("api_key".to_string()).or_insert_with(|| self.cfg.read_key());
        }
        params.insert("format".to_string(), "json".to_string());

        let req = if post {
            self.client.post(API_URL).form(&params)
        } else {
            self.client.get(API_URL).query(&params)
        };

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Last.fm network error: {e}"))?;

        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read Last.fm response: {e}"))?;

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Invalid JSON from Last.fm: {e}"))?;

        if let Some(err_code) = json.get("error").and_then(|c| c.as_i64()) {
            let msg = json
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return Err(format!("[{err_code}] {msg}"));
        }

        Ok(json)
    }

    pub fn auth_url(&self, callback: &str) -> String {
        let cfg_data = self.cfg.get_data();
        format!(
            "{}?api_key={}&cb={}",
            AUTH_URL,
            urlencoding::encode(&cfg_data.api_key),
            urlencoding::encode(callback)
        )
    }

    pub async fn get_session(&self, token: &str) -> Result<(String, String), String> {
        let mut p = BTreeMap::new();
        p.insert("token".to_string(), token.to_string());
        let res = self.call("auth.getSession", p, true, false).await?;
        let session = res
            .get("session")
            .ok_or_else(|| "Missing session in response".to_string())?;
        let key = session
            .get("key")
            .and_then(|k| k.as_str())
            .ok_or_else(|| "Missing session key".to_string())?
            .to_string();
        let name = session
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| "Missing session username".to_string())?
            .to_string();
        Ok((key, name))
    }

    pub async fn update_now_playing(
        &self,
        artist: &str,
        track: &str,
        album: Option<&str>,
        duration_s: Option<i64>,
    ) -> Result<(), String> {
        let mut p = BTreeMap::new();
        p.insert("artist".to_string(), artist.to_string());
        p.insert("track".to_string(), track.to_string());
        let cfg_data = self.cfg.get_data();
        p.insert("sk".to_string(), cfg_data.session_key);
        if let Some(a) = album {
            if !a.is_empty() {
                p.insert("album".to_string(), a.to_string());
            }
        }
        if let Some(d) = duration_s {
            if d > 0 {
                p.insert("duration".to_string(), d.to_string());
            }
        }

        self.call("track.updateNowPlaying", p, true, true).await?;
        Ok(())
    }

    pub async fn scrobble(&self, items: &[ScrobbleItem]) -> Result<Vec<bool>, String> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let mut p = BTreeMap::new();
        let cfg_data = self.cfg.get_data();
        p.insert("sk".to_string(), cfg_data.session_key);

        for (i, it) in items.iter().enumerate() {
            p.insert(format!("artist[{}]", i), it.artist.clone());
            p.insert(format!("track[{}]", i), it.track.clone());
            p.insert(format!("timestamp[{}]", i), it.timestamp.to_string());
            if let Some(ref alb) = it.album {
                if !alb.is_empty() {
                    p.insert(format!("album[{}]", i), alb.clone());
                }
            }
            if let Some(dur) = it.duration {
                if dur > 0 {
                    p.insert(format!("duration[{}]", i), dur.to_string());
                }
            }
        }

        let res = self.call("track.scrobble", p, true, true).await?;
        let scrobbles_val = res
            .get("scrobbles")
            .and_then(|s| s.get("scrobble"));

        let mut accepted = Vec::new();
        if let Some(sc_arr) = scrobbles_val.and_then(|s| s.as_array()) {
            for sc in sc_arr {
                let code = sc
                    .get("ignoredMessage")
                    .and_then(|im| im.get("code"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("0");
                accepted.push(code == "0");
            }
        } else if let Some(sc_obj) = scrobbles_val {
            let code = sc_obj
                .get("ignoredMessage")
                .and_then(|im| im.get("code"))
                .and_then(|c| c.as_str())
                .unwrap_or("0");
            accepted.push(code == "0");
        }

        while accepted.len() < items.len() {
            accepted.push(true);
        }

        Ok(accepted)
    }

    pub async fn love(&self, artist: &str, track: &str) -> Result<(), String> {
        let mut p = BTreeMap::new();
        p.insert("artist".to_string(), artist.to_string());
        p.insert("track".to_string(), track.to_string());
        let cfg_data = self.cfg.get_data();
        p.insert("sk".to_string(), cfg_data.session_key);
        self.call("track.love", p, true, true).await?;
        Ok(())
    }

    pub async fn similar_tracks(
        &self,
        track: &str,
        artist: &str,
        limit: i64,
    ) -> Vec<SimilarTrack> {
        let art = clean_artist(artist);
        let art_clean = if art.is_empty() { artist } else { &art };

        let key = format!("lf:simtrack:{}:{}:{}", art_clean, track, limit);
        if let Some(cached) = self.db.cache_get(&key) {
            if let Ok(tracks) = serde_json::from_value::<Vec<SimilarTrack>>(cached) {
                return tracks;
            }
        }

        let mut out = self.query_similar_tracks(track, art_clean, limit).await;
        let ct = clean_title(track);
        if out.is_empty() && !ct.is_empty() && ct != track {
            out = self.query_similar_tracks(&ct, art_clean, limit).await;
        }

        if let Ok(val) = serde_json::to_value(&out) {
            self.db.cache_set(&key, &val, DAY_SEC);
        }

        out
    }

    async fn query_similar_tracks(&self, trk: &str, art: &str, limit: i64) -> Vec<SimilarTrack> {
        let mut p = BTreeMap::new();
        p.insert("track".to_string(), trk.to_string());
        p.insert("artist".to_string(), art.to_string());
        p.insert("limit".to_string(), limit.to_string());
        p.insert("autocorrect".to_string(), "1".to_string());

        let res = match self.call("track.getSimilar", p, false, false).await {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let tracks = res
            .get("similartracks")
            .and_then(|st| st.get("track"))
            .and_then(|t| t.as_array());

        let mut out = Vec::new();
        if let Some(arr) = tracks {
            for t in arr {
                let title = t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let artist_name = t
                    .get("artist")
                    .and_then(|a| a.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let match_score = t
                    .get("match")
                    .and_then(|m| m.as_f64().or_else(|| m.as_str().and_then(|s| s.parse().ok())))
                    .unwrap_or(0.0);
                if !title.is_empty() {
                    out.push(SimilarTrack {
                        title,
                        artist: artist_name,
                        match_score,
                    });
                }
            }
        }
        out
    }

    pub async fn similar_artists(&self, artist: &str, limit: i64) -> Vec<SimilarArtist> {
        let key = format!("lf:simartist:{}:{}", artist, limit);
        if let Some(cached) = self.db.cache_get(&key) {
            if let Ok(artists) = serde_json::from_value::<Vec<SimilarArtist>>(cached) {
                return artists;
            }
        }

        let mut p = BTreeMap::new();
        p.insert("artist".to_string(), artist.to_string());
        p.insert("limit".to_string(), limit.to_string());
        p.insert("autocorrect".to_string(), "1".to_string());

        let mut out = Vec::new();
        if let Ok(res) = self.call("artist.getSimilar", p, false, false).await {
            if let Some(arr) = res
                .get("similarartists")
                .and_then(|sa| sa.get("artist"))
                .and_then(|a| a.as_array())
            {
                for a in arr {
                    let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let match_score = a
                        .get("match")
                        .and_then(|m| m.as_f64().or_else(|| m.as_str().and_then(|s| s.parse().ok())))
                        .unwrap_or(0.0);
                    if !name.is_empty() {
                        out.push(SimilarArtist {
                            artist: name,
                            match_score,
                        });
                    }
                }
            }
        }

        if let Ok(val) = serde_json::to_value(&out) {
            self.db.cache_set(&key, &val, DAY_SEC);
        }

        out
    }

    pub async fn user_top_tracks(
        &self,
        user: &str,
        period: &str,
        limit: i64,
    ) -> Vec<UserTopTrack> {
        if user.is_empty() {
            return Vec::new();
        }
        let key = format!("lf:utoptrk:{}:{}:{}", user, period, limit);
        if let Some(cached) = self.db.cache_get(&key) {
            if let Ok(tracks) = serde_json::from_value::<Vec<UserTopTrack>>(cached) {
                return tracks;
            }
        }

        let mut p = BTreeMap::new();
        p.insert("user".to_string(), user.to_string());
        p.insert("period".to_string(), period.to_string());
        p.insert("limit".to_string(), limit.to_string());

        let mut out = Vec::new();
        if let Ok(res) = self.call("user.getTopTracks", p, false, false).await {
            if let Some(arr) = res
                .get("toptracks")
                .and_then(|tt| tt.get("track"))
                .and_then(|t| t.as_array())
            {
                for t in arr {
                    let title = t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let artist_name = t
                        .get("artist")
                        .and_then(|a| a.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let playcount = t
                        .get("playcount")
                        .and_then(|pc| pc.as_i64().or_else(|| pc.as_str().and_then(|s| s.parse().ok())))
                        .unwrap_or(0);
                    if !title.is_empty() {
                        out.push(UserTopTrack {
                            title,
                            artist: artist_name,
                            playcount,
                        });
                    }
                }
            }
        }

        if let Ok(val) = serde_json::to_value(&out) {
            self.db.cache_set(&key, &val, 1800); // 30 min cache
        }

        out
    }

    pub async fn user_recent_tracks(&self, user: &str, limit: i64) -> Vec<UserRecentTrack> {
        if user.is_empty() {
            return Vec::new();
        }
        let mut p = BTreeMap::new();
        p.insert("user".to_string(), user.to_string());
        p.insert("limit".to_string(), limit.to_string());

        let res = match self.call("user.getRecentTracks", p, false, false).await {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let tracks_val = res.get("recenttracks").and_then(|rt| rt.get("track"));
        let mut out = Vec::new();

        let process_track = |t: &serde_json::Value, list: &mut Vec<UserRecentTrack>| {
            if let Some(attr) = t.get("@attr") {
                if attr.get("nowplaying").and_then(|np| np.as_str()) == Some("true") {
                    return;
                }
            }
            let ts = t
                .get("date")
                .and_then(|d| d.get("uts"))
                .and_then(|u| u.as_str().and_then(|s| s.parse::<i64>().ok()))
                .unwrap_or(0);
            if ts == 0 {
                return;
            }
            let title = t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let artist_name = t
                .get("artist")
                .and_then(|a| a.get("#text"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let album = t
                .get("album")
                .and_then(|a| a.get("#text"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();

            if !title.is_empty() {
                list.push(UserRecentTrack {
                    title,
                    artist: artist_name,
                    album,
                    timestamp: ts,
                });
            }
        };

        if let Some(arr) = tracks_val.and_then(|v| v.as_array()) {
            for t in arr {
                process_track(t, &mut out);
            }
        } else if let Some(obj) = tracks_val {
            process_track(obj, &mut out);
        }

        out
    }
}
