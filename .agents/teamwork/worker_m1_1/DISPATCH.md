# Dispatch: Worker 1 (Milestone 1 Implementation)

## Identity
- Archetype: teamwork_worker
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\worker_m1_1

## Mandatory Integrity Warning
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Mandatory Reading
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md`
- Explorer 1 Report: `e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\handoff.md`
- Explorer 2 Report: `e:\杂项\music-hub\.agents\teamwork\explorer_m1_2\handoff.md`
- Explorer 3 Report: `e:\杂项\music-hub\.agents\teamwork\explorer_m1_3\handoff.md`

## Write Ownership
You own and may ONLY edit:
- `src/ui/mod.rs`
- `src/monitor.rs`
- `src/app.rs`
- `src/config.rs`

## Implementation Tasks

### 1. Feature 1 (F1): Long-lived GPUI Application & Window Lifecycle
- In `src/ui/mod.rs`:
  - Eliminate the outer `loop { ... }` in `run_console_loop`.
  - Maintain a single long-lived GPUI `Application::new().run(...)` invocation.
  - Implement `get_console_hwnd() -> Option<HWND>` searching for `w!("私人音乐中枢 · Music Hub")` verified by process ID (`GetWindowThreadProcessId`).
  - Inside `cx.open_window`, set `show: open_on_start, focus: open_on_start`.
  - Register `window.on_window_should_close` to hide the window via `ShowWindow(hwnd, SW_HIDE)` and return `false` to intercept `WM_CLOSE`.
  - If `open_on_start == false`, ensure window is hidden immediately.
  - In the background receiver for `ConsoleCommand::Open`:
    - Restore visibility via `ShowWindow(hwnd, SW_RESTORE)` and `SetForegroundWindow(hwnd)`.
    - Call `window.activate_window()`.
    - Handle tab switching (`view.switch_tab`) and toast (`view.show_toast`).
    - Drain command queue cleanly without dropping tabs or toasts.

### 2. Feature 2 (F2): Late SMTC Cover Byte Detection
- In `src/monitor.rs`:
  - Add `pub fn cover_key(&self) -> String` to `PlaySnapshot`:
    `format!("{}:{}:{}", self.play_id, self.cover, self.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0))`
  - Add unit test `test_cover_key_late_bytes_divergence` under `#[cfg(test)] mod tests` in `src/monitor.rs`.
- In `src/ui/mod.rs`:
  - Update `cur_cover` calculation in the polling loop to use `n.cover_key()`.
  - Ensure decoding priorities: `cover_bytes` decoded via `decode_bytes_to_render_image` first, falling back to `fetch_cover_render_image(cov)` on Tokio background thread.

### 3. Feature 3 (F3): Repaint Gating & Decoupled Toast Expiration
- In `src/app.rs`, `src/monitor.rs`, and `src/config.rs`:
  - Derive `#[derive(PartialEq, Eq)]` on `AppStateResponse`, `PlaySnapshot`, `PublicConfig`, `ConfigData`, and `ConsoleCommand`.
- In `src/ui/mod.rs`:
  - In `ConsoleView::show_toast`: decouple expiration to an async timer using `cx.spawn(...)` with `cx.background_executor().timer(Duration::from_millis(3500))`. Use generation check `if *ts == now { view.toast = None; cx.notify(); }`.
  - In `ConsoleView::render`: remove the passive expiration check `if ts.elapsed() > Duration::from_millis(3500) { self.toast = None; }`.
  - In the 500ms background polling loop:
    - Compare `let state_changed = view.state != new_state;`
    - Calculate `let needs_notify = state_changed || cover_changed || focus_req.is_some();`
    - Only update `view.state = new_state` when `state_changed`.
    - Gate `cx.notify()` so it is ONLY called when `needs_notify` is `true`.
- Add unit tests:
  - `test_play_snapshot_equality` in `src/monitor.rs`.
  - `test_app_state_response_equality` in `src/app.rs`.

## Verification Commands
After implementing, execute:
- `cargo check`
- `cargo test`
Ensure ALL tests pass with 0 errors and 0 warnings if possible.

## Output
Write your handoff report to `e:\杂项\music-hub\.agents\teamwork\worker_m1_1\handoff.md` and report completion back to parent.


## 2026-11-12T03:51:50Z
You are Worker 1 for Milestone 1.
Your working directory is: e:\杂项\music-hub
Your state folder is: e:\杂项\music-hub\.agents\teamwork\worker_m1_1
Read your instructions in: e:\杂项\music-hub\.agents\teamwork\worker_m1_1\DISPATCH.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

Mandatory reading:
- e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md
- e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md
- e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md
- Explorer 1 Report: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\handoff.md
- Explorer 2 Report: e:\杂项\music-hub\.agents\teamwork\explorer_m1_2\handoff.md
- Explorer 3 Report: e:\杂项\music-hub\.agents\teamwork\explorer_m1_3\handoff.md

Write Ownership:
You own and may ONLY edit:
- src/ui/mod.rs
- src/monitor.rs
- src/app.rs
- src/config.rs

Execute all implementation tasks detailed in DISPATCH.md:
1. F1: Single long-lived GPUI Application, window hide on close via Win32 ShowWindow(hwnd, SW_HIDE) returning false, get_console_hwnd() with PID check, restore on ConsoleCommand::Open via ShowWindow(hwnd, SW_RESTORE) + SetForegroundWindow + window.activate_window().
2. F2: Late SMTC cover byte detection via PlaySnapshot::cover_key() incorporating cover_bytes length, update UI polling loop, and unit test in src/monitor.rs.
3. F3: Derive PartialEq, Eq on AppStateResponse, PlaySnapshot, PublicConfig, etc. Decouple toast expiration to async timer in show_toast. Remove passive check from render(). Gate cx.notify() in 500ms loop. Add unit tests for equality.

Verify your changes by running:
- cargo check
- cargo test
Ensure all tests pass.

Write your handoff report to:
e:\杂项\music-hub\.agents\teamwork\worker_m1_1\handoff.md
When done, report completion to your parent.
