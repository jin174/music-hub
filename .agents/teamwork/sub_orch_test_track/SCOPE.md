# Scope: E2E Testing Track

## Architecture & Test Strategy
- **Target System**: `Music Hub` (Local music hub, Windows SMTC monitor, GPUI console, Last.fm scrobbler, Win32 Dynamic Island widget, SQLite database).
- **Test Philosophy**: Opaque-box, requirement-driven automated testing derived strictly from `ORIGINAL_REQUEST.md` (R1–R4) and `PROJECT.md` (F1–F15).
- **Test Execution Environment**: Windows headless-compatible tests executable via `cargo test`.
  - Modules tested via unit tests and integration tests in `tests/`.
  - Where GPUI graphics/window handles or Win32 message loops are involved, tests verify the underlying state engines, coordinate scaling math, change detection keys, grapheme cluster masking, parser normalization, and IPC protocols without requiring active interactive display hardware.

## Feature Inventory & Test Mapping
| # | Feature | Description | Requirement | Target Tiers |
|---|---------|-------------|-------------|--------------|
| F1 | Long-lived GPUI Application | Single application run, hide/show window lifecycle, no dispatcher/class crash | R1 | Tiers 1, 2, 3, 4 |
| F2 | Late SMTC Cover Byte Detection | Cover cache key includes `cover_bytes` presence and length for late thumbnail decode | R1 | Tiers 1, 2, 3 |
| F3 | Repaint Gating & Decoupled Toasts | Decouple toast expiration to async timer, gate `cx.notify()` on state/cover changes | R1 | Tiers 1, 2, 3 |
| F4 | Scrobble Error Visibility | Expose `last_error` from `AppStateResponse` in UI sidebar, recent view, and settings | R2 | Tiers 1, 2, 3, 4 |
| F5 | Bottom Bar Heart Synchronization | Unified feedback toggle updating SQLite, active radar list, roaming list, blacklist | R2 | Tiers 1, 2, 3, 4 |
| F6 | OAuth Settings Synchronization | Synchronize `username_input` display text and badge on browser authorization callback | R2 | Tiers 1, 2, 3, 4 |
| F7 | TextInput Horizontal Scrolling | Compute horizontal `scroll_offset` so long strings and trailing cursor stay visible | R3 | Tiers 1, 2, 3 |
| F8 | Focus Navigation & Enter Handling | Enable `tab_stop(true)`, bind `Tab`/`Shift-Tab`, bind Enter to save in settings | R3 | Tiers 1, 2, 3 |
| F9 | Password Masking Unicode Graphemes | Map Unicode grapheme clusters 1:1 to bullets, avoid byte offset split and panics | R3 | Tiers 1, 2, 3 |
| F10 | 900x600 Responsive Layout | Header `.flex_wrap().gap_2()` preventing button/title collision at min bounds | R4 | Tiers 1, 2, 3 |
| F11 | Modal Backdrop & Esc Dismissal | Dismiss poster modal on backdrop click and `Esc` key press | R4 | Tiers 1, 2, 3 |
| F12 | History Record Deletion Safeguard | Two-step inline confirmation (`confirm_delete_id`) before permanent deletion | R4 | Tiers 1, 2, 3 |
| F13 | Empty States & Click Feedback | Empty state cards for radar/roam; play toasts; artist hover; disabled cursor | R4 | Tiers 1, 2, 3 |
| F14 | Poster Completion Rate & GDI Emojis | Calculate actual completion % $(c/n) \times 100\%$; replace emojis with clean badges | R4 | Tiers 1, 2, 3, 4 |
| F15 | Win32 Dark Titlebar & DPI Scaling | DWM dark titlebar; remove `SetProcessDPIAware()`; scale Dynamic Island by DPI | R4 | Tiers 1, 2, 3 |

## Test Tier Decomposition
| Tier | Methodology | Coverage Requirement |
|------|-------------|----------------------|
| **Tier 1: Feature Coverage** | Category-Partition & Happy Path | >=5 test cases per feature across F1–F15 (>=75 tests) |
| **Tier 2: Boundary & Corner Cases** | Boundary Value Analysis | >=5 test cases per feature across F1–F15 (>=75 tests) |
| **Tier 3: Cross-Feature Combinations** | Pairwise Combinatorial Testing | Major feature interactions (>=15 tests) |
| **Tier 4: Real-World Scenarios** | Workload & End-to-End User Journeys | Realistic multi-step listening workflows (>=8 tests) |

## Deliverables
1. `TEST_INFRA.md` at project root (`e:\杂项\music-hub\TEST_INFRA.md`).
2. Automated test suite in `tests/` compiling and passing with `cargo test`.
3. `TEST_READY.md` at project root (`e:\杂项\music-hub\TEST_READY.md`).
4. `handoff.md` and gate completion report to parent orchestrator.
