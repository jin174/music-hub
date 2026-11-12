# Dispatch: Reviewer 1 (Milestone 1 Code Review)

## Identity
- Archetype: teamwork_reviewer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\reviewer_m1_1

## Task
Perform an independent, high-rigor code review of the Milestone 1 implementation (F1, F2, F3).
Do NOT modify source files.

## Mandatory Reading
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md`
- Worker 1 Report: `e:\杂项\music-hub\.agents\teamwork\worker_m1_1\handoff.md`

## Review Scope
Review modified files:
- `src/ui/mod.rs`
- `src/monitor.rs`
- `src/app.rs`
- `src/config.rs`

## Review Criteria
1. **F1 (Lifecycle)**: Does `run_console_loop` maintain a single `Application::new().run(...)`? Does `on_window_should_close` return `false` and call `ShowWindow(hwnd, SW_HIDE)`? Does `get_console_hwnd()` reliably match title and current process ID? Does `ConsoleCommand::Open` restore via `SW_RESTORE`, `SetForegroundWindow`, and `window.activate_window()`?
2. **F2 (SMTC Cover)**: Does `PlaySnapshot::cover_key()` incorporate `cover_bytes` length? Does `cur_cover` use this key? Does late thumbnail arrival trigger decode?
3. **F3 (Repaint Gating & Decoupled Toasts)**: Are `AppStateResponse`, `PlaySnapshot`, `PublicConfig` deriving `PartialEq, Eq`? Is toast expiration decoupled to an async timer with generation guard in `show_toast`? Is passive check removed from `render()`? Is `cx.notify()` gated on `state_changed || cover_changed || focus_changed` in the 500ms loop?
4. **Build & Tests**: Run `cargo check --bins --lib`, `cargo test --lib`, `cargo test --bin MusicHub`, and `cargo test --test tier1_feature_tests`. Verify all pass.

## Output
Write your review report to `e:\杂项\music-hub\.agents\teamwork\reviewer_m1_1\handoff.md` including a clear verdict (`APPROVE` or `REQUEST_CHANGES`).
Report completion to your parent.


## 2026-11-12T04:32:22Z
You are Reviewer 1 for Milestone 1.
Your working directory is: e:\杂项\music-hub
Your state folder is: e:\杂项\music-hub\.agents\teamwork\reviewer_m1_1
Read your instructions in: e:\杂项\music-hub\.agents\teamwork\reviewer_m1_1\DISPATCH.md

Mandatory reading:
- e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md
- e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md
- e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md
- Worker Report: e:\杂项\music-hub\.agents\teamwork\worker_m1_1\handoff.md

Review all code changes in:
- src/ui/mod.rs
- src/monitor.rs
- src/app.rs
- src/config.rs

Verify that:
1. F1: run_console_loop is a single long-lived GPUI Application run, on_window_should_close returns false and hides window via ShowWindow(hwnd, SW_HIDE), get_console_hwnd() matches title and PID, and wake on ConsoleCommand::Open restores via SW_RESTORE + SetForegroundWindow + activate_window().
2. F2: PlaySnapshot::cover_key() incorporates cover_bytes length, cur_cover uses this key, and late-arriving thumbnails decode cleanly.
3. F3: AppStateResponse, PlaySnapshot, PublicConfig derive PartialEq, Eq. Decoupled toast timer in show_toast with generation guard. Passive expiration removed from render(). Repaint gated on state/cover/focus changes.
4. Run verification commands: cargo check --bins --lib, cargo test --lib, cargo test --bin MusicHub, cargo test --test tier1_feature_tests.

Write your review to:
e:\杂项\music-hub\.agents\teamwork\reviewer_m1_1\handoff.md
Include a clear verdict: APPROVE or REQUEST_CHANGES.
When done, report completion to your parent.
