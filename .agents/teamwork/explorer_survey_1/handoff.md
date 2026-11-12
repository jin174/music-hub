# Handoff Report: R1 Architecture Survey

## 1. Observation

1. **Outer loop recreating `Application::new().run(...)` in `src/ui/mod.rs:3951-4091`**:
   `run_console_loop` runs an infinite `loop` around `Application::new().run(move |cx: &mut App| { ... })`.
   Inside GPUI Windows backend (`gpui-0.2.2\src\platform\windows\platform.rs:91, 107, 1098`):
   ```rust
   pub(crate) fn new() -> Result<Self> {
       OleInitialize(None).context("unable to initialize Windows OLE")?;
       ...
       register_platform_window_class();
   ```
   `register_platform_window_class()` calls `RegisterClassW(&wc)` without `Once` protection. Calling `Application::new()` a second time causes `RegisterClassW` to fail with `ERROR_CLASS_ALREADY_EXISTS`. Furthermore, GPUI `WindowsDispatcher` (`dispatcher.rs:24`) holds a `platform_window_handle: SafeHwnd` and `validation_number`. When the first `Application` run exits, the platform window is destroyed and the flume receiver is dropped; re-entering causes background tasks to target dead HWNDs or mismatched validation numbers.
   In addition, tearing down `Application` drops `ConsoleView`, completely losing user inputs, active tab, and in-memory caches for radar (`radar_result`, `radar_songs`) and roaming (`roam_result`, `roaming_songs`).

2. **GPUI Window Close Interception Mechanism**:
   In `gpui-0.2.2\src\platform\windows\events.rs:55, 256`:
   ```rust
   WM_CLOSE => self.handle_close_msg(),
   ...
   fn handle_close_msg(&self) -> Option<isize> {
       let mut callback = self.state.borrow_mut().callbacks.should_close.take()?;
       let should_close = callback();
       self.state.borrow_mut().callbacks.should_close = Some(callback);
       if should_close { None } else { Some(0) }
   }
   ```
   In `gpui-0.2.2\src\window.rs:4329`:
   ```rust
   pub fn on_window_should_close(
       &self,
       cx: &App,
       f: impl Fn(&mut Window, &mut App) -> bool + 'static,
   )
   ```
   When `f` returns `false`, `handle_close_msg` returns `Some(0)`, which intercepts `WM_CLOSE` and prevents `DestroyWindow` / `DefWindowProcW`. The window is NOT destroyed, and GPUI's event loop does NOT exit.

3. **SMTC Cover Detection Omitting `cover_bytes`**:
   In `src/ui/mod.rs:4039-4044`:
   ```rust
   let cur_cover = st
       .now
       .as_ref()
       .map(|n| format!("{}:{}", n.play_id, n.cover.len()))
       .unwrap_or_default();
   let new_img = if cur_cover != prev_key { ... }
   ```
   In `src/monitor.rs:331, 348-350`:
   When a track starts, `extract_smtc_thumbnail(props)` may return `None` because the Windows SMTC stream has not finished downloading thumbnail bytes. At this point `n.cover` is empty `""`. `cur_cover` is recorded as `"{play_id}:0"`.
   When `extract_smtc_thumbnail` succeeds on a subsequent tick (1 second later), `n.cover` remains `""`. `cur_cover` remains `"{play_id}:0"`. `cur_cover != prev_key` evaluates to `false`. The late-arriving image bytes in `cover_bytes` are never decoded, and the bottom bar displays a generic note icon indefinitely.

4. **Unconditional 500ms `cx.notify()` loop in `src/ui/mod.rs:4019-4082`**:
   Every 500ms, `window_handle.update` unconditionally invokes `cx.notify()`, forcing full tree layout, shaping, and DirectX redraw even when playback is stopped or paused.
   In `src/ui/mod.rs:508-512`:
   ```rust
   // Auto-expire toast after 3.5 seconds
   if let Some((_, ts)) = &self.toast {
       if ts.elapsed() > Duration::from_millis(3500) {
           self.toast = None;
       }
   }
   ```
   The author relied on the 500ms `cx.notify()` loop solely to drive the passive toast expiration check in `render()`.

5. **Existing Automated Test Command and Results**:
   Running `cargo test` in `e:\杂项\music-hub`:
   ```
   test normalize::tests::test_norm ... ok
   test normalize::tests::test_clean_artist ... ok
   test normalize::tests::test_artist_variants ... ok
   test normalize::tests::test_is_junk_stream ... ok
   test db::tests::test_db_lifecycle ... ok
   test normalize::tests::test_clean_title ... ok
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; finished in 0.01s
   ```

---

## 2. Logic Chain

1. **Window Lifecycle (Observation 1 & 2 -> Conclusion)**:
   Recreating `Application::new().run(...)` inside an outer loop fails because GPUI's Windows backend was designed around a single application instance registering global window classes and initializing a thread dispatcher once.
   By intercepting `on_window_should_close` and returning `false`, the window is preserved against destruction.
   By invoking Win32 `ShowWindow(hwnd, SW_HIDE)` upon close, and `ShowWindow(hwnd, SW_RESTORE)` / `SetForegroundWindow(hwnd)` / `window.activate_window()` on `ConsoleCommand::Open`, the window hides and restores cleanly without tearing down the GPUI runtime.
   All in-memory settings, text inputs, active tabs, and cached radar/roaming results in `ConsoleView` survive across window closes.

2. **SMTC Cover Updates (Observation 3 -> Conclusion)**:
   Because `cur_cover` only checked `n.play_id` and `n.cover.len()`, changes in `n.cover_bytes` were completely invisible to change detection.
   Incorporating `cover_bytes` length into the key (`format!("{}:{}:{}", n.play_id, n.cover, n.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0))`) guarantees that when thumbnail bytes arrive after track start, the key changes from `"{play_id}::0"` to `"{play_id}::{len}"`, triggering `decode_bytes_to_render_image(bytes)`.

3. **Repaint Gating & Toast Decoupling (Observation 4 -> Conclusion)**:
   Decoupling toast expiration from `render()` into an asynchronous timer task inside `show_toast` (`timer(Duration::from_millis(3500))`) eliminates the need for polling toast expiration in `render()`.
   Deriving `PartialEq` on `AppStateResponse`, `PlaySnapshot`, and `PublicConfig` enables precise state mutation detection (`view.state != new_state`).
   Gating `cx.notify()` on `state_changed || cover_changed || focus_req.is_some()` completely eliminates idle repaints when music is stopped or paused.

---

## 3. Caveats

- Win32 HWND resolution uses `FindWindowW(None, w!("私人音乐中枢 · Music Hub"))` combined with `GetWindowThreadProcessId` verification against current PID. If the window title string is ever modified, `get_console_hwnd()` must use the matching title.
- When the window is hidden (`SW_HIDE`), `view.state` and `now_cover_img` continue to be updated in memory so they are instantly current when the window is reopened.

---

## 4. Conclusion

The architectural changes required for R1 are clear, feasible, and localized:
1. Replace the outer loop in `src/ui/mod.rs:3951` with a single long-lived `Application::new().run(...)`. Register `window.on_window_should_close` to hide the HWND via `ShowWindow(hwnd, SW_HIDE)` and return `false`. Handle `ConsoleCommand::Open` by restoring visibility with `ShowWindow(hwnd, SW_RESTORE)` and activating the window.
2. Update the cover change key in `src/ui/mod.rs` to include `cover_bytes.len()`.
3. In `src/ui/mod.rs:show_toast`, spawn a 3.5s async timer to clear the toast and call `cx.notify()`, removing toast polling from `render()`.
4. Derive `PartialEq` for `AppStateResponse`, `PlaySnapshot`, and `PublicConfig`, and gate `cx.notify()` in the polling loop on `state_changed || cover_changed || focus_req.is_some()`.

---

## 5. Verification Method

1. **Compilation Check**:
   Run `cargo check` in `e:\杂项\music-hub`. Must succeed without errors or warnings on modified files.
2. **Unit Test Verification**:
   Run `cargo test` in `e:\杂项\music-hub`. Must pass existing 6 unit tests plus new unit tests verifying cover key generation and state equality comparison.
3. **Interactive Lifecycle Verification**:
   - Close the console window via the top-right `X` button.
   - Verify process remains active and Dynamic Island/tray icon remain responsive.
   - Click "打开控制台" from the tray icon or the Dynamic Island console button.
   - Verify console window restores immediately without panic, with previous inputs, selected tab, and cached recommendations preserved.
4. **Invalidation Conditions**:
   - If `ShowWindow(hwnd, SW_HIDE)` does not suppress taskbar visibility or causes GPUI frame starvation, verify Win32 window styles.
   - If `on_window_should_close` returns `true`, GPUI will destroy the window and exit the event loop.
