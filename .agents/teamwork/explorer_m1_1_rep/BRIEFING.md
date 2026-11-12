# BRIEFING — 2026-11-12T03:48:00Z

## Mission
Investigate Feature 1 (F1): Long-lived GPUI Application and Window Hide/Restore Lifecycle in src/ui/mod.rs and src/main.rs.

## 🔒 My Identity
- Archetype: teamwork_explorer
- Roles: explorer, analyst
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep
- Original parent: 0938fee6-a149-4028-954d-d64c1f44f0ec
- Milestone: Milestone 1 (M1)

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- Do NOT modify source files or execute build/write tasks
- Write analysis report to e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\handoff.md
- Report completion via send_message to parent

## Current Parent
- Conversation ID: 0938fee6-a149-4028-954d-d64c1f44f0ec
- Updated: 2026-11-12T03:48:00Z

## Investigation State
- **Explored paths**:
  - `src/main.rs`: process entry, single-instance mutex, CLI flags, thread spawns, ui launcher
  - `src/ui/mod.rs`: `run_console_loop`, `ConsoleView`, background loop, window options, callbacks
  - `src/app.rs`: `ConsoleCommand`, `HubApp` IPC channels
  - `src/tray.rs` & `src/widget.rs`: wake triggers, window commands, process exit handling
  - `gpui-0.2.2\src\platform\windows\events.rs`, `window.rs`, `platform.rs`: window procedure, WM_CLOSE intercept, activate, visibility placement
- **Key findings**:
  - `Application::new().run` must only be called once to prevent `ERROR_CLASS_ALREADY_EXISTS` and dispatcher corruption.
  - `window.on_window_should_close` returning `false` returns `Some(0)` on `WM_CLOSE`, preventing `DefWindowProcW` and window destruction.
  - `Window::activate` only calls `ShowWindowAsync` if `IsIconic` (minimized); explicit `ShowWindow(hwnd, SW_RESTORE)` is required for hidden windows.
  - `get_console_hwnd()` reliably identifies the window via `FindWindowW` / `FindWindowExW` and `pid == std::process::id()`.
  - Edge cases (`open_on_start = false`, command batching, tab preservation) are fully resolved.
- **Unexplored areas**: None for Feature 1 scope.

## Key Decisions Made
- All questions 1-5 analyzed and documented in `handoff.md`.

## Artifact Index
- DISPATCH.md — instructions and dispatch history
- BRIEFING.md — persistent working memory
- progress.md — liveness heartbeat
- handoff.md — 5-component handoff report for Feature 1
