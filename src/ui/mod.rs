pub mod input;

use crate::app::{
    App as HubApp, AppStateResponse, ConsoleCommand, RadarResponse, RadarTrack, RoamResponse,
    RoamTrack, StatsResponse,
};
use crate::db::{FeedbackRow, PlayRow};
use crate::poster::{render_wrapped_poster, WrappedPoster};
use crate::tray::{is_autostart_enabled, set_autostart};
use base64::Engine;
use chrono::{Local, TimeZone};
use gpui::{
    div, img, prelude::*, px, relative, rgb, rgba, size, App, Application, Bounds, ClipboardItem,
    Context, Entity, FocusHandle, Image, ImageFormat, MouseButton, ObjectFit, RenderImage,
    SharedString, Subscription, Window, WindowBounds, WindowOptions,
};
use input::{bind_input_keys, InputEvent, TextInput};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const APP_PRESETS: &[(&str, &str)] = &[
    ("cloudmusic.exe", "网易云音乐 (cloudmusic)"),
    ("qqmusic.exe", "QQ 音乐 (qqmusic)"),
    ("sodamusic.exe", "汽水音乐 (sodamusic)"),
    ("musicfree", "MusicFree"),
    ("yesplaymusic", "YesPlayMusic"),
    ("listen1", "Listen 1"),
    ("kugou", "酷狗音乐 (kugou)"),
    ("kwmusic", "酷我音乐 (kwmusic)"),
    ("applemusic", "Apple Music"),
    ("spotify", "Spotify"),
    ("foobar2000", "Foobar2000"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    Radar,
    Roam,
    Stats,
    Recent,
    Settings,
}

impl NavTab {
    pub fn from_str(s: &str) -> Self {
        match s {
            "roam" => Self::Roam,
            "stats" => Self::Stats,
            "recent" => Self::Recent,
            "settings" => Self::Settings,
            _ => Self::Radar,
        }
    }
}

pub fn format_app_name(app_id: &str) -> &'static str {
    let s = app_id.to_lowercase();
    if s.contains("cloudmusic") {
        "网易云音乐"
    } else if s.contains("qqmusic") {
        "QQ 音乐"
    } else if s.contains("sodamusic") || s.contains("qishui") {
        "汽水音乐"
    } else if s.contains("musicfree") {
        "MusicFree"
    } else if s.contains("yesplaymusic") {
        "YesPlayMusic"
    } else if s.contains("listen1") {
        "Listen 1"
    } else if s.contains("kugou") {
        "酷狗音乐"
    } else if s.contains("kwmusic") {
        "酷我音乐"
    } else if s.contains("applemusic") {
        "Apple Music"
    } else if s.contains("spotify") {
        "Spotify"
    } else if s.contains("foobar") {
        "Foobar2000"
    } else if s.contains("last.fm") {
        "Last.fm 导入"
    } else {
        "音乐播放器"
    }
}

fn fmt_mm_ss(ms: i64) -> String {
    if ms <= 0 {
        return "00:00".to_string();
    }
    let total_sec = ms / 1000;
    let m = total_sec / 60;
    let s = total_sec % 60;
    format!("{:02}:{:02}", m, s)
}

fn decode_bytes_to_render_image(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let mut rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(SmallVec::from_elem(
        image::Frame::new(rgba),
        1,
    ))))
}

// Cover downloads sit on the 500 ms console poll path, so they must never stall
// the UI for long: bound connect/total time and refuse oversized payloads instead
// of buffering whatever the CDN returns.
const COVER_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const COVER_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const COVER_MAX_BYTES: u64 = 8 * 1024 * 1024;

fn cover_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36",
            ),
        );
        headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_static("https://music.163.com/"),
        );
        reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(COVER_CONNECT_TIMEOUT)
            .timeout(COVER_FETCH_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

async fn fetch_cover_render_image(cover_str: &str) -> Option<Arc<RenderImage>> {
    if cover_str.is_empty() {
        return None;
    }
    if let Some(idx) = cover_str.find("base64,") {
        let b64 = &cover_str[idx + 7..];
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .ok()?;
        return decode_bytes_to_render_image(&bytes);
    }
    if cover_str.starts_with("http://") || cover_str.starts_with("https://") {
        let url = if cover_str.contains("music.126.net") && !cover_str.contains("?param=") {
            format!("{}?param=140y140", cover_str)
        } else {
            cover_str.to_string()
        };
        let resp = cover_http_client().get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        if resp.content_length().is_some_and(|len| len > COVER_MAX_BYTES) {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        if bytes.len() as u64 > COVER_MAX_BYTES {
            return None;
        }
        return decode_bytes_to_render_image(&bytes);
    }
    None
}

pub struct ConsoleView {
    hub: Arc<HubApp>,
    focus_handle: FocusHandle,
    active_tab: NavTab,

    // Live SMTC state & cover
    state: AppStateResponse,
    now_cover_key: String,
    now_cover_img: Option<Arc<RenderImage>>,

    // Radar state
    track_input: Entity<TextInput>,
    artist_input: Entity<TextInput>,
    radar_loading: bool,
    radar_result: Option<RadarResponse>,
    radar_query_label: String,

    // Roam state
    roam_hide_heard: bool,
    roam_loading: bool,
    roam_result: Option<RoamResponse>,

    // Stats & Wrapped Poster state
    stats_days: i64,
    stats_data: StatsResponse,
    wrapped_poster: Option<WrappedPoster>,

    // Recent plays state
    recent_rows: Vec<PlayRow>,

    // Settings state
    username_input: Entity<TextInput>,
    api_key_input: Entity<TextInput>,
    api_secret_input: Entity<TextInput>,
    scrobble_unmatched: bool,
    selected_apps: HashSet<String>,
    autostart_enabled: bool,
    importing_lastfm: bool,
    feedback_list: Vec<FeedbackRow>,

    // Toast notification
    toast: Option<(String, Instant)>,

    _subscriptions: Vec<Subscription>,
}

impl ConsoleView {
    pub fn new(
        hub: Arc<HubApp>,
        initial_tab: NavTab,
        initial_toast: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let cfg_data = hub.cfg.get_data();
        let pub_cfg = hub.cfg.public();
        let now = hub.monitor.snapshot();
        let pending = hub.db.count_pending_scrobbles();
        let session_app = hub.monitor.session_app.read().unwrap().clone();

        let state = AppStateResponse {
            now,
            config: pub_cfg.clone(),
            scrobbling: hub.cfg.is_scrobbling(),
            pending,
            last_error: None,
            last_ok: None,
            session_app,
        };

        let track_input = TextInput::new(
            cx,
            "",
            "输入歌名，或直接粘贴网易云链接（自动解析 ID）",
            false,
        );
        let artist_input = TextInput::new(
            cx,
            "",
            "例如：中島みゆき / Tommee Profitt / 周杰伦",
            false,
        );

        let username_input = TextInput::new(cx, cfg_data.username.clone(), "输入 Last.fm 用户名", false);
        let api_key_input = TextInput::new(cx, cfg_data.api_key.clone(), "粘贴 32 位 API Key", false);
        let api_secret_input = TextInput::new(
            cx,
            cfg_data.api_secret.clone(),
            "粘贴 32 位 Shared Secret",
            true,
        );

        let mut selected_apps: HashSet<String> = cfg_data
            .apps
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        if selected_apps.is_empty() {
            for (id, _) in APP_PRESETS {
                selected_apps.insert((*id).to_string());
            }
        }

        let stats_data = hub.stats(7);
        let recent_rows = hub.db.recent(150);
        let feedback_list = hub.db.get_feedback_list(200);

        let sub1 = cx.subscribe(&track_input, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::EnterPressed) {
                this.run_radar(cx);
            }
        });
        let sub2 = cx.subscribe(&artist_input, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::EnterPressed) {
                this.run_radar(cx);
            }
        });

        let toast = initial_toast.map(|msg| (msg, Instant::now()));
        if let Some((_, now)) = toast {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(3500))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    if let Some((_, ts)) = &view.toast {
                        if *ts == now {
                            view.toast = None;
                            cx.notify();
                        }
                    }
                });
            })
            .detach();
        }

        Self {
            hub,
            focus_handle: cx.focus_handle(),
            active_tab: initial_tab,
            state,
            now_cover_key: String::new(),
            now_cover_img: None,
            track_input,
            artist_input,
            radar_loading: false,
            radar_result: None,
            radar_query_label: String::new(),
            roam_hide_heard: true,
            roam_loading: false,
            roam_result: None,
            stats_days: 7,
            stats_data,
            wrapped_poster: None,
            recent_rows,
            username_input,
            api_key_input,
            api_secret_input,
            scrobble_unmatched: cfg_data.scrobble_unmatched,
            selected_apps,
            autostart_enabled: is_autostart_enabled(),
            importing_lastfm: false,
            feedback_list,
            toast,
            _subscriptions: vec![sub1, sub2],
        }
    }

    pub fn show_toast(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.toast = Some((msg.into(), now));
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(3500))
                .await;
            let _ = this.update(cx, |view, cx| {
                if let Some((_, ts)) = &view.toast {
                    if *ts == now {
                        view.toast = None;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub fn switch_tab(&mut self, tab: NavTab, cx: &mut Context<Self>) {
        self.active_tab = tab;
        match tab {
            NavTab::Stats => {
                self.stats_data = self.hub.stats(self.stats_days);
            }
            NavTab::Recent => {
                self.recent_rows = self.hub.db.recent(150);
            }
            NavTab::Settings => {
                self.feedback_list = self.hub.db.get_feedback_list(200);
                self.autostart_enabled = is_autostart_enabled();
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn run_radar(&mut self, cx: &mut Context<Self>) {
        if self.radar_loading {
            return;
        }
        let raw_track = self.track_input.read(cx).text().trim().to_string();
        let raw_artist = self.artist_input.read(cx).text().trim().to_string();
        if raw_track.is_empty() {
            self.show_toast("请输入歌曲名或粘贴网易云链接", cx);
            return;
        }

        self.radar_loading = true;
        cx.notify();

        let hub = self.hub.clone();
        let track_input = self.track_input.clone();
        let artist_input = self.artist_input.clone();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let rt = hub.rt.clone();
        rt.spawn(async move {
            let mut trk = raw_track;
            let mut art = raw_artist;

            // Parse NetEase song ID if user pasted a share link
            if let Ok(re) = regex::Regex::new(r"id=(\d+)") {
                if let Some(caps) = re.captures(&trk) {
                    if let Some(id_match) = caps.get(1) {
                        if let Ok(song_id) = id_match.as_str().parse::<i64>() {
                            if let Ok(Some(detail)) = hub.ncm.detail(song_id).await {
                                if !detail.title.is_empty() {
                                    trk = detail.title;
                                    if !detail.artist.is_empty() {
                                        art = detail.artist;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let res = hub.radar(&trk, &art).await;
            let _ = tx.send((trk, art, res));
        });

        cx.spawn(async move |this, cx| {
            if let Ok((resolved_trk, resolved_art, res)) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    track_input.update(cx, |inp, cx| inp.set_text(resolved_trk.clone(), cx));
                    artist_input.update(cx, |inp, cx| inp.set_text(resolved_art.clone(), cx));
                    this.radar_loading = false;
                    this.radar_query_label = if resolved_art.is_empty() {
                        resolved_trk
                    } else {
                        format!("{} · {}", resolved_art, resolved_trk)
                    };
                    this.radar_result = Some(res);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn quick_radar(&mut self, track: &str, artist: &str, cx: &mut Context<Self>) {
        let trk = track.to_string();
        let art = artist.to_string();
        self.track_input.update(cx, |inp, cx| inp.set_text(trk, cx));
        self.artist_input.update(cx, |inp, cx| inp.set_text(art, cx));
        self.active_tab = NavTab::Radar;
        self.run_radar(cx);
    }

    pub fn run_roam(&mut self, shuffle: bool, cx: &mut Context<Self>) {
        if self.roam_loading {
            return;
        }
        self.roam_loading = true;
        cx.notify();

        let hub = self.hub.clone();
        let hide_heard = self.roam_hide_heard;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let rt = hub.rt.clone();
        rt.spawn(async move {
            let res = hub.roam(None, hide_heard, shuffle).await;
            let _ = tx.send(res);
        });

        cx.spawn(async move |this, cx| {
            if let Ok(res) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    this.roam_loading = false;
                    let cnt = res.tracks.len();
                    this.roam_result = Some(res);
                    this.show_toast(format!("✨ 已为你合成 {} 首专属漫游推荐", cnt), cx);
                });
            }
        })
        .detach();
    }

    fn toggle_feedback_item(
        &mut self,
        title: String,
        artist: String,
        target_action: &str,
        cx: &mut Context<Self>,
    ) {
        let current_fb = self
            .radar_result
            .as_ref()
            .and_then(|r| {
                r.tracks
                    .iter()
                    .find(|t| t.title == title && t.artist == artist)
                    .and_then(|t| t.feedback.clone())
            })
            .or_else(|| {
                self.roam_result.as_ref().and_then(|r| {
                    r.tracks
                        .iter()
                        .find(|t| t.title == title && t.artist == artist)
                        .and_then(|t| t.feedback.clone())
                })
            });

        let final_action = if current_fb.as_deref() == Some(target_action) {
            "remove"
        } else {
            target_action
        };

        self.hub.db.set_feedback(&title, &artist, final_action);
        let new_fb_opt = if final_action == "remove" {
            None
        } else {
            Some(final_action.to_string())
        };

        if let Some(ref mut radar) = self.radar_result {
            for t in &mut radar.tracks {
                if t.title == title && t.artist == artist {
                    t.feedback = new_fb_opt.clone();
                }
            }
        }
        if let Some(ref mut roam) = self.roam_result {
            for t in &mut roam.tracks {
                if t.title == title && t.artist == artist {
                    t.feedback = new_fb_opt.clone();
                }
            }
        }

        let toast_msg = match final_action {
            "like" => format!("👍 已标记喜欢：{} - {}", artist, title),
            "dislike" => format!("🚫 已加入避雷黑名单：{} - {}", artist, title),
            _ => format!("已取消偏好标记：{} - {}", artist, title),
        };
        self.feedback_list = self.hub.db.get_feedback_list(200);
        self.show_toast(toast_msg, cx);
    }

    fn save_file_with_dialog(
        &mut self,
        suggested_name: String,
        bytes: Vec<u8>,
        success_label: String,
        cx: &mut Context<Self>,
    ) {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let prompt_rx = cx.prompt_for_new_path(&dir, Some(&suggested_name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = prompt_rx.await {
                let write_res = std::fs::write(&path, &bytes);
                let _ = this.update(cx, |this, cx| match write_res {
                    Ok(_) => {
                        this.show_toast(
                            format!("✅ {} 已保存至：{}", success_label, path.display()),
                            cx,
                        );
                    }
                    Err(e) => {
                        this.show_toast(format!("❌ 保存失败：{}", e), cx);
                    }
                });
            }
        })
        .detach();
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x0d1117))
            .text_color(rgb(0xe6edf3))
            .text_size(px(13.))
            .track_focus(&self.focus_handle)
            .relative()
            // Top main area: Left Sidebar + Right Active View
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .bg(rgb(0x0d1117))
                            .child(match self.active_tab {
                                NavTab::Radar => self.render_radar_view(cx).into_any_element(),
                                NavTab::Roam => self.render_roam_view(cx).into_any_element(),
                                NavTab::Stats => self.render_stats_view(cx).into_any_element(),
                                NavTab::Recent => self.render_recent_view(cx).into_any_element(),
                                NavTab::Settings => self.render_settings_view(cx).into_any_element(),
                            }),
                    ),
            )
            // Bottom Persistent Spotify-style Now Playing Bar
            .child(self.render_now_playing_bar(cx))
            // Wrapped Poster Modal Overlay (if active)
            .children(self.wrapped_poster.as_ref().map(|poster| {
                self.render_poster_modal(poster.render_image.clone(), poster.png_bytes.clone(), cx)
            }))
            // Floating Toast Notification
            .children(self.toast.as_ref().map(|(msg, _)| {
                div()
                    .absolute()
                    .bottom(px(88.))
                    .right(px(24.))
                    .px_4()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(0x1f2630))
                    .border_1()
                    .border_color(rgb(0xe84142))
                    .shadow_lg()
                    .text_size(px(13.))
                    .text_color(rgb(0xffffff))
                    .child(msg.clone())
            }))
    }
}

impl ConsoleView {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let nav_items = [
            (NavTab::Radar, "🔍  单曲雷达", "nav-radar"),
            (NavTab::Roam, "🌌  全盘漫游", "nav-roam"),
            (NavTab::Stats, "📊  统计画像", "nav-stats"),
            (NavTab::Recent, "🕒  播放流水账", "nav-recent"),
            (NavTab::Settings, "⚙️  账号与设置", "nav-settings"),
        ];

        let is_scrobble = self.state.scrobbling;
        let active_app = self
            .state
            .now
            .as_ref()
            .map(|n| n.app.as_str())
            .or(self.state.session_app.as_deref())
            .unwrap_or("");
        let smtc_status = if !active_app.is_empty() {
            format!("监听: {}", format_app_name(active_app))
        } else {
            "SMTC 实时监听中".to_string()
        };

        div()
            .w(px(216.))
            .h_full()
            .flex()
            .flex_col()
            .justify_between()
            .bg(rgb(0x12161f))
            .border_r_1()
            .border_color(rgb(0x262c36))
            .p_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    // Brand Header
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .px_2()
                            .py_2()
                            .mb_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(17.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("🎵 私人音乐中枢"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .size(px(7.))
                                            .rounded_full()
                                            .bg(if self.state.now.is_some() {
                                                rgb(0x3fb950)
                                            } else {
                                                rgb(0x8b949e)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child(smtc_status),
                                    ),
                            ),
                    )
                    // Navigation Links
                    .children(nav_items.into_iter().map(|(tab, label, id)| {
                        let active = self.active_tab == tab;
                        div()
                            .id(id)
                            .w_full()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if active {
                                rgba(0xe8414228)
                            } else {
                                rgba(0x00000000)
                            })
                            .border_1()
                            .border_color(if active {
                                rgb(0xe84142)
                            } else {
                                rgba(0x00000000)
                            })
                            .text_color(if active {
                                rgb(0xff7b7c)
                            } else {
                                rgb(0xc9d1d9)
                            })
                            .font_weight(if active {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::MEDIUM
                            })
                            .hover(|s| {
                                if active {
                                    s
                                } else {
                                    s.bg(rgb(0x1c222d)).text_color(rgb(0xffffff))
                                }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.switch_tab(tab, cx);
                            }))
                            .child(label)
                    })),
            )
            // Bottom Controls: Mode Switcher + Pop-out Mini Island
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgb(0x262c36))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x8b949e))
                            .px_1()
                            .child(if self.state.pending > 0 {
                                format!("工作模式 (待打卡 {} 首)", self.state.pending)
                            } else {
                                "工作模式".to_string()
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .p_1()
                            .rounded_md()
                            .bg(rgb(0x0d1117))
                            .border_1()
                            .border_color(rgb(0x262c36))
                            .child(
                                div()
                                    .id("btn-mode-record")
                                    .flex_1()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .flex()
                                    .justify_center()
                                    .text_size(px(11.))
                                    .bg(if !is_scrobble {
                                        rgb(0xe84142)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .text_color(if !is_scrobble {
                                        rgb(0xffffff)
                                    } else {
                                        rgb(0x8b949e)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.hub.cfg.update(|d| d.mode = "record".to_string());
                                        this.state.scrobbling = false;
                                        this.show_toast("已切换为：只记本地库（不向 Last.fm 打卡）", cx);
                                    }))
                                    .child("只记本地"),
                            )
                            .child(
                                div()
                                    .id("btn-mode-scrobble")
                                    .flex_1()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .flex()
                                    .justify_center()
                                    .text_size(px(11.))
                                    .bg(if is_scrobble {
                                        rgb(0xe84142)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .text_color(if is_scrobble {
                                        rgb(0xffffff)
                                    } else {
                                        rgb(0x8b949e)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.hub.cfg.is_authorized() {
                                            this.active_tab = NavTab::Settings;
                                            this.show_toast("请先完成 Last.fm 授权后再开启自动打卡", cx);
                                            return;
                                        }
                                        this.hub.cfg.update(|d| d.mode = "scrobble".to_string());
                                        this.hub.scrobbler.kick();
                                        this.state.scrobbling = true;
                                        this.show_toast("已开启：自动接管 Last.fm 打卡", cx);
                                    }))
                                    .child("Last.fm 打卡"),
                            ),
                    )
                    .child(
                        div()
                            .id("btn-open-mini")
                            .w_full()
                            .py_1p5()
                            .px_3()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(0x1c222d))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .hover(|s| s.bg(rgb(0x262e3d)).border_color(rgb(0x484f58)))
                            .flex()
                            .justify_center()
                            .items_center()
                            .text_size(px(12.))
                            .text_color(rgb(0xe6edf3))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hub.open_mini_window_sync();
                                this.show_toast("🪟 已在桌面唤醒灵动岛迷你挂件", cx);
                            }))
                            .child("🪟 弹出灵动岛挂件"),
                    ),
            )
    }

    fn render_now_playing_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let now_opt = self.state.now.as_ref();
        let has_song = now_opt.is_some();

        let (title, artist_album, pos_ms, dur_ms, ratio, thr_pct, badge_text, badge_color) =
            if let Some(n) = now_opt {
                let sub = if n.album.is_empty() {
                    format!("{} · {}", n.artist, format_app_name(&n.app))
                } else {
                    format!("{} · 《{}》 · {}", n.artist, n.album, format_app_name(&n.app))
                };
                let dur = n.duration_ms;
                let pos = n.played_ms.min(dur.max(n.played_ms));
                let r = if dur > 0 {
                    (pos as f32 / dur as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let thr_ms = n.threshold_ms.unwrap_or_else(|| {
                    if dur > 0 {
                        (dur / 2).min(240_000)
                    } else {
                        120_000
                    }
                });
                let thr = if dur > 0 {
                    (thr_ms as f32 / dur as f32).clamp(0.1, 0.95)
                } else {
                    0.5
                };
                let (b_txt, b_col) = if n.scrobble_state == "done" {
                    ("☁️ 已打卡".to_string(), rgb(0x3fb950))
                } else if n.threshold_reached {
                    ("🎯 已达打卡线".to_string(), rgb(0x58a6ff))
                } else if n.ncm_id.is_some() {
                    ("🎵 曲库已对齐".to_string(), rgb(0x8b949e))
                } else {
                    ("🎙 非正式曲库".to_string(), rgb(0xd29922))
                };
                (
                    n.title.clone(),
                    sub,
                    pos,
                    dur,
                    r,
                    thr,
                    Some(b_txt),
                    b_col,
                )
            } else {
                (
                    "暂未检测到正在播放的曲目".to_string(),
                    "打开网易云 / QQ音乐 / 汽水音乐 / MusicFree / Spotify 放一首歌，这里将实时同步".to_string(),
                    0,
                    0,
                    0.0,
                    0.5,
                    None,
                    rgb(0x8b949e),
                )
            };

        let cover_element = if let Some(ref render_img) = self.now_cover_img {
            img(render_img.clone())
                .size(px(50.))
                .rounded_md()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        } else {
            div()
                .size(px(50.))
                .rounded_md()
                .bg(rgb(0x1f2630))
                .border_1()
                .border_color(rgb(0x30363d))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(20.))
                .child("🎵")
                .into_any_element()
        };

        div()
            .w_full()
            .h(px(74.))
            .px_4()
            .bg(rgb(0x141923))
            .border_t_1()
            .border_color(rgb(0x2b323c))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            // Left: Cover + Track Info
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .w(px(360.))
                    .overflow_hidden()
                    .child(cover_element)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .truncate()
                                            .child(title),
                                    )
                                    .children(badge_text.map(|bt| {
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x1f2630))
                                            .text_size(px(10.))
                                            .text_color(badge_color)
                                            .child(bt)
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x8b949e))
                                    .truncate()
                                    .child(artist_album),
                            ),
                    ),
            )
            // Center: Progress bar + threshold line
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x8b949e))
                            .w(px(38.))
                            .child(fmt_mm_ss(pos_ms)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.))
                            .rounded_full()
                            .bg(rgb(0x262e3d))
                            .relative()
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(ratio))
                                    .rounded_full()
                                    .bg(rgb(0xe84142)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(relative(thr_pct))
                                    .w(px(2.))
                                    .bg(rgba(0xffffff88)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x8b949e))
                            .w(px(38.))
                            .child(fmt_mm_ss(dur_ms)),
                    ),
            )
            // Right: Action buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("now-btn-radar")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if has_song {
                                rgb(0xe84142)
                            } else {
                                rgb(0x21262d)
                            })
                            .text_color(if has_song {
                                rgb(0xffffff)
                            } else {
                                rgb(0x6e7681)
                            })
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(n) = this.state.now.clone() {
                                    this.quick_radar(&n.title, &n.artist, cx);
                                }
                            }))
                            .child("🎯 雷达这首"),
                    )
                    .child(
                        div()
                            .id("now-btn-love")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(0x21262d))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .hover(|s| s.bg(rgb(0x30363d)))
                            .text_color(if has_song {
                                rgb(0xe6edf3)
                            } else {
                                rgb(0x6e7681)
                            })
                            .text_size(px(12.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let Some(n) = this.state.now.clone() else {
                                    return;
                                };
                                this.hub.db.set_feedback(&n.title, &n.artist, "like");
                                if this.hub.cfg.is_authorized() {
                                    let lf = this.hub.lf.clone();
                                    let art = n.artist.clone();
                                    let trk = n.title.clone();
                                    this.hub.rt.spawn(async move {
                                        let _ = lf.love(&art, &trk).await;
                                    });
                                    this.show_toast(
                                        format!("❤️ 已同步红心至 Last.fm 与本地喜欢：{}", n.title),
                                        cx,
                                    );
                                } else {
                                    this.show_toast(
                                        format!("❤️ 已加入本地真爱喜欢：{}", n.title),
                                        cx,
                                    );
                                }
                            }))
                            .child("❤️ 红心"),
                    ),
            )
    }
}

impl ConsoleView {
    fn render_radar_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let quick_presets: &[(&str, &str, &str)] = &[
            ("永遠の嘘 · 中島みゆき", "永遠の嘘をついてくれ", "中島みゆき"),
            ("草莓白皮书 · バンバン", "「いちご白書」をもう一度", "バンバン"),
            ("500 Miles · PPM", "500 Miles", "Peter, Paul and Mary"),
            ("Anyone · Tommee Profitt", "Anyone", "Tommee Profitt"),
            ("晴天 · 周杰伦", "晴天", "周杰伦"),
        ];

        div()
            .id("radar-scroll")
            .size_full()
            .overflow_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            // Top Search Toolbox Card
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("🔍 单曲双源雷达"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x8b949e))
                                    .child("融合 Last.fm 全球协同过滤 + 网易云相似推荐 + 本地完播/切歌加权"),
                            ),
                    )
                    // Input row
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_end()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child("歌曲名 或 网易云分享链接"),
                                    )
                                    .child(self.track_input.clone()),
                            )
                            .child(
                                div()
                                    .w(px(240.))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child("歌手 / 乐队（选填）"),
                                    )
                                    .child(self.artist_input.clone()),
                            )
                            .child(
                                div()
                                    .id("btn-run-radar")
                                    .h(px(34.))
                                    .px_4()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0xe84142))
                                    .hover(|s| s.bg(rgb(0xff5556)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(0xffffff))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.run_radar(cx);
                                    }))
                                    .child(if self.radar_loading {
                                        "⏳ 计算中..."
                                    } else {
                                        "🚀 开始探索"
                                    }),
                            ),
                    )
                    // Quick test chips
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x8b949e))
                                    .child("快捷测试："),
                            )
                            .children(quick_presets.iter().enumerate().map(
                                |(idx, (label, trk, art))| {
                                    let trk_s = (*trk).to_string();
                                    let art_s = (*art).to_string();
                                    div()
                                        .id(("chip-radar", idx))
                                        .px_2p5()
                                        .py_0p5()
                                        .rounded_full()
                                        .cursor_pointer()
                                        .bg(rgb(0x21262d))
                                        .border_1()
                                        .border_color(rgb(0x30363d))
                                        .hover(|s| {
                                            s.border_color(rgb(0xe84142)).text_color(rgb(0xff7b7c))
                                        })
                                        .text_size(px(11.))
                                        .text_color(rgb(0xc9d1d9))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.quick_radar(&trk_s, &art_s, cx);
                                        }))
                                        .child(*label)
                                },
                            )),
                    ),
            )
            // Results Area
            .children(self.radar_result.as_ref().map(|res| {
                let tracks = &res.tracks;
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Results Header
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .pb_2()
                            .border_b_1()
                            .border_color(rgb(0x262c36))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child(format!(
                                                "基于「{}」发现 {} 首同频曲目",
                                                self.radar_query_label,
                                                tracks.len()
                                            )),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x21262d))
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child(format!(
                                                "Last.fm {} 首 + 网易云 {} 首",
                                                res.sources.lastfm, res.sources.ncm
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn-copy-radar")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x21262d))
                                    .border_1()
                                    .border_color(rgb(0x30363d))
                                    .hover(|s| s.bg(rgb(0x30363d)))
                                    .text_size(px(12.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(ref r) = this.radar_result {
                                            let txt = r
                                                .tracks
                                                .iter()
                                                .enumerate()
                                                .map(|(i, t)| {
                                                    format!("{}. {} - {}", i + 1, t.artist, t.title)
                                                })
                                                .collect::<Vec<_>>()
                                                .join("\n");
                                            cx.write_to_clipboard(ClipboardItem::new_string(txt));
                                            let len = r.tracks.len();
                                            this.show_toast(
                                                format!("📋 已复制 {} 首雷达歌单到剪贴板", len),
                                                cx,
                                            );
                                        }
                                    }))
                                    .child("📋 复制歌单文本"),
                            ),
                    )
                    // Similar Artists chips (if any)
                    .children((!res.artists.is_empty()).then(|| {
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_1p5()
                            .pb_2()
                            .border_b_1()
                            .border_color(rgb(0x21262d))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x8b949e))
                                    .child("🎤 同频音乐人（点击直接播代表作）："),
                            )
                            .children(res.artists.iter().take(8).enumerate().map(|(i, a)| {
                                let art_name = a.artist.clone();
                                div()
                                    .id(("sim-art", i))
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .bg(rgb(0x1c222d))
                                    .border_1()
                                    .border_color(rgb(0x30363d))
                                    .hover(|s| s.border_color(rgb(0x58a6ff)).text_color(rgb(0x58a6ff)))
                                    .text_size(px(11.))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let hub = this.hub.clone();
                                        let rt = hub.rt.clone();
                                        let art = art_name.clone();
                                        rt.spawn(async move {
                                            let _ = hub.play_artist(&art).await;
                                        });
                                        this.show_toast(
                                            format!("▶ 正在网易云为你播放 {} 的代表作", art_name),
                                            cx,
                                        );
                                    }))
                                    .child(format!(
                                        "{} ({:.0}%)",
                                        a.artist,
                                        (a.match_score * 100.0).min(100.0)
                                    ))
                            }))
                    }))
                    // High-density Track Rows
                    .children(if tracks.is_empty() {
                        vec![div()
                            .py_6()
                            .flex()
                            .justify_center()
                            .text_color(rgb(0x8b949e))
                            .child("未找到相似曲目，建议补充歌手名或换一首代表作试试")
                            .into_any_element()]
                    } else {
                        tracks
                            .iter()
                            .enumerate()
                            .map(|(idx, trk)| {
                                self.render_radar_track_row(idx, trk, cx).into_any_element()
                            })
                            .collect()
                    })
            }))
    }

    fn render_radar_track_row(
        &self,
        idx: usize,
        it: &RadarTrack,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_like = it.feedback.as_deref() == Some("like");
        let is_dislike = it.feedback.as_deref() == Some("dislike");
        let has_both = it.sources.len() >= 2;
        let ncm_id = it.ncm.as_ref().map(|n| n.id);
        let title_c = it.title.clone();
        let artist_c = it.artist.clone();

        div()
            .w_full()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(if is_dislike {
                rgb(0x11151c)
            } else {
                rgb(0x121720)
            })
            .border_1()
            .border_color(if is_like {
                rgba(0x3fb95066)
            } else {
                rgb(0x21262d)
            })
            .hover(|s| s.bg(rgb(0x1b222d)).border_color(rgb(0x38414d)))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            // Left: Index + Track & Artist + Tags
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .overflow_hidden()
                    .child(
                        div()
                            .w(px(24.))
                            .text_size(px(12.))
                            .text_color(rgb(0x6e7681))
                            .child(format!("{:02}", idx + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(if is_dislike {
                                                rgb(0x6e7681)
                                            } else {
                                                rgb(0xe6edf3)
                                            })
                                            .truncate()
                                            .child(it.title.clone()),
                                    )
                                    .children(has_both.then(|| {
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgba(0xe8414228))
                                            .text_size(px(10.))
                                            .text_color(rgb(0xff7b7c))
                                            .child("🔥 双源共振")
                                    }))
                                    .children(it.sources.iter().map(|s| {
                                        let (lbl, col) = if s == "lastfm" {
                                            ("Last.fm", rgb(0x58a6ff))
                                        } else {
                                            ("网易云相似", rgb(0x3fb950))
                                        };
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x1f2630))
                                            .text_size(px(10.))
                                            .text_color(col)
                                            .child(lbl)
                                    }))
                                    .children(it.heard.as_ref().map(|h| {
                                        let is_skip = h.skipped >= 2 && h.skipped > h.completed;
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x1f2630))
                                            .text_size(px(10.))
                                            .text_color(if is_skip {
                                                rgb(0xd29922)
                                            } else {
                                                rgb(0x8b949e)
                                            })
                                            .child(if is_skip {
                                                format!("⏭ 曾跳过 {} 次", h.skipped)
                                            } else {
                                                format!("🎧 听过 {} 次", h.plays)
                                            })
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x8b949e))
                                    .truncate()
                                    .child(format!(
                                        "{} · 契合度 {:.0}%",
                                        it.artist,
                                        (it.score * 100.0).min(99.0)
                                    )),
                            ),
                    ),
            )
            // Right: Action Buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child({
                        let t_radar = it.title.clone();
                        let a_radar = it.artist.clone();
                        div()
                            .id(("radar-row-re", idx))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(rgb(0x21262d))
                            .hover(|s| s.bg(rgb(0x30363d)))
                            .text_size(px(11.))
                            .text_color(rgb(0xc9d1d9))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.quick_radar(&t_radar, &a_radar, cx);
                            }))
                            .child("🎯 雷达")
                    })
                    .child({
                        let t_like = title_c.clone();
                        let a_like = artist_c.clone();
                        div()
                            .id(("radar-row-like", idx))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(if is_like {
                                rgba(0x2ea04344)
                            } else {
                                rgb(0x21262d)
                            })
                            .border_1()
                            .border_color(if is_like {
                                rgb(0x3fb950)
                            } else {
                                rgba(0x00000000)
                            })
                            .text_size(px(11.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_feedback_item(
                                    t_like.clone(),
                                    a_like.clone(),
                                    "like",
                                    cx,
                                );
                            }))
                            .child("👍")
                    })
                    .child({
                        let t_dis = title_c.clone();
                        let a_dis = artist_c.clone();
                        div()
                            .id(("radar-row-dis", idx))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(if is_dislike {
                                rgba(0xe8414244)
                            } else {
                                rgb(0x21262d)
                            })
                            .border_1()
                            .border_color(if is_dislike {
                                rgb(0xe84142)
                            } else {
                                rgba(0x00000000)
                            })
                            .text_size(px(11.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_feedback_item(
                                    t_dis.clone(),
                                    a_dis.clone(),
                                    "dislike",
                                    cx,
                                );
                            }))
                            .child("🚫")
                    })
                    .child(if let Some(id) = ncm_id {
                        let t_play = title_c;
                        div()
                            .id(("radar-row-play", idx))
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(rgb(0xe84142))
                            .hover(|s| s.bg(rgb(0xff5556)))
                            .text_size(px(11.))
                            .text_color(rgb(0xffffff))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                HubApp::play_in_client(id);
                                this.show_toast(format!("▶ 已唤起网易云播放：{}", t_play), cx);
                            }))
                            .child("▶ 网易云播放")
                            .into_any_element()
                    } else {
                        let kw = urlencoding::encode(&format!("{} {}", artist_c, title_c)).to_string();
                        let bili_url = format!("https://search.bilibili.com/all?keyword={}", kw);
                        div()
                            .id(("radar-row-bili", idx))
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(rgb(0x21262d))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .hover(|s| s.bg(rgb(0x30363d)))
                            .text_size(px(11.))
                            .text_color(rgb(0x58a6ff))
                            .on_click(move |_, _, _| {
                                let _ = open::that(&bili_url);
                            })
                            .child("📺 B站试听")
                            .into_any_element()
                    }),
            )
    }

    fn render_roam_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("roam-scroll")
            .size_full()
            .overflow_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            // Top Control Card
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_size(px(16.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("🌌 我的专属全盘漫游"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(0x8b949e))
                                            .child("自动提取你本地完播率最高的真爱曲目与 Last.fm 常听榜，多源合成新歌漫游清单"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2p5()
                                    .child(
                                        div()
                                            .id("chk-hide-heard")
                                            .px_2p5()
                                            .py_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x0d1117))
                                            .border_1()
                                            .border_color(if self.roam_hide_heard {
                                                rgb(0xe84142)
                                            } else {
                                                rgb(0x30363d)
                                            })
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.roam_hide_heard = !this.roam_hide_heard;
                                                cx.notify();
                                            }))
                                            .child(if self.roam_hide_heard {
                                                "☑ 自动剔除已听过的歌"
                                            } else {
                                                "☐ 自动剔除已听过的歌"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("btn-roam-run")
                                            .px_3p5()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0xe84142))
                                            .hover(|s| s.bg(rgb(0xff5556)))
                                            .text_color(rgb(0xffffff))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.run_roam(false, cx);
                                            }))
                                            .child(if self.roam_loading {
                                                "⏳ 漫游合成中..."
                                            } else {
                                                "✨ 生成今日漫游歌单"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("btn-roam-shuffle")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .hover(|s| s.bg(rgb(0x30363d)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.run_roam(true, cx);
                                            }))
                                            .child("🎲 换一批种子"),
                                    ),
                            ),
                    )
                    // Seeds row if available
                    .children(self.roam_result.as_ref().map(|res| {
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_1p5()
                            .pt_2()
                            .border_t_1()
                            .border_color(rgb(0x21262d))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x8b949e))
                                    .child("🌱 本次漫游种子（点击可单曲雷达）："),
                            )
                            .children(res.seeds.iter().enumerate().map(|(i, s)| {
                                let t_s = s.title.clone();
                                let a_s = s.artist.clone();
                                div()
                                    .id(("roam-seed", i))
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .cursor_pointer()
                                    .bg(rgb(0x21262d))
                                    .border_1()
                                    .border_color(rgb(0x30363d))
                                    .hover(|st| {
                                        st.border_color(rgb(0xe84142)).text_color(rgb(0xff7b7c))
                                    })
                                    .text_size(px(11.))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.quick_radar(&t_s, &a_s, cx);
                                    }))
                                    .child(format!("{} · {}", s.title, s.artist))
                            }))
                    })),
            )
            // Roam Results Table
            .children(self.roam_result.as_ref().map(|res| {
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .pb_2()
                            .border_b_1()
                            .border_color(rgb(0x262c36))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child(format!(
                                        "为你合成的专属漫游（共 {} 首）",
                                        res.tracks.len()
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn-roam-netease")
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgba(0xe841422e))
                                            .border_1()
                                            .border_color(rgb(0xe84142))
                                            .text_color(rgb(0xff7b7c))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(ref r) = this.roam_result {
                                                    let txt = r
                                                        .tracks
                                                        .iter()
                                                        .map(|t| format!("{} - {}", t.title, t.artist))
                                                        .collect::<Vec<_>>()
                                                        .join("\n");
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(txt),
                                                    );
                                                    this.show_toast(
                                                        format!(
                                                            "📤 已复制 {} 首网易云导入格式！在网易云歌单点击「导入外部歌单 -> 文本导入」粘贴即可",
                                                            r.tracks.len()
                                                        ),
                                                        cx,
                                                    );
                                                }
                                            }))
                                            .child("📤 灌入网易云"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-roam-copy")
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(ref r) = this.roam_result {
                                                    let txt = r
                                                        .tracks
                                                        .iter()
                                                        .enumerate()
                                                        .map(|(i, t)| {
                                                            format!(
                                                                "{}. {} - {}",
                                                                i + 1,
                                                                t.artist,
                                                                t.title
                                                            )
                                                        })
                                                        .collect::<Vec<_>>()
                                                        .join("\n");
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(txt),
                                                    );
                                                    this.show_toast("📋 已复制漫游歌单到剪贴板", cx);
                                                }
                                            }))
                                            .child("📋 复制文本"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-roam-txt")
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(ref r) = this.roam_result {
                                                    let txt = r
                                                        .tracks
                                                        .iter()
                                                        .map(|t| format!("{} - {}", t.title, t.artist))
                                                        .collect::<Vec<_>>()
                                                        .join("\r\n");
                                                    let date_s = Local::now().format("%Y%m%d").to_string();
                                                    let fname = format!("MusicHub_今日漫游_{}.txt", date_s);
                                                    this.save_file_with_dialog(
                                                        fname,
                                                        txt.into_bytes(),
                                                        "漫游歌单 .txt".to_string(),
                                                        cx,
                                                    );
                                                }
                                            }))
                                            .child("📥 另存为 .txt"),
                                    ),
                            ),
                    )
                    .children(res.tracks.iter().enumerate().map(|(idx, it)| {
                        self.render_roam_track_row(idx, it, cx)
                    }))
            }))
    }

    fn render_roam_track_row(
        &self,
        idx: usize,
        it: &RoamTrack,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_like = it.feedback.as_deref() == Some("like");
        let is_dislike = it.feedback.as_deref() == Some("dislike");
        let ncm_id = it.ncm.as_ref().map(|n| n.id);
        let title_c = it.title.clone();
        let artist_c = it.artist.clone();
        let reasons_str = if it.reasons.is_empty() {
            String::new()
        } else {
            format!(" · 源于：{}", it.reasons.join(" / "))
        };

        div()
            .w_full()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(if is_dislike {
                rgb(0x11151c)
            } else {
                rgb(0x121720)
            })
            .border_1()
            .border_color(if is_like {
                rgba(0x3fb95066)
            } else {
                rgb(0x21262d)
            })
            .hover(|s| s.bg(rgb(0x1b222d)).border_color(rgb(0x38414d)))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .overflow_hidden()
                    .child(
                        div()
                            .w(px(24.))
                            .text_size(px(12.))
                            .text_color(rgb(0x6e7681))
                            .child(format!("{:02}", idx + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(if is_dislike {
                                                rgb(0x6e7681)
                                            } else {
                                                rgb(0xe6edf3)
                                            })
                                            .truncate()
                                            .child(it.title.clone()),
                                    )
                                    .children(it.sources.iter().map(|s| {
                                        let (lbl, col) = if s == "lastfm" {
                                            ("Last.fm", rgb(0x58a6ff))
                                        } else {
                                            ("网易云相似", rgb(0x3fb950))
                                        };
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x1f2630))
                                            .text_size(px(10.))
                                            .text_color(col)
                                            .child(lbl)
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x8b949e))
                                    .truncate()
                                    .child(format!("{}{}", it.artist, reasons_str)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child({
                        let t_like = title_c.clone();
                        let a_like = artist_c.clone();
                        div()
                            .id(("roam-like", idx))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(if is_like {
                                rgba(0x2ea04344)
                            } else {
                                rgb(0x21262d)
                            })
                            .text_size(px(11.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_feedback_item(
                                    t_like.clone(),
                                    a_like.clone(),
                                    "like",
                                    cx,
                                );
                            }))
                            .child("👍")
                    })
                    .child({
                        let t_dis = title_c.clone();
                        let a_dis = artist_c.clone();
                        div()
                            .id(("roam-dis", idx))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(if is_dislike {
                                rgba(0xe8414244)
                            } else {
                                rgb(0x21262d)
                            })
                            .text_size(px(11.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_feedback_item(
                                    t_dis.clone(),
                                    a_dis.clone(),
                                    "dislike",
                                    cx,
                                );
                            }))
                            .child("🚫")
                    })
                    .child(if let Some(id) = ncm_id {
                        let t_play = title_c;
                        div()
                            .id(("roam-play", idx))
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(rgb(0xe84142))
                            .hover(|s| s.bg(rgb(0xff5556)))
                            .text_size(px(11.))
                            .text_color(rgb(0xffffff))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                HubApp::play_in_client(id);
                                this.show_toast(format!("▶ 已唤起网易云播放：{}", t_play), cx);
                            }))
                            .child("▶ 播放")
                            .into_any_element()
                    } else {
                        let kw = urlencoding::encode(&format!("{} {}", artist_c, title_c)).to_string();
                        let bili_url = format!("https://search.bilibili.com/all?keyword={}", kw);
                        div()
                            .id(("roam-bili", idx))
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(rgb(0x21262d))
                            .text_size(px(11.))
                            .text_color(rgb(0x58a6ff))
                            .on_click(move |_, _, _| {
                                let _ = open::that(&bili_url);
                            })
                            .child("📺 B站")
                            .into_any_element()
                    }),
            )
    }
}

impl ConsoleView {
    fn render_stats_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let d = &self.stats_data;
        let mins = ((d.totals.ms as f64) / 60000.0).round() as i64;
        let dur_str = if mins >= 60 {
            format!("{:.1} 小时", mins as f64 / 60.0)
        } else {
            format!("{} 分钟", mins)
        };
        let comp_rate = if d.totals.n > 0 {
            ((d.totals.c as f64 / d.totals.n as f64) * 100.0).round() as i64
        } else {
            0
        };
        let skip_rate = if d.totals.n > 0 {
            ((d.totals.s as f64 / d.totals.n as f64) * 100.0).round() as i64
        } else {
            0
        };
        let done_scrobble = d.scrobble.get("done").copied().unwrap_or(0);

        let mut max_hour_cnt = 1i64;
        let mut peak_h = 0i64;
        let mut peak_cnt = 0i64;
        for h in 0..24 {
            let cnt = d.hours.get(&h).copied().unwrap_or(0);
            if cnt > max_hour_cnt {
                max_hour_cnt = cnt;
            }
            if cnt > peak_cnt {
                peak_cnt = cnt;
                peak_h = h;
            }
        }
        let peak_label = if peak_cnt == 0 {
            "等待积累数据".to_string()
        } else if (0..6).contains(&peak_h) {
            format!("🌙 深夜党 ({:02}:00 峰值 {} 首)", peak_h, peak_cnt)
        } else if (6..12).contains(&peak_h) {
            format!("🌅 晨间派 ({:02}:00 峰值 {} 首)", peak_h, peak_cnt)
        } else if (12..18).contains(&peak_h) {
            format!("☀️ 午后心流 ({:02}:00 峰值 {} 首)", peak_h, peak_cnt)
        } else {
            format!("🌆 晚间沉浸 ({:02}:00 峰值 {} 首)", peak_h, peak_cnt)
        };

        let max_art_n = d.top_artists.first().map(|a| a.n.max(1)).unwrap_or(1);

        div()
            .id("stats-scroll")
            .size_full()
            .overflow_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            // Header & Period Switcher
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("📊 本地听歌画像与统计"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x8b949e))
                                    .child("基于本地 SQLite 毫秒级完播与切歌流水统计"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("btn-wrapped-poster")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0xe84142))
                                    .hover(|s| s.bg(rgb(0xff5556)))
                                    .text_color(rgb(0xffffff))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_size(px(12.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.stats_data = this.hub.stats(this.stats_days);
                                        let poster = render_wrapped_poster(
                                            &this.stats_data,
                                            this.stats_days,
                                        );
                                        this.wrapped_poster = Some(poster);
                                        cx.notify();
                                    }))
                                    .child("📸 生成听歌名片海报"),
                            )
                            .children([(7i64, "近 7 天"), (30, "近 30 天"), (3650, "全部历史")].into_iter().map(
                                |(days, lbl)| {
                                    let active = self.stats_days == days;
                                    div()
                                        .id(("btn-days", days as usize))
                                        .px_3()
                                        .py_1p5()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(if active {
                                            rgba(0xe8414233)
                                        } else {
                                            rgb(0x161b22)
                                        })
                                        .border_1()
                                        .border_color(if active {
                                            rgb(0xe84142)
                                        } else {
                                            rgb(0x30363d)
                                        })
                                        .text_color(if active {
                                            rgb(0xff7b7c)
                                        } else {
                                            rgb(0xc9d1d9)
                                        })
                                        .text_size(px(12.))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.stats_days = days;
                                            this.stats_data = this.hub.stats(days);
                                            cx.notify();
                                        }))
                                        .child(lbl)
                                },
                            )),
                    ),
            )
            // 4 Core Metric Cards Row
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .children(
                        [
                            (
                                "累计播放曲目",
                                format!("{} 首", d.totals.n),
                                format!("不重复单曲 {} 首", d.totals.uniq),
                                rgb(0xffffff),
                            ),
                            (
                                "累计专注时长",
                                dur_str,
                                format!("共陪伴 {} 分钟", mins),
                                rgb(0x58a6ff),
                            ),
                            (
                                "完整听完率",
                                format!("{}%", comp_rate),
                                format!("完播 {} 次 · 切歌率 {}%", d.totals.c, skip_rate),
                                rgb(0x3fb950),
                            ),
                            (
                                "Last.fm 已打卡",
                                format!("{} 次", done_scrobble),
                                format!("切歌跳过 {} 次", d.totals.s),
                                rgb(0xff7b7c),
                            ),
                        ]
                        .into_iter()
                        .map(|(label, val, sub, col)| {
                            div()
                                .flex_1()
                                .p_3p5()
                                .rounded_lg()
                                .bg(rgb(0x161b22))
                                .border_1()
                                .border_color(rgb(0x30363d))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(0x8b949e))
                                        .child(label),
                                )
                                .child(
                                    div()
                                        .text_size(px(22.))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(col)
                                        .child(val),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(0x6e7681))
                                        .child(sub),
                                )
                        }),
                    ),
            )
            // Recommendation Closed-Loop Hit Rate Banner
            .child(
                div()
                    .w_full()
                    .px_4()
                    .py_2p5()
                    .rounded_lg()
                    .bg(rgb(0x131a26))
                    .border_1()
                    .border_color(rgba(0x388bfd55))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0x58a6ff))
                                    .child("🎯 智能推荐闭环追踪"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0xc9d1d9))
                                    .child(if d.recs.total > 0 {
                                        format!(
                                            "周期内共为你推荐 {} 首 · 实际播放转化 {} 首 ({:.1}%) · 听完满意 {} 首 (命中率 {:.1}%)",
                                            d.recs.total,
                                            d.recs.heard,
                                            d.recs.conversion_rate,
                                            d.recs.completed,
                                            d.recs.hit_rate
                                        )
                                    } else {
                                        "在「单曲雷达」或「全盘漫游」生成推荐并试听后，系统将自动闭环统计推荐完播命中率".to_string()
                                    }),
                            ),
                    ),
            )
            // Middle 2 Columns: Top 10 Artists + 24h Biological Clock
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .gap_4()
                    // Left: Top 10 Artists
                    .child(
                        div()
                            .flex_1()
                            .p_4()
                            .rounded_lg()
                            .bg(rgb(0x161b22))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .flex()
                            .flex_col()
                            .gap_2p5()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("🎤 常听歌手 Top 10（点击播代表作）"),
                            )
                            .children(if d.top_artists.is_empty() {
                                vec![div()
                                    .py_4()
                                    .text_color(rgb(0x6e7681))
                                    .child("暂无统计数据")
                                    .into_any_element()]
                            } else {
                                d.top_artists
                                    .iter()
                                    .enumerate()
                                    .map(|(i, a)| {
                                        let pct = (a.n as f32 / max_art_n as f32).clamp(0.06, 1.0);
                                        let a_mins = ((a.ms as f64) / 60000.0).round() as i64;
                                        let art_click = a.artist.clone();
                                        div()
                                            .id(("top-artist", i))
                                            .w_full()
                                            .cursor_pointer()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let hub = this.hub.clone();
                                                let rt = hub.rt.clone();
                                                let art = art_click.clone();
                                                rt.spawn(async move {
                                                    let _ = hub.play_artist(&art).await;
                                                });
                                                this.show_toast(
                                                    format!("▶ 正在播放 {} 的代表作", art_click),
                                                    cx,
                                                );
                                            }))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .justify_between()
                                                    .text_size(px(12.))
                                                    .child(
                                                        div()
                                                            .text_color(rgb(0xe6edf3))
                                                            .font_weight(gpui::FontWeight::MEDIUM)
                                                            .child(format!("{}. {}", i + 1, a.artist)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(rgb(0x8b949e))
                                                            .text_size(px(11.))
                                                            .child(format!(
                                                                "{} 首 · {} 分钟",
                                                                a.n, a_mins
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(px(5.))
                                                    .rounded_full()
                                                    .bg(rgb(0x21262d))
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .h_full()
                                                            .w(relative(pct))
                                                            .rounded_full()
                                                            .bg(rgb(0xe84142)),
                                                    ),
                                            )
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                    )
                    // Right: 24-Hour Biological Clock
                    .child(
                        div()
                            .flex_1()
                            .p_4()
                            .rounded_lg()
                            .bg(rgb(0x161b22))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("🕰 24 小时听歌生物钟"),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x21262d))
                                            .text_size(px(11.))
                                            .text_color(rgb(0xff7b7c))
                                            .child(peak_label),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(190.))
                                    .pt_4()
                                    .flex()
                                    .flex_row()
                                    .items_end()
                                    .justify_between()
                                    .gap_1()
                                    .children((0..24i64).map(|h| {
                                        let cnt = d.hours.get(&h).copied().unwrap_or(0);
                                        let bar_h = if cnt == 0 {
                                            4.0
                                        } else {
                                            ((cnt as f32 / max_hour_cnt as f32) * 135.0).max(8.0)
                                        };
                                        let is_peak = cnt > 0 && h == peak_h;
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(9.))
                                                    .text_color(if is_peak {
                                                        rgb(0xff7b7c)
                                                    } else {
                                                        rgb(0x6e7681)
                                                    })
                                                    .child(if cnt > 0 {
                                                        cnt.to_string()
                                                    } else {
                                                        String::new()
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(px(bar_h))
                                                    .rounded_sm()
                                                    .bg(if is_peak {
                                                        rgb(0xe84142)
                                                    } else if cnt > 0 {
                                                        rgb(0x388bfd)
                                                    } else {
                                                        rgb(0x21262d)
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(9.))
                                                    .text_color(rgb(0x8b949e))
                                                    .child(format!("{:02}", h)),
                                            )
                                    })),
                            ),
                    ),
            )
            // Bottom 2 Columns: Loved List vs Skipped List
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .gap_4()
                    // Left: Loved List
                    .child(
                        div()
                            .flex_1()
                            .p_4()
                            .rounded_lg()
                            .bg(rgb(0x161b22))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0x3fb950))
                                    .child("💚 听完率最高（真爱榜 Top 10）"),
                            )
                            .children(if d.loved.is_empty() {
                                vec![div()
                                    .py_3()
                                    .text_color(rgb(0x6e7681))
                                    .child("暂无完播记录")
                                    .into_any_element()]
                            } else {
                                d.loved
                                    .iter()
                                    .enumerate()
                                    .map(|(i, t)| {
                                        let trk_r = t.title.clone();
                                        let art_r = t.artist.clone();
                                        let ncm_id = t.ncm_id;
                                        div()
                                            .w_full()
                                            .px_2p5()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(rgb(0x121720))
                                            .border_1()
                                            .border_color(rgb(0x21262d))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                                            .truncate()
                                                            .child(format!("{}. {}", i + 1, t.title)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(0x8b949e))
                                                            .truncate()
                                                            .child(format!(
                                                                "{} · 完播 {}/{} 次",
                                                                t.artist, t.c, t.n
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .id(("loved-radar", i))
                                                            .px_2()
                                                            .py_0p5()
                                                            .rounded_sm()
                                                            .cursor_pointer()
                                                            .bg(rgb(0x21262d))
                                                            .text_size(px(11.))
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.quick_radar(
                                                                        &trk_r, &art_r, cx,
                                                                    );
                                                                },
                                                            ))
                                                            .child("🎯"),
                                                    )
                                                    .children(ncm_id.map(|id| {
                                                        div()
                                                            .id(("loved-play", i))
                                                            .px_2()
                                                            .py_0p5()
                                                            .rounded_sm()
                                                            .cursor_pointer()
                                                            .bg(rgb(0xe84142))
                                                            .text_color(rgb(0xffffff))
                                                            .text_size(px(11.))
                                                            .on_click(move |_, _, _| {
                                                                HubApp::play_in_client(id);
                                                            })
                                                            .child("▶")
                                                    })),
                                            )
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                    )
                    // Right: Skipped List
                    .child(
                        div()
                            .flex_1()
                            .p_4()
                            .rounded_lg()
                            .bg(rgb(0x161b22))
                            .border_1()
                            .border_color(rgb(0x30363d))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xff7b7c))
                                    .child("⏭ 多次切歌跳过（累计跳过≥2次且多于完播）"),
                            )
                            .children(if d.skipped.is_empty() {
                                vec![div()
                                    .py_3()
                                    .text_color(rgb(0x6e7681))
                                    .child("暂无高频跳过曲目")
                                    .into_any_element()]
                            } else {
                                d.skipped
                                    .iter()
                                    .enumerate()
                                    .map(|(i, t)| {
                                        let trk_c = t.title.clone();
                                        let art_c = t.artist.clone();
                                        div()
                                            .w_full()
                                            .px_2p5()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(rgb(0x121720))
                                            .border_1()
                                            .border_color(rgb(0x21262d))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                                            .truncate()
                                                            .child(format!("{}. {}", i + 1, t.title)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(0x8b949e))
                                                            .truncate()
                                                            .child(format!(
                                                                "{} · 跳过 {} 次 / 播放 {} 次",
                                                                t.artist, t.s, t.n
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .id(("btn-clear-skip", i))
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .cursor_pointer()
                                                    .bg(rgb(0x21262d))
                                                    .border_1()
                                                    .border_color(rgb(0x30363d))
                                                    .hover(|s| s.bg(rgb(0x30363d)))
                                                    .text_size(px(11.))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.hub.db.clear_skip(&trk_c, &art_c);
                                                        this.stats_data =
                                                            this.hub.stats(this.stats_days);
                                                        this.show_toast(
                                                            format!("已赦免跳过记录：{}", trk_c),
                                                            cx,
                                                        );
                                                    }))
                                                    .child("赦免"),
                                            )
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                    ),
            )
    }

    fn render_recent_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("recent-scroll")
            .size_full()
            .overflow_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Header Toolbar
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .pb_2()
                            .border_b_1()
                            .border_color(rgb(0x262c36))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_size(px(16.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child(format!(
                                                "🕒 最近播放流水账（最近 {} 条）",
                                                self.recent_rows.len()
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(0x8b949e))
                                            .child("记录每一首歌的实际停留时长、完播率、切歌跳过判定与 Last.fm 打卡状态"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn-export-csv")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .hover(|s| s.bg(rgb(0x30363d)))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let bytes = this.hub.export_csv();
                                                let now_s =
                                                    Local::now().format("%Y%m%d_%H%M%S").to_string();
                                                this.save_file_with_dialog(
                                                    format!("musichub_history_{}.csv", now_s),
                                                    bytes,
                                                    "听歌流水 CSV".to_string(),
                                                    cx,
                                                );
                                            }))
                                            .child("📥 导出流水 CSV"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-export-db")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .hover(|s| s.bg(rgb(0x30363d)))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let bytes = this.hub.export_backup();
                                                let now_s =
                                                    Local::now().format("%Y%m%d_%H%M%S").to_string();
                                                this.save_file_with_dialog(
                                                    format!("musichub_backup_{}.db", now_s),
                                                    bytes,
                                                    "SQLite 数据库备份".to_string(),
                                                    cx,
                                                );
                                            }))
                                            .child("💾 备份数据库"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-scrobble-all")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .hover(|s| s.bg(rgb(0x30363d)))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                match this.hub.manual_scrobble(None) {
                                                    Ok(v) => {
                                                        let q = v
                                                            .get("queued")
                                                            .and_then(|x| x.as_u64())
                                                            .unwrap_or(0);
                                                        this.recent_rows = this.hub.db.recent(150);
                                                        this.show_toast(
                                                            format!(
                                                                "📤 已将 {} 条达标记录加入 Last.fm 补打卡队列",
                                                                q
                                                            ),
                                                            cx,
                                                        );
                                                    }
                                                    Err(e) => {
                                                        this.show_toast(e, cx);
                                                    }
                                                }
                                            }))
                                            .child("📤 一键补打卡达标记录"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-refresh-recent")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0xe84142))
                                            .text_color(rgb(0xffffff))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.recent_rows = this.hub.db.recent(150);
                                                this.show_toast("已刷新播放流水账", cx);
                                            }))
                                            .child("🔄 刷新"),
                                    ),
                            ),
                    )
                    // Rows
                    .children(self.recent_rows.iter().enumerate().map(|(idx, r)| {
                        let ts_str = Local
                            .timestamp_opt(r.started_at, 0)
                            .single()
                            .map(|t| t.format("%m-%d %H:%M").to_string())
                            .unwrap_or_default();
                        let app_lbl = format_app_name(r.app.as_deref().unwrap_or(""));
                        let dur_s = fmt_mm_ss(r.duration_ms.unwrap_or(0));
                        let played_s = fmt_mm_ss(r.played_ms);
                        let play_id = r.id;
                        let ncm_id = r.ncm_id;
                        let trk_radar = r.title.clone();
                        let art_radar = r.artist.clone();

                        let (comp_lbl, comp_col) = if r.completed == 1 {
                            ("✅ 完播", rgb(0x3fb950))
                        } else if r.skipped == 1 {
                            ("⏭ 跳过", rgb(0xff7b7c))
                        } else {
                            ("⏹ 部分", rgb(0x8b949e))
                        };

                        let (sc_lbl, sc_col) = match r.scrobble_state.as_str() {
                            "done" => ("☁️ 已打卡", rgb(0x3fb950)),
                            "pending" => ("⏳ 打卡排队", rgb(0x58a6ff)),
                            "eligible" => ("🎯 达标未打卡", rgb(0xd29922)),
                            _ => ("—", rgb(0x6e7681)),
                        };

                        let can_manual_scrobble =
                            r.scrobble_state != "done" && r.scrobble_state != "pending";

                        div()
                            .w_full()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(rgb(0x121720))
                            .border_1()
                            .border_color(rgb(0x21262d))
                            .hover(|s| s.bg(rgb(0x1b222d)))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .w(px(76.))
                                            .text_size(px(11.))
                                            .text_color(rgb(0x6e7681))
                                            .child(ts_str),
                                    )
                                    .child(
                                        div()
                                            .w(px(68.))
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(0x1f2630))
                                            .text_size(px(10.))
                                            .text_color(rgb(0x8b949e))
                                            .truncate()
                                            .child(app_lbl),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0xe6edf3))
                                                    .truncate()
                                                    .child(r.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(0x8b949e))
                                                    .truncate()
                                                    .child(format!("— {}", r.artist)),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w(px(82.))
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child(format!("{} / {}", played_s, dur_s)),
                                    )
                                    .child(
                                        div()
                                            .w(px(52.))
                                            .text_size(px(11.))
                                            .text_color(comp_col)
                                            .child(comp_lbl),
                                    )
                                    .child(
                                        div()
                                            .w(px(76.))
                                            .text_size(px(11.))
                                            .text_color(sc_col)
                                            .child(sc_lbl),
                                    )
                                    .child(
                                        div()
                                            .id(("rec-radar", idx))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .text_size(px(11.))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.quick_radar(&trk_radar, &art_radar, cx);
                                            }))
                                            .child("🎯"),
                                    )
                                    .children(ncm_id.map(|id| {
                                        div()
                                            .id(("rec-play", idx))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .text_size(px(11.))
                                            .on_click(move |_, _, _| {
                                                HubApp::play_in_client(id);
                                            })
                                            .child("▶")
                                    }))
                                    .children(can_manual_scrobble.then(|| {
                                        div()
                                            .id(("rec-scrobble", idx))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .text_size(px(11.))
                                            .text_color(rgb(0x58a6ff))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                match this.hub.manual_scrobble(Some(play_id)) {
                                                    Ok(_) => {
                                                        this.recent_rows =
                                                            this.hub.db.recent(150);
                                                        this.show_toast(
                                                            "☁️ 已加入补打卡队列",
                                                            cx,
                                                        );
                                                    }
                                                    Err(e) => {
                                                        this.show_toast(e, cx);
                                                    }
                                                }
                                            }))
                                            .child("补打卡")
                                    }))
                                    .child(
                                        div()
                                            .id(("rec-del", idx))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .text_size(px(11.))
                                            .text_color(rgb(0xff7b7c))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.hub.db.delete_play(play_id);
                                                this.recent_rows = this.hub.db.recent(150);
                                                this.show_toast("已删除该条播放记录", cx);
                                            }))
                                            .child("✕"),
                                    ),
                            )
                    }))
            )
    }

    fn render_settings_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_auth = self.hub.cfg.is_authorized();
        let cfg_pub = self.hub.cfg.public();

        div()
            .id("settings-scroll")
            .size_full()
            .overflow_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            // Section 1: Last.fm Username & Import
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("1. 绑定 Last.fm 用户名（免 Key 即可开启漫游与历史导入）"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_end()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(300.))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child("Last.fm 用户名 (Username)"),
                                    )
                                    .child(self.username_input.clone()),
                            )
                            .child(
                                div()
                                    .id("btn-save-user")
                                    .h(px(34.))
                                    .px_4()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0xe84142))
                                    .text_color(rgb(0xffffff))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .flex()
                                    .items_center()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let u = this.username_input.read(cx).text().trim().to_string();
                                        this.hub.cfg.update(|d| d.username = u.clone());
                                        this.show_toast(
                                            format!("✅ 已保存 Last.fm 用户名：{}", u),
                                            cx,
                                        );
                                    }))
                                    .child("保存用户名"),
                            )
                            .child(
                                div()
                                    .id("btn-import-lastfm")
                                    .h(px(34.))
                                    .px_4()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x21262d))
                                    .border_1()
                                    .border_color(rgb(0x30363d))
                                    .hover(|s| s.bg(rgb(0x30363d)))
                                    .flex()
                                    .items_center()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.importing_lastfm {
                                            return;
                                        }
                                        let u = this.username_input.read(cx).text().trim().to_string();
                                        if u.is_empty() {
                                            this.show_toast("请先填写 Last.fm 用户名", cx);
                                            return;
                                        }
                                        this.hub.cfg.update(|d| d.username = u.clone());
                                        this.importing_lastfm = true;
                                        cx.notify();

                                        let hub = this.hub.clone();
                                        let rt = hub.rt.clone();
                                        let (tx, rx) = tokio::sync::oneshot::channel();
                                        rt.spawn(async move {
                                            let res = hub.import_lastfm(Some(&u)).await;
                                            let _ = tx.send(res);
                                        });
                                        cx.spawn(async move |this, cx| {
                                            if let Ok(res) = rx.await {
                                                let _ = this.update(cx, |this, cx| {
                                                    this.importing_lastfm = false;
                                                    match res {
                                                        Ok(v) => {
                                                            let imp = v
                                                                .get("imported")
                                                                .and_then(|x| x.as_u64())
                                                                .unwrap_or(0);
                                                            let tot = v
                                                                .get("total_fetched")
                                                                .and_then(|x| x.as_u64())
                                                                .unwrap_or(0);
                                                            this.show_toast(
                                                                format!(
                                                                    "🎉 成功同步 {} 条新记录（扫描最近 {} 条）",
                                                                    imp, tot
                                                                ),
                                                                cx,
                                                            );
                                                        }
                                                        Err(e) => {
                                                            this.show_toast(e, cx);
                                                        }
                                                    }
                                                });
                                            }
                                        })
                                        .detach();
                                    }))
                                    .child(if self.importing_lastfm {
                                        "⏳ 正在从 Last.fm 同步..."
                                    } else {
                                        "📥 一键从 Last.fm 导入打卡历史"
                                    }),
                            ),
                    ),
            )
            // Section 2: Last.fm API Key & OAuth
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("2. 接管 Last.fm 自动打卡（OAuth 授权）"),
                            )
                            .child(
                                div()
                                    .px_2p5()
                                    .py_0p5()
                                    .rounded_full()
                                    .bg(if is_auth {
                                        rgba(0x2ea04333)
                                    } else {
                                        rgb(0x21262d)
                                    })
                                    .text_size(px(11.))
                                    .text_color(if is_auth {
                                        rgb(0x3fb950)
                                    } else {
                                        rgb(0xd29922)
                                    })
                                    .child(if is_auth {
                                        format!("✅ 已授权绑定 ({})", cfg_pub.username)
                                    } else {
                                        "未完成网页授权".to_string()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child("API Key (32 位)"),
                                    )
                                    .child(self.api_key_input.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x8b949e))
                                            .child("Shared Secret (32 位)"),
                                    )
                                    .child(self.api_secret_input.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2p5()
                            .child(
                                div()
                                    .id("btn-save-creds")
                                    .px_4()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0xe84142))
                                    .text_color(rgb(0xffffff))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let k = this.api_key_input.read(cx).text().trim().to_string();
                                        let s = this.api_secret_input.read(cx).text().trim().to_string();
                                        this.hub.cfg.update(|d| {
                                            if k != d.api_key {
                                                d.api_key = k;
                                                d.session_key.clear();
                                            }
                                            if !s.is_empty() {
                                                d.api_secret = s;
                                            }
                                        });
                                        this.show_toast(
                                            "✅ 已保存 Key 与 Secret，请点击「前往 Last.fm 网页授权」",
                                            cx,
                                        );
                                    }))
                                    .child("保存 Key"),
                            )
                            .child(
                                div()
                                    .id("btn-oauth-start")
                                    .px_3p5()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x21262d))
                                    .border_1()
                                    .border_color(rgb(0x30363d))
                                    .hover(|s| s.bg(rgb(0x30363d)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let k = this.api_key_input.read(cx).text().trim().to_string();
                                        let s = this.api_secret_input.read(cx).text().trim().to_string();
                                        this.hub.cfg.update(|d| {
                                            if !k.is_empty() && k != d.api_key {
                                                d.api_key = k;
                                                d.session_key.clear();
                                            }
                                            if !s.is_empty() {
                                                d.api_secret = s;
                                            }
                                        });
                                        if !this.hub.cfg.has_credentials() {
                                            this.show_toast("请先填写 API Key 和 Shared Secret", cx);
                                            return;
                                        }
                                        let url = format!("{}/api/auth/start", this.hub.base_url());
                                        let _ = open::that(url);
                                        this.show_toast(
                                            "🔑 已在浏览器打开 Last.fm 授权页，点击允许后将自动同步回客户端",
                                            cx,
                                        );
                                    }))
                                    .child("🔑 前往 Last.fm 网页授权"),
                            )
                            .child(
                                div()
                                    .id("btn-open-lf-api")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x1c222d))
                                    .text_color(rgb(0x58a6ff))
                                    .text_size(px(12.))
                                    .on_click(|_, _, _| {
                                        let _ = open::that("https://www.last.fm/api/account/create");
                                    })
                                    .child("🌐 免费申请 API Key"),
                            )
                            .children(is_auth.then(|| {
                                div()
                                    .id("btn-logout-lf")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x21262d))
                                    .text_color(rgb(0xff7b7c))
                                    .text_size(px(12.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.hub.cfg.update(|d| {
                                            d.session_key.clear();
                                            d.mode = "record".to_string();
                                        });
                                        this.state.scrobbling = false;
                                        this.show_toast("已断开 Last.fm 授权并切回只记本地模式", cx);
                                    }))
                                    .child("断开授权")
                            })),
                    )
                    .child(
                        div()
                            .id("chk-scrobble-unmatched")
                            .pt_2()
                            .border_t_1()
                            .border_color(rgb(0x21262d))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(rgb(0xc9d1d9))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scrobble_unmatched = !this.scrobble_unmatched;
                                let v = this.scrobble_unmatched;
                                this.hub.cfg.update(|d| d.scrobble_unmatched = v);
                                this.show_toast(
                                    if v {
                                        "已开启：非正式曲库也将打卡到 Last.fm"
                                    } else {
                                        "已开启保护：非正式曲库仅记入本地数据库"
                                    },
                                    cx,
                                );
                            }))
                            .child(if self.scrobble_unmatched {
                                "☑ 允许将非网易云正式曲库内容（如播客/电台/B站自制音频）也打卡到 Last.fm"
                            } else {
                                "☐ 允许将非网易云正式曲库内容（如播客/电台/B站自制音频）也打卡到 Last.fm（默认仅记本地保护账本纯净）"
                            }),
                    ),
            )
            // Section 3: Monitored Players & Autostart
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("3. 监听播放器范围与系统自启"),
                            )
                            .child(
                                div()
                                    .id("chk-autostart")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x0d1117))
                                    .border_1()
                                    .border_color(if self.autostart_enabled {
                                        rgb(0x3fb950)
                                    } else {
                                        rgb(0x30363d)
                                    })
                                    .text_size(px(12.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let next = !this.autostart_enabled;
                                        set_autostart(next);
                                        this.autostart_enabled = is_autostart_enabled();
                                        this.show_toast(
                                            if this.autostart_enabled {
                                                "✅ 已开启 Windows 开机自启"
                                            } else {
                                                "已关闭开机自启"
                                            },
                                            cx,
                                        );
                                    }))
                                    .child(if self.autostart_enabled {
                                        "☑ 开机自动启动 Music Hub"
                                    } else {
                                        "☐ 开机自动启动 Music Hub"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .children(APP_PRESETS.iter().enumerate().map(|(i, (id, label))| {
                                let id_str = (*id).to_string();
                                let checked = self.selected_apps.contains(&id_str);
                                div()
                                    .id(("preset-app", i))
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if checked {
                                        rgba(0xe8414222)
                                    } else {
                                        rgb(0x0d1117)
                                    })
                                    .border_1()
                                    .border_color(if checked {
                                        rgb(0xe84142)
                                    } else {
                                        rgb(0x30363d)
                                    })
                                    .text_size(px(12.))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.selected_apps.contains(&id_str) {
                                            if this.selected_apps.len() > 1 {
                                                this.selected_apps.remove(&id_str);
                                            }
                                        } else {
                                            this.selected_apps.insert(id_str.clone());
                                        }
                                        let mut list: Vec<String> =
                                            this.selected_apps.iter().cloned().collect();
                                        if list.iter().any(|x| x == "sodamusic.exe")
                                            && !list.iter().any(|x| x == "qishui")
                                        {
                                            list.push("qishui".to_string());
                                        }
                                        this.hub.cfg.update(|d| d.apps = list);
                                        cx.notify();
                                    }))
                                    .child(format!(
                                        "{} {}",
                                        if checked { "☑" } else { "☐" },
                                        label
                                    ))
                            })),
                    ),
            )
            // Section 4: Feedback & Blacklist Manager
            .child(
                div()
                    .w_full()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child(format!(
                                        "4. 偏好喜欢与避雷黑名单管理（共 {} 首）",
                                        self.feedback_list.len()
                                    )),
                            ),
                    )
                    .children(if self.feedback_list.is_empty() {
                        vec![div()
                            .py_3()
                            .text_color(rgb(0x6e7681))
                            .child("暂无偏好或黑名单记录。在雷达、漫游或灵动岛点击 👍 / 🚫 即可添加。")
                            .into_any_element()]
                    } else {
                        self.feedback_list
                            .iter()
                            .enumerate()
                            .map(|(i, fb)| {
                                let is_like = fb.action == "like";
                                let t_rm = fb.title.clone();
                                let a_rm = fb.artist.clone();
                                div()
                                    .w_full()
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgb(0x121720))
                                    .border_1()
                                    .border_color(rgb(0x21262d))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .bg(rgb(0x1f2630))
                                                    .text_size(px(10.))
                                                    .text_color(if is_like {
                                                        rgb(0x3fb950)
                                                    } else {
                                                        rgb(0xff7b7c)
                                                    })
                                                    .child(if is_like {
                                                        "👍 喜欢"
                                                    } else {
                                                        "🚫 避雷"
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .child(fb.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_color(rgb(0x8b949e))
                                                    .child(format!("— {}", fb.artist)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id(("fb-rm", i))
                                            .px_2p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .text_color(rgb(0xff7b7c))
                                            .text_size(px(11.))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.hub.db.set_feedback(&t_rm, &a_rm, "remove");
                                                this.feedback_list =
                                                    this.hub.db.get_feedback_list(200);
                                                this.show_toast(
                                                    format!("已移除标记：{}", t_rm),
                                                    cx,
                                                );
                                            }))
                                            .child("移除"),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    }),
            )
    }

    fn render_poster_modal(
        &self,
        render_img: Arc<RenderImage>,
        png_bytes: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bytes_for_copy = png_bytes.clone();
        let bytes_for_save = png_bytes;

        div()
            .id("poster-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0x000000bb))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_, _, _| {})
            .child(
                div()
                    .w(px(460.))
                    .p_4()
                    .rounded_xl()
                    .bg(rgb(0x161b22))
                    .border_1()
                    .border_color(rgb(0x30363d))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("📸 我的听歌专属名片（750×1000 原生渲染）"),
                            )
                            .child(
                                div()
                                    .id("btn-close-poster")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(rgb(0x21262d))
                                    .hover(|s| s.bg(rgb(0x30363d)))
                                    .text_size(px(12.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.wrapped_poster = None;
                                        cx.notify();
                                    }))
                                    .child("✕ 关闭"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .p_2()
                            .rounded_lg()
                            .bg(rgb(0x090d13))
                            .flex()
                            .justify_center()
                            .child(
                                img(render_img)
                                    .w(px(360.))
                                    .h(px(480.))
                                    .rounded_md()
                                    .object_fit(ObjectFit::Contain),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x8b949e))
                                    .child("💡 纯本地 GDI + tiny-skia 矢量光栅化 · 零云端上传"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn-copy-poster")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0x21262d))
                                            .border_1()
                                            .border_color(rgb(0x30363d))
                                            .hover(|s| s.bg(rgb(0x30363d)))
                                            .text_size(px(12.))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let clip_img = Image::from_bytes(
                                                    ImageFormat::Png,
                                                    bytes_for_copy.clone(),
                                                );
                                                cx.write_to_clipboard(ClipboardItem::new_image(
                                                    &clip_img,
                                                ));
                                                this.show_toast(
                                                    "🎉 海报图片已复制到剪贴板！可直接粘贴发微信/QQ",
                                                    cx,
                                                );
                                            }))
                                            .child("📋 复制图片"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-save-poster")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(rgb(0xe84142))
                                            .hover(|s| s.bg(rgb(0xff5556)))
                                            .text_color(rgb(0xffffff))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_size(px(12.))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let date_s =
                                                    Local::now().format("%Y%m%d").to_string();
                                                this.save_file_with_dialog(
                                                    format!("MusicHub_听歌名片_{}.png", date_s),
                                                    bytes_for_save.clone(),
                                                    "高清听歌海报 PNG".to_string(),
                                                    cx,
                                                );
                                            }))
                                            .child("💾 另存为高清 PNG"),
                                    ),
                            ),
                    ),
            )
    }
}

#[cfg(target_os = "windows")]
fn get_console_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowThreadProcessId};

    let title = w!("私人音乐中枢 · Music Hub");
    let current_pid = std::process::id();

    unsafe {
        // First try fast top-level lookup
        if let Ok(hwnd) = FindWindowW(None, title) {
            if !hwnd.is_invalid() {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == current_pid {
                    return Some(hwnd);
                }
            }
        }

        // Fallback: iterate top-level windows matching title to avoid collisions
        let mut prev = HWND::default();
        while let Ok(hwnd) = FindWindowExW(None, prev, None, title) {
            if hwnd.is_invalid() {
                break;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == current_pid {
                return Some(hwnd);
            }
            prev = hwnd;
        }
    }
    None
}

pub fn run_console_loop(
    hub: Arc<HubApp>,
    cmd_rx: std::sync::mpsc::Receiver<ConsoleCommand>,
    open_on_start: bool,
) {
    let cmd_rx = Arc::new(std::sync::Mutex::new(cmd_rx));

    Application::new().run(move |cx: &mut App| {
        bind_input_keys(cx);

        let bounds = Bounds::centered(None, size(px(1120.0), px(740.0)), cx);
        let window_opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(900.0), px(600.0))),
            show: open_on_start,
            focus: open_on_start,
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(SharedString::from("私人音乐中枢 · Music Hub")),
                ..Default::default()
            }),
            ..Default::default()
        };

        let hub_for_app = hub.clone();
        let Ok(window_handle) = cx.open_window(window_opts, |window, cx| {
            #[cfg(target_os = "windows")]
            window.on_window_should_close(cx, move |_window, _cx| {
                if let Some(hwnd) = get_console_hwnd() {
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                            hwnd,
                            windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                        );
                    }
                }
                false
            });

            cx.new(|cx| ConsoleView::new(hub_for_app.clone(), NavTab::Radar, None, cx))
        }) else {
            return;
        };

        if open_on_start {
            cx.activate(true);
        } else {
            #[cfg(target_os = "windows")]
            if let Some(hwnd) = get_console_hwnd() {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                    );
                }
            }
        }

        // Spawn background state synchronizer & IPC command listener while window is alive
        let hub_poll = hub.clone();
        let rx_poll = cmd_rx.clone();
        cx.spawn(async move |cx| {
            let mut last_cover_key = String::new();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;

                // Drain any ConsoleCommand::Open sent while window is already open
                let mut focus_req: Option<(Option<String>, Option<String>)> = None;
                if let Ok(guard) = rx_poll.lock() {
                    while let Ok(ConsoleCommand::Open { tab, toast }) = guard.try_recv() {
                        let prev = focus_req.take();
                        focus_req = Some((
                            tab.or_else(|| prev.as_ref().and_then(|p| p.0.clone())),
                            toast.or_else(|| prev.as_ref().and_then(|p| p.1.clone())),
                        ));
                    }
                }

                // Fetch live SMTC state from tokio runtime
                let hub_c = hub_poll.clone();
                let rt = hub_c.rt.clone();
                let prev_key = last_cover_key.clone();
                let (st_tx, st_rx) = tokio::sync::oneshot::channel();
                rt.spawn(async move {
                    let st = hub_c.state().await;
                    let cur_cover = st
                        .now
                        .as_ref()
                        .map(|n| n.cover_key())
                        .unwrap_or_default();
                    let new_img = if cur_cover != prev_key {
                        let decoded = st
                            .now
                            .as_ref()
                            .and_then(|n| n.cover_bytes.as_ref())
                            .and_then(|bytes| decode_bytes_to_render_image(bytes));
                        if decoded.is_some() {
                            decoded
                        } else if let Some(cov) = st.now.as_ref().map(|n| n.cover.as_str()) {
                            fetch_cover_render_image(cov).await
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let _ = st_tx.send((st, cur_cover, new_img));
                });

                let Ok((new_state, cur_cover, new_img)) = st_rx.await else {
                    break;
                };
                let cover_changed = cur_cover != last_cover_key;
                if cover_changed {
                    last_cover_key = cur_cover.clone();
                }

                let update_res = window_handle.update(cx, |view, window, cx| {
                    let state_changed = view.state != new_state;
                    let focus_changed = focus_req.is_some();
                    let needs_notify = state_changed || cover_changed || focus_changed;

                    if state_changed {
                        view.state = new_state;
                    }
                    if cover_changed {
                        view.now_cover_key = cur_cover;
                        view.now_cover_img = new_img;
                    }
                    if let Some((tab_opt, toast_opt)) = focus_req {
                        #[cfg(target_os = "windows")]
                        if let Some(hwnd) = get_console_hwnd() {
                            unsafe {
                                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                                    hwnd,
                                    windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                                );
                                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                            }
                        }
                        window.activate_window();
                        if let Some(t) = tab_opt {
                            view.switch_tab(NavTab::from_str(&t), cx);
                        }
                        if let Some(msg) = toast_opt {
                            view.show_toast(msg, cx);
                        }
                    }
                    if needs_notify {
                        cx.notify();
                    }
                });

                if update_res.is_err() {
                    break;
                }
            }
        })
        .detach();
    });
}
