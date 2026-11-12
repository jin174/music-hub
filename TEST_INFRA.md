# Test Infrastructure Specification: Music Hub

## 1. Executive Summary & Test Philosophy

### 1.1 Scope and Philosophy
Music Hub is a local music listening coordinator, Windows SMTC media monitor, Last.fm scrobbler, and GPUI desktop console with Win32 Dynamic Island platform integration.

The test infrastructure is designed under an **opaque-box, requirement-driven automated testing philosophy** derived directly from the authoritative specifications in `ORIGINAL_REQUEST.md` (Requirements R1 through R4) and `PROJECT.md` (Features F1 through F15).

Testing principles:
1. **Opaque-Box & Behavior-Driven**: Tests assert observable behaviors, interface contracts, state mutations, and outputs rather than internal transient variables.
2. **Deterministic & Offline-Compatible**: All automated tests execute cleanly in continuous integration (CI) and headless Windows environments without requiring physical GPUs, interactive desktop displays, or external internet connectivity.
3. **Multi-Tiered Coverage**:
   - **Tier 1 (Feature Coverage)**: Happy path and category-partition testing covering every feature (F1–F15).
   - **Tier 2 (Boundary & Corner Cases)**: Boundary Value Analysis (BVA), Unicode grapheme safety, layout limits, and failure recovery.
   - **Tier 3 (Cross-Feature Interactions)**: Combinatorial pairwise testing verifying state synchronization across concurrent subsystems.
   - **Tier 4 (Real-World Scenarios)**: Multi-step end-to-end user workflows and lifecycle persistence under realistic listening conditions.

---

## 2. Feature Coverage Matrix (F1–F15)

| Feature ID | Feature Name | Requirement Source | Target Subsystems | Tier 1 Tests | Tier 2 Tests | Tier 3 Tests | Tier 4 Scenarios | Total Planned Tests |
|---|---|---|---|---|---|---|---|---|
| **F1** | Long-lived GPUI Application Lifecycle | R1 (Lifecycle) | `src/ui/mod.rs`, `src/main.rs`, `src/app.rs` | 5 | 5 | 5 | 4 | **19** |
| **F2** | Late SMTC Cover Byte Detection | R1 (Media Monitor) | `src/ui/mod.rs`, `src/monitor.rs` | 5 | 5 | 3 | 2 | **15** |
| **F3** | Repaint Gating & Decoupled Toasts | R1 (UI Repaint Loop) | `src/ui/mod.rs`, `src/app.rs` | 5 | 5 | 3 | 4 | **17** |
| **F4** | Scrobble Error Visibility | R2 (Error Visibility) | `src/scrobbler.rs`, `src/ui/mod.rs`, `src/app.rs` | 5 | 5 | 3 | 3 | **16** |
| **F5** | Bottom Bar Heart Synchronization | R2 (State Sync) | `src/ui/mod.rs`, `src/db.rs`, `src/app.rs` | 5 | 5 | 4 | 3 | **17** |
| **F6** | OAuth Settings Synchronization | R2 (OAuth Sync) | `src/server.rs`, `src/ui/mod.rs`, `src/config.rs` | 5 | 5 | 3 | 2 | **15** |
| **F7** | TextInput Horizontal Scrolling | R3 (Input Scrolling) | `src/ui/input.rs` | 5 | 5 | 3 | 2 | **15** |
| **F8** | Focus Navigation & Enter Handling | R3 (Focus & Shortcuts) | `src/ui/input.rs`, `src/ui/mod.rs` | 5 | 5 | 3 | 2 | **15** |
| **F9** | Password Masking Unicode Graphemes | R3 (Grapheme Masking) | `src/ui/input.rs` | 5 | 5 | 3 | 2 | **15** |
| **F10** | 900x600 Responsive Layout | R4 (Responsive UI) | `src/ui/mod.rs` | 5 | 5 | 3 | 3 | **16** |
| **F11** | Modal Backdrop & Esc Dismissal | R4 (Modal Safeguard) | `src/ui/mod.rs` | 5 | 5 | 2 | 3 | **15** |
| **F12** | History Record Deletion Safeguard | R4 (Inline Confirmation)| `src/ui/mod.rs`, `src/db.rs` | 5 | 5 | 3 | 3 | **16** |
| **F13** | Empty States & Click Feedback | R4 (Feedback & States) | `src/ui/mod.rs` | 5 | 5 | 4 | 4 | **18** |
| **F14** | Poster Completion Rate & GDI Emojis| R4 (Poster Typography) | `src/poster.rs`, `src/app.rs` | 5 | 5 | 3 | 3 | **16** |
| **F15** | Win32 Dark Titlebar & DPI Scaling | R4 (Platform UI) | `src/widget.rs`, `src/ui/mod.rs` | 5 | 5 | 2 | 2 | **14** |
| **TOTALS** | **15 Core Features** | | | **75** | **75** | **17** | **9** | **176** |

---

## 3. Test Architecture & Directory Structure

The automated test suite is organized under `tests/` linking directly against `music_hub` as a library crate declared via `[lib]` in `Cargo.toml`.

```
e:\杂项\music-hub\
├── Cargo.toml                      # Includes [lib] name = "music_hub" path = "src/lib.rs"
├── src/
│   ├── lib.rs                      # Re-exports app, config, db, lastfm, monitor, ncm,
│   │                               # normalize, poster, scrobbler, server, tray, ui, widget
│   └── main.rs                     # Standalone binary entry point
├── tests/
│   ├── common/
│   │   └── mod.rs                  # Shared test helpers, in-memory DB fixtures, mock config,
│   │                               # test models, synthetic SMTC data, and assertions
│   ├── tier1_feature_tests.rs      # Tier 1: 75 Feature happy path tests (5 per F1-F15)
│   ├── tier2_boundary_tests.rs     # Tier 2: 75 Boundary value & edge case tests (5 per F1-F15)
│   ├── tier3_pairwise_tests.rs     # Tier 3: 17 Pairwise combinatorial interaction tests
│   └── tier4_scenario_tests.rs     # Tier 4: 9 End-to-end realistic user workflow scenarios
├── TEST_INFRA.md                   # This document
└── TEST_READY.md                   # Test execution instructions and verification status
```

---

## 4. Test Tier Decomposition & Catalog

### 4.1 Tier 1: Feature Coverage (Category-Partition & Happy Path)
75 test cases verifying the core functionality of each feature under standard operational inputs.

- **F1 (Lifecycle)**:
  - `TEST_F01_T1_01_command_channel_delivery`: Dispatch `ConsoleCommand::Open` and verify receipt across channel.
  - `TEST_F01_T1_02_multiple_sequential_wake_commands`: FIFO delivery of consecutive wake commands.
  - `TEST_F01_T1_03_state_preservation_across_hide_wake`: Cache preservation across window hide and restore cycles.
  - `TEST_F01_T1_04_window_should_close_intercept`: Intercept close event and confirm `false` return to avoid destruction.
  - `TEST_F01_T1_05_nav_tab_from_str_mapping`: Accurate mapping of tab strings to `NavTab` variants.
- **F2 (SMTC Covers)**:
  - `TEST_F02_T1_01_initial_empty_to_late_bytes_transition`: Key changes when late cover bytes arrive.
  - `TEST_F02_T1_02_web_url_cover_transition`: Key changes when web URL arrives.
  - `TEST_F02_T1_03_track_change_same_cover_size`: Change detected across distinct track IDs with identical byte lengths.
  - `TEST_F02_T1_04_stable_playback_no_spurious_key_change`: Unchanged track bytes produce identical keys.
  - `TEST_F02_T1_05_valid_jpeg_png_bytes_decode`: Valid PNG/JPEG byte arrays decode into image dimensions.
- **F3 (Repaint Gating)**:
  - `TEST_F03_T1_01_app_state_equality_identical`: Identical `AppStateResponse` evaluations yield equality.
  - `TEST_F03_T1_02_app_state_inequality_on_played_ms`: Changed play progress flags state mutation.
  - `TEST_F03_T1_03_app_state_inequality_on_pending_count`: Pending queue backlog mutation flags state change.
  - `TEST_F03_T1_04_idle_state_zero_repaints`: Repaint gating evaluates `false` when state and cover are stationary.
  - `TEST_F03_T1_05_toast_expiration_timer_decoupling`: Decoupled timer lifecycle removes toast without polling churn.
- **F4 (Scrobble Error Visibility)**:
  - `TEST_F04_T1_01_scrobbler_records_last_error_on_failure`: API failures record error description in `last_error`.
  - `TEST_F04_T1_02_scrobbler_clears_last_error_on_success`: Successful scrobble clears `last_error` to `None`.
  - `TEST_F04_T1_03_app_state_response_reflects_error`: `AppStateResponse` properly exposes `last_error`.
  - `TEST_F04_T1_04_console_view_initializes_with_existing_error`: UI model constructor captures existing errors.
  - `TEST_F04_T1_05_error_appearance_triggers_state_diff`: Transition to `Some(err)` flags state change.
- **F5 (Heart Synchronization)**:
  - `TEST_F05_T1_01_toggle_like_to_remove_in_db`: Toggling like records in DB; second toggle removes.
  - `TEST_F05_T1_02_radar_tracks_sync_on_toggle`: In-memory Radar list updates track feedback state.
  - `TEST_F05_T1_03_roam_tracks_sync_on_toggle`: In-memory Roaming list updates track feedback state.
  - `TEST_F05_T1_04_feedback_list_refreshed_on_toggle`: Settings feedback table refreshes immediately.
  - `TEST_F05_T1_05_bottom_bar_button_state_sync`: Player bar heart button resolves loved status accurately.
- **F6 (OAuth Synchronization)**:
  - `TEST_F06_T1_01_config_update_persists_credentials`: Updating config saves `session_key` and `username`.
  - `TEST_F06_T1_02_sync_settings_updates_username_input`: Sync logic updates `username_input.text()`.
  - `TEST_F06_T1_03_oauth_callback_endpoint_processing`: OAuth callback endpoint exchanges token and updates config.
  - `TEST_F06_T1_04_oauth_callback_sends_wake_command`: Callback dispatches wake command with Settings tab and toast.
  - `TEST_F06_T1_05_enter_key_saves_username`: Pressing Enter in username input persists value.
- **F7 (TextInput Scrolling)**:
  - `TEST_F07_T1_01_scroll_offset_zero_when_text_fits`: Zero scroll when text width is within container width.
  - `TEST_F07_T1_02_scroll_offset_shifts_right_on_typing_overflow`: Cursor past right bound shifts scroll offset.
  - `TEST_F07_T1_03_scroll_offset_snaps_left_on_home_navigation`: Navigating cursor to 0 snaps scroll offset to 0.
  - `TEST_F07_T1_04_scroll_offset_clamped_at_max_scroll`: Offset never exceeds maximum scrollable distance.
  - `TEST_F07_T1_05_mouse_click_coordinate_mapping_with_scroll`: Click mapping translates by scroll offset.
- **F8 (Focus & Shortcuts)**:
  - `TEST_F08_T1_01_text_input_tab_stop_flag_enabled`: Input components have `tab_stop` enabled.
  - `TEST_F08_T1_02_tab_navigation_forward_cycling_order`: Tab key cycles forward modulo $N$.
  - `TEST_F08_T1_03_shift_tab_backward_cycling_order`: Shift-Tab wraps focus backward modulo $N$.
  - `TEST_F08_T1_04_text_input_enter_event_emission`: Enter key dispatches `InputEvent::EnterPressed`.
  - `TEST_F08_T1_05_settings_enter_triggers_config_save`: Enter event in Settings initiates config save.
- **F9 (Unicode Masking)**:
  - `TEST_F09_T1_01_ascii_password_masking_1_to_1`: 1:1 bullet mapping for ASCII passwords.
  - `TEST_F09_T1_02_cjk_password_masking_graphemes`: CJK characters map 1:1 to uniform bullets.
  - `TEST_F09_T1_03_emoji_password_masking_no_four_stars`: 4-byte emojis map 1:1 to single bullets (no `****`).
  - `TEST_F09_T1_04_content_to_display_offset_mapping`: Byte offset mapping from content to display strings.
  - `TEST_F09_T1_05_display_to_content_offset_mapping`: Display offset mapping back to valid content char boundaries.
- **F10 (900x600 Responsive Layout)**:
  - `TEST_F10_T1_01_responsive_inner_width_at_900_bounds`: Inner container calculates to 612px.
  - `TEST_F10_T1_02_roaming_header_wrap_activation_at_612_width`: Header elements wrap when $W > 612$.
  - `TEST_F10_T1_03_history_toolbar_wrap_activation_at_612_width`: History toolbar buttons wrap cleanly.
  - `TEST_F10_T1_04_toolbar_buttons_no_horizontal_collision`: Non-overlapping vertical bounding boxes on wrap.
  - `TEST_F10_T1_05_large_window_unwrapped_single_row_placement`: Single-row layout preserved on wide windows.
- **F11 (Modal Safeguard)**:
  - `TEST_F11_T1_01_poster_modal_initial_state_none`: Modal starts as `None`.
  - `TEST_F11_T1_02_poster_modal_backdrop_click_dismisses`: Left click on backdrop dismisses modal.
  - `TEST_F11_T1_03_poster_modal_dialog_interior_click_retains_open`: Interior dialog click does not dismiss.
  - `TEST_F11_T1_04_poster_modal_escape_key_dismisses`: Escape key dismisses modal.
  - `TEST_F11_T1_05_poster_modal_close_button_click_dismisses`: Close button dismisses modal.
- **F12 (Inline Deletion Safeguard)**:
  - `TEST_F12_T1_01_history_delete_first_click_sets_confirmation_state`: First click sets `confirm_delete_id`.
  - `TEST_F12_T1_02_history_delete_second_click_executes_permanent_deletion`: Second click deletes record.
  - `TEST_F12_T1_03_history_delete_button_label_and_style_transition`: Label and color switch to warning state.
  - `TEST_F12_T1_04_history_delete_generates_toast_notification`: Confirmation deletion triggers toast.
  - `TEST_F12_T1_05_history_delete_row_count_decrements_by_one`: Row count strictly decrements by 1.
- **F13 (Empty States & Feedback)**:
  - `TEST_F13_T1_01_roaming_initial_state_card_selection`: Unseeded Roam view renders initial card.
  - `TEST_F13_T1_02_roaming_empty_result_card_selection`: Filtered empty Roam view renders empty card.
  - `TEST_F13_T1_03_radar_initial_state_card_selection`: Unsearched Radar view renders initial card.
  - `TEST_F13_T1_04_loved_play_action_toast_feedback`: Playing loved track triggers feedback toast.
  - `TEST_F13_T1_05_history_play_action_toast_feedback`: Playing history track triggers feedback toast.
- **F14 (Poster Stats & Typography)**:
  - `TEST_F14_T1_01_poster_completion_rate_calculation_standard`: Normal $c=8, n=10$ calculates to $80\%$.
  - `TEST_F14_T1_02_poster_completion_rate_perfect_100`: Perfect $c=25, n=25$ calculates to $100\%$.
  - `TEST_F14_T1_03_poster_completion_rate_fractional_rounding`: Proper integer rounding (e.g. $2/3 \to 67\%$).
  - `TEST_F14_T1_04_poster_medal_text_badges_replace_emojis`: Clean typography badges replace surrogate emojis.
  - `TEST_F14_T1_05_poster_gdi_text_sanitizer_cleans_section_headers`: Section headers sanitized of SMP emojis.
- **F15 (Platform UI & DPI)**:
  - `TEST_F15_T1_01_dpi_scale_factor_calculations`: Monitor DPI scaling factors calculated accurately.
  - `TEST_F15_T1_02_widget_dimensions_at_100_percent_dpi`: Baseline $380 \times 72$ at 96 DPI.
  - `TEST_F15_T1_03_widget_dimensions_at_150_percent_dpi`: Scaled $570 \times 108$ at 144 DPI.
  - `TEST_F15_T1_04_widget_dimensions_at_200_percent_dpi`: Scaled $760 \times 144$ at 192 DPI.
  - `TEST_F15_T1_05_dwm_colorref_conversion_for_dark_theme`: `#0d1117` converts to Win32 `0x0017110D`.

---

### 4.2 Tier 2: Boundary Value Analysis & Edge Cases
75 test cases verifying boundary conditions, resource stress, and malicious or degenerate inputs.

- **F1 (Lifecycle)**:
  - `TEST_F01_T2_01_command_channel_empty_options`: Graceful handling of `Open { tab: None, toast: None }`.
  - `TEST_F01_T2_02_invalid_tab_name_fallback`: Unrecognized tab name safely falls back to default.
  - `TEST_F01_T2_03_rapid_wake_burst_stress`: Burst of 500 wake commands without channel corruption.
  - `TEST_F01_T2_04_dropped_receiver_resilience`: Sending to dropped receiver returns false without panicking.
  - `TEST_F01_T2_05_mutex_concurrency_stress`: Multi-threaded wake commands under contention.
- **F2 (SMTC Covers)**:
  - `TEST_F02_T2_01_corrupted_cover_bytes_safety`: Corrupted image byte stream fails safely without panic.
  - `TEST_F02_T2_02_zero_length_cover_bytes`: `cover_bytes` of length 0 handled cleanly.
  - `TEST_F02_T2_03_massive_cover_bytes_handling`: 8MB thumbnail buffer key evaluation without memory bloat.
  - `TEST_F02_T2_04_simultaneous_url_and_bytes_transition`: Byte buffer prioritized over remote URL.
  - `TEST_F02_T2_05_idle_to_play_and_play_to_idle`: Transitions between active playback and idle states.
- **F3 (Repaint Gating)**:
  - `TEST_F03_T2_01_rapid_toast_replacement_race`: Newer toast timer does not get cancelled by old timer.
  - `TEST_F03_T2_02_paused_playback_gating`: Paused playback suppresses spurious repaints.
  - `TEST_F03_T2_03_config_mutation_flags_state_changed`: Config field mutation accurately flags repaint.
  - `TEST_F03_T2_04_focus_command_bypasses_gating`: Focus command bypasses gating to ensure UI response.
  - `TEST_F03_T2_05_sub_second_toast_cancellation`: Rapid toast cancellation handles gracefully.
- **F4 (Scrobble Error Visibility)**:
  - `TEST_F04_T2_01_special_characters_in_error_message`: Quotes, brackets, newlines, and CJK in error message.
  - `TEST_F04_T2_02_consecutive_different_errors_diff`: Consecutive distinct errors trigger repaint.
  - `TEST_F04_T2_03_high_pending_count_formatting`: Backlog formatting with large counts ($>10,000$).
  - `TEST_F04_T2_04_empty_error_string_resilience`: Empty error string handled without index errors.
  - `TEST_F04_T2_05_concurrent_error_rw_stress`: High-concurrency reads and writes to `last_error`.
- **F5 (Heart Synchronization)**:
  - `TEST_F05_T2_01_normalization_matching`: Whitespace, casing, and slash normalized matching.
  - `TEST_F05_T2_02_song_not_in_radar_or_roam`: Toggling track outside recommendation lists updates DB.
  - `TEST_F05_T2_03_blacklist_to_like_transition`: Liking a blacklisted track overrides dislike in DB.
  - `TEST_F05_T2_04_duplicate_track_instances_sync`: Multiple instances of track in list all update state.
  - `TEST_F05_T2_05_unauthorized_lastfm_skip`: Local like toggle succeeds even when Last.fm is unauthorized.
- **F6 (OAuth Synchronization)**:
  - `TEST_F06_T2_01_callback_missing_token_rejection`: Callback missing token returns error without updating config.
  - `TEST_F06_T2_02_special_characters_in_username`: Usernames with periods, dashes, and Unicode characters.
  - `TEST_F06_T2_03_stale_draft_overwritten_by_auth`: Unsaved draft input replaced by authenticated username.
  - `TEST_F06_T2_04_api_keys_sync_alongside_username`: API key and secret inputs synchronized when changed.
  - `TEST_F06_T2_05_failed_session_exchange_retains_config`: Exchange failure retains existing credentials.
- **F7 (TextInput Scrolling)**:
  - `TEST_F07_T2_01_scroll_offset_exact_boundary_equality`: Zero jitter when text width exactly equals container.
  - `TEST_F07_T2_02_scroll_offset_empty_string_behavior`: Offset remains 0 on empty input without divide-by-zero.
  - `TEST_F07_T2_03_scroll_offset_collapsed_container_width`: Container width 0 handled cleanly.
  - `TEST_F07_T2_04_scroll_offset_massive_paste_50k_characters`: 50,000 characters paste clamps without overflow.
  - `TEST_F07_T2_05_scroll_offset_select_all_and_delete_reset`: Content clearing instantly resets scroll offset.
- **F8 (Focus & Shortcuts)**:
  - `TEST_F08_T2_01_tab_cycle_single_element_form`: Single-input form tab cycle does not crash.
  - `TEST_F08_T2_02_tab_skips_disabled_or_invisible_inputs`: Disabled inputs bypassed in tab cycle.
  - `TEST_F08_T2_03_enter_on_whitespace_only_input`: Blank/whitespace input rejected from saving empty config.
  - `TEST_F08_T2_04_enter_does_not_insert_newline_character`: Enter key does not inject `\n` or `\r`.
  - `TEST_F08_T2_05_rapid_enter_successive_dispatch_idempotence`: Rapid successive enter presses are idempotent.
- **F9 (Unicode Masking)**:
  - `TEST_F09_T2_01_empty_password_masking`: Empty password masks to empty string.
  - `TEST_F09_T2_02_zwj_and_combining_mark_grapheme_masking`: Complex ZWJ emoji sequences mask to single bullets.
  - `TEST_F09_T2_03_sub_bullet_display_click_offset_safety`: Clicking intermediate bytes never slices invalid UTF-8.
  - `TEST_F09_T2_04_backspace_on_trailing_emoji_in_password`: Backspacing multibyte characters deletes entire grapheme.
  - `TEST_F09_T2_05_non_password_mode_identity_passthrough`: Plaintext mode passes content untouched.
- **F10 (900x600 Responsive Layout)**:
  - `TEST_F10_T2_01_exact_threshold_width_boundary`: Deterministic wrap behavior at exact threshold boundaries.
  - `TEST_F10_T2_02_window_minimum_size_enforcement`: Minimum window size 900x600 strictly enforced.
  - `TEST_F10_T2_03_history_toolbar_zero_records_title_width`: Toolbar wrapping with empty history title.
  - `TEST_F10_T2_04_roaming_buttons_sub_row_wrap_at_narrow_bounds`: Sub-row wrapping within card container.
  - `TEST_F10_T2_05_sidebar_fixed_geometry_invariance`: Sidebar width remains strictly 216px across window resizes.
- **F11 (Modal Safeguard)**:
  - `TEST_F11_T2_01_escape_key_when_modal_not_open_is_noop`: Escape key when modal is closed does nothing.
  - `TEST_F11_T2_02_non_escape_keys_do_not_dismiss_modal`: Enter, space, arrows do not dismiss modal.
  - `TEST_F11_T2_03_rapid_toggle_modal_cycles`: Rapid 50-cycle modal open/dismiss loop stability.
  - `TEST_F11_T2_04_backdrop_right_mouse_down_ignored`: Right clicks on backdrop do not dismiss modal.
  - `TEST_F11_T2_05_modal_dismissal_preserves_underlying_inputs`: Background input state preserved after modal closes.
- **F12 (Inline Deletion Safeguard)**:
  - `TEST_F12_T2_01_history_delete_target_switch_cancels_previous`: Clicking another row cancels previous confirmation.
  - `TEST_F12_T2_02_history_delete_tab_switch_clears_confirmation`: Tab navigation resets confirmation state.
  - `TEST_F12_T2_03_history_delete_already_removed_record_handling`: Deleting non-existent ID handled safely.
  - `TEST_F12_T2_04_history_delete_isolation_preserves_other_records`: Deleting row leaves other rows intact.
  - `TEST_F12_T2_05_history_delete_cancel_on_outside_click`: Clicking outside inline prompt cancels deletion.
- **F13 (Empty States & Feedback)**:
  - `TEST_F13_T2_01_bottom_bar_inactive_cursor_not_allowed`: Inactive bottom bar buttons have disabled cursor.
  - `TEST_F13_T2_02_bottom_bar_active_cursor_pointer`: Active bottom bar buttons have pointer cursor.
  - `TEST_F13_T2_03_top_artists_hover_style_defined`: Top artists row defines hover highlight style.
  - `TEST_F13_T2_04_roaming_loading_state_takes_precedence`: Loading state displays loading card, not empty card.
  - `TEST_F13_T2_05_empty_roam_guidance_message_content`: Guidance text provides actionable instructions.
- **F14 (Poster Stats & Typography)**:
  - `TEST_F14_T2_01_poster_completion_rate_zero_total_plays`: Division by zero guard when $n=0$ returns 100%.
  - `TEST_F14_T2_02_poster_completion_rate_zero_completed_plays`: $c=0, n>0$ returns 0%.
  - `TEST_F14_T2_03_poster_completion_rate_anomaly_c_exceeds_n`: Anomaly $c > n$ clamped to 100%.
  - `TEST_F14_T2_04_gdi_sanitizer_removes_all_utf16_surrogate_pairs`: Strips UTF-16 surrogate pairs cleanly.
  - `TEST_F14_T2_05_gdi_sanitizer_preserves_cjk_and_punctuation`: Preserves CJK, Latin, kana, and punctuation.
- **F15 (Platform UI & DPI)**:
  - `TEST_F15_T2_01_dpi_scaling_odd_non_standard_dpi`: Non-standard DPI values (110, 288) scale without NaN.
  - `TEST_F15_T2_02_dpi_scaling_zero_or_negative_clamp`: Corrupt 0 or negative DPI clamps to 96 baseline.
  - `TEST_F15_T2_03_widget_reposition_on_dpi_changed_event`: `WM_DPICHANGED` updates internal scale factor.
  - `TEST_F15_T2_04_elimination_of_set_process_dpi_aware`: Deprecated process-wide DPI virtualization removed.
  - `TEST_F15_T2_05_dwm_immersive_dark_mode_attribute_fallback`: Attribute sequence orders 20 then 19 fallback.

---

### 4.3 Tier 3: Cross-Feature Combinations (Pairwise Testing)
17 pairwise tests verifying cross-feature communication and state synchronization across subsystem boundaries.

- `TEST_T3_01_f1_lifecycle_f6_oauth_sync`: OAuth callback arrives while window is hidden; wakes window, switches to Settings, and populates `username_input`.
- `TEST_T3_02_f5_heart_sync_f13_feedback`: Player bar heart click updates DB, Roaming list, Radar list, and shows toast.
- `TEST_T3_03_f7_scroll_f9_password_masking`: Pasting 30-grapheme password containing multi-byte characters scrolls and masks 1:1 without panic.
- `TEST_T3_04_f4_scrobble_error_f12_delete_safeguard`: Deleting pending failed scrobble engages confirmation safeguard and reconciles queue counts.
- `TEST_T3_05_f14_poster_completion_f5_heart_sync`: Loved tracks in DB are correctly aggregated by poster generator to compute true completion rate.
- `TEST_T3_06_f1_lifecycle_f2_smtc_cover`: SMTC cover bytes arriving while window is hidden decode immediately upon wake.
- `TEST_T3_07_f3_repaint_gating_f13_toast`: Toasts auto-dismiss via async timer, repainting only on mutation without polling churn.
- `TEST_T3_08_f7_scroll_f8_focus_nav`: Tabbing between inputs preserves independent horizontal scroll offsets and cursor positions.
- `TEST_T3_09_f10_layout_f12_delete_safeguard`: Inline deletion confirmation renders cleanly within 900x600 layout bounds.
- `TEST_T3_10_f11_modal_backdrop_f1_lifecycle`: Poster modal retained across window hide/wake; Esc dismisses modal without closing window.
- `TEST_T3_11_f4_scrobble_error_f6_oauth_sync`: Re-authenticating via OAuth updates credentials and clears scrobble error on next flush.
- `TEST_T3_12_f5_heart_sync_f10_layout`: Player bar heart toggle updates Roam list at 900x600 without layout jitter or horizontal overflow.
- `TEST_T3_13_f2_smtc_cover_f3_repaint_gating`: Late cover byte arrival triggers single repaint upon decode, then resumes gating.
- `TEST_T3_14_f8_focus_nav_f9_password_masking`: Tab navigation preserves selection ranges in masked password inputs.
- `TEST_T3_15_f13_empty_state_f10_layout`: Empty state cards render cleanly at 900x600 without clipping.
- `TEST_T3_16_f14_poster_stats_f11_modal_dismiss`: Poster with clean typography badges renders in modal preview; Esc key dismisses cleanly.
- `TEST_T3_17_f1_lifecycle_f15_platform_dpi`: DPI change and hide/wake cycle preserves Win32 dark title bar and scales Dynamic Island.

---

### 4.4 Tier 4: Real-World Scenarios (End-to-End User Journeys)
9 comprehensive multi-step workflows modeling realistic listening sessions and edge conditions.

- `TEST_T4_01_full_scrobble_transient_failure_and_recovery`: Full track scrobble with simulated network outage, error banner display, retry, and clean queue recovery.
- `TEST_T4_02_music_discovery_roaming_to_hearting_and_poster`: Roam recommendation playback, bottom bar heart toggle, stats aggregation, and poster generation.
- `TEST_T4_03_account_setup_and_credential_input_high_dpi`: First-time setup on 150% DPI display with focus navigation, masked passwords, long URL scrolling, and OAuth sync.
- `TEST_T4_04_destructive_history_cleaning_safeguard`: History review at 900x600, cancelation safeguard test, confirmed deletion, and queue reconciliation.
- `TEST_T4_05_multilingual_unicode_collection_and_poster`: Playback and search with CJK, Arabic, Cyrillic, and emoji metadata; clean GDI typography poster generation.
- `TEST_T4_06_background_tray_listening_and_island_sync`: Background operation with hidden console, late cover decode, Dynamic Island interaction, and wake synchronization.
- `TEST_T4_07_offline_queue_accumulation_and_batch_catchup`: Long offline listening session, scrobble backlog accumulation, error banner display, and batch flush upon reconnection.
- `TEST_T4_08_fresh_install_empty_states_and_first_scrobble`: Clean database launch, empty state illustrations, quick presets, and initial scrobble.
- `TEST_T4_09_rapid_lifecycle_stress_and_state_audit`: High-frequency window hide/restore cycles, tab switching churn, clipboard paste stress, and memory stability audit.

---

## 5. Verification Commands & Execution

To execute the entire automated test suite:
```powershell
cargo test
```

To execute specific tiers:
```powershell
cargo test --test tier1_feature_tests
cargo test --test tier2_boundary_tests
cargo test --test tier3_pairwise_tests
cargo test --test tier4_scenario_tests
```

To check compilation without executing:
```powershell
cargo check --tests
```
