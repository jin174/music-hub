# Original User Request

## Initial Request — 2026-11-12T01:18:42Z

Fix the identified bugs, interaction hazards, and visual inconsistencies across the music-hub application, verify changes with automated tests and builds, and report the results.

Working directory: e:\杂项\music-hub
Integrity mode: development

## Requirements

### R1. Core Application Lifecycle and Media Monitoring Fixes
- Fix window re-opening lifecycle in `src/ui/mod.rs`: maintain a single long-lived GPUI `Application` run instead of restarting `Application::new().run(...)` inside an outer loop on re-wake from tray/dynamic island, preserving in-memory settings, input states, and cached radar/roaming results without window class or dispatcher corruption.
- Fix asynchronous SMTC cover updates in `src/ui/mod.rs` and `src/monitor.rs`: incorporate `cover_bytes` presence and length into change detection so late-arriving image bytes trigger decoding even when initial song metadata had no web cover URL.
- Eliminate unconditional 500ms full UI tree repaints (`cx.notify()`) in `src/ui/mod.rs`: gate notifications on actual state changes or decouple toast expiration checks from full console view invalidation.

### R2. State Synchronization and Error Visibility Fixes
- Expose `last_error` from `AppStateResponse` in `src/ui/mod.rs`: display Last.fm/scrobble failure reasons to the user instead of silently accumulating pending counts.
- Synchronize favorite/heart state when toggled from the bottom player bar (`src/ui/mod.rs`): update in-memory radar, roaming, and blacklist views in addition to the SQLite database.
- Refresh settings text fields on OAuth completion (`src/server.rs`): synchronize `username_input` display text when credentials are saved back from the browser authorization flow.

### R3. TextInput Component Boundary & Focus Fixes
- Fix `TextInput` (`src/ui/input.rs`) horizontal scrolling: implement horizontal scroll offset when text exceeds container width so cursor and trailing characters remain visible.
- Add focus navigation and shortcut handling: support `Tab` cycling between inputs and `Enter` key handling to trigger save/submit in settings forms.
- Fix password masking in `display_string`: ensure character masking accurately matches Unicode graphemes to prevent byte offset and cursor position misalignment.

### R4. Interaction Safeguards and UI Polishing
- Adjust 900x600 layout: add wrapping or responsive flow to roaming and history toolbars so buttons and titles do not collide in minimum window sizes.
- Enhance modal and destructive action safeguards: allow closing poster modals via background click and `Esc` key; add confirmation or undo prompts for deleting history records.
- Complete empty states and click feedback: provide empty state visuals for empty roaming lists and initial radar/roaming loads; add toast feedback for play actions in true love and history lists; add hover highlights to top artists; set proper non-pointer cursor for disabled bottom bar controls when no track is active.
- Correct poster rendering: replace hardcoded "100% 真爱完播" with actual calculated completion rates; ensure stable emoji rendering under Win32 GDI or fallback gracefully to prevent missing glyph boxes.
- Improve system visual consistency: apply dark mode window titlebar styling to match `#0d1117` theme; ensure dynamic island dimensions scale with PerMonitorV2 monitor DPI and remove redundant `SetProcessDPIAware()`.

## Acceptance Criteria

### Lifecycle & Core Functionality
- [ ] Closing the window and reopening from system tray or dynamic island does not cause GPUI panics, dispatcher crashes, or state resets.
- [ ] SMTC cover bytes arriving after track change successfully trigger decode and appear in the bottom player bar.
- [ ] Scrobble errors are clearly rendered in the UI with actionable error messages.
- [ ] Unconditional 500ms `cx.notify()` loop is eliminated; view re-renders only on state mutations or toast timeouts.
- [ ] Bottom bar heart toggle immediately reflects across active radar/roaming song lists.
- [ ] OAuth browser callback updates both the status badge and `username_input` in settings.

### Input & Interaction Controls
- [ ] Pasting long text (such as NetEase Cloud Music share links) into `TextInput` allows horizontal scrolling to the end without visual clipping.
- [ ] Pressing `Tab` cycles focus between inputs; `Enter` in settings inputs triggers save.
- [ ] Poster modal closes on backdrop click and `Esc` press.
- [ ] History record deletion requires confirmation or offers undo protection.
- [ ] Empty roaming lists display an informative empty state instead of an unstyled dark void.

### Visual Polish & Platform Integration
- [ ] Roaming top bar and history toolbar display cleanly without overlapping elements at 900x600 window size.
- [ ] Poster completion rate reflects actual song statistics instead of a static 100% string.
- [ ] Win32 title bar matches dark theme appearance.
- [ ] Dynamic island widget respects PerMonitorV2 display scaling on high-DPI monitors without clipping or undersizing.

### Build and Verification
- [ ] Project compiles cleanly via `cargo check`.
- [ ] Existing and updated unit tests pass via `cargo test`.

## Follow-up — 2026-11-12T04:32:19Z

User instruction received:
The user has decided to wrap up Milestone 1 and then pause the run cleanly.
Please finalize Milestone 1, ensure all M1 edits compile cleanly (`cargo check`) and M1 tests pass, and do NOT proceed to Milestone 2, Milestone 3, or Milestone 4. Deliver your Milestone 1 completion checkpoint report.


## Follow-up — 2026-11-12T04:31:12Z

User decision: "先跑完当前 Milestone 1 收尾就停" (Finish wrapping up the current Milestone 1 and then pause/stop).

Please follow these exact instructions:
1. Finish Milestone 1 review/verification and ensure all M1 edits in the working tree compile cleanly (`cargo check`) and M1-related tests pass.
2. Do NOT proceed to Milestone 2, Milestone 3, or Milestone 4.
3. Preserve all on-disk plans, progress ledgers (`.agents/`), and the 4-tier test suite (`tests/`) cleanly so work can resume later.
4. Once Milestone 1 is cleanly wrapped up and verified, send me a final checkpoint summary (including exact files changed, verification results observed, and remaining M2–M4 items) and finish your run cleanly.
