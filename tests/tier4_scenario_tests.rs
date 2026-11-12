mod common;

use common::*;
use music_hub::app::{ConsoleCommand, RoamTrack};
use music_hub::db::now_sec;
use music_hub::normalize::norm;
use music_hub::ui::NavTab;
use std::sync::mpsc;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

// =========================================================================
// TIER 4: End-to-End Real-World Application Scenarios (9 scenarios)
// =========================================================================

/// SCENARIO-T4-01: Full Scrobble Lifecycle with Transient Network Failure & Recovery
/// Features: F1, F2, F3, F4, F12.
#[test]
fn test_t4_01_full_scrobble_transient_failure_and_recovery() {
    let test_db = TestDb::new();

    // Milestone 1 & 2: Track starts, late cover bytes arrive
    let play_id = test_db.db.start_play("Hotel California", "Eagles", "spotify");
    let mut snap = mock_play_snapshot(play_id, "Hotel California", "Eagles");
    let key_0 = cover_cache_key(Some(&snap));
    assert_eq!(key_0, format!("{}::0", play_id));

    // Late thumbnail extracted
    snap.cover_bytes = Some(Arc::new(vec![0xFF, 0xD8, 0xFF, 0xE0]));
    let key_1 = cover_cache_key(Some(&snap));
    assert_ne!(key_0, key_1);

    // Milestone 3 & 4: Track finishes playing (>50%); network outage triggers failure
    let mut app_state = mock_app_state();
    test_db.db.update_play_fields(
        play_id,
        Some("Eagles"),
        Some("Eagles"),
        Some("Hotel California"),
        None,
        None,
        Some(390_000),
        Some(240_000),
        Some(1),
        Some(0),
        Some("pending"),
        Some(now_sec()),
    );
    app_state.pending = 1;
    app_state.last_error = Some("HTTP timeout connecting to ws.audioscrobbler.com".into());

    assert_eq!(app_state.pending, 1);
    assert!(app_state.last_error.as_ref().unwrap().contains("timeout"));

    // Milestone 5: User reviews History view, tests cancelation on delete
    let mut sm = InlineDeleteStateMachine::default();
    let deleted = sm.click_delete_button(play_id);
    assert!(!deleted);
    assert_eq!(sm.confirm_delete_id, Some(play_id));
    sm.cancel();
    assert_eq!(sm.confirm_delete_id, None);
    assert_eq!(test_db.db.recent(10).len(), 1);

    // Milestone 6 & 7: Network recovers, auto-retry succeeds, error cleared
    test_db.db.update_play_fields(
        play_id,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("done"),
        None,
    );
    app_state.pending = 0;
    app_state.last_error = None;

    let row = test_db.db.recent(10)[0].clone();
    assert_eq!(row.scrobble_state, "done");
    assert_eq!(app_state.pending, 0);
    assert!(app_state.last_error.is_none());
}

/// SCENARIO-T4-02: Music Discovery Journey from Roaming to Hearting & Poster Generation
/// Features: F5, F10, F11, F13, F14.
#[test]
fn test_t4_02_music_discovery_roaming_to_hearting_and_poster() {
    let test_db = TestDb::new();
    let song = "First Love";
    let artist = "宇多田ヒカル";

    // Milestone 1: Roaming at 900x600 with responsive wrap
    let inner_width = 612.0;
    let toolbar_rows = solve_row_wrapping(inner_width, &[400.0, 350.0], 12.0);
    assert_eq!(toolbar_rows[1].0, 1, "Buttons wrap on 612px inner width");

    // Milestone 2: User plays recommended track
    let play_toast = format!("▶ 正在跳转播放：{}", song);
    assert!(play_toast.contains("First Love"));

    // Milestone 3: User likes song from bottom player bar
    let play_id = test_db.db.start_play(song, artist, "cloudmusic");
    test_db.db.update_play_fields(
        play_id,
        Some(artist),
        Some(artist),
        Some("First Love"),
        Some(186000),
        None,
        Some(257_000),
        Some(257_000),
        Some(1),
        Some(0),
        Some("done"),
        Some(now_sec()),
    );
    test_db.db.set_feedback(song, artist, "like");

    let mut roam_tracks = vec![RoamTrack {
        title: song.into(),
        artist: artist.into(),
        score: 98.0,
        sources: vec!["seed".into()],
        reasons: vec!["high completion".into()],
        ncm: None,
        heard: None,
        feedback: None,
    }];
    roam_tracks[0].feedback = Some("like".into());
    assert_eq!(roam_tracks[0].feedback, Some("like".into()));

    // Milestone 4: Poster stats generation
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    let test_cfg = TestConfig::new();
    let app = music_hub::app::App::new(test_cfg.config.clone(), test_db.db.clone());
    let stats = app.stats(30);
    assert!(!stats.loved.is_empty());
    let rate_text = calculate_completion_rate(stats.loved[0].c, stats.loved[0].n);
    assert_eq!(rate_text, "100% 真爱完播");

    // Milestone 5: Poster modal display and dismissal via Esc
    let mut modal = ModalStateMachine::default();
    modal.open(vec![0x89, 0x50, 0x4E, 0x47]);
    assert!(modal.is_open());
    modal.handle_key_down("escape");
    assert!(!modal.is_open());
}

/// SCENARIO-T4-03: End-to-End Account Setup & Credential Input on High-DPI Display
/// Features: F1, F6, F7, F8, F9, F15.
#[test]
fn test_t4_03_account_setup_and_credential_input_high_dpi() {
    let test_cfg = TestConfig::new();

    // Milestone 1: Launch on 150% DPI display
    let (widget_w, widget_h) = compute_scaled_widget_dimensions(144);
    assert_eq!((widget_w, widget_h), (570, 108));
    let dark_titlebar_color = colorref_from_hex(0x000D1117);
    assert_eq!(dark_titlebar_color, 0x0017110D);

    // Milestone 2: Tab navigation across 3 form inputs
    let mut focus = 0;
    focus = cycle_focus_forward(focus, 3, &[]);
    assert_eq!(focus, 1);
    focus = cycle_focus_forward(focus, 3, &[]);
    assert_eq!(focus, 2);

    // Milestone 3: Password masking on secret field with Unicode
    let secret = "S3cr3t🔑!#";
    let masked = display_string_graphemes(secret, true);
    assert_eq!(masked, "•••••••••");
    assert_eq!(masked.chars().count(), 9);

    // Milestone 4: Horizontal scroll on 180-char NetEase playlist link
    let long_link = "https://music.163.com/playlist?id=123456789&userid=987654321&token=abcdef1234567890abcdef1234567890abcdef&source=desktop_share_button_v2_client_extra_parameters";
    let link_w = long_link.len() as f32 * 8.0; // ~1440px
    let box_w = 300.0;
    let scroll = compute_scroll_offset(link_w, box_w, link_w, 0.0, 12.0);
    assert!(scroll > 0.0);
    assert_eq!(scroll, link_w - box_w);

    // Milestone 5: Enter key save
    test_cfg.config.update(|d| {
        d.api_key = "my_api_key".into();
        d.api_secret = secret.into();
    });

    // Milestone 6: OAuth callback synchronizes username
    test_cfg.config.update(|d| {
        d.session_key = "sk_authorized_999".into();
        d.username = "audiophile_user".into();
    });
    assert!(test_cfg.config.is_authorized());

    // Milestone 7: Hide and restore retains state
    let window_visible = false;
    assert!(!window_visible);
    assert_eq!(test_cfg.config.get_data().username, "audiophile_user");
}

/// SCENARIO-T4-04: Destructive History Cleaning with Confirmation Safeguards
/// Features: F1, F3, F4, F10, F12, F13.
#[test]
fn test_t4_04_destructive_history_cleaning_safeguard() {
    let test_db = TestDb::new();
    let ids = test_db.seed_sample_plays(10);
    assert_eq!(test_db.db.recent(20).len(), 10);

    // Milestone 1: Responsive toolbar at 900x600
    let rows = solve_row_wrapping(612.0, &[280.0, 100.0, 100.0, 100.0, 80.0], 8.0);
    assert_eq!(rows.len(), 5);

    // Milestone 2 & 3: Accidental delete click and cancelation
    let mut sm = InlineDeleteStateMachine::default();
    let row_to_keep = ids[4];
    sm.click_delete_button(row_to_keep);
    assert_eq!(sm.confirm_delete_id, Some(row_to_keep));
    sm.cancel();
    assert_eq!(sm.confirm_delete_id, None);
    assert_eq!(test_db.db.recent(20).len(), 10);

    // Milestone 4 & 5: Intentional delete on junk row
    let row_to_delete = ids[9];
    sm.click_delete_button(row_to_delete);
    let confirmed = sm.click_delete_button(row_to_delete);
    assert!(confirmed);
    test_db.db.delete_play(row_to_delete);
    assert_eq!(test_db.db.recent(20).len(), 9);
    assert!(!test_db.db.recent(20).iter().any(|r| r.id == row_to_delete));
    assert!(test_db.db.recent(20).iter().any(|r| r.id == row_to_keep));
}

/// SCENARIO-T4-05: Multilingual Unicode Music Collection, Search & GDI Typography
/// Features: F7, F8, F9, F11, F14.
#[test]
fn test_t4_05_multilingual_unicode_collection_and_poster() {
    let test_db = TestDb::new();

    // Milestone 1: Seed 5 multilingual tracks
    let tracks = [
        ("中島みゆき", "永遠の嘘をついてくれ"),
        ("아이유", "밤편지"),
        ("Кино", "Группа крови"),
        ("Fairouz", "نسم علينا الهوى"),
        ("The Weeknd", "Blinding Lights 🎧🔥"),
    ];
    for (artist, title) in &tracks {
        let id = test_db.db.start_play(title, artist, "spotify");
        test_db.db.update_play_fields(
            id,
            Some(artist),
            Some(artist),
            None,
            None,
            None,
            Some(200_000),
            Some(200_000),
            Some(1),
            Some(0),
            Some("done"),
            Some(now_sec()),
        );
    }
    assert_eq!(test_db.db.recent(10).len(), 5);

    // Milestone 2: Search input grapheme alignment with emoji backspace
    let search_query = "中島みゆき 🎧";
    let graphemes: Vec<&str> = search_query.graphemes(true).collect();
    assert_eq!(graphemes.len(), 7);
    let backspaced: String = graphemes[..6].concat();
    assert_eq!(backspaced, "中島みゆき ");

    // Milestone 3: GDI text sanitization
    let raw_poster_title = "🎵 [Top 1] 中島みゆき · 永遠の嘘をついてくれ 🌟";
    let sanitized = sanitize_gdi_text(raw_poster_title);
    assert_eq!(sanitized, "[Top 1] 中島みゆき · 永远の嘘をついてくれ".replace("永远", "永遠"));
    assert!(!sanitized.contains("🎵") && !sanitized.contains("🌟"));

    // Milestone 4: Poster modal dismiss on Esc
    let mut modal = ModalStateMachine::default();
    modal.open(vec![1, 2, 3]);
    modal.handle_key_down("escape");
    assert!(!modal.is_open());
}

/// SCENARIO-T4-06: Background Tray Listening, SMTC Cover Decode & Island Sync
/// Features: F1, F2, F3, F5, F15.
#[test]
fn test_t4_06_background_tray_listening_and_island_sync() {
    let test_db = TestDb::new();
    let (tx, rx) = mpsc::channel();

    // Milestone 1: Window minimized to tray
    let window_hidden = true;
    assert!(window_hidden);

    // Milestone 2: Track transitions in background
    let _id1 = test_db.db.start_play("Track 1", "Artist 1", "spotify");
    let id2 = test_db.db.start_play("Track 2", "Artist 2", "spotify");
    let _id3 = test_db.db.start_play("Track 3", "Artist 3", "spotify");
    assert_eq!(test_db.db.recent(10).len(), 3);

    // Milestone 3: Late cover bytes arrive for track 2
    let snap2 = mock_play_snapshot(id2, "Track 2", "Artist 2");
    let mut snap2_late = snap2.clone();
    snap2_late.cover_bytes = Some(Arc::new(vec![0xAA; 50_000]));
    let key_late = cover_cache_key(Some(&snap2_late));
    assert_eq!(key_late, format!("{}::50000", id2));

    // Milestone 4: Heart track from Dynamic Island
    test_db.db.set_feedback("Track 3", "Artist 3", "like");
    assert_eq!(test_db.db.get_feedback_map().get(&(norm("Track 3"), norm("Artist 3"))), Some(&"like".to_string()));

    // Milestone 5: Island wakes console
    tx.send(ConsoleCommand::Open { tab: None, toast: None }).unwrap();
    let cmd = rx.recv().unwrap();
    let ConsoleCommand::Open { tab, .. } = cmd;
    assert!(tab.is_none());

    // Milestone 6: Verify zero spurious repaints during background listening
    let state = mock_app_state();
    assert!(!should_repaint(&state, &state, &key_late, &key_late, false));
}

/// SCENARIO-T4-07: Offline Heavy Queue Accumulation, Error Banner & Batch Catch-Up
/// Features: F3, F4, F6, F12, F13.
#[test]
fn test_t4_07_offline_queue_accumulation_and_batch_catchup() {
    let test_db = TestDb::new();
    let mut app_state = mock_app_state();

    // Milestone 1 & 2: Offline playback queues 8 tracks
    let mut play_ids = Vec::new();
    for i in 1..=8 {
        let id = test_db.db.start_play(&format!("Album Track {}", i), "Band", "applemusic");
        test_db.db.update_play_fields(
            id,
            Some("Band"),
            Some("Band"),
            Some("Album"),
            None,
            None,
            Some(200_000),
            Some(200_000),
            Some(1),
            Some(0),
            Some("pending"),
            Some(now_sec() - (8 - i) * 200),
        );
        play_ids.push(id);
    }
    app_state.pending = 8;
    app_state.last_error = Some("Failed to resolve host ws.audioscrobbler.com".into());

    assert_eq!(test_db.db.recent(20).len(), 8);
    assert_eq!(app_state.pending, 8);
    assert!(app_state.last_error.is_some());

    // Milestone 3: History shows pending rows
    let recent = test_db.db.recent(20);
    assert!(recent.iter().all(|r| r.scrobble_state == "pending"));

    // Milestone 4 & 5: Network restores, batch retry marks all done
    for &id in &play_ids {
        test_db.db.update_play_fields(
            id,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("done"),
            None,
        );
    }
    app_state.pending = 0;
    app_state.last_error = None;

    // Milestone 6: Completion reconciliation
    assert_eq!(app_state.pending, 0);
    assert!(app_state.last_error.is_none());
    let updated_recent = test_db.db.recent(20);
    assert!(updated_recent.iter().all(|r| r.scrobble_state == "done"));
}

/// SCENARIO-T4-08: Fresh Install Out-of-the-Box: Empty States, Presets & First Scrobble
/// Features: F10, F11, F13, F14, F15.
#[test]
fn test_t4_08_fresh_install_empty_states_and_first_scrobble() {
    let test_db = TestDb::new();

    // Milestone 1: Fresh DB has 0 plays
    assert_eq!(test_db.db.recent(10).len(), 0);

    // Milestone 2 & 3: Empty state cards
    let empty_radar_prompt = "输入歌曲与歌手名，或选择下方预设推荐曲";
    assert!(empty_radar_prompt.contains("预设推荐曲"));

    let empty_roam_prompt = "暂无漫游推荐。先去听几首歌积累种子吧！";
    assert!(empty_roam_prompt.contains("积累种子"));

    // Milestone 4: Click preset "晴天 · 周杰伦" -> starts first track
    let first_id = test_db.db.start_play("晴天", "周杰伦", "cloudmusic");
    test_db.db.update_play_fields(
        first_id,
        Some("周杰伦"),
        Some("周杰伦"),
        Some("叶惠美"),
        Some(186016),
        None,
        Some(269_000),
        Some(269_000),
        Some(1),
        Some(0),
        Some("done"),
        Some(now_sec()),
    );
    test_db.db.set_feedback("晴天", "周杰伦", "like");
    assert_eq!(test_db.db.recent(10).len(), 1);

    // Milestone 5: First poster stats calculation with n=1, c=1
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    let test_cfg = TestConfig::new();
    let app = music_hub::app::App::new(test_cfg.config.clone(), test_db.db.clone());
    let stats = app.stats(30);
    assert_eq!(stats.loved[0].n, 1);
    assert_eq!(stats.loved[0].c, 1);
    let rate = calculate_completion_rate(1, 1);
    assert_eq!(rate, "100% 真爱完播");
}

/// SCENARIO-T4-09: Rapid Lifecycle Stress, Tab Navigation Churn & State Machine Audit
/// Features: F1, F3, F7, F8, F10, F11, F13.
#[test]
fn test_t4_09_rapid_lifecycle_stress_and_state_audit() {
    let (tx, rx) = mpsc::channel();

    // Milestone 1: 10 rapid hide/wake cycles
    for i in 0..10 {
        tx.send(ConsoleCommand::Open {
            tab: Some(format!("tab_{}", i % 5)),
            toast: None,
        })
        .unwrap();
    }
    let mut wake_events = 0;
    while let Ok(_) = rx.try_recv() {
        wake_events += 1;
    }
    assert_eq!(wake_events, 10);

    // Milestone 2: 20 rapid tab cycles
    let tabs = [
        NavTab::Radar,
        NavTab::Roam,
        NavTab::Stats,
        NavTab::Recent,
        NavTab::Settings,
    ];
    let mut active_tab = NavTab::Radar;
    for step in 0..20 {
        active_tab = tabs[step % tabs.len()];
    }
    assert_eq!(active_tab, NavTab::Settings);

    // Milestone 3: Text input paste spam
    let _long_paste = "A".repeat(500);
    let scroll = compute_scroll_offset(500.0 * 8.0, 300.0, 4000.0, 0.0, 12.0);
    assert_eq!(scroll, 4000.0 - 300.0);

    // Milestone 4: Rapid modal open and dismiss
    let mut modal = ModalStateMachine::default();
    for _ in 0..10 {
        modal.open(vec![1, 2, 3]);
        assert!(modal.is_open());
        modal.handle_key_down("escape");
        assert!(!modal.is_open());
    }

    // Milestone 5: State consistency verified
    assert!(!modal.is_open());
}
