# Scope: Milestone 1 (Core Application Lifecycle & SMTC Covers)

## Architecture
Milestone 1 addresses the core application loop, window lifecycle, cover change detection, and repaint frequency in the GPUI frontend and its interaction with the media monitor:
- `src/ui/mod.rs`: `run_console_loop`, `ConsoleView`, background polling loop, toast timer, window hide/show callbacks.
- `src/monitor.rs`: `PlaySnapshot`, thumbnail extraction.
- `src/app.rs`: `AppStateResponse` (derive `PartialEq`).
- `src/config.rs`: `PublicConfig` (derive `PartialEq`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | F1: Long-lived GPUI Application | Single `Application::new().run(...)` preventing window class/dispatcher corruption; hide on close and restore on wake | M1 | R1 survey |
| 2 | F2: Late SMTC Cover Byte Detection | Include `cover_bytes` presence and length in cover change detection key so late thumbnails decode | M1 | R1 survey |
| 3 | F3: Repaint Gating & Decoupled Toasts | Decouple toast expiration to async timer in `show_toast`; gate `cx.notify()` on state/cover mutations | M1 | R1 survey |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Core Application Lifecycle & SMTC Covers | F1, F2, F3 (`src/ui/mod.rs`, `src/monitor.rs`, `src/app.rs`, `src/config.rs`) | none | IN_PROGRESS |

## Interface Contracts
### Main Window ↔ Dynamic Island & Tray
- Window hide: `on_window_should_close` executes Win32 `ShowWindow(hwnd, SW_HIDE)` and returns `false`.
- Window wake: Tray/Island sends `ConsoleCommand::Open { tab, toast }` through `cmd_rx` channel; GPUI background listener executes `ShowWindow(hwnd, SW_RESTORE)`, `SetForegroundWindow(hwnd)`, `window.activate_window()`, switches tab, and displays toast.

### State Equality & Cover Detection Key
- `AppStateResponse`, `PlaySnapshot`, `PublicConfig` derive `PartialEq`.
- Cover Key format: `format!("{}:{}:{}", n.play_id, n.cover, b_len)` where `b_len` is `n.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0)`.
- Toast auto-clear: `cx.spawn(...)` with `cx.background_executor().timer(Duration::from_millis(3500)).await`, clearing `view.toast` if timestamp matches.

## Code Layout (Write Ownership)
- `src/ui/mod.rs`
- `src/monitor.rs`
- `src/app.rs`
- `src/config.rs`
