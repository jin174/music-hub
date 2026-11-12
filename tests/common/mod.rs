#![allow(dead_code)]

use music_hub::app::AppStateResponse;
use music_hub::config::{Config, PublicConfig};
use music_hub::db::{now_sec, DB};
use music_hub::monitor::PlaySnapshot;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_segmentation::UnicodeSegmentation;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Fixture for an isolated in-memory SQLite database.
pub struct TestDb {
    pub db: DB,
}

impl TestDb {
    pub fn new() -> Self {
        let db = DB::open_in_memory().expect("Failed to initialize in-memory SQLite DB");
        Self { db }
    }

    pub fn seed_sample_plays(&self, count: usize) -> Vec<i64> {
        let mut ids = Vec::new();
        for i in 1..=count {
            let title = format!("Song {}", i);
            let artist = format!("Artist {}", (i % 3) + 1);
            let id = self.db.start_play(&title, &artist, "spotify");
            self.db.update_play_fields(
                id,
                Some(&artist),
                Some(&artist),
                Some("Test Album"),
                Some(1000 + i as i64),
                Some("http://example.com/cover.jpg"),
                Some(200_000),
                Some(180_000),
                Some(1),
                Some(0),
                Some("done"),
                Some(now_sec()),
            );
            ids.push(id);
        }
        ids
    }
}

/// Fixture for isolated temporary directory configuration.
pub struct TestConfig {
    pub config: Config,
    pub dir: PathBuf,
}

impl TestConfig {
    pub fn new() -> Self {
        let num = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("music_hub_test_{}_{}", ts, num));
        let _ = fs::create_dir_all(&dir);
        let config = Config::load_or_create(Some(&dir));
        Self { config, dir }
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// =========================================================================
// Specification Algorithm Models (authoritative requirements from specs)
// =========================================================================

/// F2: SMTC Cover Change Detection Key
/// Key format incorporates play_id, cover url, and cover_bytes length.
pub fn cover_cache_key(now: Option<&PlaySnapshot>) -> String {
    now.as_ref()
        .map(|n| {
            let byte_len = n.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0);
            format!("{}:{}:{}", n.play_id, n.cover, byte_len)
        })
        .unwrap_or_default()
}

/// F3: Repaint Gating Decision Model
pub fn should_repaint(
    prev_state: &AppStateResponse,
    cur_state: &AppStateResponse,
    prev_cover_key: &str,
    cur_cover_key: &str,
    focus_requested: bool,
) -> bool {
    let state_changed = is_app_state_different(prev_state, cur_state);
    let cover_changed = prev_cover_key != cur_cover_key;
    state_changed || cover_changed || focus_requested
}

pub fn is_app_state_different(a: &AppStateResponse, b: &AppStateResponse) -> bool {
    // Structural diff checking key UI-affecting properties
    if a.pending != b.pending {
        return true;
    }
    if a.last_error != b.last_error {
        return true;
    }
    if a.scrobbling != b.scrobbling {
        return true;
    }
    if a.session_app != b.session_app {
        return true;
    }
    if a.config.authorized != b.config.authorized
        || a.config.username != b.config.username
        || a.config.mode != b.config.mode
    {
        return true;
    }
    match (&a.now, &b.now) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(p1), Some(p2)) => {
            p1.play_id != p2.play_id
                || p1.played_ms != p2.played_ms
                || p1.playing != p2.playing
                || p1.scrobble_state != p2.scrobble_state
                || p1.title != p2.title
                || p1.artist != p2.artist
        }
    }
}

/// F7: TextInput Horizontal Scroll Offset Clamping Algorithm
pub fn compute_scroll_offset(
    content_width: f32,
    container_width: f32,
    cursor_pos: f32,
    current_scroll: f32,
    margin: f32,
) -> f32 {
    if container_width <= 0.0 || content_width <= container_width {
        return 0.0;
    }
    let max_scroll = (content_width - container_width).max(0.0);
    let right_limit = current_scroll + container_width - margin;
    let left_limit = current_scroll + margin;

    if cursor_pos > right_limit {
        (cursor_pos - container_width + margin).min(max_scroll)
    } else if cursor_pos < left_limit {
        (cursor_pos - margin).max(0.0)
    } else {
        current_scroll.min(max_scroll)
    }
}

/// F7: Mouse click coordinate mapping with horizontal scroll offset
pub fn index_for_mouse_position_with_offset(
    bounds_left: f32,
    click_x: f32,
    scroll_offset: f32,
    char_width: f32,
    max_len: usize,
) -> usize {
    if char_width <= 0.0 {
        return 0;
    }
    let rel_x = (click_x - bounds_left).max(0.0) + scroll_offset;
    let idx = (rel_x / char_width).round() as usize;
    idx.min(max_len)
}

/// F8: Form Focus Cycle Model
pub fn cycle_focus_forward(current_index: usize, count: usize, disabled_indices: &[usize]) -> usize {
    if count == 0 {
        return 0;
    }
    let mut next = (current_index + 1) % count;
    for _ in 0..count {
        if !disabled_indices.contains(&next) {
            return next;
        }
        next = (next + 1) % count;
    }
    current_index
}

pub fn cycle_focus_backward(current_index: usize, count: usize, disabled_indices: &[usize]) -> usize {
    if count == 0 {
        return 0;
    }
    let mut prev = if current_index == 0 {
        count - 1
    } else {
        current_index - 1
    };
    for _ in 0..count {
        if !disabled_indices.contains(&prev) {
            return prev;
        }
        prev = if prev == 0 { count - 1 } else { prev - 1 };
    }
    current_index
}

/// F9: Unicode Grapheme Password Masking
pub fn display_string_graphemes(content: &str, is_password: bool) -> String {
    if !is_password || content.is_empty() {
        return content.to_string();
    }
    let grapheme_count = content.graphemes(true).count();
    "•".repeat(grapheme_count)
}

/// F9: Map byte offset in content to byte offset in masked display string
pub fn content_to_display_byte_offset(
    content: &str,
    content_offset: usize,
    is_password: bool,
) -> usize {
    if !is_password || content.is_empty() {
        return content_offset.min(content.len());
    }
    let safe_offset = content_offset.min(content.len());
    // Count graphemes up to safe_offset
    let mut grapheme_idx = 0;
    for (start, g) in content.grapheme_indices(true) {
        if start >= safe_offset {
            break;
        }
        grapheme_idx += 1;
        if start + g.len() > safe_offset {
            break;
        }
    }
    grapheme_idx * "•".len()
}

/// F9: Map byte offset in masked display string back to char-aligned byte offset in content
pub fn display_to_content_byte_offset(
    content: &str,
    display_offset: usize,
    is_password: bool,
) -> usize {
    if !is_password || content.is_empty() {
        return display_offset.min(content.len());
    }
    let bullet_len = "•".len();
    let target_grapheme = display_offset / bullet_len;
    let mut graphemes = content.grapheme_indices(true);
    let mut last_byte_offset = 0;
    for _ in 0..target_grapheme {
        if let Some((offset, g)) = graphemes.next() {
            last_byte_offset = offset + g.len();
        } else {
            return content.len();
        }
    }
    last_byte_offset.min(content.len())
}

/// F10: Responsive Layout Row Wrapping Engine
pub fn solve_row_wrapping(
    container_width: f32,
    item_widths: &[f32],
    gap: f32,
) -> Vec<(usize, f32)> {
    let mut rows = Vec::new();
    let mut current_row = 0;
    let mut current_x = 0.0;

    for &w in item_widths {
        if current_x > 0.0 && current_x + w > container_width {
            current_row += 1;
            current_x = 0.0;
        }
        rows.push((current_row, current_x));
        current_x += w + gap;
    }
    rows
}

/// F11: Modal Backdrop & Esc Dismissal State Machine
#[derive(Debug, Clone, Default)]
pub struct ModalStateMachine {
    pub poster_bytes: Option<Vec<u8>>,
}

impl ModalStateMachine {
    pub fn open(&mut self, bytes: Vec<u8>) {
        self.poster_bytes = Some(bytes);
    }

    pub fn handle_backdrop_mouse_down(&mut self, is_left_button: bool, is_inside_dialog: bool) {
        if is_left_button && !is_inside_dialog {
            self.poster_bytes = None;
        }
    }

    pub fn handle_key_down(&mut self, key: &str) {
        if key == "escape" {
            self.poster_bytes = None;
        }
    }

    pub fn handle_close_button(&mut self) {
        self.poster_bytes = None;
    }

    pub fn is_open(&self) -> bool {
        self.poster_bytes.is_some()
    }
}

/// F12: History Two-Step Inline Delete State Machine
#[derive(Debug, Clone, Default)]
pub struct InlineDeleteStateMachine {
    pub confirm_delete_id: Option<i64>,
}

impl InlineDeleteStateMachine {
    pub fn click_delete_button(&mut self, id: i64) -> bool {
        match self.confirm_delete_id {
            Some(existing) if existing == id => {
                // Second click on the same item executes delete!
                self.confirm_delete_id = None;
                true
            }
            _ => {
                // First click sets confirmation state
                self.confirm_delete_id = Some(id);
                false
            }
        }
    }

    pub fn cancel(&mut self) {
        self.confirm_delete_id = None;
    }

    pub fn switch_tab(&mut self) {
        self.confirm_delete_id = None;
    }
}

/// F14: Poster Completion Rate Calculation Formula
pub fn calculate_completion_rate(completed: i64, total: i64) -> String {
    if total <= 0 {
        return "100% 真爱完播".to_string();
    }
    let c = completed.max(0).min(total);
    let rate = ((c as f64 / total as f64) * 100.0).round() as i64;
    format!("{}% 真爱完播", rate)
}

/// F14: GDI Text Sanitizer eliminating surrogate pairs and SMP emojis
pub fn sanitize_gdi_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let code = ch as u32;
        // Strip SMP astral emoji characters (U+1F000..=U+1FFFF) and surrogate bounds
        if (0x1F000..=0x1FFFF).contains(&code) || (0xD800..=0xDFFF).contains(&code) {
            continue;
        }
        out.push(ch);
    }
    // Clean up double spaces if any emoji stripped
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// F15: Dynamic Island DPI Geometry Resolver
pub const BASE_WIDGET_WIDTH: f32 = 380.0;
pub const BASE_WIDGET_HEIGHT: f32 = 72.0;

pub fn compute_scaled_widget_dimensions(dpi: u32) -> (u32, u32) {
    let effective_dpi = dpi.max(96) as f32;
    let scale = effective_dpi / 96.0;
    (
        (BASE_WIDGET_WIDTH * scale).round() as u32,
        (BASE_WIDGET_HEIGHT * scale).round() as u32,
    )
}

/// F15: Win32 DWM Color Mask Converter
/// Converts hex RGB 0xRRGGBB into Win32 COLORREF format 0x00BBGGRR
pub fn colorref_from_hex(rgb: u32) -> u32 {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    (b << 16) | (g << 8) | r
}

/// Helper to construct a default AppStateResponse
pub fn mock_app_state() -> AppStateResponse {
    AppStateResponse {
        now: None,
        config: PublicConfig {
            api_key_set: false,
            api_secret_set: false,
            authorized: false,
            username: String::new(),
            mode: "record".into(),
            port: 8765,
            apps: vec!["spotify".into()],
            scrobble_unmatched: false,
        },
        scrobbling: false,
        pending: 0,
        last_error: None,
        last_ok: None,
        session_app: None,
    }
}

/// Helper to construct a mock PlaySnapshot
pub fn mock_play_snapshot(play_id: i64, title: &str, artist: &str) -> PlaySnapshot {
    PlaySnapshot {
        play_id,
        title: title.to_string(),
        artist: artist.to_string(),
        artists: vec![artist.to_string()],
        album: "Sample Album".to_string(),
        cover: String::new(),
        cover_bytes: None,
        ncm_id: None,
        duration_ms: 240_000,
        played_ms: 0,
        threshold_ms: Some(120_000),
        threshold_reached: false,
        scrobble_state: "none".to_string(),
        playing: true,
        lookup: String::new(),
        started_at: now_sec(),
        app: "spotify".to_string(),
    }
}
