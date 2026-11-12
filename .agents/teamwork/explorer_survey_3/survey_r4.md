# Survey Report: R4 (Interaction Safeguards & Visual Consistency) & Test Architecture

**Author**: Explorer Agent Survey 3  
**Date**: 2026-11-12  
**Target Project**: `Music Hub` (Rust GPUI + Win32 SMTC Scrobbler)  
**State Directory**: `e:\杂项\music-hub\.agents\teamwork\explorer_survey_3`  

---

## Executive Summary

This survey analyzes Requirement **R4 (Interaction Safeguards and UI Polishing)** and the **Existing Test & Verification Architecture** for the `Music Hub` codebase. Every finding is anchored directly to source file locations, line numbers, and concrete code paths.

### Core Survey Takeaways
1. **Layout Overflows at 900×600 (R4.1)**: Both `render_roam_view` (`src/ui/mod.rs:1685-1778`) and `render_recent_view` (`src/ui/mod.rs:2864-3000`) implement unwrapped flex containers (`flex_row().justify_between()`) combining wide descriptive headers (~450–516px) and multiple action buttons (~450–500px). At the minimum window boundary (900×600, yielding ~612px content width after subtracting the 216px sidebar and padding), elements collide, text clips, and buttons wrap or overflow unexpectedly.
2. **Modal Backdrop & Esc Key Deficiencies (R4.2a)**: In `render_poster_modal` (`src/ui/mod.rs:3808-3820`), the backdrop has a dummy no-op mouse-down listener (`.on_mouse_down(MouseButton::Left, |_, _, _| {})`). Furthermore, no `Esc` keybinding or `KeyDownEvent` listener exists to dismiss the poster modal.
3. **Destructive History Deletion (R4.2b)**: In `render_recent_view` (`src/ui/mod.rs:3184-3199`), the delete button (`✕`) immediately deletes the row from SQLite (`this.hub.db.delete_play(play_id)`) upon a single click with zero confirmation or undo safety net.
4. **Missing Empty States & Loading Feedback (R4.3a)**: In `render_roam_view` (`src/ui/mod.rs:1820-1961`) and `render_radar_view` (`src/ui/mod.rs:1242-1396`), empty tracks (`res.tracks.is_empty()`) or initial unqueried states (`result.is_none()`) either render nothing or an unstyled empty header table without informative user guidance or loading spinners.
5. **Missing Toast Feedback on Play Actions (R4.3b)**: While radar and roam play buttons trigger toasts (`this.show_toast(...)`), the "loved" list (`src/ui/mod.rs:2722-2736`) and history list (`src/ui/mod.rs:3142-3155`) invoke `HubApp::play_in_client(id)` silently without toast notifications.
6. **Top Artists Lack Hover Feedback (R4.3c)**: Top artists in `render_stats_view` (`src/ui/mod.rs:2466-2485`) are clickable but have no hover styling (`hover:bg(...)`), leaving users unsure if the card is interactive.
7. **Bottom Bar Pointer Cursor When Inactive (R4.3d)**: In `render_now_playing_bar` (`src/ui/mod.rs:1026, 1052`), `now-btn-radar` and `now-btn-love` unconditionally retain `.cursor_pointer()` even when `has_song == false`.
8. **Poster Stats & Emoji Box Glitches (R4.4)**: In `src/poster.rs:508`, the completion badge is hardcoded to `"100% 真爱完播"`. Emojis (`🥇`, `🥈`, `🥉`, `🌙`, `💚`, etc.) rendered through Win32 GDI `DrawTextW` with `"Microsoft YaHei UI"` produce empty tofu glyph boxes due to missing glyph linking and surrogate pair rendering limitations.
9. **Win32 Window Consistency & DPI (R4.5)**: The main GPUI window creates a default light Win32 title bar contrasting with `#0d1117`. In `src/widget.rs:137`, legacy `SetProcessDPIAware()` is called, and widget dimensions are hardcoded to 380×72 without PerMonitorV2 DPI scaling.
10. **Test Infrastructure**: `cargo test` executes cleanly (6 tests in 0.01s). Testing can be directly expanded with headless unit tests covering completion rate calculations, layout models, input grapheme masking, DPI scaling, and state transitions.

---

## Detailed Investigation

### 1. 900×600 Responsive Layout & Toolbar Collision

#### File & Line Evidence
- `src/ui/mod.rs:3990-3999`: Window constraints defined as `window_min_size: Some(size(px(900.0), px(600.0)))`, with default dimensions 1120×740.
- `src/ui/mod.rs:598-600`: Sidebar fixed width is `px(216.0)`.
- `src/ui/mod.rs:1685-1778`: Roam view top control card.
- `src/ui/mod.rs:2864-3000`: History (recent) view top control card.

#### Analysis & Root Cause
1. **Available Geometry**:
   At minimum window size (900px wide):
   $$\text{Content Width} = 900 - 216 (\text{sidebar}) - 40 (\text{page padding } p\_5) = 644\text{px}$$
   Subtracting card padding ($p\_4 = 32\text{px}$), the card's inner width is only **612px**.

2. **Roaming Header Collision (`src/ui/mod.rs:1685-1778`)**:
   ```rust
   // Lines 1685-1690
   div()
       .flex()
       .flex_row()
       .items_center()
       .justify_between()
       .gap_3()
   ```
   - **Left column**: Title `"🌌 我的专属全盘漫游"` + subtitle `"自动提取你本地完播率最高的真爱曲目与 Last.fm 常听榜，多源合成新歌漫游清单"` (43 CJK characters ≈ 516px width).
   - **Right column**: 3 buttons in a non-wrapping flex row:
     - Checkbox `"☐ 自动剔除已听过的歌"` (~150px)
     - Button `"✨ 生成今日漫游歌单"` (~165px)
     - Button `"🎲 换一批种子"` (~110px)
     - Total buttons width ≈ 425px + gaps = **450px**.
   - **Total minimum required width**: $516 + 450 + 12 = 978\text{px} \gg 612\text{px}$.
   - **Symptom**: Right-hand buttons collide with the title text, compress the text box, or overflow out of bounds.

3. **History Header Collision (`src/ui/mod.rs:2864-3000`)**:
   ```rust
   // Lines 2864-2870
   div()
       .flex()
       .flex_row()
       .items_center()
       .justify_between()
   ```
   - **Left column**: Title `"🕒 最近播放流水账（最近 X 条）"` + subtitle `"记录每一首歌的实际停留时长、完播率、切歌跳过判定与 Last.fm 打卡状态"` (~460px width).
   - **Right column**: 4 buttons in a non-wrapping row:
     - `"📥 导出流水 CSV"` (~125px)
     - `"💾 备份数据库"` (~115px)
     - `"📤 一键补打卡达标记录"` (~185px)
     - `"🔄 刷新"` (~75px)
     - Total buttons width ≈ **500px**.
   - **Total minimum required width**: $460 + 500 = 960\text{px} \gg 612\text{px}$.

#### Recommended Architecture Fix
- Apply `.flex_wrap()` to both header container `div`s.
- In both Roaming and History toolbars, encapsulate action buttons into a right-aligned or wrapped sub-container (`.flex().flex_row().flex_wrap().gap_2()`).
- Wrap titles with `.flex_1().min_w(px(260.))` so when the window is narrow (< 980px total width), the buttons cleanly drop onto a secondary responsive toolbar row without colliding into the title or overflowing the container.

---

### 2. Modals and Destructive Actions

#### A. Poster Modal Backdrop & Esc Key Dismissal
- **File & Line Evidence**:
  - `src/ui/mod.rs:3808-3820`:
    ```rust
    div()
        .id("poster-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(rgba(0x000000bb))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, |_, _, _| {}) // Empty closure!
    ```
  - `src/ui/mod.rs:522`: `ConsoleView` root element has `.track_focus(&self.focus_handle)`.
  - `src/ui/mod.rs:3854-3857`: Close button sets `this.wrapped_poster = None; cx.notify();`.
- **Root Cause**:
  1. The backdrop listener `.on_mouse_down(MouseButton::Left, |_, _, _| {})` is an intentional event sink that absorbs clicks but never dismisses the modal.
  2. There is no `on_key_down` or keybinding checking for the `Escape` key to close the modal.
- **Recommended Fix**:
  1. Backdrop dismiss:
     ```rust
     div()
         .id("poster-backdrop")
         .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
             this.wrapped_poster = None;
             cx.notify();
         }))
         .child(
             // Modal Dialog Box: stop click propagation
             div()
                 .on_mouse_down(MouseButton::Left, |_, _, _| {})
                 // ... dialog content ...
         )
     ```
  2. Escape key dismissal:
     On `ConsoleView` root element (line 522), attach:
     ```rust
     .on_key_down(cx.listener(|this, event: &KeyDownEvent, cx| {
         if event.keystroke.key == "escape" && this.wrapped_poster.is_some() {
             this.wrapped_poster = None;
             cx.notify();
         }
     }))
     ```

#### B. Destructive History Record Deletion
- **File & Line Evidence**:
  - `src/ui/mod.rs:3184-3199`:
    ```rust
    div()
        .id(("rec-del", idx))
        .px_2()
        .py_0p5()
        .rounded_sm()
        .cursor_pointer()
        .bg(rgb(0x21262d))
        .text_size(px(11.))
        .text_color(rgb(0xff7b7c))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.hub.db.delete_play(play_id);
            this.recent_rows = this.hub.db.recent(150);
            this.show_toast("已删除该条播放记录", cx);
        }))
        .child("✕"),
    ```
  - `src/db.rs:267-270`: Direct hard delete: `DELETE FROM plays WHERE id=?`.
- **Root Cause**: Immediate execution on single click without confirmation or undo capability. An inadvertent click permanently deletes listening history and scrobble records.
- **Recommended Fix**:
  Add an inline confirmation state to `ConsoleView`:
  ```rust
  pub confirm_delete_id: Option<i64>,
  ```
  In `rec-del` rendering:
  ```rust
  let is_confirming = self.confirm_delete_id == Some(play_id);
  div()
      .id(("rec-del", idx))
      .px_2()
      .py_0p5()
      .rounded_sm()
      .cursor_pointer()
      .bg(if is_confirming { rgb(0xe84142) } else { rgb(0x21262d) })
      .text_color(if is_confirming { rgb(0xffffff) } else { rgb(0xff7b7c) })
      .child(if is_confirming { "确认删除?" } else { "✕" })
      .on_click(cx.listener(move |this, _, _, cx| {
          if this.confirm_delete_id == Some(play_id) {
              this.hub.db.delete_play(play_id);
              this.recent_rows = this.hub.db.recent(150);
              this.confirm_delete_id = None;
              this.show_toast("已永久删除该条播放记录", cx);
          } else {
              this.confirm_delete_id = Some(play_id);
              cx.notify();
          }
      }))
  ```
  If the user clicks outside or clicks another record, `confirm_delete_id` resets.

---

### 3. Empty States & Click Feedback

#### A. Empty States & Initial Loads
- **File & Line Evidence**:
  - `src/ui/mod.rs:1820-1961` (`render_roam_view`): If `self.roam_result` is None, nothing is rendered below the control card. If `res.tracks.is_empty()`, only table header is rendered without rows.
  - `src/ui/mod.rs:1242-1396` (`render_radar_view`): If `self.radar_result` is None, nothing is rendered below the preset chips.
- **Root Cause**:
  Neither view provides an initial placeholder card explaining how the feature functions before first use, nor does roam handle the zero-track result case.
- **Recommended Fix**:
  1. For Roaming (`render_roam_view`):
     - Initial state (`self.roam_result.is_none() && !self.roam_loading`): Render a placeholder card with an icon (`🌌`), title `"开启你的全盘漫游"`, and description `"点击上方「生成今日漫游歌单」，音乐中枢将基于你的历史高完播曲目与协同过滤，智能生成漫游候选"`.
     - Loading state (`self.roam_loading`): Render `"⏳ 正在聚合多源曲库与本地偏好，生成专属漫游..."`.
     - Empty result (`res.tracks.is_empty()`): Render an informative empty card: `"未找到符合条件的漫游曲目（可能均已被已听过滤剔除），建议关闭「剔除已听」或换一批种子再试"`.
  2. For Radar (`render_radar_view`):
     - Initial state (`self.radar_result.is_none() && !self.radar_loading`): Render an initial state card: `"🔍 输入歌曲与歌手名，或点击上方热门预设，即可启动双源雷达推荐"`.
     - Loading state (`self.radar_loading`): Render `"⏳ 正在联动物联网关与推荐引擎，探索同频音乐..."`.

#### B. Toast Feedback on Play Actions
- **File & Line Evidence**:
  - True love play button (`src/ui/mod.rs:2722-2736`):
    ```rust
    .on_click(move |_, _, _| {
        HubApp::play_in_client(id);
    })
    ```
  - History list play button (`src/ui/mod.rs:3142-3155`):
    ```rust
    .on_click(move |_, _, _| {
        HubApp::play_in_client(id);
    })
    ```
- **Root Cause**:
  Both closures capture only `id` and invoke `HubApp::play_in_client(id)` via a static call without passing a GPUI context listener, leaving the UI completely silent.
- **Recommended Fix**:
  Change `on_click` to `cx.listener`:
  ```rust
  let t_name = t.title.clone();
  .on_click(cx.listener(move |this, _, _, cx| {
      HubApp::play_in_client(id);
      this.show_toast(format!("▶ 已唤起客户端播放：{}", t_name), cx);
  }))
  ```

#### C. Hover Highlights for Top Artists
- **File & Line Evidence**:
  - `src/ui/mod.rs:2466-2485`:
    ```rust
    div()
        .id(("top-artist", i))
        .w_full()
        .cursor_pointer()
        .flex()
        .flex_col()
        .gap_1()
        .on_click(...)
    ```
- **Root Cause**:
  Although the row is clickable and sets `.cursor_pointer()`, it lacks background styling, padding, and hover states. Users cannot visually perceive row boundaries or affordance.
- **Recommended Fix**:
  Add container styling and hover feedback:
  ```rust
  div()
      .id(("top-artist", i))
      .w_full()
      .px_2()
      .py_1p5()
      .rounded_md()
      .cursor_pointer()
      .hover(|s| s.bg(rgb(0x1f2630)))
      .flex()
      .flex_col()
      .gap_1()
  ```

#### D. Bottom Bar Non-Pointer Cursor for Disabled Controls
- **File & Line Evidence**:
  - `src/ui/mod.rs:1020-1087`:
    - `now-btn-radar` (line 1026): Unconditional `.cursor_pointer()`.
    - `now-btn-love` (line 1052): Unconditional `.cursor_pointer()` and `.hover(|s| s.bg(rgb(0x30363d)))`.
- **Root Cause**:
  When `has_song == false`, `now-btn-radar` and `now-btn-love` still display hand pointer cursors and highlight on hover, even though clicking them executes early returns with no action.
- **Recommended Fix**:
  ```rust
  .cursor(if has_song { CursorStyle::PointingHand } else { CursorStyle::NotAllowed })
  // Or in GPUI:
  // .when(has_song, |d| d.cursor_pointer().hover(|s| s.bg(rgb(0x30363d))))
  // .when(!has_song, |d| d.cursor_default())
  ```

---

### 4. Poster Rendering: Completion Rate & Win32 GDI Emoji Fallback

#### A. Hardcoded Completion Rate in Poster
- **File & Line Evidence**:
  - `src/poster.rs:501-510`:
    ```rust
    draw_str(
        f_body_b,
        c_green,
        520,
        710,
        (w as i32) - 74,
        774,
        "100% 真爱完播",
        true,
    );
    ```
  - `src/app.rs:109-116`: `StatsTrackItem` has `n` (total plays), `c` (completed plays), `s` (skipped plays).
  - `src/app.rs:804-817`: `d.loved` is populated with `StatsTrackItem`.
- **Root Cause**:
  The completion percentage was hardcoded as `"100% 真爱完播"` regardless of the song's actual `c` and `n` counts.
- **Recommended Fix**:
  ```rust
  let comp_rate = if loved.n > 0 {
      ((loved.c as f64 / loved.n as f64) * 100.0).round() as i64
  } else {
      100
  };
  let comp_badge = format!("{}% 真爱完播", comp_rate.clamp(0, 100));
  draw_str(
      f_body_b,
      c_green,
      520,
      710,
      (w as i32) - 74,
      774,
      &comp_badge,
      true,
  );
  ```

#### B. Win32 GDI Emoji Tofu / Missing Glyph Rendering
- **File & Line Evidence**:
  - `src/poster.rs:52-69`:
    ```rust
    unsafe fn make_font(height_px: i32, weight: i32, italic: u32) -> HFONT {
        CreateFontW(..., w!("Microsoft YaHei UI"))
    }
    ```
  - `src/poster.rs:349-366`: `draw_str` uses `DrawTextW(mem_dc, &mut utf16, &mut rc, flags)`.
  - Emoji occurrences:
    - Line 430: `let medals = ["🥇", "🥈", "🥉"];` (Unicode surrogate pairs U+1F947, U+1F948, U+1F949)
    - Line 255: `"🌙 深夜造梦家"`
    - Line 260: `"🌅 晨光独奏者"`
    - Line 264: `"☀️ 午后心流体"`
    - Line 266: `"🌆 暮光旅人"`
    - Line 384: `"🎵 MUSIC HUB · 听歌画像报告"`
    - Line 427: `"🎤 灵魂常听音乐人 TOP 3"`
    - Line 477: `"💚 本期真爱单曲（完播率最高）"`
    - Line 532: `"🎯 智能推荐闭环满意度"`
- **Root Cause**:
  Win32 GDI's `DrawTextW` rendering to a memory DIBSection does NOT support color emojis (DirectWrite is required for color glyphs). When drawing characters with `"Microsoft YaHei UI"`, surrogate pairs and non-BMP emoji codepoints either render as double empty rectangle boxes (`▯▯`) or produce garbled glyphs depending on Windows version and font-link registry keys.
- **Recommended Fix**:
  1. Replace medal emojis with crisp typography badges:
     ```rust
     let medals = ["NO.1", "NO.2", "NO.3"];
     ```
  2. Implement an emoji fallback sanitizer or replace emoji prefixes with clean, supported Unicode typographic symbols:
     - Persona: `"深夜造梦家"`, `"晨光独奏者"`, `"午后心流体"`, `"暮光旅人"` (or prefix with standard bullet `●` or star `★`)
     - Header: `"MUSIC HUB · 听歌画像报告"`
     - Section titles: `"灵魂常听音乐人 TOP 3"`, `"本期真爱单曲（完播率最高）"`, `"智能推荐闭环满意度"`
  3. Provide a helper `fn sanitize_gdi_text(input: &str) -> String` that filters out surrogate-pair emojis or converts common music emojis to guaranteed CJK symbols, preventing missing glyph boxes on all Windows versions.

---

### 5. System Visual Consistency: Win32 Dark Titlebar & Dynamic Island DPI

#### A. Win32 Dark Mode Window Title Bar Matching `#0d1117`
- **File & Line Evidence**:
  - `src/ui/mod.rs:3990-4010`: Window created via `cx.open_window(window_opts, ...)`.
  - `Cargo.toml:19-35`: `windows = { version = "0.58", features = [...] }`.
- **Root Cause**:
  By default, Win32 windows created by GPUI inherit the Windows shell light titlebar theme (white/grey). Without DWM attribute overrides, the window has an abrupt white title bar contrasting against the deep dark `#0d1117` UI.
- **Recommended Fix**:
  1. Add `"Win32_Graphics_Dwm"` to `Cargo.toml` under `windows` dependencies.
  2. After `cx.open_window` or upon first frame, locate the window handle `HWND` via `FindWindowW(None, w!("私人音乐中枢 · Music Hub"))`.
  3. Apply DWM attributes:
     - `DWMWA_USE_IMMERSIVE_DARK_MODE` (attribute 20, fallback to 19): `BOOL(1)`.
     - `DWMWA_CAPTION_COLOR` (attribute 35): `0x0017110D` (COLORREF for `#0d1117`).
     - `DWMWA_TEXT_COLOR` (attribute 36): `0x00F3EDE6` (COLORREF for `#e6edf3`).

#### B. Dynamic Island Dimensions & PerMonitorV2 DPI Scaling
- **File & Line Evidence**:
  - `src/widget.rs:137`: Legacy `let _ = SetProcessDPIAware();`.
  - `src/widget.rs:33-34`: `pub const WIDGET_WIDTH: u32 = 380; pub const WIDGET_HEIGHT: u32 = 72;`.
  - `src/widget.rs:150-163`: Coordinates calculated using `GetSystemMetrics(SM_CXSCREEN)` without monitor DPI awareness.
  - `src/widget.rs:893-940`: `flush_to_layered_window` creates a fixed 380×72 bitmap.
- **Root Cause**:
  1. `SetProcessDPIAware()` is a deprecated Windows Vista API that enables system-wide DPI virtualization, overriding modern PerMonitorV2 behavior and causing blurry scaling on secondary monitors.
  2. The Dynamic Island uses hardcoded physical pixels (380×72). On a 150% or 200% scaling monitor (e.g. 144 or 192 DPI), the island renders at half physical size, appearing tiny.
- **Recommended Fix**:
  1. Delete line 137: `let _ = SetProcessDPIAware();`.
  2. Detect monitor DPI using `GetDpiForWindow(hwnd)` (or `GetDpiForSystem()` fallback).
  3. Calculate scale factor $S = \text{DPI} / 96.0$:
     - $\text{Scaled Width} = (380.0 \times S)\text{.round() as i32}$
     - $\text{Scaled Height} = (72.0 \times S)\text{.round() as i32}$
  4. Scale font sizes, pill radius, icon positions, and hitboxes proportionally by $S$.
  5. Handle `WM_DPICHANGED` in `widget_wnd_proc`: update the scale factor, recreate fonts, and call `SetWindowPos` to adjust the layered window bounds.

---

## Existing Test Architecture & Verification Analysis

### 1. Current Test Suite Status
Running `cargo test` yields:
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

All 6 tests are pure in-process unit tests:
1. `src/normalize.rs`: 5 tests validating string cleaning, noise removal, and artist variations.
2. `src/db.rs`: 1 lifecycle test using SQLite in-memory database (`DB::open_in_memory()`), verifying play tracking, scrobble count, and feedback persistence.

### 2. Test Architecture Expansion Matrix for R1 – R4

| Requirement | Proposed Test Target | Test Method & Scope |
| :--- | :--- | :--- |
| **R1 (Lifecycle & SMTC)** | Cover change detection | Unit test in `src/monitor.rs` testing that `cover_bytes` transition from `None` to `Some(bytes)` triggers cover state updates. |
| **R1 (Lifecycle & SMTC)** | Redundant repaint prevention | Unit test validating state comparison logic between successive `AppStateResponse` polls. |
| **R2 (Error & Sync)** | Scrobble error visibility | Unit test in `src/app.rs` ensuring `last_error` is populated and exposed in `AppStateResponse`. |
| **R2 (Error & Sync)** | Bottom-bar favorite sync | Unit test in `src/app.rs` / `src/db.rs` confirming that updating feedback updates both SQLite and in-memory lists. |
| **R3 (TextInput)** | Grapheme cluster masking | Unit test in `src/ui/input.rs` ensuring Unicode / CJK passwords mask 1:1 with graphemes without slice panics. |
| **R3 (TextInput)** | Horizontal scroll boundary | Unit test in `src/ui/input.rs` computing visible cursor offset when content width exceeds container width. |
| **R4 (Safeguards)** | Poster completion rate formula | Unit test in `src/poster.rs` verifying completion percentage calculation with diverse inputs (e.g., $c=0, n=0$; $c=8, n=10 \to 80\%$; $c=5, n=5 \to 100\%$). |
| **R4 (Safeguards)** | Emoji fallback sanitizer | Unit test in `src/poster.rs` verifying that strings with emojis are safely converted to clean GDI-compatible text. |
| **R4 (Safeguards)** | History delete confirmation state | Unit test verifying two-step delete state machine (`None` $\to$ `Some(id)` $\to$ deleted). |
| **R4 (Visuals)** | Widget DPI scaling math | Unit test in `src/widget.rs` verifying dimension and coordinate scaling across 96, 120, 144, and 192 DPI. |

---

## Summary Matrix of R4 Action Items

| Item | Problem Description | Source File & Location | Solution |
| :--- | :--- | :--- | :--- |
| **R4.1** | Roam top bar collision at 900×600 | `src/ui/mod.rs:1685-1778` | Add `.flex_wrap().gap_3()` to title/button container. |
| **R4.1** | History toolbar collision at 900×600 | `src/ui/mod.rs:2864-3000` | Add `.flex_wrap().gap_2()` to toolbar container and button sub-row. |
| **R4.2a**| Poster modal backdrop click does not close | `src/ui/mod.rs:3808-3820` | Replace no-op with `this.wrapped_poster = None; cx.notify()`, stop click propagation on card. |
| **R4.2a**| Poster modal cannot close via `Esc` key | `src/ui/mod.rs:522` | Add `escape` key check in `on_key_down` to dismiss modal. |
| **R4.2b**| History record deleted on single click without prompt | `src/ui/mod.rs:3184-3199` | Implement two-step inline confirmation (`confirm_delete_id`). |
| **R4.3a**| Empty/initial roaming & radar lack visuals | `src/ui/mod.rs:1242, 1820` | Add dedicated empty state cards and loading indicators. |
| **R4.3b**| Missing toast on true love & history play | `src/ui/mod.rs:2732, 3151` | Wrap play callback in `cx.listener` and call `this.show_toast(...)`. |
| **R4.3c**| Top artists lack hover highlight | `src/ui/mod.rs:2466-2485` | Add `.p_2().rounded_md().hover(\|s\| s.bg(...))`. |
| **R4.3d**| Bottom bar controls show pointer cursor when empty | `src/ui/mod.rs:1026, 1052` | Switch to `.cursor_default()` and suppress hover styles when `!has_song`. |
| **R4.4a**| Poster hardcodes "100% 真爱完播" | `src/poster.rs:508` | Calculate actual rate from `loved.c` and `loved.n`. |
| **R4.4b**| Win32 GDI emoji missing glyph boxes (tofu) | `src/poster.rs:430, 255-266` | Replace medals with text badges and sanitize emojis for GDI. |
| **R4.5a**| Main window has light Win32 title bar | `src/ui/mod.rs:4006` | Call `DwmSetWindowAttribute` for dark mode & `#0d1117` caption. |
| **R4.5b**| Legacy `SetProcessDPIAware()` call | `src/widget.rs:137` | Remove redundant function call. |
| **R4.5c**| Dynamic Island ignores PerMonitorV2 DPI | `src/widget.rs:893-940` | Scale dimensions and fonts by `GetDpiForWindow / 96.0` and handle `WM_DPICHANGED`. |
