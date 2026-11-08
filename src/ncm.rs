use crate::db::DB;
use crate::normalize::{artist_variants, clean_artist, clean_title, norm};
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DAY_SEC: i64 = 86400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NcmSong {
    pub id: i64,
    pub title: String,
    pub artists: Vec<String>,
    pub artist: String,
    pub album: String,
    pub cover: String,
    pub duration_ms: i64,
    pub status: i32,
}

#[derive(Clone)]
pub struct Ncm {
    db: DB,
    client: reqwest::Client,
}

impl Ncm {
    pub fn new(db: DB) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36"),
        );
        headers.insert(REFERER, HeaderValue::from_static("https://music.163.com/"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(8))
            .build()
            .unwrap_or_default();

        Self { db, client }
    }

    fn simplify_song(s: &serde_json::Value) -> Option<NcmSong> {
        let id = s.get("id")?.as_i64()?;
        let title = s.get("name")?.as_str()?.to_string();

        let artists_val = s.get("artists").or_else(|| s.get("ar"));
        let mut artists = Vec::new();
        if let Some(arr) = artists_val.and_then(|v| v.as_array()) {
            for a in arr {
                if let Some(name) = a.get("name").and_then(|n| n.as_str()) {
                    artists.push(name.to_string());
                }
            }
        }
        let first_artist = artists.first().cloned().unwrap_or_default();

        let album_val = s.get("album").or_else(|| s.get("al"));
        let album_name = album_val
            .and_then(|a| a.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let cover = album_val
            .and_then(|a| a.get("picUrl"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();

        let duration_ms = s
            .get("duration")
            .or_else(|| s.get("dt"))
            .and_then(|d| d.as_i64())
            .unwrap_or(0);

        let status = s.get("status").and_then(|st| st.as_i64()).unwrap_or(0) as i32;

        Some(NcmSong {
            id,
            title,
            artists,
            artist: first_artist,
            album: album_name,
            cover,
            duration_ms,
            status,
        })
    }

    pub async fn search(&self, keyword: &str, limit: i64) -> Result<Vec<NcmSong>, String> {
        let key = format!("ncm:search:{}:{}", keyword, limit);
        if let Some(cached) = self.db.cache_get(&key) {
            if let Ok(songs) = serde_json::from_value::<Vec<NcmSong>>(cached) {
                return Ok(songs);
            }
        }

        let url = format!(
            "https://music.163.com/api/search/get/web?s={}&type=1&limit={}&offset=0",
            urlencoding::encode(keyword),
            limit
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid JSON response: {e}"))?;

        let songs_val = json
            .get("result")
            .and_then(|r| r.get("songs"))
            .and_then(|s| s.as_array());

        let mut songs = Vec::new();
        if let Some(arr) = songs_val {
            for item in arr {
                if let Some(song) = Self::simplify_song(item) {
                    songs.push(song);
                }
            }
        }

        if let Ok(val) = serde_json::to_value(&songs) {
            self.db.cache_set(&key, &val, 3 * DAY_SEC);
        }

        Ok(songs)
    }

    pub async fn detail(&self, song_id: i64) -> Result<Option<NcmSong>, String> {
        let key = format!("ncm:detail:{}", song_id);
        if let Some(cached) = self.db.cache_get(&key) {
            if let Ok(song) = serde_json::from_value::<NcmSong>(cached) {
                return Ok(Some(song));
            }
        }

        let url = format!(
            "https://music.163.com/api/song/detail/?id={}&ids=[{}]",
            song_id, song_id
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid JSON response: {e}"))?;

        let songs_val = json.get("songs").and_then(|s| s.as_array());
        if let Some(first) = songs_val.and_then(|arr| arr.first()) {
            if let Some(song) = Self::simplify_song(first) {
                if let Ok(val) = serde_json::to_value(&song) {
                    self.db.cache_set(&key, &val, 7 * DAY_SEC);
                }
                return Ok(Some(song));
            }
        }

        Ok(None)
    }

    pub async fn simi(&self, song_id: i64, limit: i64) -> Result<Vec<NcmSong>, String> {
        let key = format!("ncm:simi:{}", song_id);
        if let Some(cached) = self.db.cache_get(&key) {
            if let Ok(songs) = serde_json::from_value::<Vec<NcmSong>>(cached) {
                return Ok(songs);
            }
        }

        let url = format!(
            "https://music.163.com/api/v1/discovery/simiSong?songid={}&limit={}&offset=0",
            song_id, limit
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid JSON response: {e}"))?;

        let songs_val = json.get("songs").and_then(|s| s.as_array());
        let mut songs = Vec::new();
        if let Some(arr) = songs_val {
            for item in arr {
                if let Some(song) = Self::simplify_song(item) {
                    songs.push(song);
                }
            }
        }

        if let Ok(val) = serde_json::to_value(&songs) {
            self.db.cache_set(&key, &val, DAY_SEC);
        }

        Ok(songs)
    }

    pub fn artist_ok(&self, candidates: &[String], target: &str) -> bool {
        if target.trim().is_empty() {
            return true;
        }
        let targets: Vec<String> = artist_variants(target)
            .into_iter()
            .map(|v| norm(&v))
            .filter(|n| !n.is_empty())
            .collect();
        if targets.is_empty() {
            return true;
        }

        for c in candidates {
            for cv in artist_variants(c) {
                let cn = norm(&cv);
                if !cn.is_empty() && targets.iter().any(|t| cn.contains(t) || t.contains(&cn)) {
                    return true;
                }
            }
        }
        false
    }

    pub async fn match_song(&self, title: &str, artist: &str) -> Option<NcmSong> {
        let tk = norm(title);
        let first_art = clean_artist(artist);
        let ak = norm(&first_art);
        let cache_key = format!("ncm:match:{}:{}", tk, ak);

        if let Some(cached) = self.db.cache_get(&cache_key) {
            if let Ok(song) = serde_json::from_value::<NcmSong>(cached) {
                return Some(song);
            } else {
                return None;
            }
        }

        let query = format!("{} {}", first_art, title).trim().to_string();
        let results = match self.search(&query, 20).await {
            Ok(r) => r,
            Err(_) => return None, // do not cache network errors
        };

        let t = norm(title);
        let tc = norm(&clean_title(title));
        let mut best: Option<NcmSong> = None;
        let mut best_score: f64 = -1.0;

        for (i, s) in results.into_iter().enumerate() {
            let st = norm(&s.title);
            let stc = norm(&clean_title(&s.title));
            if st.is_empty() || t.is_empty() {
                continue;
            }

            let title_score = if st == t {
                4.0 // exact version match
            } else if !tc.is_empty() && stc == tc {
                3.0 // base title match
            } else if st.starts_with(&t) || t.starts_with(&st) {
                2.0
            } else if t.contains(&st) || st.contains(&t) {
                1.0
            } else {
                continue;
            };

            if !self.artist_ok(&s.artists, artist) {
                continue;
            }

            let score = title_score * 10.0 + (if s.status >= 0 { 2.0 } else { 0.0 })
                - (i as f64) * 0.1;
            if score > best_score {
                best_score = score;
                best = Some(s);
            }
        }

        if let Some(ref mut b) = best {
            if b.cover.is_empty() && b.id > 0 {
                if let Ok(Some(det)) = self.detail(b.id).await {
                    if !det.cover.is_empty() {
                        b.cover = det.cover;
                    }
                }
            }
        }

        if let Some(ref b) = best {
            if let Ok(val) = serde_json::to_value(b) {
                self.db.cache_set(&cache_key, &val, 7 * DAY_SEC);
            }
        } else {
            self.db.cache_set(&cache_key, &serde_json::Value::Null, DAY_SEC);
        }

        best
    }
}
