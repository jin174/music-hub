# Handoff Report: R4 & Test Architecture Survey

## 1. Observation

### R4.1 Layout Constraints & Toolbars (900×600)
- `src/ui/mod.rs:3993`: `window_min_size: Some(size(px(900.0), px(600.0)))`
- `src/ui/mod.rs:598-600`: `div().w(px(216.0)).h_full()` establishes a fixed 216px sidebar.
- `src/ui/mod.rs:1685-1778`: Roam top control card uses `.flex().flex_row().items_center().justify_between().gap_3()` containing title/subtitle (43 characters ≈ 516px) and 3 buttons (~450px total width), without `.flex_wrap()`. Available content width at 900px window width is only $900 - 216 - 40 - 32 = 612\text{px}$.
- `src/ui/mod.rs:2864-3000`: History header toolbar uses `.flex().flex_row().items_center().justify_between()` containing title (~460px) and 4 buttons (~500px), without `.flex_wrap()`.

### R4.2 Modals and Destructive Actions
- `src/ui/mod.rs:3808-3819`: Poster modal backdrop listener is `.on_mouse_down(MouseButton::Left, |_, _, _| {})`. Clicking backdrop performs no action.
- `src/ui/mod.rs:522`: `ConsoleView` root element tracks focus via `.track_focus(&self.focus_handle)`, but lacks any `escape` key handler to dismiss `self.wrapped_poster`.
- `src/ui/mod.rs:3184-3199`: History row delete button `✕` directly invokes `this.hub.db.delete_play(play_id); this.recent_rows = this.hub.db.recent(150);` on a single click without confirmation prompt or undo protection.

### R4.3 Empty States and Click Feedback
- `src/ui/mod.rs:1820-1961`: When `self.roam_result.is_none()`, roam view renders no content below top card. When `res.tracks.is_empty()`, only table header is rendered.
- `src/ui/mod.rs:1242-1396`: When `self.radar_result.is_none()`, radar view renders no content below search presets.
- `src/ui/mod.rs:2732-2734` & `src/ui/mod.rs:3151-3153`: Play buttons in loved and history lists call `HubApp::play_in_client(id)` directly via `move |_, _, _| {}` without `cx.listener` or toast notifications. In contrast, radar track play (`src/ui/mod.rs:1633-1636`) calls `this.show_toast(format!("▶ 已唤起网易云播放：{}", t_play), cx)`.
- `src/ui/mod.rs:2466-2473`: Top artists row has `.cursor_pointer()` but has no `.hover(...)` background style or padding.
- `src/ui/mod.rs:1026, 1052`: Bottom bar buttons `now-btn-radar` and `now-btn-love` unconditionally use `.cursor_pointer()` and hover background styles regardless of `has_song`.

### R4.4 Poster Rendering (Stats & Emojis)
- `src/poster.rs:508`: The completion badge is hardcoded: `draw_str(..., "100% 真爱完播", true)`. `loved` has fields `n` (total plays) and `c` (completed plays) in `StatsTrackItem` (`src/app.rs:109-116`).
- `src/poster.rs:52-69`: `make_font` creates `"Microsoft YaHei UI"` with Win32 GDI `CreateFontW`.
- `src/poster.rs:430`: `let medals = ["🥇", "🥈", "🥉"];` renders surrogate pair emojis via GDI `DrawTextW` (`src/poster.rs:365`), causing tofu / missing glyph boxes.

### R4.5 System Visual Consistency & DPI
- `src/ui/mod.rs:3987-4010`: The main window does not set DWM dark mode or caption color attributes, defaulting to a standard light titlebar on Windows 10/11.
- `src/widget.rs:137`: Calls legacy `let _ = SetProcessDPIAware();`.
- `src/widget.rs:33-34, 150-163, 893-940`: Dynamic Island uses fixed 380×72 pixel dimensions and does not query `GetDpiForWindow` or handle `WM_DPICHANGED`.

### Existing Tests & Test Commands
- Executed `cargo test`:
  ```
  running 6 tests
  test normalize::tests::test_norm ... ok
  test normalize::tests::test_is_junk_stream ... ok
  test normalize::tests::test_artist_variants ... ok
  test normalize::tests::test_clean_artist ... ok
  test db::tests::test_db_lifecycle ... ok
  test normalize::tests::test_clean_title ... ok
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```

---

## 2. Logic Chain

1. **Layout**:
   - Because the minimum window width is 900px (`src/ui/mod.rs:3993`) and sidebar width is 216px (`src/ui/mod.rs:598`), the maximum content width is $900 - 216 - 40 - 32 = 612\text{px}$.
   - Because both the roaming card (`src/ui/mod.rs:1685`) and recent toolbar (`src/ui/mod.rs:2864`) arrange their title blocks (~460–516px) and action buttons (~450–500px) in non-wrapping flex rows (`flex_row().justify_between()`), total element width (~960–978px) exceeds available width by >340px.
   - Therefore, without `.flex_wrap()` or responsive flow, element collision, text truncation, and toolbar button clipping occur at 900×600.

2. **Modals & Destructive Actions**:
   - Because `render_poster_modal` (`src/ui/mod.rs:3818`) attaches a no-op handler `|_, _, _| {}` to mouse-down on the backdrop, mouse clicks on the overlay are swallowed and cannot dismiss the modal.
   - Because `ConsoleView` lacks an `escape` key handler, keyboard users cannot exit the modal.
   - Because `rec-del` (`src/ui/mod.rs:3194`) immediately deletes plays on a single click without state confirmation or undo buffer, users are vulnerable to accidental data loss.

3. **Feedback & Empty States**:
   - Because `HubApp::play_in_client` is invoked directly without `cx.listener` in loved (`src/ui/mod.rs:2732`) and history (`src/ui/mod.rs:3151`), no toast notifications appear on play.
   - Because top artist rows lack hover background styling (`src/ui/mod.rs:2466`), interactive affordance is absent.
   - Because bottom bar buttons (`src/ui/mod.rs:1026, 1052`) use `.cursor_pointer()` even when `has_song == false`, users receive false interactivity signals.

4. **Poster & Emojis**:
   - Because `src/poster.rs:508` hardcodes `"100% 真爱完播"`, it ignores the actual completion ratio $(c / n) \times 100\%$.
   - Because Win32 GDI `DrawTextW` with `"Microsoft YaHei UI"` cannot render color emojis or unlinked surrogate pairs, emojis like `🥇`, `🥈`, `🥉` produce missing glyph boxes.

5. **Titlebar & DPI**:
   - Because `DwmSetWindowAttribute` is never invoked on the GPUI window, Windows renders the default light titlebar.
   - Because `widget.rs:137` calls `SetProcessDPIAware()`, it conflicts with PerMonitorV2 DPI awareness.
   - Because widget dimensions are fixed at 380×72, high-DPI displays render the island undersized.

---

## 3. Caveats

1. **GPUI Headless Testing**: GPUI view rendering relies on the platform dispatcher and window server; end-to-end GPUI element tree rendering cannot easily be executed in a headless CI environment without a display. However, all underlying calculation logic (completion rates, grapheme masking, DPI math, delete state machine) can be unit-tested cleanly with `cargo test`.
2. **Win32 DWM Feature Availability**: DWM immersive dark mode attribute 20 was introduced in Windows 10 build 19041 (2004) and Windows 11; older builds (1809/1903) used attribute 19. Both should be attempted sequentially for maximum compatibility. Caption color (attribute 35) requires Windows 11 build 22000+.
3. **Emoji Rendering in GDI**: GDI memory DIBSections do not support DirectWrite color fonts. Typography badges (e.g. `"NO.1"`, `"NO.2"`, `"NO.3"`) and CJK-safe typographic symbols provide 100% stability across all Windows versions.

---

## 4. Conclusion

All reported R4 defects and missing safeguards are verified in the codebase with clear, localized root causes:
1. **Layout**: Add `.flex_wrap().gap_2()` to Roaming (`src/ui/mod.rs:1685`) and History (`src/ui/mod.rs:2864`) toolbars.
2. **Modals & Safeguards**: Update backdrop click to dismiss modal (`src/ui/mod.rs:3818`), bind `escape` key in `ConsoleView`, and introduce two-step inline delete confirmation (`confirm_delete_id`) in `src/ui/mod.rs:3184`.
3. **Empty States & Feedback**: Add empty/loading state cards to Roaming and Radar; add toasts to loved and history play actions; add hover styling to top artists; set `.cursor_default()` for bottom bar controls when inactive.
4. **Poster**: Calculate `(loved.c / loved.n) * 100%` in `src/poster.rs:508`; replace medal emojis with typography badges (`NO.1`, `NO.2`, `NO.3`) and sanitize emojis for GDI.
5. **Titlebar & DPI**: Apply DWM immersive dark mode and `#0d1117` caption color to main HWND; remove `SetProcessDPIAware()` in `src/widget.rs`; scale Dynamic Island coordinates and fonts by monitor DPI factor and handle `WM_DPICHANGED`.
6. **Tests**: Expand automated test coverage with headless unit tests covering poster completion rates, emoji sanitization, DPI scaling math, input masking, and state updates.

---

## 5. Verification Method

1. **Build & Existing Tests**:
   - `cargo test`: Ensure all tests pass cleanly.
   - `cargo check`: Verify compilation without warnings.
2. **Layout Verification**:
   - Inspect `src/ui/mod.rs` to verify `.flex_wrap()` on roam top card (line 1685) and history toolbar (line 2864).
   - Resize window to 900×600 and confirm buttons wrap below titles without overlapping or clipping.
3. **Modal & Delete Safeguard Verification**:
   - Open poster modal; click backdrop or press `Esc` to verify dismissal.
   - Click `✕` on a history row: verify text changes to `"确认删除?"` and record is only deleted upon second click.
4. **Empty State & Feedback Verification**:
   - Navigate to Roam tab before generation: verify placeholder card displays instead of empty space.
   - Click play on loved item and history row: verify `"▶ 已唤起客户端播放：..."` toast appears.
   - Hover top artist row: verify background highlight (`rgb(0x1f2630)`).
   - When no song is playing: verify bottom bar radar and heart buttons show default/disabled cursor.
5. **Poster & Visual Consistency Verification**:
   - Generate poster: verify completion rate reflects actual song statistics (e.g. `"80% 真爱完播"`) instead of `"100% 真爱完播"`.
   - Verify top 3 artists render with clean badges (`NO.1`, `NO.2`, `NO.3`) with zero missing glyph boxes.
   - Launch application on Windows: verify titlebar background matches `#0d1117`.
   - On 150%/200% scaling display: verify Dynamic Island scales proportionally without undersizing or clipping.
