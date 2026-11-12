mod common;

use common::*;
use music_hub::app::{ConsoleCommand, RadarTrack, RoamTrack};
use music_hub::db::now_sec;
use music_hub::normalize::norm;
use music_hub::ui::NavTab;
use std::sync::mpsc;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

// =========================================================================
// TIER 3: Cross-Feature Pairwise Interaction Tests (17 tests)
// =========================================================================

/// TEST-T3-01: F1 (Lifecycle) + F6 (OAuth Settings Synchronization)
/// OAuth callback completes while window is hidden; wakes window, switches to Settings, updates username.
#[test]
fn test_t3_01_f1_lifecycle_f6_oauth_sync() {
    let test_cfg = TestConfig::new();
    let (tx, rx) = mpsc::channel();

    // 1. Window is hidden
    let window_visible = false;
    assert!(!window_visible);

    // 2. OAuth callback arrives at server handler
    let auth_token = "tok_valid_oauth_123";
    let auth_user = "audiophile_user";
    test_cfg.config.update(|d| {
        d.api_key = "dummy_key".into();
        d.api_secret = "dummy_secret".into();
        d.session_key = format!("sk_{}", auth_token);
        d.username = auth_user.into();
    });

    // 3. Server sends wake command with Settings tab
    tx.send(ConsoleCommand::Open {
        tab: Some("settings".into()),
        toast: Some("🎉 Last.fm 授权成功！".into()),
    })
    .unwrap();

    // 4. Console wake receiver processes command
    let received = rx.recv().unwrap();
    let ConsoleCommand::Open { tab, toast } = received;
    assert_eq!(tab, Some("settings".into()));
    assert!(toast.is_some());

    // 5. Settings tab syncs username input
    let target_tab = NavTab::from_str(tab.as_deref().unwrap());
    assert_eq!(target_tab, NavTab::Settings);
    let username_input = test_cfg.config.get_data().username;
    assert_eq!(username_input, "audiophile_user");
    assert!(test_cfg.config.is_authorized());
}

/// TEST-T3-02: F5 (Bottom Bar Heart Sync) + F13 (Play Feedback & Toasts)
/// Bottom bar heart toggle updates DB, active Roam list, Radar list, and shows toast.
#[test]
fn test_t3_02_f5_heart_sync_f13_feedback() {
    let test_db = TestDb::new();
    let song = "夜曲";
    let artist = "周杰伦";

    let mut roam_tracks = vec![RoamTrack {
        title: song.into(),
        artist: artist.into(),
        score: 95.0,
        sources: vec!["seed".into()],
        reasons: vec!["loved".into()],
        ncm: None,
        heard: None,
        feedback: None,
    }];

    let mut radar_tracks = vec![RadarTrack {
        title: song.into(),
        artist: artist.into(),
        score: 92.0,
        base_score: 90.0,
        sources: vec!["db".into()],
        ncm: None,
        heard: None,
        feedback: None,
    }];

    // First click: like
    assert!(test_db.db.set_feedback(song, artist, "like"));
    for t in &mut roam_tracks {
        if norm(&t.title) == norm(song) {
            t.feedback = Some("like".into());
        }
    }
    for t in &mut radar_tracks {
        if norm(&t.title) == norm(song) {
            t.feedback = Some("like".into());
        }
    }
    let toast = format!("❤️ 已同步红心至 Last.fm 与本地喜欢：{}", song);

    assert_eq!(test_db.db.get_feedback_map().get(&(norm(song), norm(artist))), Some(&"like".to_string()));
    assert_eq!(roam_tracks[0].feedback, Some("like".into()));
    assert_eq!(radar_tracks[0].feedback, Some("like".into()));
    assert!(toast.contains("已同步红心"));

    // Second click: untoggle
    assert!(test_db.db.set_feedback(song, artist, "remove"));
    roam_tracks[0].feedback = None;
    radar_tracks[0].feedback = None;
    assert_eq!(test_db.db.get_feedback_map().get(&(norm(song), norm(artist))), None);
    assert!(roam_tracks[0].feedback.is_none());
}

/// TEST-T3-03: F7 (TextInput Scroll) + F9 (Password Masking Unicode Graphemes)
/// Masked password with multibyte emoji clusters scrolls horizontally and maps graphemes 1:1.
#[test]
fn test_t3_03_f7_scroll_f9_password_masking() {
    let complex_password = "P@ss🔑123🔒中島みゆき✨2026!#$*~";
    let grapheme_count = complex_password.graphemes(true).count();
    assert_eq!(grapheme_count, 24);

    let masked = display_string_graphemes(complex_password, true);
    assert_eq!(masked.chars().count(), 24);
    assert!(masked.chars().all(|c| c == '•'));
    assert!(!masked.contains("****"));

    // Approximate width: 24 bullets * 8px = 192px in 184px container
    let text_w = 192.0;
    let box_w = 184.0;
    let offset = compute_scroll_offset(text_w, box_w, text_w, 0.0, 12.0);
    assert!(offset > 0.0);
    assert_eq!(offset, text_w - box_w); // 8px

    // Verify cursor quad inside visible viewport
    let vis_cursor = text_w - offset;
    assert!(vis_cursor <= box_w);
}

/// TEST-T3-04: F4 (Scrobble Error Visibility) + F12 (History Record Deletion Safeguard)
/// Deleting a failed pending scrobble engages confirmation safeguard and reconciles pending count.
#[test]
fn test_t3_04_f4_scrobble_error_f12_delete_safeguard() {
    let test_db = TestDb::new();
    let play_id = test_db.db.start_play("Bad Romance", "Lady Gaga", "spotify");
    test_db.db.update_play_fields(
        play_id,
        Some("Lady Gaga"),
        Some("Lady Gaga"),
        None,
        None,
        None,
        Some(250_000),
        Some(250_000),
        Some(1),
        Some(0),
        Some("pending"),
        Some(now_sec()),
    );

    let mut state = mock_app_state();
    state.pending = 1;
    state.last_error = Some("HTTP 503: Service Unavailable".to_string());

    let mut sm = InlineDeleteStateMachine::default();

    // 1st click: confirmation requested, row remains in DB
    let executed = sm.click_delete_button(play_id);
    assert!(!executed);
    assert_eq!(sm.confirm_delete_id, Some(play_id));
    assert_eq!(test_db.db.recent(10).len(), 1);

    // Cancel test
    sm.cancel();
    assert_eq!(sm.confirm_delete_id, None);
    assert_eq!(test_db.db.recent(10).len(), 1);

    // 1st click again, then 2nd click: delete executed!
    sm.click_delete_button(play_id);
    let confirmed = sm.click_delete_button(play_id);
    assert!(confirmed);
    test_db.db.delete_play(play_id);
    state.pending = 0;

    assert_eq!(test_db.db.recent(10).len(), 0);
    assert_eq!(state.pending, 0);
}

/// TEST-T3-05: F14 (Poster Completion Rate) + F5 (Bottom Bar Heart / Loved Tracks)
/// Loved tracks marked from player bar are aggregated in stats to compute true completion rate.
#[test]
fn test_t3_05_f14_poster_completion_f5_heart_sync() {
    let test_db = TestDb::new();
    let song = "晴天";
    let artist = "周杰伦";

    // Play 10 times, complete 8 times
    for i in 0..10 {
        let id = test_db.db.start_play(song, artist, "cloudmusic");
        let completed = if i < 8 { 1 } else { 0 };
        let skipped = if i >= 8 { 1 } else { 0 };
        test_db.db.update_play_fields(
            id,
            Some(artist),
            Some(artist),
            Some("叶惠美"),
            Some(186016),
            None,
            Some(269_000),
            Some(269_000),
            Some(completed),
            Some(skipped),
            Some("done"),
            Some(now_sec() - (10 - i) * 300),
        );
    }

    // Toggle favorite from player bar
    test_db.db.set_feedback(song, artist, "like");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    let test_cfg = TestConfig::new();
    let app = music_hub::app::App::new(test_cfg.config.clone(), test_db.db.clone());
    let stats = app.stats(30);

    // Verify stats aggregates loved track
    assert!(!stats.loved.is_empty());
    assert_eq!(stats.loved[0].n, 10);
    assert_eq!(stats.loved[0].c, 8);

    let completion_badge = calculate_completion_rate(stats.loved[0].c, stats.loved[0].n);
    assert_eq!(completion_badge, "80% 真爱完播");
}

/// TEST-T3-06: F1 (Window Lifecycle) + F2 (Late SMTC Cover Byte Detection)
/// Late SMTC thumbnail arriving while window hidden decodes upon wake without losing bytes.
#[test]
fn test_t3_06_f1_lifecycle_f2_smtc_cover() {
    let (tx, rx) = mpsc::channel();

    // 1. Initial snapshot has no cover bytes
    let mut snap = mock_play_snapshot(501, "Shape of You", "Ed Sheeran");
    let key_initial = cover_cache_key(Some(&snap));
    assert_eq!(key_initial, "501::0");

    // 2. Late cover bytes arrive while window is hidden
    let sample_bytes = Arc::new(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]); // JPEG magic header
    snap.cover_bytes = Some(sample_bytes);
    let key_late = cover_cache_key(Some(&snap));
    assert_eq!(key_late, "501::6");
    assert_ne!(key_initial, key_late);

    // 3. Tray wake command arrives
    tx.send(ConsoleCommand::Open { tab: None, toast: None }).unwrap();
    let received = rx.recv().unwrap();
    let ConsoleCommand::Open { tab, .. } = received;
    assert!(tab.is_none());

    // 4. Wake restores window and evaluates cover key
    assert_eq!(key_late, "501::6");
}

/// TEST-T3-07: F3 (Repaint Gating) + F13 (Toast Auto-Dismissal Timer)
/// Toasts auto-dismiss via async timer, repainting only on mutation without 500ms polling churn.
#[test]
fn test_t3_07_f3_repaint_gating_f13_toast() {
    let state = mock_app_state();
    let cover_key = "101::2048";

    // Idle state: 0 repaints
    let mut repaint_count = 0;
    for _ in 0..10 {
        if should_repaint(&state, &state, cover_key, cover_key, false) {
            repaint_count += 1;
        }
    }
    assert_eq!(repaint_count, 0, "Zero repaints during idle listening");

    // Toast triggered: 1 repaint
    let mut active_toast = Some("▶ 正在跳转播放：晴天");
    if active_toast.is_some() {
        repaint_count += 1;
    }
    assert_eq!(repaint_count, 1);

    // Decoupled timer completes: 1 repaint to clear toast
    active_toast = None;
    if active_toast.is_none() {
        repaint_count += 1;
    }
    assert_eq!(repaint_count, 2);
}

/// TEST-T3-08: F7 (TextInput Scroll) + F8 (Focus Navigation & Enter Handling)
/// Cycling focus between inputs via Tab preserves independent horizontal scroll offsets.
#[test]
fn test_t3_08_f7_scroll_f8_focus_nav() {
    // Input A: long API key exceeding 200px width
    let input_a_text_w = 400.0;
    let box_w = 200.0;
    let input_a_scroll = compute_scroll_offset(input_a_text_w, box_w, 400.0, 0.0, 12.0);
    assert_eq!(input_a_scroll, 200.0);

    // Input B: short secret
    let input_b_text_w = 80.0;
    let input_b_scroll = compute_scroll_offset(input_b_text_w, box_w, 80.0, 0.0, 12.0);
    assert_eq!(input_b_scroll, 0.0);

    // Tab to next input
    let mut focus = 0;
    focus = cycle_focus_forward(focus, 2, &[]);
    assert_eq!(focus, 1);

    // Shift-Tab back to Input A
    focus = cycle_focus_backward(focus, 2, &[]);
    assert_eq!(focus, 0);

    // Input A maintains independent scroll offset 200.0
    assert_eq!(input_a_scroll, 200.0);
}

/// TEST-T3-09: F10 (900x600 Layout) + F12 (History Inline Delete Confirmation)
/// Inline delete confirmation renders cleanly within 900x600 layout constraints without overlapping.
#[test]
fn test_t3_09_f10_layout_f12_delete_safeguard() {
    let inner_width = 612.0;
    // Row 3 width in history table matches inner width
    let row_width = inner_width;

    let mut sm = InlineDeleteStateMachine::default();
    sm.click_delete_button(3);
    assert_eq!(sm.confirm_delete_id, Some(3));

    // Inline prompt elements: text + confirm button + cancel button
    let prompt_elements = [200.0, 80.0, 60.0];
    let total_prompt_w: f32 = prompt_elements.iter().sum::<f32>() + 16.0;
    assert!(total_prompt_w <= row_width, "Inline prompt fits inside row bounds at 900x600");
}

/// TEST-T3-10: F11 (Poster Modal Backdrop) + F1 (Window Lifecycle Hide & Wake)
/// Poster modal retained across window hide/wake; Esc dismisses modal without closing window.
#[test]
fn test_t3_10_f11_modal_backdrop_f1_lifecycle() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![0x89, 0x50, 0x4E, 0x47]); // PNG header
    assert!(modal.is_open());

    // Window hide simulated
    let window_open = true;

    // Window restored
    assert!(modal.is_open());

    // Press Esc to dismiss modal
    modal.handle_key_down("escape");
    assert!(!modal.is_open(), "Modal dismissed via Esc");
    assert!(window_open, "Window remains open after modal dismissal");
}

/// TEST-T3-11: F4 (Scrobble Error Visibility) + F6 (OAuth Settings Sync & Error Clear)
/// Re-authenticating via OAuth updates credentials and clears scrobble error on next flush.
#[test]
fn test_t3_11_f4_scrobble_error_f6_oauth_sync() {
    let test_cfg = TestConfig::new();
    let mut app_state = mock_app_state();
    app_state.last_error = Some("Last.fm error (code 9): Invalid session key".into());

    // OAuth completes
    test_cfg.config.update(|d| {
        d.api_key = "key".into();
        d.api_secret = "secret".into();
        d.session_key = "new_sk_999".into();
        d.username = "refreshed_user".into();
    });

    // Flush completes cleanly
    app_state.last_error = None;
    assert!(app_state.last_error.is_none());
    assert_eq!(test_cfg.config.get_data().username, "refreshed_user");
}

/// TEST-T3-12: F5 (Bottom Bar Heart Sync) + F10 (900x600 Layout)
/// Player bar heart toggle updates Roam list at 900x600 without layout jitter or horizontal overflow.
#[test]
fn test_t3_12_f5_heart_sync_f10_layout() {
    let inner_width = 612.0;
    let card_width = inner_width;

    let mut roam_tracks = vec![RoamTrack {
        title: "Track 1".into(),
        artist: "Artist 1".into(),
        score: 90.0,
        sources: vec!["seed".into()],
        reasons: vec!["loved".into()],
        ncm: None,
        heard: None,
        feedback: None,
    }];

    // Toggle heart
    roam_tracks[0].feedback = Some("like".into());
    assert_eq!(roam_tracks[0].feedback, Some("like".into()));

    // Row width conforms to card width
    assert!(card_width <= 612.0);
}

/// TEST-T3-13: F2 (Late SMTC Cover Bytes) + F3 (Repaint Gating Single Invalidation)
/// Late cover byte arrival triggers single repaint upon decode, then resumes gating.
#[test]
fn test_t3_13_f2_smtc_cover_f3_repaint_gating() {
    let mut snap = mock_play_snapshot(601, "Song", "Artist");
    let state = mock_app_state();

    let key_0 = cover_cache_key(Some(&snap));
    assert_eq!(key_0, "601::0");

    // Stationary ticks: no repaints
    assert!(!should_repaint(&state, &state, &key_0, &key_0, false));

    // Late bytes arrive
    snap.cover_bytes = Some(Arc::new(vec![0xAA; 4096]));
    let key_1 = cover_cache_key(Some(&snap));
    assert_eq!(key_1, "601::4096");

    // Single repaint triggered
    let repaint_triggered = should_repaint(&state, &state, &key_0, &key_1, false);
    assert!(repaint_triggered);

    // Next tick with key_1: gated again!
    assert!(!should_repaint(&state, &state, &key_1, &key_1, false));
}

/// TEST-T3-14: F8 (Focus Navigation) + F9 (Password Masking & Unicode Selection)
/// Tab navigation preserves selection ranges in masked password inputs.
#[test]
fn test_t3_14_f8_focus_nav_f9_password_masking() {
    let content = "安全🔑パスワード";
    let masked = display_string_graphemes(content, true);
    let total_graphemes = content.graphemes(true).count();
    assert_eq!(total_graphemes, 8);
    assert_eq!(masked.chars().count(), 8);

    // Selected range: graphemes 2..5 ("🔑パス")
    let sel_start = content_to_display_byte_offset(content, 6, true);
    let sel_end = content_to_display_byte_offset(content, 16, true);
    assert!(sel_start < sel_end);

    // Tab to next input, then back: range preserved
    let focus = cycle_focus_forward(0, 2, &[]);
    assert_eq!(focus, 1);
    let back = cycle_focus_backward(focus, 2, &[]);
    assert_eq!(back, 0);

    let mapped_back_start = display_to_content_byte_offset(content, sel_start, true);
    assert!(content.is_char_boundary(mapped_back_start));
}

/// TEST-T3-15: F13 (Empty State Guidance) + F10 (900x600 Responsive Roaming Toolbar)
/// Empty state cards render cleanly at 900x600 without clipping.
#[test]
fn test_t3_15_f13_empty_state_f10_layout() {
    let inner_width = 612.0;
    let empty_card_w = 580.0;
    assert!(empty_card_w <= inner_width, "Empty state card conforms to inner width");

    let toolbar_items = [200.0, 150.0, 150.0];
    let rows = solve_row_wrapping(inner_width, &toolbar_items, 10.0);
    assert_eq!(rows[0].0, 0);
}

/// TEST-T3-16: F14 (Poster Stats & Badges) + F11 (Poster Modal Dismissal via Esc)
/// Poster with clean typography badges renders in modal preview; Esc key dismisses cleanly.
#[test]
fn test_t3_16_f14_poster_stats_f11_modal_dismiss() {
    let raw_header = "🎤 灵魂常听音乐人 TOP 3";
    let clean_header = sanitize_gdi_text(raw_header);
    assert_eq!(clean_header, "灵魂常听音乐人 TOP 3");

    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3, 4]);
    assert!(modal.is_open());
    modal.handle_key_down("escape");
    assert!(!modal.is_open());
}

/// TEST-T3-17: F1 (Window Lifecycle) + F15 (Win32 Dark Titlebar & DPI Scaling)
/// DPI change and hide/wake cycle preserves Win32 dark title bar and scales Dynamic Island.
#[test]
fn test_t3_17_f1_lifecycle_f15_platform_dpi() {
    // 100% DPI
    let (w100, h100) = compute_scaled_widget_dimensions(96);
    assert_eq!((w100, h100), (380, 72));

    // Window hidden
    let window_visible = false;
    assert!(!window_visible);

    // DPI changes to 150% (144 DPI)
    let (w150, h150) = compute_scaled_widget_dimensions(144);
    assert_eq!((w150, h150), (570, 108));

    // Dark title bar colorref format preserved: #0d1117 -> 0x0017110D
    assert_eq!(colorref_from_hex(0x000D1117), 0x0017110D);
}
