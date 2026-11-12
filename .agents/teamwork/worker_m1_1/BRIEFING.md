# BRIEFING — 2026-11-12T04:30:00Z

## Mission
Implement Milestone 1: F1 (Long-lived GPUI Application & Window Lifecycle), F2 (Late SMTC Cover Byte Detection), and F3 (Repaint Gating & Decoupled Toast Expiration) in Music Hub.

## 🔒 My Identity
- Archetype: teamwork_worker
- Roles: implementer, qa
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\worker_m1_1
- Original parent: 0938fee6-a149-4028-954d-d64c1f44f0ec
- Milestone: Milestone 1 (M1)

## 🔒 Key Constraints
- Write Ownership: ONLY edit `src/ui/mod.rs`, `src/monitor.rs`, `src/app.rs`, `src/config.rs`.
- Do not touch files outside write ownership.
- Single long-lived GPUI Application, intercept close with SW_HIDE returning false, restore with SW_RESTORE.
- PlaySnapshot::cover_key with cover_bytes length.
- Decouple toast expiration to async timer in show_toast.
- Gate cx.notify() in 500ms polling loop.
- Derive PartialEq, Eq for AppStateResponse, PlaySnapshot, PublicConfig, ConfigData, ConsoleCommand.
- Pass `cargo check` and `cargo test`.
- No cheats, no dummy implementations.

## Current Parent
- Conversation ID: 0938fee6-a149-4028-954d-d64c1f44f0ec
- Updated: 2026-11-12T04:30:00Z

## Task Summary
- **What to build**:
  - F1: Long-lived GPUI Application, window hide on close via Win32 ShowWindow(hwnd, SW_HIDE) returning false, get_console_hwnd() with PID check, restore on ConsoleCommand::Open via ShowWindow(hwnd, SW_RESTORE) + SetForegroundWindow + window.activate_window().
  - F2: Late SMTC cover byte detection via PlaySnapshot::cover_key() incorporating cover_bytes length, update UI polling loop, and unit test in src/monitor.rs.
  - F3: Derive PartialEq, Eq on AppStateResponse, PlaySnapshot, PublicConfig, ConfigData, ConsoleCommand. Decouple toast expiration to async timer in show_toast. Remove passive check from render(). Gate cx.notify() in 500ms loop. Add unit tests for equality.
- **Success criteria**:
  - `cargo check --bins --lib` passes with 0 errors.
  - `cargo test --lib`, `cargo test --bin MusicHub`, and `cargo test --test tier1_feature_tests` all pass cleanly.
- **Interface contracts**: `e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md`
- **Code layout**: `src/ui/mod.rs`, `src/monitor.rs`, `src/app.rs`, `src/config.rs`

## Key Decisions Made
- Used Win32 `FindWindowW` and `FindWindowExW` with PID verification (`GetWindowThreadProcessId`) to locate console HWND without collisions.
- Derived `PartialEq, Eq` cleanly on all data structs.
- Replaced passive `render()` toast expiration check with proactive async timer task spawned in `show_toast` and `ConsoleView::new` with timestamp generation matching.
- Gated `cx.notify()` on `state_changed || cover_changed || focus_changed`.

## Artifact Index
- `DISPATCH.md` — assignment and tasks
- `progress.md` — liveness heartbeat and progress tracking
- `handoff.md` — final handoff report

## Change Tracker
- **Files modified**:
  - `src/config.rs`: Added `PartialEq, Eq` to `ConfigData` and `PublicConfig`.
  - `src/monitor.rs`: Added `PartialEq, Eq` to `PlaySnapshot`, implemented `cover_key(&self) -> String`, added unit tests `test_cover_key_late_bytes_divergence` and `test_play_snapshot_equality`.
  - `src/app.rs`: Added `PartialEq, Eq` to `AppStateResponse` and `ConsoleCommand`, added unit test `test_app_state_response_equality`.
  - `src/ui/mod.rs`: Added `get_console_hwnd()`, restructured `run_console_loop` to single long-lived GPUI application with `on_window_should_close` returning `false` + `SW_HIDE`, restored with `SW_RESTORE` + `SetForegroundWindow` + `window.activate_window()`, updated cover change key to `n.cover_key()`, decoupled toast expiration via `cx.spawn(...)` timer, removed passive check in `render()`, gated `cx.notify()` in 500ms loop.
- **Build status**: PASS (`cargo check --bins --lib`, `cargo test --lib`, `cargo test --bin MusicHub`, `cargo test --test tier1_feature_tests`)
- **Pending issues**: None

## Quality Status
- **Build/test result**: Pass (9 unit tests in lib, 9 unit tests in bin, 75 tests in tier1)
- **Lint status**: 0 compiler warnings/errors in owned files
- **Tests added/modified**: `test_cover_key_late_bytes_divergence`, `test_play_snapshot_equality`, `test_app_state_response_equality`

## Loaded Skills
None
