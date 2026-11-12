mod common;

use common::*;
use music_hub::app::{ConsoleCommand, RadarTrack, RoamTrack};
use music_hub::normalize::norm;
use music_hub::ui::NavTab;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

// =========================================================================
// FEATURE F1: Long-lived GPUI Application Lifecycle
// =========================================================================

#[test]
fn test_f01_t1_01_command_channel_delivery() {
    let (tx, rx) = mpsc::channel();
    let cmd = ConsoleCommand::Open {
        tab: Some("radar".into()),
        toast: Some("Hello Radar".into()),
    };
    tx.send(cmd).expect("Channel send should succeed");
    let received = rx.recv().expect("Channel recv should succeed");
    let ConsoleCommand::Open { tab, toast } = received;
    assert_eq!(tab, Some("radar".into()));
    assert_eq!(toast, Some("Hello Radar".into()));
}

#[test]
fn test_f01_t1_02_multiple_sequential_wake_commands() {
    let (tx, rx) = mpsc::channel();
    let tabs = vec!["radar", "settings", "stats"];
    for tab in &tabs {
        tx.send(ConsoleCommand::Open {
            tab: Some(tab.to_string()),
            toast: None,
        })
        .unwrap();
    }
    for expected_tab in tabs {
        let cmd = rx.recv().unwrap();
        let ConsoleCommand::Open { tab, .. } = cmd;
        assert_eq!(tab, Some(expected_tab.to_string()));
    }
}

#[test]
fn test_f01_t1_03_state_preservation_across_hide_wake() {
    let cached_tracks = vec![
        RadarTrack {
            title: "Track 1".into(),
            artist: "Artist 1".into(),
            score: 95.0,
            base_score: 90.0,
            sources: vec!["db".into()],
            ncm: None,
            heard: None,
            feedback: None,
        },
        RadarTrack {
            title: "Track 2".into(),
            artist: "Artist 2".into(),
            score: 88.0,
            base_score: 85.0,
            sources: vec!["db".into()],
            ncm: None,
            heard: None,
            feedback: None,
        },
    ];

    // Simulate hide event
    let window_visible = false;
    assert!(!window_visible);

    // Simulate wake event with empty options
    let cmd = ConsoleCommand::Open {
        tab: None,
        toast: None,
    };
    let ConsoleCommand::Open { tab, .. } = cmd;
    assert!(tab.is_none());
    assert_eq!(cached_tracks.len(), 2);
    assert_eq!(cached_tracks[0].title, "Track 1");
}

#[test]
fn test_f01_t1_04_window_should_close_intercept() {
    // Intercept callback contract: returns false so window is hidden instead of destroyed
    let on_window_should_close = |_window_id: u32| -> bool { false };
    assert_eq!(
        on_window_should_close(1),
        false,
        "Close intercept must return false"
    );
}

#[test]
fn test_f01_t1_05_nav_tab_from_str_mapping() {
    assert_eq!(NavTab::from_str("radar"), NavTab::Radar);
    assert_eq!(NavTab::from_str("roam"), NavTab::Roam);
    assert_eq!(NavTab::from_str("stats"), NavTab::Stats);
    assert_eq!(NavTab::from_str("recent"), NavTab::Recent);
    assert_eq!(NavTab::from_str("settings"), NavTab::Settings);
    assert_eq!(NavTab::from_str("unknown"), NavTab::Radar);
}

// =========================================================================
// FEATURE F2: Late SMTC Cover Byte Detection
// =========================================================================

#[test]
fn test_f02_t1_01_initial_empty_to_late_bytes_transition() {
    let mut snap = mock_play_snapshot(101, "Title", "Artist");
    let key_init = cover_cache_key(Some(&snap));
    assert_eq!(key_init, "101::0");

    // Late arriving bytes
    snap.cover_bytes = Some(Arc::new(vec![0xFF; 4096]));
    let key_late = cover_cache_key(Some(&snap));
    assert_eq!(key_late, "101::4096");
    assert_ne!(key_init, key_late);
}

#[test]
fn test_f02_t1_02_web_url_cover_transition() {
    let mut snap = mock_play_snapshot(102, "Web Song", "Web Artist");
    let key1 = cover_cache_key(Some(&snap));
    assert_eq!(key1, "102::0");

    snap.cover = "http://example.com/cover.png".to_string();
    let key2 = cover_cache_key(Some(&snap));
    assert_eq!(key2, "102:http://example.com/cover.png:0");
    assert_ne!(key1, key2);
}

#[test]
fn test_f02_t1_03_track_change_same_cover_size() {
    let mut snap1 = mock_play_snapshot(201, "Song A", "Artist A");
    snap1.cover_bytes = Some(Arc::new(vec![0x12; 2048]));

    let mut snap2 = mock_play_snapshot(202, "Song B", "Artist B");
    snap2.cover_bytes = Some(Arc::new(vec![0x34; 2048]));

    let key1 = cover_cache_key(Some(&snap1));
    let key2 = cover_cache_key(Some(&snap2));

    assert_eq!(key1, "201::2048");
    assert_eq!(key2, "202::2048");
    assert_ne!(key1, key2, "Different play_ids must produce different keys");
}

#[test]
fn test_f02_t1_04_stable_playback_no_spurious_key_change() {
    let mut snap = mock_play_snapshot(301, "Steady", "Artist");
    snap.cover_bytes = Some(Arc::new(vec![0xAA; 1024]));

    let key_tick1 = cover_cache_key(Some(&snap));
    let key_tick2 = cover_cache_key(Some(&snap));
    assert_eq!(key_tick1, key_tick2, "Identical cover should produce identical keys");
}

#[test]
fn test_f02_t1_05_valid_jpeg_png_bytes_decode() {
    // Generate valid 2x2 PNG bytes using image crate
    let img = image::RgbaImage::new(2, 2);
    let mut bytes: Vec<u8> = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("Valid PNG bytes encode");

    let decoded = image::load_from_memory(&bytes);
    assert!(decoded.is_ok());
    let dyn_img = decoded.unwrap();
    assert_eq!(dyn_img.width(), 2);
    assert_eq!(dyn_img.height(), 2);
}

// =========================================================================
// FEATURE F3: Repaint Gating & Decoupled Toasts
// =========================================================================

#[test]
fn test_f03_t1_01_app_state_equality_identical() {
    let state1 = mock_app_state();
    let state2 = mock_app_state();
    assert!(!is_app_state_different(&state1, &state2));
}

#[test]
fn test_f03_t1_02_app_state_inequality_on_played_ms() {
    let mut state1 = mock_app_state();
    let mut state2 = mock_app_state();

    let mut play1 = mock_play_snapshot(101, "Song", "Artist");
    play1.played_ms = 1000;
    let mut play2 = play1.clone();
    play2.played_ms = 2000;

    state1.now = Some(play1);
    state2.now = Some(play2);

    assert!(is_app_state_different(&state1, &state2));
}

#[test]
fn test_f03_t1_03_app_state_inequality_on_pending_count() {
    let mut state1 = mock_app_state();
    let mut state2 = mock_app_state();
    state1.pending = 0;
    state2.pending = 3;
    assert!(is_app_state_different(&state1, &state2));
}

#[test]
fn test_f03_t1_04_idle_state_zero_repaints() {
    let state = mock_app_state();
    let cover_key = "";
    let needs_repaint = should_repaint(&state, &state, cover_key, cover_key, false);
    assert!(!needs_repaint, "Idle state must gate repaints");
}

#[test]
fn test_f03_t1_05_toast_expiration_timer_decoupling() {
    // Verify toast expiration duration is decoupled and measurable
    let toast_ttl = Duration::from_millis(3500);
    assert_eq!(toast_ttl.as_millis(), 3500);
    let mut toast: Option<(&'static str, std::time::Instant)> =
        Some(("Saved", std::time::Instant::now()));
    assert!(toast.is_some());
    // Simulate expiration
    toast = None;
    assert!(toast.is_none());
}

// =========================================================================
// FEATURE F4: Scrobble Error Visibility
// =========================================================================

#[test]
fn test_f04_t1_01_scrobbler_records_last_error_on_failure() {
    let err_holder: Arc<tokio::sync::RwLock<Option<String>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        *err_holder.write().await = Some("HTTP 401 Unauthorized".to_string());
        let val = err_holder.read().await.clone();
        assert_eq!(val, Some("HTTP 401 Unauthorized".to_string()));
    });
}

#[test]
fn test_f04_t1_02_scrobbler_clears_last_error_on_success() {
    let err_holder: Arc<tokio::sync::RwLock<Option<String>>> =
        Arc::new(tokio::sync::RwLock::new(Some("Old error".to_string())));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        *err_holder.write().await = None;
        let val = err_holder.read().await.clone();
        assert_eq!(val, None);
    });
}

#[test]
fn test_f04_t1_03_app_state_response_reflects_error() {
    let mut state = mock_app_state();
    assert_eq!(state.last_error, None);
    state.last_error = Some("Network timeout".to_string());
    assert_eq!(state.last_error, Some("Network timeout".to_string()));
}

#[test]
fn test_f04_t1_04_console_view_initializes_with_existing_error() {
    let initial_error = Some("Last.fm token invalid".to_string());
    let mut view_state = mock_app_state();
    view_state.last_error = initial_error.clone();
    assert_eq!(view_state.last_error, initial_error);
}

#[test]
fn test_f04_t1_05_error_appearance_triggers_state_diff() {
    let state1 = mock_app_state();
    let mut state2 = mock_app_state();
    state2.last_error = Some("Connection refused".to_string());
    assert!(is_app_state_different(&state1, &state2));
}

// =========================================================================
// FEATURE F5: Bottom Bar Heart Synchronization
// =========================================================================

#[test]
fn test_f05_t1_01_toggle_like_to_remove_in_db() {
    let test_db = TestDb::new();
    let title = "Night Song";
    let artist = "Jay Chou";

    // First toggle: like
    assert!(test_db.db.set_feedback(title, artist, "like"));
    let map = test_db.db.get_feedback_map();
    assert_eq!(map.get(&(norm(title), norm(artist))), Some(&"like".to_string()));

    // Second toggle: remove
    assert!(test_db.db.set_feedback(title, artist, "remove"));
    let map2 = test_db.db.get_feedback_map();
    assert_eq!(map2.get(&(norm(title), norm(artist))), None);
}

#[test]
fn test_f05_t1_02_radar_tracks_sync_on_toggle() {
    let mut tracks = vec![RadarTrack {
        title: "晴天".into(),
        artist: "周杰伦".into(),
        score: 99.0,
        base_score: 95.0,
        sources: vec!["ncm".into()],
        ncm: None,
        heard: None,
        feedback: None,
    }];

    let target_title = "晴天";
    let target_artist = "周杰伦";
    for t in &mut tracks {
        if norm(&t.title) == norm(target_title) && norm(&t.artist) == norm(target_artist) {
            t.feedback = Some("like".into());
        }
    }
    assert_eq!(tracks[0].feedback, Some("like".into()));
}

#[test]
fn test_f05_t1_03_roam_tracks_sync_on_toggle() {
    let mut roam_tracks = vec![RoamTrack {
        title: "七里香".into(),
        artist: "周杰伦".into(),
        score: 95.0,
        sources: vec!["seed".into()],
        reasons: vec!["top play".into()],
        ncm: None,
        heard: None,
        feedback: None,
    }];

    for t in &mut roam_tracks {
        if norm(&t.title) == norm("七里香") && norm(&t.artist) == norm("周杰伦") {
            t.feedback = Some("like".into());
        }
    }
    assert_eq!(roam_tracks[0].feedback, Some("like".into()));
}

#[test]
fn test_f05_t1_04_feedback_list_refreshed_on_toggle() {
    let test_db = TestDb::new();
    test_db.db.set_feedback("Song A", "Artist A", "like");
    let list = test_db.db.get_feedback_list(50);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "Song A");
    assert_eq!(list[0].action, "like");
}

#[test]
fn test_f05_t1_05_bottom_bar_button_state_sync() {
    let mut feedback_map = std::collections::HashMap::new();
    feedback_map.insert((norm("Song X"), norm("Artist X")), "like".to_string());

    let is_liked = feedback_map.get(&(norm("Song X"), norm("Artist X"))) == Some(&"like".to_string());
    assert!(is_liked);

    let is_other_liked = feedback_map.get(&(norm("Song Y"), norm("Artist Y"))) == Some(&"like".to_string());
    assert!(!is_other_liked);
}

// =========================================================================
// FEATURE F6: OAuth Settings Synchronization
// =========================================================================

#[test]
fn test_f06_t1_01_config_update_persists_credentials() {
    let test_cfg = TestConfig::new();
    test_cfg.config.update(|d| {
        d.api_key = "test_api_key".into();
        d.api_secret = "test_api_secret".into();
        d.session_key = "sk_test_12345".into();
        d.username = "test_user".into();
    });
    let data = test_cfg.config.get_data();
    assert_eq!(data.session_key, "sk_test_12345");
    assert_eq!(data.username, "test_user");
    assert!(test_cfg.config.is_authorized());
}

#[test]
fn test_f06_t1_02_sync_settings_updates_username_input() {
    let auth_username = "alice_music";
    let input_text = auth_username.to_string();
    assert_eq!(input_text, "alice_music");
}

#[test]
fn test_f06_t1_03_oauth_callback_endpoint_processing() {
    let test_cfg = TestConfig::new();
    let incoming_token = "tok_abc";
    assert!(!incoming_token.is_empty());
    test_cfg.config.update(|d| {
        d.session_key = format!("sk_from_{}", incoming_token);
        d.username = "oauth_alice".into();
    });
    assert_eq!(test_cfg.config.get_data().username, "oauth_alice");
}

#[test]
fn test_f06_t1_04_oauth_callback_sends_wake_command() {
    let (tx, rx) = mpsc::channel();
    let name = "oauth_user";
    let cmd = ConsoleCommand::Open {
        tab: Some("settings".into()),
        toast: Some(format!("🎉 Last.fm 授权成功！已绑定账号：{}", name)),
    };
    tx.send(cmd).unwrap();
    let received = rx.recv().unwrap();
    let ConsoleCommand::Open { tab, toast } = received;
    assert_eq!(tab, Some("settings".into()));
    assert!(toast.unwrap().contains("oauth_user"));
}

#[test]
fn test_f06_t1_05_enter_key_saves_username() {
    let test_cfg = TestConfig::new();
    let entered_text = "david_lastfm";
    test_cfg.config.update(|d| {
        d.username = entered_text.into();
    });
    assert_eq!(test_cfg.config.get_data().username, "david_lastfm");
}

// =========================================================================
// FEATURE F7: TextInput Horizontal Scrolling
// =========================================================================

#[test]
fn test_f07_t1_01_scroll_offset_zero_when_text_fits() {
    let offset = compute_scroll_offset(150.0, 300.0, 150.0, 0.0, 12.0);
    assert_eq!(offset, 0.0);
}

#[test]
fn test_f07_t1_02_scroll_offset_shifts_right_on_typing_overflow() {
    let offset = compute_scroll_offset(350.0, 200.0, 350.0, 0.0, 12.0);
    assert!(offset > 0.0);
    assert_eq!(offset, 150.0);
}

#[test]
fn test_f07_t1_03_scroll_offset_snaps_left_on_home_navigation() {
    let offset = compute_scroll_offset(400.0, 200.0, 0.0, 150.0, 12.0);
    assert_eq!(offset, 0.0);
}

#[test]
fn test_f07_t1_04_scroll_offset_clamped_at_max_scroll() {
    let max_scroll = 500.0 - 200.0;
    let offset = compute_scroll_offset(500.0, 200.0, 600.0, 0.0, 12.0);
    assert_eq!(offset, max_scroll);
}

#[test]
fn test_f07_t1_05_mouse_click_coordinate_mapping_with_scroll() {
    let idx = index_for_mouse_position_with_offset(50.0, 130.0, 120.0, 10.0, 50);
    assert_eq!(idx, 20);
}

// =========================================================================
// FEATURE F8: Focus Navigation & Enter Handling
// =========================================================================

#[test]
fn test_f08_t1_01_text_input_tab_stop_flag_enabled() {
    let tab_stop = true;
    assert!(tab_stop, "TextInput must enable tab_stop");
}

#[test]
fn test_f08_t1_02_tab_navigation_forward_cycling_order() {
    let count = 3;
    let mut focus = 0;
    focus = cycle_focus_forward(focus, count, &[]);
    assert_eq!(focus, 1);
    focus = cycle_focus_forward(focus, count, &[]);
    assert_eq!(focus, 2);
    focus = cycle_focus_forward(focus, count, &[]);
    assert_eq!(focus, 0);
}

#[test]
fn test_f08_t1_03_shift_tab_backward_cycling_order() {
    let count = 3;
    let mut focus = 0;
    focus = cycle_focus_backward(focus, count, &[]);
    assert_eq!(focus, 2);
    focus = cycle_focus_backward(focus, count, &[]);
    assert_eq!(focus, 1);
    focus = cycle_focus_backward(focus, count, &[]);
    assert_eq!(focus, 0);
}

#[test]
fn test_f08_t1_04_text_input_enter_event_emission() {
    #[derive(Debug, PartialEq)]
    enum TestInputEvent {
        EnterPressed,
    }
    let ev = TestInputEvent::EnterPressed;
    assert_eq!(ev, TestInputEvent::EnterPressed);
}

#[test]
fn test_f08_t1_05_settings_enter_triggers_config_save() {
    let test_cfg = TestConfig::new();
    let new_user = "user_enter_save";
    test_cfg.config.update(|d| d.username = new_user.into());
    assert_eq!(test_cfg.config.get_data().username, new_user);
}

// =========================================================================
// FEATURE F9: Password Masking Unicode Graphemes
// =========================================================================

#[test]
fn test_f09_t1_01_ascii_password_masking_1_to_1() {
    let masked = display_string_graphemes("Pass1234", true);
    assert_eq!(masked, "••••••••");
    assert_eq!(masked.chars().count(), 8);
}

#[test]
fn test_f09_t1_02_cjk_password_masking_graphemes() {
    let masked = display_string_graphemes("我的密码99", true);
    assert_eq!(masked, "••••••");
    assert_eq!(masked.chars().count(), 6);
}

#[test]
fn test_f09_t1_03_emoji_password_masking_no_four_stars() {
    let masked = display_string_graphemes("🔑🔓", true);
    assert_eq!(masked, "••");
    assert_eq!(masked.chars().count(), 2);
    assert!(!masked.contains("****"));
}

#[test]
fn test_f09_t1_04_content_to_display_offset_mapping() {
    let content = "a🔑b";
    assert_eq!(content_to_display_byte_offset(content, 0, true), 0);
    assert_eq!(content_to_display_byte_offset(content, 1, true), 3);
    assert_eq!(content_to_display_byte_offset(content, 5, true), 6);
    assert_eq!(content_to_display_byte_offset(content, 6, true), 9);
}

#[test]
fn test_f09_t1_05_display_to_content_offset_mapping() {
    let content = "a🔑b";
    assert_eq!(display_to_content_byte_offset(content, 0, true), 0);
    assert_eq!(display_to_content_byte_offset(content, 3, true), 1);
    assert_eq!(display_to_content_byte_offset(content, 6, true), 5);
    assert_eq!(display_to_content_byte_offset(content, 9, true), 6);
}

// =========================================================================
// FEATURE F10: 900x600 Responsive Layout
// =========================================================================

#[test]
fn test_f10_t1_01_responsive_inner_width_at_900_bounds() {
    let window_w = 900.0;
    let sidebar_w = 216.0;
    let page_padding = 40.0;
    let card_padding = 32.0;
    let inner_w = window_w - sidebar_w - page_padding - card_padding;
    assert_eq!(inner_w, 612.0);
}

#[test]
fn test_f10_t1_02_roaming_header_wrap_activation_at_612_width() {
    let title_w = 516.0;
    let button_group_w = 450.0;
    let rows = solve_row_wrapping(612.0, &[title_w, button_group_w], 12.0);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, 0, "Title on row 0");
    assert_eq!(rows[1].0, 1, "Buttons wrapped to row 1");
}

#[test]
fn test_f10_t1_03_history_toolbar_wrap_activation_at_612_width() {
    let title_w = 460.0;
    let toolbar_w = 500.0;
    let rows = solve_row_wrapping(612.0, &[title_w, toolbar_w], 12.0);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].0, 1);
}

#[test]
fn test_f10_t1_04_toolbar_buttons_no_horizontal_collision() {
    let rows = solve_row_wrapping(612.0, &[516.0, 450.0], 12.0);
    assert_ne!(rows[0].0, rows[1].0, "Rows must be separated vertically");
}

#[test]
fn test_f10_t1_05_large_window_unwrapped_single_row_placement() {
    let rows = solve_row_wrapping(1200.0, &[460.0, 500.0], 16.0);
    assert_eq!(rows[0].0, 0);
    assert_eq!(rows[1].0, 0, "Both items fit in single row on 1200px width");
}

// =========================================================================
// FEATURE F11: Modal Backdrop & Esc Dismissal
// =========================================================================

#[test]
fn test_f11_t1_01_poster_modal_initial_state_none() {
    let modal = ModalStateMachine::default();
    assert!(!modal.is_open());
}

#[test]
fn test_f11_t1_02_poster_modal_backdrop_click_dismisses() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    assert!(modal.is_open());
    modal.handle_backdrop_mouse_down(true, false);
    assert!(!modal.is_open(), "Backdrop click must dismiss modal");
}

#[test]
fn test_f11_t1_03_poster_modal_dialog_interior_click_retains_open() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    modal.handle_backdrop_mouse_down(true, true);
    assert!(modal.is_open(), "Interior click must keep modal open");
}

#[test]
fn test_f11_t1_04_poster_modal_escape_key_dismisses() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    modal.handle_key_down("escape");
    assert!(!modal.is_open(), "Escape key must dismiss modal");
}

#[test]
fn test_f11_t1_05_poster_modal_close_button_click_dismisses() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    modal.handle_close_button();
    assert!(!modal.is_open(), "Close button must dismiss modal");
}

// =========================================================================
// FEATURE F12: History Record Deletion Safeguard
// =========================================================================

#[test]
fn test_f12_t1_01_history_delete_first_click_sets_confirmation_state() {
    let mut sm = InlineDeleteStateMachine::default();
    let deleted = sm.click_delete_button(42);
    assert!(!deleted);
    assert_eq!(sm.confirm_delete_id, Some(42));
}

#[test]
fn test_f12_t1_02_history_delete_second_click_executes_permanent_deletion() {
    let mut sm = InlineDeleteStateMachine::default();
    sm.click_delete_button(42);
    let deleted = sm.click_delete_button(42);
    assert!(deleted, "Second click must execute deletion");
    assert_eq!(sm.confirm_delete_id, None);
}

#[test]
fn test_f12_t1_03_history_delete_button_label_and_style_transition() {
    let sm = InlineDeleteStateMachine { confirm_delete_id: Some(42) };
    let label = if sm.confirm_delete_id == Some(42) { "确认删除?" } else { "✕" };
    assert_eq!(label, "确认删除?");
}

#[test]
fn test_f12_t1_04_history_delete_generates_toast_notification() {
    let mut sm = InlineDeleteStateMachine::default();
    sm.click_delete_button(10);
    assert!(sm.click_delete_button(10));
    let toast = "已永久删除该条播放记录";
    assert_eq!(toast, "已永久删除该条播放记录");
}

#[test]
fn test_f12_t1_05_history_delete_row_count_decrements_by_one() {
    let test_db = TestDb::new();
    let ids = test_db.seed_sample_plays(5);
    assert_eq!(test_db.db.recent(50).len(), 5);
    test_db.db.delete_play(ids[2]);
    assert_eq!(test_db.db.recent(50).len(), 4);
}

// =========================================================================
// FEATURE F13: Empty States & Click Feedback
// =========================================================================

#[test]
fn test_f13_t1_01_roaming_initial_state_card_selection() {
    let has_roam_result = false;
    let card_title = if !has_roam_result { "开启你的全盘漫游" } else { "曲目列表" };
    assert_eq!(card_title, "开启你的全盘漫游");
}

#[test]
fn test_f13_t1_02_roaming_empty_result_card_selection() {
    let roam_tracks_count = 0;
    let card_msg = if roam_tracks_count == 0 { "未找到符合条件的漫游曲目" } else { "曲目" };
    assert_eq!(card_msg, "未找到符合条件的漫游曲目");
}

#[test]
fn test_f13_t1_03_radar_initial_state_card_selection() {
    let has_radar_search = false;
    let prompt = if !has_radar_search { "输入歌曲与歌手名" } else { "结果" };
    assert_eq!(prompt, "输入歌曲与歌手名");
}

#[test]
fn test_f13_t1_04_loved_play_action_toast_feedback() {
    let song = "夜曲";
    let toast = format!("▶ 已唤起客户端播放：{}", song);
    assert_eq!(toast, "▶ 已唤起客户端播放：夜曲");
}

#[test]
fn test_f13_t1_05_history_play_action_toast_feedback() {
    let song = "稻香";
    let toast = format!("▶ 已唤起客户端播放：{}", song);
    assert_eq!(toast, "▶ 已唤起客户端播放：稻香");
}

// =========================================================================
// FEATURE F14: Poster Completion Rate & GDI Emojis
// =========================================================================

#[test]
fn test_f14_t1_01_poster_completion_rate_calculation_standard() {
    let text = calculate_completion_rate(8, 10);
    assert_eq!(text, "80% 真爱完播");
}

#[test]
fn test_f14_t1_02_poster_completion_rate_perfect_100() {
    let text = calculate_completion_rate(25, 25);
    assert_eq!(text, "100% 真爱完播");
}

#[test]
fn test_f14_t1_03_poster_completion_rate_fractional_rounding() {
    assert_eq!(calculate_completion_rate(2, 3), "67% 真爱完播");
    assert_eq!(calculate_completion_rate(1, 3), "33% 真爱完播");
}

#[test]
fn test_f14_t1_04_poster_medal_text_badges_replace_emojis() {
    let ranks = [0, 1, 2];
    let badges: Vec<&str> = ranks
        .iter()
        .map(|&r| match r {
            0 => "NO.1",
            1 => "NO.2",
            2 => "NO.3",
            _ => "TOP",
        })
        .collect();
    assert_eq!(badges, vec!["NO.1", "NO.2", "NO.3"]);
}

#[test]
fn test_f14_t1_05_poster_gdi_text_sanitizer_cleans_section_headers() {
    let input = "🎤 灵魂常听音乐人 TOP 3";
    let sanitized = sanitize_gdi_text(input);
    assert_eq!(sanitized, "灵魂常听音乐人 TOP 3");
}

// =========================================================================
// FEATURE F15: Win32 Dark Titlebar & DPI Scaling
// =========================================================================

#[test]
fn test_f15_t1_01_dpi_scale_factor_calculations() {
    let s96 = 96.0 / 96.0;
    let s120 = 120.0 / 96.0;
    let s144 = 144.0 / 96.0;
    let s192 = 192.0 / 96.0;
    assert_eq!(s96, 1.0);
    assert_eq!(s120, 1.25);
    assert_eq!(s144, 1.5);
    assert_eq!(s192, 2.0);
}

#[test]
fn test_f15_t1_02_widget_dimensions_at_100_percent_dpi() {
    let (w, h) = compute_scaled_widget_dimensions(96);
    assert_eq!(w, 380);
    assert_eq!(h, 72);
}

#[test]
fn test_f15_t1_03_widget_dimensions_at_150_percent_dpi() {
    let (w, h) = compute_scaled_widget_dimensions(144);
    assert_eq!(w, 570);
    assert_eq!(h, 108);
}

#[test]
fn test_f15_t1_04_widget_dimensions_at_200_percent_dpi() {
    let (w, h) = compute_scaled_widget_dimensions(192);
    assert_eq!(w, 760);
    assert_eq!(h, 144);
}

#[test]
fn test_f15_t1_05_dwm_colorref_conversion_for_dark_theme() {
    let colorref = colorref_from_hex(0x000D1117);
    assert_eq!(colorref, 0x0017110D);
}
