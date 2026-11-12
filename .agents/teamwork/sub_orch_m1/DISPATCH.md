# Dispatch: Sub-Orchestrator Milestone 1 (Core Lifecycle & SMTC Covers)

## Identity & Role
- Archetype: self (sub-orchestrator)
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\sub_orch_m1
- Parent: orchestrator (ea25af0d-d74a-45d4-bd84-e58825762a14)

## Objective
Execute Milestone 1 of the Music Hub project per `PROJECT.md`.
Deliverables:
- F1: Single long-lived GPUI `Application::new().run(...)` with `window.on_window_should_close` returning `false` and hiding window (`ShowWindow(hwnd, SW_HIDE)`). On `ConsoleCommand::Open`, restore via `ShowWindow(hwnd, SW_RESTORE)` / `SetForegroundWindow` / `window.activate_window()`.
- F2: Update cover change detection key in `src/ui/mod.rs` to include `n.cover_bytes` presence and length so late-arriving SMTC thumbnail bytes trigger image decoding.
- F3: Decouple toast expiration to an async timer task in `show_toast`; derive `PartialEq` on `AppStateResponse`, `PlaySnapshot`, `PublicConfig`; gate `cx.notify()` on state mutations (`state_changed || cover_changed || focus_req.is_some()`).

## Reference Documents
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- Explorer 1 Survey Report: `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md` and `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\handoff.md`

## Write Ownership
- `src/ui/mod.rs`
- `src/monitor.rs`
- `src/app.rs`
- `src/config.rs`

## Process
Execute the sub-orchestrator procedure (Assess -> Iteration loop: Explorer -> Worker -> Reviewer -> Challenger -> Auditor gate). Ensure all builds (`cargo check`) and tests (`cargo test`) pass, and write `handoff.md` in your state folder when complete.
