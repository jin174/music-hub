# Handoff Report: Test Suite Architecture & R1-R2 (F1–F6) Test Specifications

- **Author**: Test Track Explorer 1 (`test_track_explorer_1`)
- **Recipient**: E2E Testing Orchestrator (`sub_orch_test_track`, id: `f119d8d4-4796-4e92-aede-5db30c51d61c`)
- **Working Directory**: `e:\杂项\music-hub`
- **State Folder**: `e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1`
- **Target Scope**: Test Suite Architecture in `tests/`, Requirements R1 & R2 (Features F1–F6), Tiers 1–3

---

## 1. Observation

### 1.1 Project & Cargo Configuration
Direct inspection of `e:\杂项\music-hub\Cargo.toml` lines 1–9:
```toml
[package]
name = "music-hub"
version = "2.0.0"
edition = "2021"

[[bin]]
name = "MusicHub"
path = "src/main.rs"
```
Direct inspection of root directory reveals:
- No `[lib]` target is declared in `Cargo.toml`.
- No `src/lib.rs` exists.
- No `tests/` directory exists currently.
- Execution of `cargo test` (`task-30`):
  ```
  Running unittests src\main.rs (target\debug\deps\MusicHub-ad27bc5bb9a8fe12.exe)
  running 6 tests
  test normalize::tests::test_norm ... ok
  test normalize::tests::test_artist_variants ... ok
  test normalize::tests::test_clean_artist ... ok
  test db::tests::test_db_lifecycle ... ok
  test normalize::tests::test_is_junk_stream ... ok
  test normalize::tests::test_clean_title ... ok
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```

### 1.2 Feature F1: Long-lived GPUI Application Lifecycle
Direct inspection of `src/ui/mod.rs` lines 3951–3987:
```rust
pub fn run_console_loop(
    hub: Arc<HubApp>,
    cmd_rx: std::sync::mpsc::Receiver<ConsoleCommand>,
    open_on_start: bool,
) {
    ...
    loop {
        let initial_cmd = match pending_cmd.take() {
            Some(c) => c,
            None => {
                let rx_guard = cmd_rx.lock().unwrap();
                match rx_guard.recv() {
                    Ok(c) => c,
                    Err(_) => break,
                }
            }
        };

        Application::new().run(move |cx: &mut App| {
            // creates window & ConsoleView
        });
    }
}
```
Direct inspection of `gpui-0.2.2/src/platform/windows/platform.rs:91` and `events.rs:55, 114, 256`:
- Re-calling `Application::new().run(...)` attempts to register `PLATFORM_WINDOW_CLASS_NAME` again, failing with `ERROR_CLASS_ALREADY_EXISTS`.
- GPUI provides `window.on_window_should_close(cx, move |_window, _cx| -> bool { false })` which intercepts `WM_CLOSE`, returning `LRESULT(0)` without destroying the window or tearing down the GPUI dispatcher.

### 1.3 Feature F2: Late SMTC Cover Byte Detection
Direct inspection of `src/ui/mod.rs` lines 4038–4055:
```rust
let cur_cover = st
    .now
    .as_ref()
    .map(|n| format!("{}:{}", n.play_id, n.cover.len()))
    .unwrap_or_default();
let new_img = if cur_cover != prev_key {
    if let Some(bytes) = st.now.as_ref().and_then(|n| n.cover_bytes.as_ref()) {
        decode_bytes_to_render_image(bytes)
    } else if let Some(cov) = st.now.as_ref().map(|n| n.cover.as_str()) {
        fetch_cover_render_image(cov).await
    } else {
        None
    }
} else {
    None
};
```
Direct inspection of `src/monitor.rs` lines 329–351:
- On track start (`needs_start = true`), `extract_smtc_thumbnail(props)` frequently returns `None` because the WinRT thumbnail stream is not yet buffered by Windows media transport. `cur.cover` is `""`.
- Initial key produced: `"{play_id}:0"`.
- 1s later (`monitor.rs:348-350`), `cur.cover_bytes` is populated with thumbnail bytes (e.g. 50,000 bytes), but `cur.cover` remains `""` (len 0).
- Key evaluation produces `"{play_id}:0" == "{play_id}:0"` -> `cur_cover != prev_key` evaluates to `false`.
- Late-arriving thumbnail bytes are never decoded, leaving the player bar cover empty.

### 1.4 Feature F3: Repaint Gating & Decoupled Toasts
Direct inspection of `src/ui/mod.rs` lines 508–512:
```rust
impl Render for ConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Auto-expire toast after 3.5 seconds
        if let Some((_, ts)) = &self.toast {
            if ts.elapsed() > Duration::from_millis(3500) {
                self.toast = None;
            }
        }
```
Direct inspection of `src/ui/mod.rs` lines 4019–4082:
```rust
loop {
    cx.background_executor().timer(Duration::from_millis(500)).await;
    ...
    let update_res = window_handle.update(cx, |view, window, cx| {
        view.state = new_state;
        ...
        cx.notify(); // Unconditionally repaints every 500ms
    });
}
```
Direct inspection of `src/app.rs:14-23`, `src/monitor.rs:68-80`, `src/config.rs:68-78`:
- `AppStateResponse`, `PlaySnapshot`, and `PublicConfig` currently do NOT implement `PartialEq`, preventing direct equality comparison in the loop.

### 1.5 Feature F4: Scrobble Error Visibility
Direct inspection of `src/scrobbler.rs` lines 21, 144–153:
```rust
pub last_error: Arc<RwLock<Option<String>>>,
...
Err(err) => {
    tracing::warn!("Scrobble failed: {err}");
    *self.last_error.write().await = Some(err.clone());
    return Err(err);
}
```
Direct inspection of `src/app.rs` lines 183–199:
```rust
let last_error = self.scrobbler.last_error.read().await.clone();
AppStateResponse {
    ...
    last_error,
}
```
Direct inspection of `src/ui/mod.rs` lines 194–202:
```rust
let state = AppStateResponse {
    now,
    config: pub_cfg.clone(),
    scrobbling: hub.cfg.is_scrobbling(),
    pending,
    last_error: None, // Hardcoded None on instantiation
    last_ok: None,
    session_app,
};
```
Direct inspection of `src/ui/mod.rs` lines 711–721 (`render_sidebar`), lines 2870–3050 (`render_recent_view`), and lines 3370–3400 (`render_settings_view`):
- `self.state.last_error` is never rendered or inspected anywhere in the UI hierarchy.

### 1.6 Feature F5: Bottom Bar Heart Synchronization
Direct inspection of `src/ui/mod.rs` lines 1048–1065:
```rust
div()
    .id("now-btn-love")
    .on_click(cx.listener(|this, _, _, cx| {
        let Some(n) = this.state.now.clone() else { return; };
        this.hub.db.set_feedback(&n.title, &n.artist, "like");
        ...
    }))
```
Direct inspection of `src/ui/mod.rs` lines 412–472:
- `toggle_feedback_item` updates `radar_result` and `roam_result` in memory and refreshes `self.feedback_list = self.hub.db.get_feedback_list(200)`.
- However, `now-btn-love` completely bypasses `toggle_feedback_item`, unconditionally writes `"like"` without toggle capability, fails to update in-memory `radar_result` or `roam_result`, and fails to refresh `feedback_list`.
- Furthermore, `toggle_feedback_item` only checks existing state inside `radar_result` and `roam_result`, failing to query `self.hub.db.get_feedback` when toggling tracks from outside those two lists.

### 1.7 Feature F6: OAuth Settings Synchronization
Direct inspection of `src/server.rs` lines 118–128:
```rust
match app.lf.get_session(token).await {
    Ok((sk, name)) => {
        app.cfg.update(|d| {
            d.session_key = sk;
            d.username = name.clone();
        });
        tracing::info!("Authorized Last.fm session for user {}", name);
        app.open_console_with_toast(
            Some("settings"),
            Some(format!("🎉 Last.fm 授权成功！已绑定账号：{}", name)),
        );
```
Direct inspection of `src/ui/mod.rs` lines 217, 291–307:
- `self.username_input` is initialized once at startup with `cfg_data.username.clone()`.
- `switch_tab(NavTab::Settings)` reloads `feedback_list` and `autostart_enabled`, but never calls `self.username_input.update(cx, |input, cx| input.set_text(cfg.username, cx))`.
- Consequently, when OAuth callback completes, the status badge updates to `✅ 已授权绑定`, but the username input field still displays the old or empty text.

---

## 2. Logic Chain

### 2.1 Test Suite Architecture in `tests/`
1. **The Binary-Only Limitation**: Cargo treats each `.rs` file in `tests/` as an external integration test crate that links against the package's library artifact (`[lib]`). Because `music-hub` currently has only `[[bin]] name = "MusicHub"`, integration tests in `tests/` cannot resolve `use music_hub::...;`.
2. **Canonical Architectural Split**:
   - Add `[lib] name = "music_hub" path = "src/lib.rs"` to `Cargo.toml`.
   - Create `src/lib.rs` exporting all internal modules (`app`, `config`, `db`, `lastfm`, `monitor`, `ncm`, `normalize`, `poster`, `scrobbler`, `server`, `tray`, `ui`, `widget`).
   - `src/main.rs` remains the binary entry point, importing types from `music_hub`.
   - This enables any test in `tests/` or sub-modules in `tests/common/` to import `music_hub::...` with zero duplicate compilation overhead.
3. **Headless Windows CI Compatibility**:
   - In automated CI runners (GitHub Actions Windows runner, headless VMs), no interactive GPU display or Win32 desktop session is attached.
   - Calling Win32 GUI APIs (`ShowWindow`, `SetForegroundWindow`, `FindWindowW`) or creating real DirectX swapchains for GPUI windows can hang or panic.
   - Therefore, the test suite must be organized into tiers:
     - **Tier 1 & Tier 2 State & Engine Tests**: Test core logic (cache keys, state diffs, feedback state machines, OAuth channel dispatches, SQLite persistence, grapheme alignment) as pure, headless Rust tests using Tokio runtime, temporary directories, and in-memory mock models.
     - **UI Logic Decoupling**: Isolate pure helper functions (e.g. `cover_cache_key`, `sync_settings_from_config`, `toggle_feedback_item`) so they can be tested deterministically without requiring a physical Win32 OS window.

### 2.2 Requirement R1 & R2 Feature Decomposition
1. **F1 (Lifecycle)**: Requires verification of `ConsoleCommand::Open` channel dispatch, receiver stability, state retention across hide/restore cycles, and `on_window_should_close` returning `false`.
2. **F2 (SMTC Covers)**: Requires verification that `"{play_id}:{cover}:{cover_bytes.len()}"` detects late byte arrival, URL changes, track transitions, and invalid byte handling.
3. **F3 (Repaint Gating & Toasts)**: Requires verification of `PartialEq` on `AppStateResponse`, suppression of `cx.notify()` when state/cover are unchanged, and decoupled async timer cleanup of toasts without polling.
4. **F4 (Scrobble Error Visibility)**: Requires verification that `Scrobbler::last_error` records errors on failure, clears on success, propagates through `AppStateResponse.last_error`, initializes correctly in `ConsoleView`, and flags state change.
5. **F5 (Heart Sync)**: Requires verification that toggling favorites updates SQLite, in-memory `radar_result`, `roam_result`, and `feedback_list`, handles un-toggle/remove, and synchronizes with Last.fm if authorized.
6. **F6 (OAuth Sync)**: Requires verification that `/api/callback` updates `ConfigData`, dispatches `ConsoleCommand::Open`, and synchronizes `username_input.text()` on Settings tab entry or console wake.

---

## 3. Caveats

1. **Physical Win32 Window Manipulation**: Headless CI cannot physically click Win32 window titlebars or inspect DirectX swapchain pixel buffers. All UI lifecycle and repaint tests must verify the underlying event handlers, command channels, state gating functions, and GPUI view models.
2. **Explorer Division of Responsibility**: This report covers Test Architecture and Requirements R1 & R2 (Features F1–F6). Requirements R3 & R4 (Features F7–F15: TextInput, layout, modals, history deletion, poster stats, titlebar/DPI) are surveyed by Explorer 2.
3. **Preserving Existing Unit Tests**: The 6 existing unit tests in `src/normalize.rs` and `src/db.rs` must continue to pass cleanly without regression.

---

## 4. Conclusion & Concrete Test Specifications

### 4.1 Recommended Test Suite Structure in `tests/`

```
e:\杂项\music-hub\
├── Cargo.toml                      # Add [lib] name = "music_hub" path = "src/lib.rs"
├── src/
│   ├── lib.rs                      # Re-exports all pub modules
│   └── main.rs                     # Thin binary entry point using music_hub
└── tests/
    ├── common/
    │   └── mod.rs                  # Test fixtures: temp SQLite DB, mock Config, MockApp
    ├── test_f1_lifecycle.rs        # F1: Window lifecycle, command channel, state persistence
    ├── test_f2_cover.rs            # F2: SMTC late cover byte detection & cache keys
    ├── test_f3_repaint.rs          # F3: Repaint gating & decoupled toast timers
    ├── test_f4_scrobble.rs         # F4: Scrobble error visibility & propagation
    ├── test_f5_heart_sync.rs       # F5: Bottom bar heart toggle & multi-view sync
    ├── test_f6_oauth_sync.rs       # F6: OAuth browser callback & Settings UI input sync
    └── test_tier3_combinations.rs  # Tier 3: Cross-feature integration scenarios
```

---

### 4.2 Feature F1: Long-lived GPUI Application Lifecycle

| Tier | Test ID | Test Name | Purpose / Scenario | Input / Setup | Execution | Expected Assertion |
|---|---|---|---|---|---|---|
| **T1** | F1-T1-01 | `test_f1_command_channel_delivery` | Happy path: Dispatched `ConsoleCommand::Open` is received across channel | `mpsc::channel()`, send `Open { tab: Some("radar"), toast: Some("hi") }` | `tx.send(cmd)` and `rx.recv()` | Receiver extracts matching `tab` and `toast` without loss |
| **T1** | F1-T1-02 | `test_f1_multiple_sequential_wake_commands` | Multiple sequential wake commands are processed in FIFO order | `tx` dispatches 3 consecutive commands: `"radar"`, `"settings"`, `"stats"` | `rx.try_recv()` in loop | All 3 commands received in exact order |
| **T1** | F1-T1-03 | `test_f1_state_preservation_across_hide_wake` | In-memory view state preserves cached radar/roam data when window is hidden/restored | View model initialized with 5 radar tracks and active tab `NavTab::Radar` | Simulate hide event, then receive wake command `Open { tab: None, toast: None }` | Cached radar tracks length remains 5; no data wiped |
| **T1** | F1-T1-04 | `test_f1_window_should_close_intercept` | Window close handler returns `false` to prevent window destruction | Instantiate close callback logic | Invoke `on_window_should_close` callback | Returns `false` (LRESULT 0 interception) |
| **T1** | F1-T1-05 | `test_f1_nav_tab_from_str_mapping` | String tab identifiers map accurately to `NavTab` variants | Inputs: `"radar"`, `"roam"`, `"stats"`, `"recent"`, `"settings"` | `NavTab::from_str(&s)` | Maps to corresponding `NavTab` enum variants |
| **T2** | F1-T2-01 | `test_f1_command_channel_empty_options` | Wake command with `None` for both tab and toast handled gracefully | Send `ConsoleCommand::Open { tab: None, toast: None }` | Process command in view update | Does not change active tab; does not spawn empty toast |
| **T2** | F1-T2-02 | `test_f1_invalid_tab_name_fallback` | Unrecognized tab string falls back gracefully to default without crash | Send `ConsoleCommand::Open { tab: Some("unknown_tab_xyz"), toast: None }` | Parse and execute `switch_tab` | Falls back to `NavTab::Radar` without panic |
| **T2** | F1-T2-03 | `test_f1_rapid_wake_burst_stress` | High-frequency burst of 500 wake commands does not deadlock or corrupt queue | Send 500 commands concurrently across 4 threads | Drain channel with `try_recv()` | All 500 commands processed, no deadlock |
| **T2** | F1-T2-04 | `test_f1_dropped_receiver_resilience` | Sending wake command when receiver is dropped returns `false` safely | Drop `rx`, invoke `app.open_console_with_toast(...)` | Call `open_console_with_toast` | Returns `false`, no mutex poisoning |
| **T2** | F1-T2-05 | `test_f1_mutex_concurrency_stress` | Concurrent calls to `open_console_window` and `open_console_with_toast` | 10 worker threads simultaneously dispatch wake commands | Run for 500ms | Zero panics, mutex acquires cleanly |

---

### 4.3 Feature F2: Late SMTC Cover Byte Detection

| Tier | Test ID | Test Name | Purpose / Scenario | Input / Setup | Execution | Expected Assertion |
|---|---|---|---|---|---|---|
| **T1** | F2-T1-01 | `test_f2_initial_empty_to_late_bytes_transition` | Transition from empty cover to populated `cover_bytes` triggers key change | `PlaySnapshot { play_id: 101, cover: "", cover_bytes: None }` followed by `cover_bytes: Some(vec![0xFF; 2048])` | Compute `cover_cache_key` for both | Key changes from `"101::0"` to `"101::2048"`; `key != prev_key` is `true` |
| **T1** | F2-T1-02 | `test_f2_web_url_cover_transition` | Late web cover URL arrival triggers key change | `PlaySnapshot { play_id: 101, cover: "", cover_bytes: None }` -> `cover: "http://example.com/cover.jpg"` | Compute `cover_cache_key` for both | Key changes from `"101::0"` to `"101:http://example.com/cover.jpg:0"`; `key != prev_key` is `true` |
| **T1** | F2-T1-03 | `test_f2_track_change_same_cover_size` | New track with identical byte count triggers reload due to `play_id` change | Play 1: `id: 101, bytes: 4096`; Play 2: `id: 102, bytes: 4096` | Compute `cover_cache_key` for both | Keys `"101::4096"` != `"102::4096"`; change detected |
| **T1** | F2-T1-04 | `test_f2_stable_playback_no_spurious_key_change` | Consecutive ticks with unchanged cover bytes produce identical keys | Play 1: `id: 101, cover: "", bytes: 4096` at tick $t_0$ and $t_1$ | Compute `cover_cache_key` at both ticks | Keys are identical; change detected is `false` |
| **T1** | F2-T1-05 | `test_f2_valid_jpeg_png_bytes_decode` | Decoder decodes standard JPEG/PNG bytes into image dimensions | Valid 100x100 PNG byte array | Invoke `image::load_from_memory` | Returns `Ok` with width=100, height=100 |
| **T2** | F2-T2-01 | `test_f2_corrupted_cover_bytes_safety` | Corrupted byte stream does not panic image decoder | Random 1KB binary noise | Invoke `image::load_from_memory` | Returns `Err`, handles failure safely, returns `None` |
| **T2** | F2-T2-02 | `test_f2_zero_length_cover_bytes` | `cover_bytes` with length 0 handled cleanly | `PlaySnapshot { play_id: 101, cover: "", cover_bytes: Some(vec![]) }` | Compute key and decode | Key is `"101::0"`; decode treats empty slice as `None` |
| **T2** | F2-T2-03 | `test_f2_massive_cover_bytes_handling` | Large thumbnail buffer (8MB) key computation does not clone buffer | 8MB `Arc<Vec<u8>>` | Compute `cover_cache_key` | Evaluates in $<1\mu s$, key includes length 8388608 without copying |
| **T2** | F2-T2-04 | `test_f2_simultaneous_url_and_bytes_transition` | Both URL and bytes arrive simultaneously; byte branch prioritized | `PlaySnapshot { cover: "http://...", cover_bytes: Some(valid_png) }` | Execute cover selection logic | Selects `cover_bytes` branch over URL fetch |
| **T2** | F2-T2-05 | `test_f2_idle_to_play_and_play_to_idle` | Transition between `now: None` and `now: Some(play)` | `now = None` -> `now = Some(101)` -> `now = None` | Compute `cover_cache_key` across sequence | Keys: `""` -> `"101::..."` -> `""`; transitions detected |

---

### 4.4 Feature F3: Repaint Gating & Decoupled Toasts

| Tier | Test ID | Test Name | Purpose / Scenario | Input / Setup | Execution | Expected Assertion |
|---|---|---|---|---|---|---|
| **T1** | F3-T1-01 | `test_f3_app_state_equality_identical` | Identical `AppStateResponse` instances evaluate to `==` | Two structs with identical `now`, `config`, `pending`, `last_error` | `assert_eq!(state1, state2)` | Evaluates to `true` |
| **T1** | F3-T1-02 | `test_f3_app_state_inequality_on_played_ms` | Incremented `played_ms` produces `state1 != state2` | State 1: `played_ms: 1000`; State 2: `played_ms: 1500` | `state1 != state2` | Evaluates to `true`; flags `state_changed` |
| **T1** | F3-T1-03 | `test_f3_app_state_inequality_on_pending_count` | Backlog change (e.g. 0 -> 1) flags `state_changed` | State 1: `pending: 0`; State 2: `pending: 1` | `state1 != state2` | Evaluates to `true`; flags `state_changed` |
| **T1** | F3-T1-04 | `test_f3_idle_state_zero_repaints` | Completely idle state (no track, pending 0) gates repaints | State identical across 10 simulated loop ticks | `let needs_notify = state_changed || cover_changed || focus_req.is_some()` | `needs_notify` is `false` on all 10 ticks (0 repaints) |
| **T1** | F3-T1-05 | `test_f3_toast_expiration_timer_decoupling` | Toast clears automatically after 3.5s via async timer without render loop | View with `toast = Some(("hello", t0))` | Advance timer by 3600ms in async executor | `view.toast` becomes `None` automatically |
| **T2** | F3-T2-01 | `test_f3_rapid_toast_replacement_race` | New toast before 3.5s is not cleared by old toast's timer | Toast 1 at $t=0$; Toast 2 at $t=2.0s$. Old timer fires at $t=3.5s$ | Check `if *ts == now` | Toast 2 is NOT cleared; remains until $t=5.5s$ |
| **T2** | F3-T2-02 | `test_f3_paused_playback_gating` | Paused playback (`playing: false`, stationary `played_ms`) suppresses repaints | `playing: false`, `played_ms: 45000` for 5 consecutive ticks | Evaluate `needs_notify` | `needs_notify` is `false` for all 5 ticks |
| **T2** | F3-T2-03 | `test_f3_config_mutation_flags_state_changed` | Config field modification flags `state_changed` | Toggle `config.mode` from `"record"` to `"scrobble"` | `state1 != state2` | Evaluates to `true` |
| **T2** | F3-T2-04 | `test_f3_focus_command_bypasses_gating` | Focus command forces repaint even if state and cover are unchanged | `state_changed = false`, `cover_changed = false`, `focus_req = Some(...)` | Evaluate `needs_notify` | `needs_notify` evaluates to `true` |
| **T2** | F3-T2-05 | `test_f3_sub_second_toast_cancellation` | Showing toast and immediately clearing or replacing does not panic | Set toast, overwrite with empty string, advance executor | Run async timer to completion | No panic, view toast cleanly handles update |

---

### 4.5 Feature F4: Scrobble Error Visibility

| Tier | Test ID | Test Name | Purpose / Scenario | Input / Setup | Execution | Expected Assertion |
|---|---|---|---|---|---|---|
| **T1** | F4-T1-01 | `test_f4_scrobbler_records_last_error_on_failure` | Scrobbler records error message on API/network failure | Mock Last.fm client returns `Err("HTTP 401 Unauthorized")` | `scrobbler.scrobble_pending().await` | `*scrobbler.last_error.read().await` equals `Some("HTTP 401 Unauthorized")` |
| **T1** | F4-T1-02 | `test_f4_scrobbler_clears_last_error_on_success` | Successful scrobble clears previously recorded error | `last_error` is `Some("Old err")`; mock client returns `Ok(1)` | `scrobbler.scrobble_pending().await` | `*scrobbler.last_error.read().await` becomes `None` |
| **T1** | F4-T1-03 | `test_f4_app_state_response_reflects_error` | `AppStateResponse` accurately packages `scrobbler.last_error` | `*scrobbler.last_error = Some("NetErr")` | `hub.state().await` | `app_state.last_error` is `Some("NetErr")` |
| **T1** | F4-T1-04 | `test_f4_console_view_initializes_with_existing_error` | `ConsoleView` constructor copies active error instead of hardcoded `None` | `scrobbler.last_error` contains error at startup | Instantiate view model state | `view.state.last_error` contains the active error |
| **T1** | F4-T1-05 | `test_f4_error_appearance_triggers_state_diff` | Transition from `last_error: None` to `Some(err)` flags `state_changed` | State 1: `last_error: None`; State 2: `last_error: Some("Err")` | `state1 != state2` | Evaluates to `true`; triggers UI repaint |
| **T2** | F4-T2-01 | `test_f4_special_characters_in_error_message` | Error containing quotes, brackets, newlines, CJK formats cleanly | Error: `"Last.fm 错误 [code 9]: 'Token invalid' \n Retry?"` | Format sidebar and banner strings | Formats without panic, quotes preserved |
| **T2** | F4-T2-02 | `test_f4_consecutive_different_errors_diff` | Error transition (Timeout -> Invalid Session) triggers repaint | State 1: `Some("Timeout")`; State 2: `Some("Invalid Session")` | `state1 != state2` | Evaluates to `true` |
| **T2** | F4-T2-03 | `test_f4_high_pending_count_formatting` | High backlog with error formats correctly | `pending: 12500`, `last_error: Some("Connection refused")` | Generate banner text | Produces `"Last.fm 打卡异常: Connection refused (积压 12500 首待打卡)"` |
| **T2** | F4-T2-04 | `test_f4_empty_error_string_resilience` | Empty error string `Some("")` does not panic UI banner | `last_error: Some("".into())` | Evaluate banner visibility condition | Safe display without indexing out-of-bounds |
| **T2** | F4-T2-05 | `test_f4_concurrent_error_rw_stress` | High-concurrency reads and writes to `scrobbler.last_error` | 5 background scrobble tasks write errors; 5 UI polling tasks read | Run for 500ms | Zero deadlock, RwLock resolves smoothly |

---

### 4.6 Feature F5: Bottom Bar Heart Synchronization

| Tier | Test ID | Test Name | Purpose / Scenario | Input / Setup | Execution | Expected Assertion |
|---|---|---|---|---|---|---|
| **T1** | F5-T1-01 | `test_f5_toggle_like_to_remove_in_db` | Toggling `"like"` records in SQLite; toggling again removes | DB with track `"Song A" / "Artist A"` | `db.set_feedback(..., "like")`, then `db.set_feedback(..., "remove")` | First call inserts row; second call deletes row |
| **T1** | F5-T1-02 | `test_f5_radar_tracks_sync_on_toggle` | Toggling like updates `feedback` property of matching radar tracks | In-memory `radar_result` containing `"Song A" / "Artist A"` | Execute `toggle_feedback_item` with `"like"` | Track in `radar_result.tracks` has `feedback == Some("like")` |
| **T1** | F5-T1-03 | `test_f5_roam_tracks_sync_on_toggle` | Toggling like updates matching tracks in roam list | In-memory `roam_result` containing `"Song A" / "Artist A"` | Execute `toggle_feedback_item` with `"like"` | Track in `roam_result.tracks` has `feedback == Some("like")` |
| **T1** | F5-T1-04 | `test_f5_feedback_list_refreshed_on_toggle` | `self.feedback_list` for Settings tab reloads immediately on toggle | Initial empty feedback list | Execute `toggle_feedback_item` | `feedback_list` contains 1 item matching song |
| **T1** | F5-T1-05 | `test_f5_bottom_bar_button_state_sync` | Player bar button state reflects liked status of currently playing track | `now: Some(Song A)`, `feedback_map` has `Song A: "like"` | Query player bar heart status | Returns `is_liked == true`, button text `❤️ 已喜欢` |
| **T2** | F5-T2-01 | `test_f5_normalization_matching` | Titles and artists with casing/whitespace/slash match correctly | Current play: `" Song A  " / "Artist A / Extra"`; DB key: normalized | Query feedback status | Resolves match via `norm()` key matching |
| **T2** | F5-T2-02 | `test_f5_song_not_in_radar_or_roam` | Toggling playing song not present in active radar/roam still updates DB | Song playing from local player, not in radar/roam lists | Execute `toggle_feedback_item` | DB and `feedback_list` updated; toast shown |
| **T2** | F5-T2-03 | `test_f5_blacklist_to_like_transition` | Liking a currently blacklisted (`"dislike"`) song overrides blacklist | Song has `action == "dislike"` in DB | Toggle `"like"` | Row in DB updated to `"like"`; removed from blacklist |
| **T2** | F5-T2-04 | `test_f5_duplicate_track_instances_sync` | Multiple occurrences of same track in playlist all receive updated state | Roam playlist contains same track twice | Execute `toggle_feedback_item` | Both track entries in memory update `feedback` |
| **T2** | F5-T2-05 | `test_f5_unauthorized_lastfm_skip` | When Last.fm is not authorized, local toggle succeeds without network call | `cfg.is_authorized() == false` | Execute `toggle_feedback_item` | Updates DB and in-memory lists, skips Last.fm HTTP |

---

### 4.7 Feature F6: OAuth Settings Synchronization

| Tier | Test ID | Test Name | Purpose / Scenario | Input / Setup | Execution | Expected Assertion |
|---|---|---|---|---|---|---|
| **T1** | F6-T1-01 | `test_f6_config_update_persists_credentials` | Updating config saves `session_key` and `username` | In-memory `Config` | `cfg.update(\|d\| { d.session_key = "sk_123".into(); d.username = "alice".into(); })` | `cfg.get_data().username == "alice"`, `cfg.is_authorized() == true` |
| **T1** | F6-T1-02 | `test_f6_sync_settings_updates_username_input` | `sync_settings_from_config` updates `username_input.text()` | `username_input` with text `""`; `cfg.username = "bob"` | Invoke `sync_settings_from_config` | `username_input.text()` equals `"bob"` |
| **T1** | F6-T1-03 | `test_f6_oauth_callback_endpoint_processing` | `/api/callback?token=tok123` updates config | Mock `Lastfm::get_session` returning `Ok(("sk_test", "charlie"))` | Call `handle_callback` handler | `app.cfg.get_data().username` equals `"charlie"` |
| **T1** | F6-T1-04 | `test_f6_oauth_callback_sends_wake_command` | Callback dispatches `ConsoleCommand::Open { tab: "settings", .. }` | Channel receiver listening on `console_tx` | Call `handle_callback` | Channel receives `Open { tab: Some("settings"), toast: Some(...) }` |
| **T1** | F6-T1-05 | `test_f6_enter_key_saves_username` | Pressing Enter in `username_input` persists text to config | `username_input` contains `"david"` | Simulate `InputEvent::EnterPressed` | `app.cfg.get_data().username` equals `"david"` |
| **T2** | F6-T2-01 | `test_f6_callback_missing_token_rejection` | Request to `/api/callback` without `token` returns error without updating config | Call `handle_callback` with empty query params | Execute handler | Config credentials unchanged; error toast dispatched |
| **T2** | F6-T2-02 | `test_f6_special_characters_in_username` | Username with underscores, periods, Unicode survives sync | Username `"user_01.测试-name"` | Perform OAuth callback & settings sync | Input text matches exact string without corruption |
| **T2** | F6-T2-03 | `test_f6_stale_draft_overwritten_by_auth` | Uncommitted draft in `username_input` replaced by authenticated username | User typed draft `"unsaved"`; OAuth completes for `"alice"` | Invoke `sync_settings_from_config` | `username_input.text()` updates to `"alice"` |
| **T2** | F6-T2-04 | `test_f6_api_keys_sync_alongside_username` | API key and Secret fields also synchronize if updated | Config has new `api_key` and `api_secret` | Invoke `sync_settings_from_config` | `api_key_input` and `api_secret_input` reflect new values |
| **T2** | F6-T2-05 | `test_f6_failed_session_exchange_retains_config` | Failed token exchange preserves existing credentials | Config already had username `"existing"`; callback gets error | Call `handle_callback` with invalid token | Config still holds `"existing"`; no fields wiped |

---

### 4.8 Tier 3: Cross-Feature Combinations

| Test ID | Features Involved | Combination Scenario | Execution & Verification Flow |
|---|---|---|---|
| **T3-01** | F1 + F6 | OAuth callback completes while console window is hidden | 1. Window is hidden via `SW_HIDE` (`F1`).<br>2. Browser completes OAuth authorization (`F6`).<br>3. `handle_callback` sends `ConsoleCommand::Open { tab: Some("settings"), toast: Some(...) }`.<br>4. Channel delivers command -> Window wakes (`SW_RESTORE`) -> switches to Settings tab -> `sync_settings_from_config` updates `username_input` -> displays toast.<br>5. Assert window active, tab is Settings, `username_input.text() == auth_username`. |
| **T3-02** | F1 + F5 | Toggling favorite while window is hidden, waking to Radar view | 1. Window is hidden.<br>2. User likes current song via mini widget / bottom bar (`F5`).<br>3. Win32 tray/island sends wake command `Open { tab: Some("radar") }` (`F1`).<br>4. Console wakes; verify `radar_result` track retains `feedback: Some("like")` without being wiped by window re-opening. |
| **T3-03** | F2 + F3 | Late SMTC cover byte arrival triggers single repaint, then resumes gating | 1. Song playing, cover bytes initially empty (`F2`). Loop running.<br>2. Late cover bytes arrive -> `cover_cache_key` transitions from `"101::0"` to `"101::65536"`.<br>3. `cover_changed = true` satisfies `needs_notify = state_changed || cover_changed` (`F3`).<br>4. Repaint fires exactly once to display cover.<br>5. Subsequent ticks have unchanged cover key -> `needs_notify = false` (zero repaints). |
| **T3-04** | F4 + F6 | Expired Last.fm session recovery via OAuth re-authorization | 1. Scrobbler encounters invalid session key (code 9) -> writes `last_error` (`F4`).<br>2. UI displays error alert banner in Recent Plays view (`F4`).<br>3. User clicks Settings, re-authorizes via OAuth callback (`F6`).<br>4. OAuth updates `session_key` in config and syncs settings inputs.<br>5. Next scrobble run succeeds -> `last_error` cleared to `None` -> banner disappears (`F4`+`F3`). |
| **T3-05** | F5 + F3 | Player bar heart toggle triggers decoupled toast and single repaint | 1. User clicks player bar heart button (`F5`).<br>2. `toggle_feedback_item` updates DB, in-memory lists, and calls `show_toast` (`F3`).<br>3. Single repaint renders updated heart icon and toast.<br>4. After 3.5s, decoupled timer clears toast and triggers single repaint.<br>5. Polling loop remains completely gated throughout idle periods. |
| **T3-06** | F1 + F4 | Background scrobble error accumulates while window is hidden; displayed upon wake | 1. Window is hidden.<br>2. 3 songs played; scrobble fails due to network outage; `pending = 3`, `last_error = Some("Timeout")` (`F4`).<br>3. Tray icon triggers wake command to Recent tab (`F1`).<br>4. View initializes/updates with `last_error`; callout banner displays error message and 3 pending tracks. |

---

## 5. Verification Method

### 5.1 Compilation Check
To verify that the proposed test structure and library separation compile cleanly:
```powershell
cargo check --tests
```
Expected: Clean exit (code 0), zero compilation errors or missing crate dependencies.

### 5.2 Unit & Integration Test Execution
To execute all existing tests along with the R1 & R2 test specifications:
```powershell
cargo test --test test_f1_lifecycle
cargo test --test test_f2_cover
cargo test --test test_f3_repaint
cargo test --test test_f4_scrobble
cargo test --test test_f5_heart_sync
cargo test --test test_f6_oauth_sync
cargo test --test test_tier3_combinations
```
Or execute the entire suite:
```powershell
cargo test
```
Expected: All tests pass with 0 failures in $<5.0$ seconds.

### 5.3 Invalidation Conditions
The test architecture and specifications will be invalidated if:
1. `Cargo.toml` is modified without a `[lib]` entry, causing external `tests/*.rs` files to fail with `can't find crate for 'music_hub'`.
2. Tests directly instantiate physical Win32 OS windows or call `ShowWindow` in headless CI environments without mock interception, causing CI hangs or crashes.
3. Tests rely on hardcoded shared file paths (e.g. `data/hub.db`) instead of isolated temporary databases, causing race conditions under concurrent test execution (`cargo test -- --test-threads=4`).
