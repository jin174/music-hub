# Handoff Report: Survey of R2 and R3

**Date**: 2026-11-12  
**Agent**: teamwork_explorer (`explorer_survey_2`)  
**Mission**: Codebase investigation of R2 (State Synchronization & Error Visibility) and R3 (TextInput Component Boundary & Focus Fixes)  
**Detailed Survey File**: `e:\杂项\music-hub\.agents\teamwork\explorer_survey_2\survey_r2_r3.md`

---

## 1. Observation

### R2: State Synchronization & Error Visibility
1. **`last_error` in `AppStateResponse` vs UI**:
   - In `src/app.rs:20`, `pub struct AppStateResponse` includes `pub last_error: Option<String>`.
   - In `src/scrobbler.rs:150`, scrobble failures record the error: `*self.last_error.write().await = Some(err.clone());`.
   - In `src/ui/mod.rs:199`, `ConsoleView::new()` initializes `last_error: None`.
   - In `src/ui/mod.rs:4038-4067`, the 500ms background loop regularly fetches `hub_c.state().await` and sets `view.state = new_state;`.
   - In `src/ui/mod.rs:716-720`, `render_sidebar()` displays `format!("工作模式 (待打卡 {} 首)", self.state.pending)`.
   - Grep search for `last_error` across `src/ui/mod.rs` showed exactly 1 occurrence (`src/ui/mod.rs:199: last_error: None`), meaning `view.state.last_error` is never rendered anywhere in the interface.
2. **Bottom Player Bar Heart/Favorite Toggle**:
   - In `src/ui/mod.rs:1048-1087`, `now-btn-love` has an on-click handler that unconditionally executes:
     `this.hub.db.set_feedback(&n.title, &n.artist, "like");`
     followed by an optional `lf.love` call and a toast.
   - It does not mutate `this.radar_result`, does not mutate `this.roam_result`, and does not reload `this.feedback_list`.
   - In `src/ui/mod.rs:412-472`, `ConsoleView::toggle_feedback_item()` exists and already updates `radar_result`, `roam_result`, and reloads `this.feedback_list = this.hub.db.get_feedback_list(200)`. However, `toggle_feedback_item` only checks in-memory `radar_result` and `roam_result` for the current feedback, omitting a query to `self.hub.db.get_feedback(&title, &artist)`.
3. **OAuth Callback & Settings Synchronization**:
   - In `src/server.rs:118-128`, upon receiving a valid callback token, `handle_callback` updates `app.cfg`:
     `app.cfg.update(|d| { d.session_key = sk; d.username = name.clone(); });`
     and calls `app.open_console_with_toast(Some("settings"), Some(...));`.
   - In `src/ui/mod.rs:4072-4076`, the console loop catches the command and executes `view.switch_tab(NavTab::from_str(&t), cx);`.
   - In `src/ui/mod.rs:291-307`, `switch_tab(NavTab::Settings)` only updates `self.feedback_list` and `self.autostart_enabled`. It does NOT touch `self.username_input` (`Entity<TextInput>`), leaving the username input field stale while the authorization badge reflects the new username from `hub.cfg.public()`.

### R3: TextInput Component Boundary & Focus Fixes
1. **Horizontal Scrolling**:
   - In `src/ui/input.rs:496-500`, `TextElement::request_layout` sets `style.size.width = relative(1.).into()`.
   - In `src/ui/input.rs:559-574`, `line` is shaped from `display_text`, and cursor/selection positions are calculated as `bounds.left() + line.x_for_index(...)`.
   - In `src/ui/input.rs:619-620`, `line.paint(bounds.origin, ...)` paints at the unshifted element origin.
   - In `src/ui/input.rs:677`, the wrapping `div()` specifies `.overflow_hidden()`.
   - In `src/ui/input.rs:250-266`, `index_for_mouse_position` computes `line.closest_index_for_x(position.x - bounds.left())`.
   - There is no scroll offset field in `TextInput`. Any text exceeding the container width is clipped on the right; the cursor becomes invisible when at the end of the text.
2. **Focus Navigation & Form Shortcuts**:
   - In `src/ui/input.rs:32-48`, `bind_input_keys` does not include `Tab` or `Shift-Tab`.
   - In `src/ui/input.rs:80`, `TextInput::new` initializes `focus_handle: cx.focus_handle()`. In GPUI (`C:\Users\1\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\gpui-0.2.2\src\window.rs:286`), `tab_stop` defaults to `false`.
   - In `src/ui/mod.rs:241-250`, only `track_input` and `artist_input` subscribe to `InputEvent::EnterPressed`. `username_input`, `api_key_input`, and `api_secret_input` have no `EnterPressed` subscriptions.
3. **Password Masking & Unicode Graphemes**:
   - In `src/ui/input.rs:106-118`:
     ```rust
     for ch in self.content.chars() {
         match ch.len_utf8() {
             1 => masked.push('*'),
             2 => masked.push('·'),
             3 => masked.push('•'),
             _ => masked.push_str("****"),
         }
     }
     ```
   - For a 4-byte character (such as emoji), 4 asterisks are pushed.
   - In `src/ui/input.rs:264`, `line.closest_index_for_x` operates on `display_string`. Clicking between the 2nd and 3rd asterisk yields an offset that is not a UTF-8 character boundary in `self.content`.
   - In `src/ui/input.rs:384`, `replace_text_in_range` slices `self.content[0..range.start]`, triggering an immediate panic on non-char boundaries.

---

## 2. Logic Chain

1. **R2.1 Logic**: Scrobbler errors are actively captured into `AppStateResponse::last_error` and received in `view.state`. Because `src/ui/mod.rs` lacks any UI rendering logic for `self.state.last_error`, users cannot diagnose scrobbler failures or know why pending counts increase. Adding visible error callouts in the sidebar, recent view, and settings view directly solves this.
2. **R2.2 Logic**: `now-btn-love` performs a direct database write without going through in-memory state synchronization. In-memory data structures (`radar_result`, `roam_result`, and `feedback_list`) hold independent copies of feedback state. Updating them in `toggle_feedback_item` and pointing `now-btn-love` to this unified method guarantees immediate cross-view consistency.
3. **R2.3 Logic**: `Entity<TextInput>` maintains its own internal text buffer (`content: SharedString`). Updating config in `src/server.rs` does not mutate the GPUI entity unless `input.set_text(...)` is explicitly called. Adding a synchronization call in `switch_tab(NavTab::Settings)` and upon receiving `ConsoleCommand::Open` ensures `username_input` stays in sync with OAuth credentials.
4. **R3.1 Logic**: Without an X-axis offset, text layout is pinned to the element's left border, causing trailing content and cursors longer than the box width to be clipped by `overflow_hidden()`. Introducing a `scroll_offset: Pixels` in `TextInput`, adjusting it dynamically in `prepaint` to keep the cursor within `[scroll_offset, scroll_offset + container_width]`, and translating `line.paint`, cursor/selection quads, and mouse hit-testing by `scroll_offset` fully enables horizontal scrolling.
5. **R3.2 Logic**: GPUI provides native `window.focus_next()` and `window.focus_prev()` that automatically traverse elements registered with `tab_stop: true`. Setting `tab_stop(true)` on `TextInput`'s focus handle and mapping `Tab`/`Shift-Tab` keys to these methods enables tab navigation. Subscribing `ConsoleView` to `username_input`, `api_key_input`, and `api_secret_input` for `InputEvent::EnterPressed` enables Enter-to-save in settings.
6. **R3.3 Logic**: Using `chars().len_utf8()` creates mismatched glyph counts and non-char byte boundaries for 4-byte characters. Mapping each grapheme cluster to a uniform `'•'` bullet and translating offsets between `content` grapheme boundaries and `display_string` bullet offsets guarantees both visual consistency and panic-free string slicing.

---

## 3. Caveats

- **Network Mode**: The investigation was conducted in local read-only inspection mode. Actual Last.fm network requests were not executed against live Last.fm endpoints during the survey.
- **R1 and R4 Scope**: Issues pertaining to R1 (window lifecycle, SMTC cover bytes, 500ms notify loop) and R4 (900x600 layout, poster modals, titlebar dark styling) are investigated by peer survey agents and were excluded from this survey's primary focus, although interactions with the 500ms poll loop were reviewed to verify `last_error` propagation.
- **No other caveats.**

---

## 4. Conclusion

All root causes for R2 and R3 have been pinpointed to exact lines and functions in `src/ui/mod.rs`, `src/ui/input.rs`, `src/server.rs`, `src/app.rs`, and `src/scrobbler.rs`. Concrete code architectural solutions have been designed and documented in `e:\杂项\music-hub\.agents\teamwork\explorer_survey_2\survey_r2_r3.md`. These changes require no new external dependencies and align cleanly with GPUI 0.2's element and text layout system.

---

## 5. Verification Method

To verify the codebase and future implementation:
1. **Compilation Check**:
   ```powershell
   cargo check
   ```
   Must compile cleanly without errors or warnings.
2. **Unit Tests**:
   ```powershell
   cargo test
   ```
   Existing unit tests (e.g. `normalize::tests`, `db::tests`) must pass.
3. **Manual / Integration Verification Scenarios**:
   - **R2.1**: Set an invalid Last.fm session key in config; verify that an error banner appears in the sidebar and recent plays view showing the Last.fm API error message instead of silently accumulating pending songs.
   - **R2.2**: Play a track, click the bottom bar heart icon; verify that the track immediately shows as liked in the active Radar list, active Roaming list, and Settings blacklist/favorites list. Click again; verify that like is toggled off and removed from all three views.
   - **R2.3**: Initiate OAuth flow via browser; upon redirection to `/callback`, verify that the client window activates to Settings and `username_input` immediately displays the authorized username.
   - **R3.1**: Paste a 150-character NetEase music link into `track_input`; verify that the input smoothly scrolls horizontally, keeping the cursor and trailing characters visible and selectable.
   - **R3.2**: Press `Tab` in the Radar view to switch between track and artist inputs; press `Tab` in the Settings view to cycle across username, API key, and secret fields; press `Enter` in settings inputs to verify that values are saved and a confirmation toast appears.
   - **R3.3**: Enter passwords containing emojis (e.g. `🔥`), CJK characters, and symbols; verify uniform bullet masking and that clicking anywhere in the password field never triggers a UTF-8 slice panic.
