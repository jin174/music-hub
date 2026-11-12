use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const FALLBACK_READ_KEY: &str = "b25b959554ed76058ac220b7b2e0a026";

pub fn default_apps() -> Vec<String> {
    vec![
        "cloudmusic.exe".into(),
        "qqmusic.exe".into(),
        "sodamusic.exe".into(),
        "qishui".into(),
        "musicfree".into(),
        "kugou".into(),
        "kwmusic".into(),
        "yesplaymusic".into(),
        "listen1".into(),
        "applemusic".into(),
        "spotify".into(),
        "foobar2000".into(),
    ]
}

fn default_mode() -> String {
    "record".into()
}

fn default_port() -> u16 {
    8765
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigData {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_secret: String,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_apps")]
    pub apps: Vec<String>,
    #[serde(default)]
    pub scrobble_unmatched: bool,
}

impl Default for ConfigData {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_secret: String::new(),
            session_key: String::new(),
            username: String::new(),
            mode: default_mode(),
            port: default_port(),
            apps: default_apps(),
            scrobble_unmatched: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicConfig {
    pub api_key_set: bool,
    pub api_secret_set: bool,
    pub authorized: bool,
    pub username: String,
    pub mode: String,
    pub port: u16,
    pub apps: Vec<String>,
    pub scrobble_unmatched: bool,
}

#[derive(Clone)]
pub struct Config {
    inner: Arc<RwLock<ConfigData>>,
    path: PathBuf,
}

impl Config {
    pub fn load_or_create(custom_data_dir: Option<&Path>) -> Self {
        let base_dir = if let Some(dir) = custom_data_dir {
            dir.to_path_buf()
        } else {
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."));
            exe_dir.join("data")
        };

        let _ = fs::create_dir_all(&base_dir);
        let config_path = base_dir.join("config.json");

        let mut data = ConfigData::default();
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(loaded) = serde_json::from_str::<ConfigData>(&content) {
                    data = loaded;
                    // Auto-upgrade if apps is legacy ["cloudmusic.exe"]
                    if data.apps == vec!["cloudmusic.exe"] {
                        data.apps = default_apps();
                    }
                }
            }
        }

        let cfg = Self {
            inner: Arc::new(RwLock::new(data)),
            path: config_path,
        };
        cfg.save();
        cfg
    }

    pub fn get_data(&self) -> ConfigData {
        self.inner.read().unwrap().clone()
    }

    pub fn read_key(&self) -> String {
        let guard = self.inner.read().unwrap();
        if guard.api_key.trim().is_empty() {
            FALLBACK_READ_KEY.to_string()
        } else {
            guard.api_key.trim().to_string()
        }
    }

    pub fn has_credentials(&self) -> bool {
        let guard = self.inner.read().unwrap();
        !guard.api_key.trim().is_empty() && !guard.api_secret.trim().is_empty()
    }

    pub fn is_authorized(&self) -> bool {
        let guard = self.inner.read().unwrap();
        !guard.api_key.trim().is_empty()
            && !guard.api_secret.trim().is_empty()
            && !guard.session_key.trim().is_empty()
    }

    pub fn is_scrobbling(&self) -> bool {
        let guard = self.inner.read().unwrap();
        guard.mode == "scrobble"
            && !guard.api_key.trim().is_empty()
            && !guard.api_secret.trim().is_empty()
            && !guard.session_key.trim().is_empty()
    }

    pub fn public(&self) -> PublicConfig {
        let guard = self.inner.read().unwrap();
        PublicConfig {
            api_key_set: !guard.api_key.trim().is_empty(),
            api_secret_set: !guard.api_secret.trim().is_empty(),
            authorized: !guard.api_key.trim().is_empty()
                && !guard.api_secret.trim().is_empty()
                && !guard.session_key.trim().is_empty(),
            username: guard.username.clone(),
            mode: guard.mode.clone(),
            port: guard.port,
            apps: guard.apps.clone(),
            scrobble_unmatched: guard.scrobble_unmatched,
        }
    }

    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut ConfigData),
    {
        {
            let mut guard = self.inner.write().unwrap();
            f(&mut guard);
        }
        self.save();
    }

    pub fn save(&self) {
        let guard = self.inner.read().unwrap();
        if let Ok(json_str) = serde_json::to_string_pretty(&*guard) {
            let tmp_path = self.path.with_extension("json.tmp");
            if fs::write(&tmp_path, json_str).is_ok() {
                let _ = fs::rename(&tmp_path, &self.path);
            }
        }
    }
}
