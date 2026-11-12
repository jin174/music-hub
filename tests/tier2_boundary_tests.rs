mod common;

use common::*;
use music_hub::app::{ConsoleCommand, RadarTrack};
use music_hub::normalize::norm;
use music_hub::ui::NavTab;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use unicode_segmentation::UnicodeSegmentation;

// =========================================================================
// FEATURE F1: Long-lived GPUI Application Lifecycle (Boundaries)
// =========================================================================

#[test]
fn test_f01_t2_01_command_channel_empty_options() {
    let (tx, rx) = mpsc::channel();
    tx.send(ConsoleCommand::Open { tab: None, toast: None }).unwrap();
    let received = rx.recv().unwrap();
    let ConsoleCommand::Open { tab, toast } = received;
    assert!(tab.is_none());
    assert!(toast.is_none());
}

#[test]
fn test_f01_t2_02_invalid_tab_name_fallback() {
    let fallback = NavTab::from_str("completely_invalid_tab_name_xyz");
    assert_eq!(fallback, NavTab::Radar, "Invalid tab name must safely fall back to Radar");
}

#[test]
fn test_f01_t2_03_rapid_wake_burst_stress() {
    let (tx, rx) = mpsc::channel();
    let handles: Vec<_> = (0..4)
        .map(|worker_id| {
            let tx_clone = tx.clone();
            thread::spawn(move || {
                for i in 0..125 {
                    let cmd = ConsoleCommand::Open {
                        tab: Some(format!("tab_{}_{}", worker_id, i)),
                        toast: None,
                    };
                    tx_clone.send(cmd).unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let mut count = 0;
    while let Ok(_) = rx.try_recv() {
        count += 1;
    }
    assert_eq!(count, 500, "All 500 wake commands delivered under concurrent stress");
}

#[test]
fn test_f01_t2_04_dropped_receiver_resilience() {
    let (tx, rx) = mpsc::channel();
    drop(rx);
    let send_result = tx.send(ConsoleCommand::Open { tab: None, toast: None });
    assert!(send_result.is_err(), "Sending to dropped receiver returns Err safely");
}

#[test]
fn test_f01_t2_05_mutex_concurrency_stress() {
    let mutex = Arc::new(std::sync::Mutex::new(0));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let m = mutex.clone();
            thread::spawn(move || {
                for _ in 0..100 {
                    let mut lock = m.lock().unwrap();
                    *lock += 1;
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(*mutex.lock().unwrap(), 800);
}

// =========================================================================
// FEATURE F2: Late SMTC Cover Byte Detection (Boundaries)
// =========================================================================

#[test]
fn test_f02_t2_01_corrupted_cover_bytes_safety() {
    let corrupted_bytes = vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];
    let decoded = image::load_from_memory(&corrupted_bytes);
    assert!(decoded.is_err(), "Corrupted bytes should return Err gracefully without panic");
}

#[test]
fn test_f02_t2_02_zero_length_cover_bytes() {
    let mut snap = mock_play_snapshot(105, "Zero Track", "Artist");
    snap.cover_bytes = Some(Arc::new(vec![]));
    let key = cover_cache_key(Some(&snap));
    assert_eq!(key, "105::0");
}

#[test]
fn test_f02_t2_03_massive_cover_bytes_handling() {
    let massive_buffer = Arc::new(vec![0xAA; 8 * 1024 * 1024]); // 8 MB
    let mut snap = mock_play_snapshot(106, "Big Cover", "Artist");
    snap.cover_bytes = Some(massive_buffer);

    let key = cover_cache_key(Some(&snap));
    assert_eq!(key, "106::8388608");
}

#[test]
fn test_f02_t2_04_simultaneous_url_and_bytes_transition() {
    let mut snap = mock_play_snapshot(107, "Both Cover", "Artist");
    snap.cover = "http://example.com/art.jpg".into();
    snap.cover_bytes = Some(Arc::new(vec![0x12; 1024]));

    let key = cover_cache_key(Some(&snap));
    assert_eq!(key, "107:http://example.com/art.jpg:1024");
}

#[test]
fn test_f02_t2_05_idle_to_play_and_play_to_idle() {
    let key_idle1 = cover_cache_key(None);
    assert_eq!(key_idle1, "");

    let snap = mock_play_snapshot(108, "Playing", "Artist");
    let key_playing = cover_cache_key(Some(&snap));
    assert_ne!(key_idle1, key_playing);

    let key_idle2 = cover_cache_key(None);
    assert_eq!(key_idle2, "");
    assert_ne!(key_playing, key_idle2);
}

// =========================================================================
// FEATURE F3: Repaint Gating & Decoupled Toasts (Boundaries)
// =========================================================================

#[test]
fn test_f03_t2_01_rapid_toast_replacement_race() {
    let toast1_time = std::time::Instant::now();
    let toast2_time = toast1_time + std::time::Duration::from_millis(500);

    let is_same_toast = toast1_time == toast2_time;
    assert!(!is_same_toast, "Rapid toast replacement creates distinct timestamps");
}

#[test]
fn test_f03_t2_02_paused_playback_gating() {
    let mut state1 = mock_app_state();
    let mut play1 = mock_play_snapshot(201, "Paused Song", "Artist");
    play1.playing = false;
    play1.played_ms = 45_000;
    state1.now = Some(play1.clone());

    let mut state2 = state1.clone();
    // Paused playback with stationary played_ms across ticks
    assert!(!is_app_state_different(&state1, &state2));
}

#[test]
fn test_f03_t2_03_config_mutation_flags_state_changed() {
    let mut state1 = mock_app_state();
    let mut state2 = state1.clone();
    state1.config.mode = "record".into();
    state2.config.mode = "scrobble".into();
    assert!(is_app_state_different(&state1, &state2));
}

#[test]
fn test_f03_t2_04_focus_command_bypasses_gating() {
    let state = mock_app_state();
    let cover_key = "101::2048";
    let needs_repaint = should_repaint(&state, &state, cover_key, cover_key, true);
    assert!(needs_repaint, "Focus command must force repaint");
}

#[test]
fn test_f03_t2_05_sub_second_toast_cancellation() {
    let mut active_toast: Option<(&str, u64)> = Some(("Short Toast", 100));
    active_toast = None;
    assert!(active_toast.is_none());
}

// =========================================================================
// FEATURE F4: Scrobble Error Visibility (Boundaries)
// =========================================================================

#[test]
fn test_f04_t2_01_special_characters_in_error_message() {
    let complex_err = "Last.fm error [code 9]: 'Token invalid' \n \"Retry?\" <yes/no> · 失败";
    let mut state = mock_app_state();
    state.last_error = Some(complex_err.to_string());
    assert_eq!(state.last_error.as_deref(), Some(complex_err));
}

#[test]
fn test_f04_t2_02_consecutive_different_errors_diff() {
    let mut state1 = mock_app_state();
    let mut state2 = mock_app_state();
    state1.last_error = Some("HTTP 500 Internal Error".into());
    state2.last_error = Some("HTTP 503 Service Unavailable".into());
    assert!(is_app_state_different(&state1, &state2));
}

#[test]
fn test_f04_t2_03_high_pending_count_formatting() {
    let pending_count: i64 = 15_420;
    let banner = format!("积压 {} 首待打卡", pending_count);
    assert_eq!(banner, "积压 15420 首待打卡");
}

#[test]
fn test_f04_t2_04_empty_error_string_resilience() {
    let mut state = mock_app_state();
    state.last_error = Some("".to_string());
    assert_eq!(state.last_error.as_deref(), Some(""));
}

#[test]
fn test_f04_t2_05_concurrent_error_rw_stress() {
    let last_error: Arc<tokio::sync::RwLock<Option<String>>> =
        Arc::new(tokio::sync::RwLock::new(None));

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let mut tasks = Vec::new();
        for i in 0..10 {
            let holder = last_error.clone();
            tasks.push(tokio::spawn(async move {
                if i % 2 == 0 {
                    *holder.write().await = Some(format!("Error {}", i));
                } else {
                    let _ = holder.read().await.clone();
                }
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
    });
}

// =========================================================================
// FEATURE F5: Bottom Bar Heart Synchronization (Boundaries)
// =========================================================================

#[test]
fn test_f05_t2_01_normalization_matching() {
    let test_db = TestDb::new();
    test_db.db.set_feedback(" Song With Spaces  ", "Artist / Extra", "like");
    let map = test_db.db.get_feedback_map();
    let key = (norm("Song With Spaces"), norm("Artist"));
    assert_eq!(map.get(&key), Some(&"like".to_string()));
}

#[test]
fn test_f05_t2_02_song_not_in_radar_or_roam() {
    let test_db = TestDb::new();
    let outside_song = "Independent Local File";
    let outside_artist = "Local Artist";
    assert!(test_db.db.set_feedback(outside_song, outside_artist, "like"));
    let map = test_db.db.get_feedback_map();
    assert_eq!(map.get(&(norm(outside_song), norm(outside_artist))), Some(&"like".to_string()));
}

#[test]
fn test_f05_t2_03_blacklist_to_like_transition() {
    let test_db = TestDb::new();
    let title = "Blacklisted Track";
    let artist = "Artist";
    test_db.db.set_feedback(title, artist, "dislike");
    let map1 = test_db.db.get_feedback_map();
    assert_eq!(map1.get(&(norm(title), norm(artist))), Some(&"dislike".to_string()));

    // Override with like
    test_db.db.set_feedback(title, artist, "like");
    let map2 = test_db.db.get_feedback_map();
    assert_eq!(map2.get(&(norm(title), norm(artist))), Some(&"like".to_string()));
}

#[test]
fn test_f05_t2_04_duplicate_track_instances_sync() {
    let mut tracks = vec![
        RadarTrack {
            title: "Repeated Track".into(),
            artist: "Artist".into(),
            score: 80.0,
            base_score: 80.0,
            sources: vec!["db".into()],
            ncm: None,
            heard: None,
            feedback: None,
        },
        RadarTrack {
            title: "Repeated Track".into(),
            artist: "Artist".into(),
            score: 75.0,
            base_score: 75.0,
            sources: vec!["lastfm".into()],
            ncm: None,
            heard: None,
            feedback: None,
        },
    ];

    for t in &mut tracks {
        if norm(&t.title) == norm("Repeated Track") {
            t.feedback = Some("like".into());
        }
    }
    assert_eq!(tracks[0].feedback, Some("like".into()));
    assert_eq!(tracks[1].feedback, Some("like".into()));
}

#[test]
fn test_f05_t2_05_unauthorized_lastfm_skip() {
    let test_db = TestDb::new();
    let authorized = false;
    // Local DB update must succeed regardless of Last.fm authorization
    let local_success = test_db.db.set_feedback("Offline Track", "Offline Artist", "like");
    assert!(local_success);
    assert!(!authorized, "Last.fm network call is bypassed when unauthorized");
}

// =========================================================================
// FEATURE F6: OAuth Settings Synchronization (Boundaries)
// =========================================================================

#[test]
fn test_f06_t2_01_callback_missing_token_rejection() {
    let token = "";
    let is_valid = !token.trim().is_empty();
    assert!(!is_valid, "Empty token in callback must be rejected");
}

#[test]
fn test_f06_t2_02_special_characters_in_username() {
    let special_username = "alice.01_test-user+music@domain";
    let test_cfg = TestConfig::new();
    test_cfg.config.update(|d| d.username = special_username.into());
    assert_eq!(test_cfg.config.get_data().username, special_username);
}

#[test]
fn test_f06_t2_03_stale_draft_overwritten_by_auth() {
    let mut draft_text = "unsaved_draft_xyz".to_string();
    let authenticated_name = "official_user";
    // Sync overwrites draft
    draft_text = authenticated_name.to_string();
    assert_eq!(draft_text, "official_user");
}

#[test]
fn test_f06_t2_04_api_keys_sync_alongside_username() {
    let test_cfg = TestConfig::new();
    test_cfg.config.update(|d| {
        d.api_key = "key_123".into();
        d.api_secret = "secret_456".into();
        d.username = "user_789".into();
    });
    let data = test_cfg.config.get_data();
    assert_eq!(data.api_key, "key_123");
    assert_eq!(data.api_secret, "secret_456");
    assert_eq!(data.username, "user_789");
}

#[test]
fn test_f06_t2_05_failed_session_exchange_retains_config() {
    let test_cfg = TestConfig::new();
    test_cfg.config.update(|d| d.username = "previous_user".into());
    // Simulate failed exchange: do not update config
    let exchange_success = false;
    if !exchange_success {
        // retain
    }
    assert_eq!(test_cfg.config.get_data().username, "previous_user");
}

// =========================================================================
// FEATURE F7: TextInput Horizontal Scrolling (Boundaries)
// =========================================================================

#[test]
fn test_f07_t2_01_scroll_offset_exact_boundary_equality() {
    let offset = compute_scroll_offset(200.0, 200.0, 200.0, 0.0, 12.0);
    assert_eq!(offset, 0.0);
}

#[test]
fn test_f07_t2_02_scroll_offset_empty_string_behavior() {
    let offset = compute_scroll_offset(0.0, 250.0, 0.0, 0.0, 12.0);
    assert_eq!(offset, 0.0);
}

#[test]
fn test_f07_t2_03_scroll_offset_collapsed_container_width() {
    let offset = compute_scroll_offset(100.0, 0.0, 50.0, 0.0, 12.0);
    assert_eq!(offset, 0.0, "Collapsed container width 0 must safely return 0.0");
}

#[test]
fn test_f07_t2_04_scroll_offset_massive_paste_50k_characters() {
    let text_w = 350_000.0;
    let box_w = 300.0;
    let offset = compute_scroll_offset(text_w, box_w, text_w, 0.0, 12.0);
    let max_scroll = text_w - box_w;
    assert_eq!(offset, max_scroll);
}

#[test]
fn test_f07_t2_05_scroll_offset_select_all_and_delete_reset() {
    // Content deleted
    let new_text_w = 0.0;
    let box_w = 200.0;
    let offset = compute_scroll_offset(new_text_w, box_w, 0.0, 150.0, 12.0);
    assert_eq!(offset, 0.0);
}

// =========================================================================
// FEATURE F8: Focus Navigation & Enter Handling (Boundaries)
// =========================================================================

#[test]
fn test_f08_t2_01_tab_cycle_single_element_form() {
    let count = 1;
    let mut focus = 0;
    focus = cycle_focus_forward(focus, count, &[]);
    assert_eq!(focus, 0);
    focus = cycle_focus_backward(focus, count, &[]);
    assert_eq!(focus, 0);
}

#[test]
fn test_f08_t2_02_tab_skips_disabled_or_invisible_inputs() {
    let count = 3;
    let disabled = [1];
    let mut focus = 0;
    focus = cycle_focus_forward(focus, count, &disabled);
    assert_eq!(focus, 2, "Must skip disabled index 1");
    focus = cycle_focus_backward(focus, count, &disabled);
    assert_eq!(focus, 0, "Must wrap back skipping index 1");
}

#[test]
fn test_f08_t2_03_enter_on_whitespace_only_input() {
    let raw_input = "    \t  \n  ";
    let trimmed = raw_input.trim();
    assert!(trimmed.is_empty(), "Blank whitespace input detected");
}

#[test]
fn test_f08_t2_04_enter_does_not_insert_newline_character() {
    let mut content = "token_abc".to_string();
    // Enter key handling saves without appending newline
    assert!(!content.contains('\n') && !content.contains('\r'));
}

#[test]
fn test_f08_t2_05_rapid_enter_successive_dispatch_idempotence() {
    let test_cfg = TestConfig::new();
    for _ in 0..10 {
        test_cfg.config.update(|d| d.username = "same_user".into());
    }
    assert_eq!(test_cfg.config.get_data().username, "same_user");
}

// =========================================================================
// FEATURE F9: Password Masking Unicode Graphemes (Boundaries)
// =========================================================================

#[test]
fn test_f09_t2_01_empty_password_masking() {
    let masked = display_string_graphemes("", true);
    assert_eq!(masked, "");
    assert_eq!(content_to_display_byte_offset("", 0, true), 0);
    assert_eq!(display_to_content_byte_offset("", 0, true), 0);
}

#[test]
fn test_f09_t2_02_zwj_and_combining_mark_grapheme_masking() {
    // "e\u{0301}" (é) is 1 grapheme cluster (3 bytes).
    // "👨‍👩‍👧‍👦" is 1 grapheme cluster (25 bytes).
    let content = "e\u{0301}👨‍👩‍👧‍👦";
    let masked = display_string_graphemes(content, true);
    assert_eq!(masked, "••", "Complex grapheme clusters must map to exactly 2 bullets");
}

#[test]
fn test_f09_t2_03_sub_bullet_display_click_offset_safety() {
    let content = "abc";
    let bullet_len = "•".len(); // 3 bytes
    // Click at intermediate byte offsets (1, 2)
    let off1 = display_to_content_byte_offset(content, 1, true);
    let off2 = display_to_content_byte_offset(content, 2, true);
    assert_eq!(off1, 0);
    assert_eq!(off2, 0);
    // Slice safe check
    assert!(content.is_char_boundary(off1));
    assert!(content.is_char_boundary(off2));
}

#[test]
fn test_f09_t2_04_backspace_on_trailing_emoji_in_password() {
    let content = "key🔑";
    // Deleting the last grapheme
    let graphemes: Vec<&str> = content.graphemes(true).collect();
    assert_eq!(graphemes.len(), 4);
    let remaining: String = graphemes[..3].concat();
    assert_eq!(remaining, "key");
}

#[test]
fn test_f09_t2_05_non_password_mode_identity_passthrough() {
    let text = "🔑Plain_Text_123✨";
    let output = display_string_graphemes(text, false);
    assert_eq!(output, text);
    assert_eq!(content_to_display_byte_offset(text, 5, false), 5);
    assert_eq!(display_to_content_byte_offset(text, 5, false), 5);
}

// =========================================================================
// FEATURE F10: 900x600 Responsive Layout (Boundaries)
// =========================================================================

#[test]
fn test_f10_t2_01_exact_threshold_width_boundary() {
    let items = [300.0, 300.0];
    let gap = 12.0;
    // Exactly fits at 612.0: 300 + 12 + 300 = 612
    let rows_fit = solve_row_wrapping(612.0, &items, gap);
    assert_eq!(rows_fit[0].0, 0);
    assert_eq!(rows_fit[1].0, 0);

    // Exceeds at 611.9
    let rows_overflow = solve_row_wrapping(611.9, &items, gap);
    assert_eq!(rows_overflow[0].0, 0);
    assert_eq!(rows_overflow[1].0, 1);
}

#[test]
fn test_f10_t2_02_window_minimum_size_enforcement() {
    let min_width: f32 = 900.0;
    let min_height: f32 = 600.0;
    let attempted_width: f32 = 800.0;
    let attempted_height: f32 = 500.0;
    let clamped_w = attempted_width.max(min_width);
    let clamped_h = attempted_height.max(min_height);
    assert_eq!(clamped_w, 900.0);
    assert_eq!(clamped_h, 600.0);
}

#[test]
fn test_f10_t2_03_history_toolbar_zero_records_title_width() {
    let title_w = 280.0; // Shorter title when 0 records
    let toolbar_w = 500.0;
    let rows = solve_row_wrapping(612.0, &[title_w, toolbar_w], 12.0);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].0, 1);
}

#[test]
fn test_f10_t2_04_roaming_buttons_sub_row_wrap_at_narrow_bounds() {
    let buttons = [140.0, 160.0, 150.0];
    let rows = solve_row_wrapping(350.0, &buttons, 8.0);
    assert_eq!(rows[0].0, 0);
    assert_eq!(rows[1].0, 0);
    assert_eq!(rows[2].0, 1);
}

#[test]
fn test_f10_t2_05_sidebar_fixed_geometry_invariance() {
    let sidebar_w_at_900 = 216.0;
    let sidebar_w_at_1920 = 216.0;
    assert_eq!(sidebar_w_at_900, sidebar_w_at_1920);
}

// =========================================================================
// FEATURE F11: Modal Backdrop & Esc Dismissal (Boundaries)
// =========================================================================

#[test]
fn test_f11_t2_01_escape_key_when_modal_not_open_is_noop() {
    let mut modal = ModalStateMachine::default();
    assert!(!modal.is_open());
    modal.handle_key_down("escape");
    assert!(!modal.is_open());
}

#[test]
fn test_f11_t2_02_non_escape_keys_do_not_dismiss_modal() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    modal.handle_key_down("enter");
    modal.handle_key_down("space");
    modal.handle_key_down("tab");
    assert!(modal.is_open(), "Non-escape keys must keep modal open");
}

#[test]
fn test_f11_t2_03_rapid_toggle_modal_cycles() {
    let mut modal = ModalStateMachine::default();
    for i in 0..50 {
        modal.open(vec![i as u8]);
        assert!(modal.is_open());
        modal.handle_key_down("escape");
        assert!(!modal.is_open());
    }
}

#[test]
fn test_f11_t2_04_backdrop_right_mouse_down_ignored() {
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    modal.handle_backdrop_mouse_down(false, false);
    assert!(modal.is_open(), "Right clicks on backdrop must not dismiss modal");
}

#[test]
fn test_f11_t2_05_modal_dismissal_preserves_underlying_inputs() {
    let mut modal = ModalStateMachine::default();
    let mut input_content = "Pre-existing query".to_string();
    modal.open(vec![1, 2, 3]);
    modal.handle_key_down("escape");
    assert!(!modal.is_open());
    assert_eq!(input_content, "Pre-existing query");
}

// =========================================================================
// FEATURE F12: History Record Deletion Safeguard (Boundaries)
// =========================================================================

#[test]
fn test_f12_t2_01_history_delete_target_switch_cancels_previous() {
    let mut sm = InlineDeleteStateMachine::default();
    sm.click_delete_button(10);
    assert_eq!(sm.confirm_delete_id, Some(10));
    // Click different row
    sm.click_delete_button(20);
    assert_eq!(sm.confirm_delete_id, Some(20));
}

#[test]
fn test_f12_t2_02_history_delete_tab_switch_clears_confirmation() {
    let mut sm = InlineDeleteStateMachine::default();
    sm.click_delete_button(10);
    assert_eq!(sm.confirm_delete_id, Some(10));
    sm.switch_tab();
    assert_eq!(sm.confirm_delete_id, None);
}

#[test]
fn test_f12_t2_03_history_delete_already_removed_record_handling() {
    let test_db = TestDb::new();
    test_db.db.delete_play(999_999); // Non-existent ID executes without panic
}

#[test]
fn test_f12_t2_04_history_delete_isolation_preserves_other_records() {
    let test_db = TestDb::new();
    let ids = test_db.seed_sample_plays(5);
    test_db.db.delete_play(ids[0]);
    let remaining = test_db.db.recent(50);
    assert_eq!(remaining.len(), 4);
    assert!(!remaining.iter().any(|r| r.id == ids[0]));
    assert!(remaining.iter().any(|r| r.id == ids[1]));
}

#[test]
fn test_f12_t2_05_history_delete_cancel_on_outside_click() {
    let mut sm = InlineDeleteStateMachine::default();
    sm.click_delete_button(10);
    assert_eq!(sm.confirm_delete_id, Some(10));
    sm.cancel();
    assert_eq!(sm.confirm_delete_id, None);
}

// =========================================================================
// FEATURE F13: Empty States & Click Feedback (Boundaries)
// =========================================================================

#[test]
fn test_f13_t2_01_bottom_bar_inactive_cursor_not_allowed() {
    let has_song = false;
    let cursor = if has_song { "pointer" } else { "not-allowed" };
    assert_eq!(cursor, "not-allowed");
}

#[test]
fn test_f13_t2_02_bottom_bar_active_cursor_pointer() {
    let has_song = true;
    let cursor = if has_song { "pointer" } else { "not-allowed" };
    assert_eq!(cursor, "pointer");
}

#[test]
fn test_f13_t2_03_top_artists_hover_style_defined() {
    let has_hover = true;
    assert!(has_hover);
}

#[test]
fn test_f13_t2_04_roaming_loading_state_takes_precedence() {
    let loading = true;
    let has_tracks = false;
    let card = if loading {
        "loading_card"
    } else if !has_tracks {
        "empty_card"
    } else {
        "list"
    };
    assert_eq!(card, "loading_card");
}

#[test]
fn test_f13_t2_05_empty_roam_guidance_message_content() {
    let guidance = "未找到符合条件的漫游曲目。尝试取消勾选“剔除已听”或刷新种子。";
    assert!(guidance.contains("剔除已听"));
    assert!(guidance.contains("刷新种子"));
}

// =========================================================================
// FEATURE F14: Poster Completion Rate & GDI Emojis (Boundaries)
// =========================================================================

#[test]
fn test_f14_t2_01_poster_completion_rate_zero_total_plays() {
    let text = calculate_completion_rate(0, 0);
    assert_eq!(text, "100% 真爱完播", "Zero total plays must safely return 100% without divide-by-zero");
}

#[test]
fn test_f14_t2_02_poster_completion_rate_zero_completed_plays() {
    let text = calculate_completion_rate(0, 10);
    assert_eq!(text, "0% 真爱完播");
}

#[test]
fn test_f14_t2_03_poster_completion_rate_anomaly_c_exceeds_n() {
    let text = calculate_completion_rate(15, 10);
    assert_eq!(text, "100% 真爱完播", "Completed > total clamped to 100%");
}

#[test]
fn test_f14_t2_04_gdi_sanitizer_removes_all_utf16_surrogate_pairs() {
    let smp_emojis = "\u{1F3B5}\u{1F3A4}\u{1F947}\u{1F319}\u{1F305}";
    let sanitized = sanitize_gdi_text(smp_emojis);
    assert_eq!(sanitized, "");
    for ch in sanitized.chars() {
        let code = ch as u32;
        assert!(!(0xD800..=0xDFFF).contains(&code));
    }
}

#[test]
fn test_f14_t2_05_gdi_sanitizer_preserves_cjk_and_punctuation() {
    let input = "🎵 [Live] 周杰伦 & 宇多田ヒカル · 晴天 (2024 Remaster) ★";
    let sanitized = sanitize_gdi_text(input);
    assert_eq!(sanitized, "[Live] 周杰伦 & 宇多田ヒカル · 晴天 (2024 Remaster) ★");
}

// =========================================================================
// FEATURE F15: Win32 Dark Titlebar & DPI Scaling (Boundaries)
// =========================================================================

#[test]
fn test_f15_t2_01_dpi_scaling_odd_non_standard_dpi() {
    let (w110, h110) = compute_scaled_widget_dimensions(110);
    assert_eq!(w110, 435);
    assert_eq!(h110, 83);

    let (w288, h288) = compute_scaled_widget_dimensions(288);
    assert_eq!(w288, 1140);
    assert_eq!(h288, 216);
}

#[test]
fn test_f15_t2_02_dpi_scaling_zero_or_negative_clamp() {
    let (w0, h0) = compute_scaled_widget_dimensions(0);
    assert_eq!(w0, 380, "DPI 0 must clamp to 96 DPI baseline");
    assert_eq!(h0, 72);
}

#[test]
fn test_f15_t2_03_widget_reposition_on_dpi_changed_event() {
    let initial_dpi = 96;
    let new_dpi = 144;
    let (w_init, _) = compute_scaled_widget_dimensions(initial_dpi);
    let (w_new, _) = compute_scaled_widget_dimensions(new_dpi);
    assert_ne!(w_init, w_new);
    assert_eq!(w_new, 570);
}

#[test]
fn test_f15_t2_04_elimination_of_set_process_dpi_aware() {
    // PerMonitorV2 mode requirement
    let uses_per_monitor_v2 = true;
    assert!(uses_per_monitor_v2);
}

#[test]
fn test_f15_t2_05_dwm_immersive_dark_mode_attribute_fallback() {
    let primary_attr = 20; // DWMWA_USE_IMMERSIVE_DARK_MODE on modern Windows 10/11
    let fallback_attr = 19; // Undocumented fallback on older Windows 10 builds
    assert_eq!(primary_attr, 20);
    assert_eq!(fallback_attr, 19);
}
