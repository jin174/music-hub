# Project: Music Hub Bug Fixes, Hazards & UI Polishing

## Architecture
Music Hub is a local music listening hub and scrobbler built with GPUI 0.2.2 and Win32 APIs in Rust.
- **Subsystems**:
  - `src/main.rs`: Application entry point, single-instance mutex, worker threads, GPUI console launcher.
  - `src/app.rs`: Central coordinator (`HubApp`), state querying (`AppStateResponse`), radar and roaming playlist generator.
  - `src/monitor.rs`: Windows SMTC listener tracking active playback, thresholds, and thumbnail bytes extraction.
  - `src/scrobbler.rs`: Last.fm scrobbler queue and API client, recording `last_error`.
  - `src/server.rs`: Local IPC HTTP server handling web callbacks (OAuth), commands, and status.
  - `src/db.rs`: SQLite storage for plays, scrobble queue, feedback (loved/blacklisted).
  - `src/ui/mod.rs`: GPUI console (`ConsoleView`), navigation tabs, bottom player bar, poster modal, polling loop.
  - `src/ui/input.rs`: Custom GPUI `TextInput` component.
  - `src/poster.rs`: Win32 GDI poster generator for annual/monthly listening stats.
  - `src/widget.rs`: Win32 layered translucent Dynamic Island widget.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| F1 | Long-lived GPUI Application | Single `Application::new().run(...)` preventing window class/dispatcher corruption; hide on close and restore on wake | M1 | R1 survey |
| F2 | Late SMTC Cover Byte Detection | Include `cover_bytes` presence and length in cover change detection key so late thumbnails decode | M1 | R1 survey |
| F3 | Repaint Gating & Decoupled Toasts | Decouple toast expiration to async timer in `show_toast`; gate `cx.notify()` on state/cover mutations | M1 | R1 survey |
| F4 | Scrobble Error Visibility | Expose `last_error` from `AppStateResponse` in UI sidebar, recent view, and settings | M2 | R2 survey |
| F5 | Bottom Bar Heart Synchronization | Unified `toggle_feedback_item` updating SQLite, active radar list, roaming list, and blacklist | M2 | R2 survey |
| F6 | OAuth Settings Synchronization | Synchronize `username_input` display text and badge when browser authorization completes | M2 | R2 survey |
| F7 | TextInput Horizontal Scrolling | Add `scroll_offset` and viewport translation so long strings and trailing cursor stay visible | M3 | R3 survey |
| F8 | Focus Navigation & Enter Handling | Enable `tab_stop(true)`, bind `Tab`/`Shift-Tab` to GPUI focus navigation, bind Enter to save in settings | M3 | R3 survey |
| F9 | Password Masking Unicode Graphemes | Map Unicode grapheme clusters to uniform bullets to avoid byte offset misalignment and slice panics | M3 | R3 survey |
| F10 | 900x600 Responsive Layout | Add `.flex_wrap().gap_2()` to Roaming card and History toolbar to prevent collision at min size | M4 | R4 survey |
| F11 | Modal Backdrop & Esc Dismissal | Dismiss poster modal on backdrop click and `Esc` key press | M4 | R4 survey |
| F12 | History Record Deletion Safeguard | Add two-step inline delete confirmation (`confirm_delete_id`) or undo protection | M4 | R4 survey |
| F13 | Empty States & Click Feedback | Add empty state visuals for roaming/radar; toasts for loved/history plays; artist hover; disabled cursor | M4 | R4 survey |
| F14 | Poster Completion Rate & GDI Emojis | Calculate actual completion percentage $(c / n) \times 100\%$; replace medal emojis with badges/clean symbols | M4 | R4 survey |
| F15 | Win32 Dark Titlebar & DPI Scaling | Set DWM dark mode/caption color on main window; remove `SetProcessDPIAware()`; scale Dynamic Island by DPI | M4 | R4 survey |
| F16 | Automated Test Suite & Coverage | Unit tests and integration tests verifying all bug fixes, boundary conditions, and regressions | M5 / Test Track | AC & Survey |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Core Application Lifecycle & SMTC Covers | F1, F2, F3 (`src/ui/mod.rs`, `src/monitor.rs`, `src/app.rs`, `src/config.rs`) | none | PLANNED |
| M2 | State Synchronization & Error Visibility | F4, F5, F6 (`src/ui/mod.rs`, `src/server.rs`, `src/scrobbler.rs`, `src/app.rs`) | M1 | PLANNED |
| M3 | TextInput Component Boundary, Focus & Masking | F7, F8, F9 (`src/ui/input.rs`, `src/ui/mod.rs`) | none | PLANNED |
| M4 | Interaction Safeguards & UI Polishing | F10, F11, F12, F13, F14, F15 (`src/ui/mod.rs`, `src/poster.rs`, `src/widget.rs`) | M1, M2 | PLANNED |
| M5 | Final Verification, Adversarial Hardening & Audit | F16, 100% automated test pass, adversarial tests, forensic audit | M1, M2, M3, M4 | PLANNED |

## Interface Contracts
### Main Window ↔ Dynamic Island & Tray
- Window hide: `on_window_should_close` executes Win32 `ShowWindow(hwnd, SW_HIDE)` and returns `false`.
- Window wake: Tray/Island sends `ConsoleCommand::Open { tab, toast }` through `cmd_rx` channel; GPUI background listener executes `ShowWindow(hwnd, SW_RESTORE)`, `SetForegroundWindow(hwnd)`, `window.activate_window()`, switches tab, and displays toast.
- Win32 Titlebar: Set `DWMWA_USE_IMMERSIVE_DARK_MODE` (20, 19 fallback) and `DWMWA_CAPTION_COLOR` (35, color `0x0017110d`).

### Player Bar ↔ Recommendation Views
- `toggle_feedback_item(title, artist)`: Queries DB to determine if liked; toggles "like" or clears; updates in-memory `radar_result` and `roam_result` track items; reloads `feedback_list`; calls Last.fm love/unlove if configured; triggers notification.

### TextInput ↔ Form Inputs
- Focus: `tab_stop(true)` on `TextInput`'s focus handle; `Tab` calls `window.focus_next(cx)`, `Shift-Tab` calls `window.focus_prev(cx)`.
- Enter: Dispatches `InputEvent::EnterPressed`; form views subscribe to trigger action (search in Radar, save in Settings).
- Layout: `scroll_offset: Pixels` tracks horizontal translation; `prepaint` clamps offset so cursor remains within view bounds; `paint` translates bounds origin by `-scroll_offset`.

## Code Layout
- `src/ui/mod.rs`: Window lifecycle, `ConsoleView`, UI rendering, navigation, feedback sync, toasts.
- `src/ui/input.rs`: `TextInput` component, layout, mouse hit-testing, cursor, scrolling, key bindings, masking.
- `src/monitor.rs`: SMTC media monitoring, thumbnail extraction, play snapshot.
- `src/server.rs`: IPC web server, OAuth callback, config update, console wake commands.
- `src/poster.rs`: Win32 GDI poster rendering, stats completion calculation, typography badges.
- `src/widget.rs`: Win32 Dynamic Island, DPI awareness, per-monitor scaling, `WM_DPICHANGED`.
- `tests/`: Automated unit & integration tests covering R1-R4.
