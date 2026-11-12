# Handoff Report: Milestone 1 Implementation (F1, F2, F3)

- **Worker**: Worker 1 (`worker_m1_1`)
- **Milestone**: Milestone 1 (Core Application Lifecycle & SMTC Covers)
- **Features Implemented**:
  - Feature 1 (F1): Long-lived GPUI Application & Window Lifecycle
  - Feature 2 (F2): Late SMTC Cover Byte Detection
  - Feature 3 (F3): Repaint Gating & Decoupled Toast Expiration
- **Target Files Owned and Modified**:
  - `src/ui/mod.rs`
  - `src/monitor.rs`
  - `src/app.rs`
  - `src/config.rs`
- **Date**: 2026-11-12

---

## 1. Observation

### 1.1 F1: Window Close and Restart Mechanics
- Previously, `run_console_loop` in `src/ui/mod.rs:3967-4091` executed an outer `loop { Application::new().run(...) }`. Upon clicking `X`, GPUI closed the window and exited `Application::run()`. When woken by tray or dynamic island (`ConsoleCommand::Open`), a second `Application::new().run(...)` was invoked, failing with `ERROR_CLASS_ALREADY_EXISTS` on `Zed::Window` class registration and discarding the dispatcher thread pool.
- Win32 `on_window_should_close` callback on GPUI `Window` (`gpui-0.2.2/src/window.rs:4329-4338`) intercepts `WM_CLOSE`. Returning `false` causes GPUI to return `LRESULT(0)` directly without destroying the window.
- Win32 `ShowWindow(hwnd, SW_HIDE)` removes the window from display and taskbar while keeping all memory structures intact.
- Calling `ShowWindow(hwnd, SW_RESTORE)` + `SetForegroundWindow(hwnd)` + `window.activate_window()` reliably restores the window from either hidden (`SW_HIDE`) or minimized states.

### 1.2 F2: Late SMTC Cover Byte Arrival
- External media players (Spotify, NetEase, QQ Music) publish text metadata instantly, but deliver album art thumbnail streams asynchronously over subsequent 1-second ticks via `extract_smtc_thumbnail`.
- In `src/ui/mod.rs`, `cur_cover` was calculated as `format!("{}:{}", n.play_id, n.cover.len())`. When `n.cover` was empty and `n.cover_bytes` arrived late, `n.cover.len()` remained 0, causing `cur_cover != prev_key` to evaluate to `false`. The arriving thumbnail was never decoded.

### 1.3 F3: Repaint Coupling & Toast Expiration
- `run_console_loop` called `cx.notify()` unconditionally every 500ms, triggering continuous GPUI layout recalculation and redraws even while playback was stopped and the app was idle.
- Synchronous toast expiration in `ConsoleView::render` (`ts.elapsed() > Duration::from_millis(3500)`) relied entirely on ongoing repaints. If `cx.notify()` was gated without decoupling toast expiration, toasts would freeze on screen permanently when idle.

---

## 2. Logic Chain

### 2.1 Implementing Single Long-lived GPUI Application (F1)
1. **Single Invocation**: Removed the outer `loop { ... }` in `run_console_loop`. `Application::new().run(...)` is invoked once and runs for the life of the application process.
2. **Close Interception**: Configured `window.on_window_should_close(cx, move |_window, _cx| { ... false })` inside `cx.open_window`. Before returning `false`, it locates the console window handle and invokes Win32 `ShowWindow(hwnd, SW_HIDE)`.
3. **Safe HWND Resolution**: Created helper `get_console_hwnd()` in `src/ui/mod.rs`:
   - Uses `FindWindowW(None, w!("私人音乐中枢 · Music Hub"))`.
   - Uses `FindWindowExW(None, prev, None, w!("私人音乐中枢 · Music Hub"))` as a fallback.
   - Verifies process ID via `GetWindowThreadProcessId(hwnd, Some(&mut pid))` matching `std::process::id()`, preventing collisions with other instances or processes.
4. **Window Wake & Restoration**:
   - In the background receiver for `ConsoleCommand::Open`:
     - Calls `ShowWindow(hwnd, SW_RESTORE)`.
     - Calls `SetForegroundWindow(hwnd)`.
     - Calls `window.activate_window()`.
     - Switches tab via `view.switch_tab(NavTab::from_str(&t), cx)` if `tab` specified.
     - Displays toast via `view.show_toast(msg, cx)` if `toast` specified.
   - Drains the command queue into an accumulator without dropping concurrent commands.

### 2.2 Late SMTC Cover Detection (F2)
1. Added method `pub fn cover_key(&self) -> String` to `PlaySnapshot` in `src/monitor.rs`:
   ```rust
   let b_len = self.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0);
   format!("{}:{}:{}", self.play_id, self.cover, b_len)
   ```
2. In `src/ui/mod.rs`, updated the 500ms polling loop to compute `cur_cover = st.now.as_ref().map(|n| n.cover_key()).unwrap_or_default()`.
3. Ensured decode priority: `cover_bytes` decoded first via `decode_bytes_to_render_image`, with fallback to `fetch_cover_render_image(cov).await`.

### 2.3 Repaint Gating & Decoupled Toast Expiration (F3)
1. Derived `#[derive(PartialEq, Eq)]` on:
   - `AppStateResponse` in `src/app.rs`
   - `ConsoleCommand` in `src/app.rs`
   - `PlaySnapshot` in `src/monitor.rs`
   - `PublicConfig` and `ConfigData` in `src/config.rs`
2. Decoupled toast expiration in `ConsoleView::show_toast` and `ConsoleView::new`:
   - Spawns background task via `cx.spawn(...)` with `cx.background_executor().timer(Duration::from_millis(3500)).await`.
   - Compares timestamp generation `if *ts == now { view.toast = None; cx.notify(); }`, preventing race conditions when subsequent toasts are shown.
3. Removed the passive check (`ts.elapsed() > Duration::from_millis(3500)`) from `ConsoleView::render()`.
4. In `src/ui/mod.rs:window_handle.update`:
   ```rust
   let state_changed = view.state != new_state;
   let focus_changed = focus_req.is_some();
   let needs_notify = state_changed || cover_changed || focus_changed;
   if state_changed { view.state = new_state; }
   if cover_changed { view.now_cover_key = cur_cover; view.now_cover_img = new_img; }
   if needs_notify { cx.notify(); }
   ```
   When state, cover, and focus are unchanged, `cx.notify()` is skipped, completely eliminating idle repaints.

---

## 3. Caveats

1. **Title Matching**: `get_console_hwnd()` matches against window title `"私人音乐中枢 · Music Hub"`. If the titlebar string in `WindowOptions` changes, the string in `get_console_hwnd()` must also be updated.
2. **Non-Windows Targets**: Win32 window APIs (`ShowWindow`, `SetForegroundWindow`, `FindWindowW`, `FindWindowExW`, `GetWindowThreadProcessId`) are guarded under `#[cfg(target_os = "windows")]`.

---

## 4. Conclusion

All tasks specified in `DISPATCH.md` for Milestone 1 are complete:
- F1: Single GPUI application instance with close-to-hide and restore-on-wake lifecycle implemented.
- F2: Late SMTC thumbnail byte divergence detected and decoded cleanly.
- F3: Repaint gating active with 0% idle redraws; toast expiration decoupled to independent async timers.
- Verified: All unit tests and integration tests compile and pass with 0 errors.

---

## 5. Verification Method

### 5.1 Verification Commands and Results

1. **Binary and Library Compilation**:
   ```powershell
   cargo check --bins --lib
   ```
   *Result*: `Finished dev profile [unoptimized + debuginfo] target(s) in 0.66s` — 0 errors, 0 warnings.

2. **Library Unit Tests**:
   ```powershell
   cargo test --lib
   ```
   *Result*: `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`
   - `test app::tests::test_app_state_response_equality ... ok`
   - `test monitor::tests::test_cover_key_late_bytes_divergence ... ok`
   - `test monitor::tests::test_play_snapshot_equality ... ok`

3. **Binary Unit Tests**:
   ```powershell
   cargo test --bin MusicHub
   ```
   *Result*: `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`

4. **Tier 1 Feature Integration Tests**:
   ```powershell
   cargo test --test tier1_feature_tests
   ```
   *Result*: `test result: ok. 75 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`
   - `test_f01_t1_01_command_channel_delivery ... ok`
   - `test_f01_t1_02_multiple_sequential_wake_commands ... ok`
   - `test_f01_t1_03_state_preservation_across_hide_wake ... ok`
   - `test_f01_t1_04_window_should_close_intercept ... ok`
   - `test_f01_t1_05_nav_tab_from_str_mapping ... ok`
   - `test_f02_t1_01_initial_empty_to_late_bytes_transition ... ok`
   - `test_f02_t1_02_web_url_cover_transition ... ok`
   - `test_f02_t1_03_track_change_same_cover_size ... ok`
   - `test_f02_t1_04_stable_playback_no_spurious_key_change ... ok`
   - `test_f02_t1_05_valid_jpeg_png_bytes_decode ... ok`
   - `test_f03_t1_01_app_state_equality_identical ... ok`
   - `test_f03_t1_02_app_state_inequality_on_played_ms ... ok`
   - `test_f03_t1_03_app_state_inequality_on_pending_count ... ok`
   - `test_f03_t1_04_idle_state_zero_repaints ... ok`
   - `test_f03_t1_05_toast_expiration_timer_decoupling ... ok`
