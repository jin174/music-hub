# BRIEFING — 2026-11-12T02:00:00Z

## Mission
Conduct in-depth investigation on R1 (Core Application Lifecycle, SMTC covers, 500ms cx.notify loop) and overall project build/test architecture.

## 🔒 My Identity
- Archetype: teamwork_explorer
- Roles: explorer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\explorer_survey_1
- Original parent: ea25af0d-d74a-45d4-bd84-e58825762a14
- Milestone: survey

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- Produce survey_r1.md and handoff.md in state folder
- Send message to parent upon completion

## Current Parent
- Conversation ID: ea25af0d-d74a-45d4-bd84-e58825762a14
- Updated: 2026-11-12T01:50:00Z

## Investigation State
- **Explored paths**:
  - `Cargo.toml`, `Cargo.lock`
  - `src/main.rs`, `src/app.rs`, `src/config.rs`, `src/db.rs`, `src/monitor.rs`, `src/tray.rs`, `src/widget.rs`, `src/ui/mod.rs`
  - GPUI 0.2.2 source (`app.rs`, `window.rs`, `platform/windows/platform.rs`, `platform/windows/events.rs`, `platform/windows/dispatcher.rs`)
- **Key findings**:
  1. Destructive outer loop in `run_console_loop` (`src/ui/mod.rs:3951`) crashes because GPUI's `register_platform_window_class` registers `Zed::PlatformWindow` without `Once`, and `WindowsDispatcher` breaks upon re-wake.
  2. In GPUI, `Window::on_window_should_close` returning `false` intercepts `WM_CLOSE` and prevents window destruction. Combining this with Win32 `ShowWindow(hwnd, SW_HIDE)` and `ShowWindow(hwnd, SW_RESTORE)` allows a single long-lived `Application` run without state loss.
  3. SMTC thumbnail bug in `src/ui/mod.rs:4039`: `cur_cover` calculation only checks `n.play_id` and `n.cover.len()`. Incorporating `n.cover_bytes.as_ref().map(|b| b.len())` fixes late-arriving image bytes detection.
  4. 500ms `cx.notify()` loop was used to poll toast expiration in `ConsoleView::render`. Spawning a 3.5s async timer in `show_toast` decouples toast expiration. Deriving `PartialEq` on `AppStateResponse` allows gating `cx.notify()` on state changes, eliminating idle repaints.
- **Unexplored areas**: None for R1.

## Key Decisions Made
- Fully documented R1 survey findings in `survey_r1.md` and complete handoff report in `handoff.md`.

## Artifact Index
- DISPATCH.md — Task instructions
- BRIEFING.md — Persistent context & state
- progress.md — Liveness heartbeat
- survey_r1.md — Comprehensive R1 survey and architecture blueprint
- handoff.md — 5-component handoff report for planner and executor
