# Test Track Explorer 2 Handoff: Test Case Specifications for R3 & R4 (F7–F15)

- **Date**: 2026-11-12
- **Agent**: Teamwork Explorer (`test_track_explorer_2`)
- **Working Directory**: `e:\杂项\music-hub`
- **State Folder**: `e:\杂项\music-hub\.agents\teamwork\test_track_explorer_2`
- **Parent Conversation ID**: `f119d8d4-4796-4e92-aede-5db30c51d61c`
- **Scope**: Requirements R3 & R4 (Features F7 through F15), Tiers 1–3 Test Case Specifications

---

## 1. Observation

### 1.1 Baseline Test Suite Execution
Direct execution of `cargo test` in `e:\杂项\music-hub`:
```
running 6 tests
test normalize::tests::test_norm ... ok
test normalize::tests::test_clean_artist ... ok
test normalize::tests::test_artist_variants ... ok
test normalize::tests::test_is_junk_stream ... ok
test db::tests::test_db_lifecycle ... ok
test normalize::tests::test_clean_title ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```
- Existing tests cover only `src/normalize.rs` and `src/db.rs::test_db_lifecycle`.
- There is currently no `tests/` directory at the project root; all automated tests must be introduced cleanly without disrupting the existing 6 tests.

### 1.2 Feature F7: TextInput Horizontal Scrolling
In `src/ui/input.rs:283-308`:
```rust
// TextElement::request_layout
let mut style = Style::default();
style.size.width = relative(1.).into();
style.size.height = window.line_height().into();
(window.request_layout(style, [], cx), ())
```
And in `src/ui/input.rs:563-574`:
```rust
let cursor_pos = line.x_for_index(cursor);
let (selection, cursor) = if selected_range.is_empty() {
    (
        None,
        Some(fill(
            Bounds::new(
                point(bounds.left() + cursor_pos, bounds.top()),
                size(px(1.5), bounds.bottom() - bounds.top()),
            ),
            rgb(0xe84142),
        )),
    )
}
```
And in `src/ui/input.rs:620`:
```rust
let _ = line.paint(bounds.origin, window.line_height(), window, cx);
```
- **Direct observation**: `line.paint` uses `bounds.origin` with zero horizontal translation. The input div has `.overflow_hidden()` (line 677). When text width $W_{text} > W_{box}$, trailing characters and the cursor $cursor\_pos > W_{box}$ are painted outside the element bounds and clipped.
- Mouse click mapping (`index_for_mouse_position`, lines 250-266) uses `line.closest_index_for_x(position.x - bounds.left())`, which inspects only the clipped $[0, W_{box}]$ range.

### 1.3 Feature F8: Focus Navigation & Enter Handling
In `src/ui/input.rs:32-48`:
```rust
pub fn bind_input_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        ...
        KeyBinding::new("enter", Enter, Some("TextInput")),
    ]);
}
```
And in `src/ui/input.rs:79-90`:
```rust
cx.new(|cx| Self {
    focus_handle: cx.focus_handle(), // tab_stop is NOT set, defaults to false
    content,
    ...
})
```
And in `src/ui/mod.rs:241-250`:
```rust
let sub1 = cx.subscribe(&track_input, |this, _, ev: &InputEvent, cx| {
    if matches!(ev, InputEvent::EnterPressed) {
        this.run_radar(cx);
    }
});
```
- **Direct observation**: `TextInput` does not bind `"tab"` or `"shift-tab"`. `cx.focus_handle()` lacks `.tab_stop(true)`. In `src/ui/mod.rs`, `username_input`, `api_key_input`, and `api_secret_input` in the settings view have zero subscriptions to `InputEvent::EnterPressed`.

### 1.4 Feature F9: Password Masking Unicode Graphemes
In `src/ui/input.rs:106-122`:
```rust
fn display_string(&self) -> SharedString {
    if self.is_password && !self.content.is_empty() {
        // Preserve exact UTF-8 byte length per character so cursor/selection byte offsets stay 1:1
        let mut masked = String::with_capacity(self.content.len());
        for ch in self.content.chars() {
            match ch.len_utf8() {
                1 => masked.push('*'),
                2 => masked.push('·'),
                3 => masked.push('•'),
                _ => masked.push_str("****"),
            }
        }
        masked.into()
    } else {
        self.content.clone()
    }
}
```
- **Direct observation**: For 4-byte characters (such as emojis `🔑`, `🔒`, `🎶`), `display_string` pushes four `*` characters. In `index_for_mouse_position` and `replace_text_in_range` (`src/ui/input.rs:384-386`):
  `self.content[0..range.start]`
  If the user clicks on the 2nd or 3rd asterisk of a 4-byte character, the byte offset lands at non-char boundaries, causing a fatal Rust runtime panic: `byte index is not a char boundary`.

### 1.5 Feature F10: 900x600 Responsive Layout
In `src/ui/mod.rs:1685-1745` (Roam Header):
```rust
div()
    .flex()
    .flex_row()
    .items_center()
    .justify_between()
    .gap_3()
    .child(
        div().flex().flex_col().child("🌌 我的专属全盘漫游").child("自动提取你本地完播率最高的真爱曲目与 Last.fm 常听榜，多源合成新歌漫游清单")
    )
    .child(
        div().flex().flex_row().items_center().gap_2p5()
            .child(/* chk-hide-heard */)
            .child(/* btn-generate-roam */)
            .child(/* btn-reroll-seed */)
    )
```
In `src/ui/mod.rs:2864-2920` (History Toolbar):
```rust
div()
    .flex()
    .flex_row()
    .items_center()
    .justify_between()
    .pb_2()
    .border_b_1()
    .border_color(rgb(0x262c36))
    .child(
        div().flex().flex_col().child("🕒 最近播放流水账...").child("记录每一首歌的实际停留时长...")
    )
    .child(
        div().flex().flex_row().items_center().gap_2()
            .child(/* btn-export-csv */)
            .child(/* btn-backup-db */)
            .child(/* btn-retry-all-scrobble */)
            .child(/* btn-refresh */)
    )
```
- **Direct observation**: Window min size is `size(px(900.0), px(600.0))` (`src/ui/mod.rs:3993`). Inner content width at 900px is $900 - 216\text{ (sidebar)} - 40\text{ (padding)} - 32\text{ (card)} = 612\text{px}$. The Roam title (~516px) and 3 buttons (~450px) require $\sim 978\text{px}$. Without `.flex_wrap()`, buttons and text collide and overflow. The History toolbar similarly requires $\sim 960\text{px}$ against $612\text{px}$.

### 1.6 Feature F11: Modal Backdrop & Esc Dismissal
In `src/ui/mod.rs:3808-3820`:
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
    .on_mouse_down(MouseButton::Left, |_, _, _| {})
```
And in `src/ui/mod.rs:522`:
```rust
div()
    .track_focus(&self.focus_handle)
    // No on_key_down handler for Escape key!
```
- **Direct observation**: `poster-backdrop` has a dummy empty closure that absorbs left mouse down events without resetting `this.wrapped_poster = None`. There is no key event listener checking for `"escape"`.

### 1.7 Feature F12: History Record Deletion Safeguard
In `src/ui/mod.rs:3184-3199`:
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
    .child("✕")
```
- **Direct observation**: Single click immediately and permanently invokes `this.hub.db.delete_play(play_id)` without confirmation, undo prompt, or state safeguard.

### 1.8 Feature F13: Empty States & Click Feedback
In `src/ui/mod.rs:1820-1961` (`render_roam_view`):
- If `self.roam_result.is_none()`, no content is rendered below the header card. If `res.tracks.is_empty()`, only table header is drawn.
In `src/ui/mod.rs:2732` and `3151`:
- Loved track play click: captures `id` and executes `HubApp::play_in_client(id)` via static closure with no `this.show_toast` callback.
- History play click: executes `HubApp::play_in_client(id)` with no toast callback.
In `src/ui/mod.rs:2466-2485`:
- Top artists row has `.cursor_pointer()` but lacks `.hover(|s| s.bg(...))` background hover feedback.
In `src/ui/mod.rs:1026, 1052`:
- `now-btn-radar` and `now-btn-love` unconditionally retain `.cursor_pointer()` even when `has_song == false`.

### 1.9 Feature F14: Poster Completion Rate & GDI Emojis
In `src/poster.rs:501-510`:
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
In `src/poster.rs:430` and `255-266`:
```rust
let medals = ["🥇", "🥈", "🥉"];
...
let (persona, quote) = if (0..6).contains(&peak_h) {
    ("🌙 深夜造梦家", "...")
...
draw_str(f_sec, c_white, 54, 446, 500, 480, "🎤 灵魂常听音乐人 TOP 3", false);
draw_str(f_sec, c_green, 54, 668, 500, 702, "💚 本期真爱单曲（完播率最高）", false);
draw_str(f_sec, c_blue, 54, 728, 500, 820, "🎯 智能推荐闭环满意度", false);
```
- **Direct observation**: Completion percentage is hardcoded to `"100% 真爱完播"` regardless of `loved.c` and `loved.n`. Medals use surrogate-pair emojis `🥇` (`U+1F947`), `🥈` (`U+1F948`), `🥉` (`U+1F949`) which Win32 GDI `DrawTextW` with `"Microsoft YaHei UI"` renders as tofu boxes `▯▯`.

### 1.10 Feature F15: Win32 Dark Titlebar & DPI Scaling
In `src/ui/mod.rs:3990-4010`:
- Window is opened via `cx.open_window(window_opts, ...)` without invoking `DwmSetWindowAttribute` for `DWMWA_USE_IMMERSIVE_DARK_MODE` (20/19) or `DWMWA_CAPTION_COLOR` (35).
In `src/widget.rs:137`:
- Calls deprecated `let _ = SetProcessDPIAware();`.
In `src/widget.rs:33-34, 893-894`:
- Dimensions are hardcoded to `WIDGET_WIDTH: u32 = 380; WIDGET_HEIGHT: u32 = 72;` without multiplying by PerMonitorV2 scale factor $S = \text{DPI} / 96.0$.

---

## 2. Logic Chain

```
[Observation 1.1–1.10]
Exact line numbers, data structures, and formulas across F7–F15
         │
         ▼
[Logic Step 1: Headless Testability Model]
GPUI view logic, Win32 coordinates, GDI text sanitization, and SQLite operations
can be fully abstracted into deterministic headless state machines, mathematical models,
and data pipelines without opening physical display hardware:
 - F7: Scrolling offset clamping algorithm (pure math & boundary functions)
 - F8: Focus navigation order & key emission handlers
 - F9: Unicode segmentation grapheme 1:1 bullet mapping & bi-directional offset engine
 - F10: Responsive layout solver calculating 900x600 wrapping conditions
 - F11: Modal backdrop & Esc event state machine (Option<Vec<u8>>)
 - F12: Two-step confirmation delete state transition engine on SQLite
 - F13: View state selectors for empty/loading/populated & toast emission callbacks
 - F14: Completion rate rounding formula & GDI surrogate sanitizer
 - F15: DPI scale factor geometry calculations & DWM COLORREF bitmasks
         │
         ▼
[Logic Step 2: Test Tier Decomposition]
Derive >=5 Tier 1 (happy path / category-partition) and >=5 Tier 2 (boundary value / corner)
test specifications for each feature F7 through F15 (total >= 90 tests),
plus 10 Tier 3 cross-feature combinatorial interaction tests.
         │
         ▼
[Conclusion: Full Specification Artifact]
Provide comprehensive test definitions including Test ID, Target API, Inputs,
Execution Steps, Assertions, and Hazard Mitigations ready for implementation.
```

---

## 3. Caveats

1. **GPUI Framework Context in Headless Tests**: GPUI 0.2.2 requires a test context (`gpui::TestAppContext` or mock models) when testing components that directly query window metrics. For pure algorithm validation (F7 scrolling math, F9 grapheme masking, F10 layout geometry, F14 poster metrics, F15 DPI math), pure unit tests in Rust modules provide the highest reliability and 100% headless execution speed.
2. **Win32 Platform API Availability**: Win32 DWM calls (`DwmSetWindowAttribute`) require Windows runtime headers and OS build $\ge 17763$. Tests for F15 must verify the mathematical color conversion, attribute constants, and DPI scaling logic headlessly, with conditional compilation (`#[cfg(windows)]`) for live Win32 handle tests.
3. **Database Concurrency**: `DB::open_in_memory()` uses an isolated SQLite in-memory database. All F12/F13 database tests must instantiate fresh in-memory DB instances to guarantee 100% test isolation and zero disk state leakage.

---

## 4. Conclusion & Test Case Specifications

### 4.1 Master Test Inventory Matrix (F7–F15)

| Feature | Description | Requirement | Tier 1 Tests | Tier 2 Tests | Tier 3 Links | Target Test File |
|---|---|---|---|---|---|---|
| **F7** | TextInput Horizontal Scrolling | R3 | 5 | 6 | F7+F8, F7+F9 | `tests/test_text_input.rs` |
| **F8** | Focus Navigation & Enter Handling | R3 | 5 | 5 | F8+F6, F8+F9, F8+F11 | `tests/test_text_input.rs` |
| **F9** | Password Masking Unicode Graphemes | R3 | 5 | 6 | F9+F7, F9+F8 | `tests/test_text_input.rs` |
| **F10**| 900x600 Responsive Layout | R4 | 5 | 5 | F10+F11, F10+F13, F10+F15 | `tests/test_layout_responsive.rs` |
| **F11**| Modal Backdrop & Esc Dismissal | R4 | 5 | 5 | F11+F8, F11+F10, F11+F14 | `tests/test_safeguards.rs` |
| **F12**| History Record Deletion Safeguard | R4 | 5 | 5 | F12+F13 | `tests/test_safeguards.rs` |
| **F13**| Empty States & Click Feedback | R4 | 5 | 5 | F13+F10, F13+F12 | `tests/test_safeguards.rs` |
| **F14**| Poster Completion Rate & GDI Emojis | R4 | 5 | 6 | F14+F11 | `tests/test_poster_metrics.rs` |
| **F15**| Win32 Dark Titlebar & DPI Scaling | R4 | 5 | 5 | F15+F10 | `tests/test_dpi_and_platform.rs` |
| **T3** | Combinatorial Cross-Feature Suites | R3, R4 | — | — | 10 Scenarios | `tests/test_tier3_combinatorial.rs` |
| **Total** | **9 Features + Combinations** | | **45** | **48** | **10** | **103 Test Specs** |

---

### 4.2 Detailed Test Case Specifications: F7 (TextInput Horizontal Scrolling)

#### Tier 1: Feature Coverage (Category-Partition & Happy Path)
- **TEST-F07-T1-01: `test_scroll_offset_zero_when_text_fits`**
  - **Component**: `TextInput::compute_scroll_offset`
  - **Inputs**: Container width $W_{box} = 300.0$, text width $W_{text} = 150.0$, cursor position $X_{cursor} = 150.0$, current scroll $S = 0.0$.
  - **Execution**: Call scroll offset calculation function with fitted parameters.
  - **Expected Outcome**: Offset remains exactly $0.0$; visible cursor quad $X_{vis} = X_{cursor} - S = 150.0 \le W_{box}$.
- **TEST-F07-T1-02: `test_scroll_offset_shifts_right_on_typing_overflow`**
  - **Component**: `TextInput::compute_scroll_offset`
  - **Inputs**: $W_{box} = 200.0$, text width $W_{text} = 350.0$, cursor at trailing end $X_{cursor} = 350.0$, padding margin $= 12.0$.
  - **Execution**: Compute new scroll offset when typing advances cursor beyond right container bound ($X_{cursor} > S + W_{box}$).
  - **Expected Outcome**: Offset adjusts to $S = X_{cursor} - W_{box} + 12.0 = 162.0$; visible cursor $X_{vis} = 350.0 - 162.0 = 188.0 \le W_{box}$.
- **TEST-F07-T1-03: `test_scroll_offset_snaps_left_on_home_navigation`**
  - **Component**: `TextInput::compute_scroll_offset`
  - **Inputs**: $W_{box} = 200.0$, $W_{text} = 400.0$, previous scroll $S = 212.0$, user presses `Home` moving cursor to $X_{cursor} = 0.0$.
  - **Execution**: Compute offset when $X_{cursor} < S$.
  - **Expected Outcome**: Offset resets to $S = 0.0$; visible cursor $X_{vis} = 0.0$.
- **TEST-F07-T1-04: `test_scroll_offset_clamped_at_max_scroll`**
  - **Component**: `TextInput::compute_scroll_offset`
  - **Inputs**: $W_{box} = 200.0$, $W_{text} = 500.0$, max scroll $S_{max} = W_{text} - W_{box} = 300.0$.
  - **Execution**: Pass an excessively large proposed offset (e.g. $450.0$).
  - **Expected Outcome**: Output offset is strictly clamped to $300.0$, preventing empty whitespace from appearing on the right.
- **TEST-F07-T1-05: `test_mouse_click_coordinate_mapping_with_scroll`**
  - **Component**: `TextInput::index_for_mouse_position_with_offset`
  - **Inputs**: Container bounds left $= 50.0$, current scroll $S = 120.0$, click event at physical screen $X = 130.0$.
  - **Execution**: Map mouse click position: $X_{rel} = X_{click} - \text{bounds.left} = 80.0$; compute text-space coordinate $X_{text} = X_{rel} + S$.
  - **Expected Outcome**: $X_{text} = 80.0 + 120.0 = 200.0$; maps to character index at 200.0px instead of 80.0px.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F07-T2-01: `test_scroll_offset_exact_boundary_equality`**
  - **Inputs**: $W_{box} = 200.0$, $W_{text} = 200.0$, $X_{cursor} = 200.0$.
  - **Execution**: Verify zero jitter or rounding oscillation when text width equals container width down to floating point $\epsilon = 10^{-5}$.
  - **Expected Outcome**: $S = 0.0$, no spurious scrolling triggered.
- **TEST-F07-T2-02: `test_scroll_offset_empty_string_behavior`**
  - **Inputs**: $W_{box} = 250.0$, $W_{text} = 0.0$, $X_{cursor} = 0.0$.
  - **Execution**: Calculate scroll on uninitialized/empty input.
  - **Expected Outcome**: $S = 0.0$, no divide-by-zero or NaN values.
- **TEST-F07-T2-03: `test_scroll_offset_collapsed_container_width`**
  - **Inputs**: $W_{box} = 0.0$ (e.g. during layout collapse/hidden state), $W_{text} = 100.0$.
  - **Execution**: Calculate scroll with zero container width.
  - **Expected Outcome**: Gracefully returns $S = 0.0$ without arithmetic panic.
- **TEST-F07-T2-04: `test_scroll_offset_massive_paste_50k_characters`**
  - **Inputs**: Paste 50,000 ASCII characters ($W_{text} \approx 350,000.0$px), $W_{box} = 300.0$. Cursor moves to end ($350,000.0$).
  - **Execution**: Compute scroll offset on massive text injection.
  - **Expected Outcome**: $S$ clamps to $349,700.0$; cursor remains within visible viewport $[0, 300.0]$ without integer overflow or lag.
- **TEST-F07-T2-05: `test_scroll_offset_select_all_and_delete_reset`**
  - **Inputs**: Text previously had $W_{text} = 600.0, S = 350.0$. User executes `SelectAll` + `Backspace`. $W_{text}$ drops to $0.0$, $X_{cursor} = 0.0$.
  - **Execution**: Recalculate scroll offset immediately following content clearing.
  - **Expected Outcome**: $S$ instantly resets to $0.0$, never retaining stale positive offsets.
- **TEST-F07-T2-06: `test_scroll_offset_intermediate_cursor_jump_hysteresis`**
  - **Inputs**: $W_{box} = 200.0, W_{text} = 600.0$, current $S = 150.0$ (viewport covers $[150.0, 350.0]$). User moves cursor to $X_{cursor} = 250.0$.
  - **Execution**: Compute scroll offset when cursor lands strictly inside current visible window.
  - **Expected Outcome**: $S$ remains completely unchanged at $150.0$ (viewport stability / hysteresis), preventing disorienting jumps.

---

### 4.3 Detailed Test Case Specifications: F8 (Focus Navigation & Enter Handling)

#### Tier 1: Feature Coverage
- **TEST-F08-T1-01: `test_text_input_tab_stop_flag_enabled`**
  - **Component**: `TextInput::new`
  - **Inputs**: Create a `TextInput` entity via `cx.focus_handle().tab_stop(true)`.
  - **Execution**: Query `focus_handle.is_tab_stop()`.
  - **Expected Outcome**: Returns `true`, confirming the input is eligible for GPUI window tab cycles.
- **TEST-F08-T1-02: `test_tab_navigation_forward_cycling_order`**
  - **Component**: Form Focus Navigation Engine
  - **Inputs**: 3 form inputs representing Settings view (`username_input` = 0, `api_key_input` = 1, `api_secret_input` = 2). Current focus = 0.
  - **Execution**: Dispatch `Tab` action twice, then once more.
  - **Expected Outcome**: Focus advances $0 \to 1 \to 2 \to 0$ (modulo 3 wrapping).
- **TEST-F08-T1-03: `test_shift_tab_backward_cycling_order`**
  - **Component**: Form Focus Navigation Engine
  - **Inputs**: 3 form inputs (0, 1, 2). Current focus = 0.
  - **Execution**: Dispatch `PrevTab` (`shift-tab`).
  - **Expected Outcome**: Focus wraps backward $0 \to 2 \to 1 \to 0$.
- **TEST-F08-T1-04: `test_text_input_enter_event_emission`**
  - **Component**: `TextInput::enter`
  - **Inputs**: Focused `TextInput` with content `"test_token"`.
  - **Execution**: Trigger `TextInput::enter(&mut self, &Enter, window, cx)`.
  - **Expected Outcome**: Dispatches `InputEvent::EnterPressed` event to listeners.
- **TEST-F08-T1-05: `test_settings_enter_triggers_config_save`**
  - **Component**: `ConsoleView` Settings Form Subscriptions
  - **Inputs**: `username_input` populated with `"new_user_123"`.
  - **Execution**: Dispatch `InputEvent::EnterPressed` on `username_input`.
  - **Expected Outcome**: `hub.cfg.get_data().username` updates to `"new_user_123"` and a toast is queued.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F08-T2-01: `test_tab_cycle_single_element_form`**
  - **Inputs**: A view containing only 1 focusable `TextInput` ($N=1$).
  - **Execution**: Dispatch `Tab` and `PrevTab`.
  - **Expected Outcome**: Focus remains on index 0 without out-of-bounds panics or loop freeze.
- **TEST-F08-T2-02: `test_tab_skips_disabled_or_invisible_inputs`**
  - **Inputs**: 3 inputs, input 1 has visibility/disabled flag set to false.
  - **Execution**: Dispatch `Tab` from input 0.
  - **Expected Outcome**: Focus advances directly from $0 \to 2$, skipping disabled element 1.
- **TEST-F08-T2-03: `test_enter_on_whitespace_only_input`**
  - **Inputs**: `username_input` containing `"   "`.
  - **Execution**: Press Enter.
  - **Expected Outcome**: Trimming logic detects blank username; does not overwrite valid config username with empty whitespace.
- **TEST-F08-T2-04: `test_enter_does_not_insert_newline_character`**
  - **Inputs**: `TextInput` with text `"my_secret"`.
  - **Execution**: Press Enter.
  - **Expected Outcome**: Text remains exactly `"my_secret"`; neither `\n` nor `\r` is appended.
- **TEST-F08-T2-05: `test_rapid_enter_successive_dispatch_idempotence`**
  - **Inputs**: Rapidly dispatch 10 `EnterPressed` events within 50ms.
  - **Execution**: Verify asynchronous credential save dispatch.
  - **Expected Outcome**: Saved state matches final input string without database lock contention or thread starvation.

---

### 4.4 Detailed Test Case Specifications: F9 (Password Masking Unicode Graphemes)

#### Tier 1: Feature Coverage
- **TEST-F09-T1-01: `test_ascii_password_masking_1_to_1`**
  - **Component**: `TextInput::display_string`
  - **Inputs**: `content = "Pass1234"`, `is_password = true`.
  - **Execution**: Call `display_string()`.
  - **Expected Outcome**: Output is exactly `"••••••••"` (8 Unicode bullets, $8 \times 3 = 24$ UTF-8 bytes).
- **TEST-F09-T1-02: `test_cjk_password_masking_graphemes`**
  - **Component**: `TextInput::display_string`
  - **Inputs**: `content = "我的密码99"`, `is_password = true` (4 CJK + 2 ASCII = 6 graphemes).
  - **Execution**: Call `display_string()`.
  - **Expected Outcome**: Output is exactly `"••••••"` (6 bullets), NOT ad-hoc dots `···`.
- **TEST-F09-T1-03: `test_emoji_password_masking_no_four_stars`**
  - **Component**: `TextInput::display_string`
  - **Inputs**: `content = "🔑🔓"`, `is_password = true` (2 4-byte emojis, total 8 bytes).
  - **Execution**: Call `display_string()`.
  - **Expected Outcome**: Output is exactly `"••"` (2 bullets), completely eliminating the previous buggy `"********"` quadruple asterisks.
- **TEST-F09-T1-04: `test_content_to_display_offset_mapping`**
  - **Component**: `TextInput::content_to_display_byte_offset`
  - **Inputs**: `content = "a🔑b"` (`'a'` = 1 byte, `'🔑'` = 4 bytes, `'b'` = 1 byte; total 6 bytes).
  - **Execution**: Map content offsets 0, 1 (after 'a'), 5 (after emoji), 6 (end).
  - **Expected Outcome**:
    - Offset 0 $\to$ display byte 0
    - Offset 1 $\to$ display byte 3 (1st bullet)
    - Offset 5 $\to$ display byte 6 (2nd bullet)
    - Offset 6 $\to$ display byte 9 (3rd bullet)
- **TEST-F09-T1-05: `test_display_to_content_offset_mapping`**
  - **Component**: `TextInput::display_to_content_byte_offset`
  - **Inputs**: `content = "a🔑b"`.
  - **Execution**: Map display offsets back to content offsets.
  - **Expected Outcome**: Display 0 $\to$ 0, 3 $\to$ 1, 6 $\to$ 5, 9 $\to$ 6. Every returned offset strictly aligns with a UTF-8 character boundary.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F09-T2-01: `test_empty_password_masking`**
  - **Inputs**: `content = ""`, `is_password = true`.
  - **Execution**: Call `display_string()`, map offset 0.
  - **Expected Outcome**: Returns `""`, offset maps 0 $\to$ 0 without panic.
- **TEST-F09-T2-02: `test_zwj_and_combining_mark_grapheme_masking`**
  - **Inputs**: `content = "e\u{0301}👨‍👩‍👧‍👦"` (combining acute accent + 25-byte family ZWJ emoji; 2 grapheme clusters).
  - **Execution**: Mask with `display_string()`.
  - **Expected Outcome**: Produces exactly `"••"` (2 bullets).
- **TEST-F09-T2-03: `test_sub_bullet_display_click_offset_safety`**
  - **Inputs**: `content = "abc"`, display string is 9 bytes (`"•••"`). User mouse clicks at display byte offset 1 or 2 (middle of first bullet).
  - **Execution**: Call `display_to_content_byte_offset(1)` and `display_to_content_byte_offset(2)`.
  - **Expected Outcome**: Safely rounds/floors to content offset 0 or 1. Slice `content[0..returned_offset]` never panics.
- **TEST-F09-T2-04: `test_backspace_on_trailing_emoji_in_password`**
  - **Inputs**: `content = "key🔑"`, cursor at offset 7 (end). User triggers `backspace`.
  - **Execution**: Call `previous_boundary(7)`.
  - **Expected Outcome**: Returns 3 (start of emoji); deleting range 3..7 leaves `"key"` with zero corrupted surrogate remnants.
- **TEST-F09-T2-05: `test_non_password_mode_identity_passthrough`**
  - **Inputs**: `content = "🔑Plain123"`, `is_password = false`.
  - **Execution**: Call `display_string()` and offset mappings.
  - **Expected Outcome**: `display_string()` returns `"🔑Plain123"` untouched; mappings return identity $X \to X$.
- **TEST-F09-T2-06: `test_mixed_script_long_password_stress`**
  - **Inputs**: 500-character string containing mixed ASCII, Arabic right-to-left, CJK, Devanagari, and surrogate emojis.
  - **Execution**: Verify grapheme count matches bullet count; iterate every display byte offset from $0$ to $3 \times N$ and verify `display_to_content_byte_offset` always points to a valid char boundary in `content`.
  - **Expected Outcome**: Zero panics across all 1500 byte probes; 100% boundary safety.

---

### 4.5 Detailed Test Case Specifications: F10 (900x600 Responsive Layout)

#### Tier 1: Feature Coverage
- **TEST-F10-T1-01: `test_responsive_inner_width_at_900_bounds`**
  - **Component**: Layout Bounds Calculator
  - **Inputs**: Window width $W = 900.0$, sidebar width $= 216.0$, page padding $= 40.0$, card padding $= 32.0$.
  - **Execution**: Compute available inner content width.
  - **Expected Outcome**: Available width $W_{inner} = 612.0$px.
- **TEST-F10-T1-02: `test_roaming_header_wrap_activation_at_612_width`**
  - **Component**: Header Wrap Model
  - **Inputs**: Container width $612.0$px. Roam title block width $= 516.0$px, action button group width $= 450.0$px.
  - **Execution**: Solve layout row placement.
  - **Expected Outcome**: $516 + 450 = 966 > 612.0$; wrap condition triggers; title assigned row 0, button group assigned row 1.
- **TEST-F10-T1-03: `test_history_toolbar_wrap_activation_at_612_width`**
  - **Component**: Toolbar Wrap Model
  - **Inputs**: Container width $612.0$px. History title width $= 460.0$px, 4-button group width $= 500.0$px.
  - **Execution**: Solve layout row placement.
  - **Expected Outcome**: $460 + 500 = 960 > 612.0$; buttons wrap cleanly to row 1.
- **TEST-F10-T1-04: `test_toolbar_buttons_no_horizontal_collision`**
  - **Component**: Responsive Collision Checker
  - **Inputs**: Solved geometry at 900x600.
  - **Execution**: Calculate bounding rect intersection between title element $[X_{1}, Y_{1}, W_{1}, H_{1}]$ and button group $[X_{2}, Y_{2}, W_{2}, H_{2}]$.
  - **Expected Outcome**: Collision area is strictly $0$; $Y_{2} \ge Y_{1} + H_{1}$ (clear vertical separation).
- **TEST-F10-T1-05: `test_large_window_unwrapped_single_row_placement`**
  - **Component**: Layout Wrap Model
  - **Inputs**: Window width $1440.0$px, $W_{inner} = 1152.0$px. Title $= 460.0$px, buttons $= 500.0$px.
  - **Execution**: Solve row placement.
  - **Expected Outcome**: $460 + 500 + \text{gap}(16) = 976 \le 1152.0$; all elements placed on row 0 without unnecessary vertical expansion.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F10-T2-01: `test_exact_threshold_width_boundary`**
  - **Inputs**: Container width exactly equals $W_{title} + W_{buttons} + \text{gap} = 976.0$px.
  - **Execution**: Test at $975.9$px, $976.0$px, and $976.1$px.
  - **Expected Outcome**: Deterministic wrap at $\le 975.9$, single row at $\ge 976.0$; zero floating point oscillation.
- **TEST-F10-T2-02: `test_window_minimum_size_enforcement`**
  - **Inputs**: Window constraints in `WindowOptions`.
  - **Execution**: Verify `window_min_size` is strictly `Some(size(px(900.0), px(600.0)))`.
  - **Expected Outcome**: Prevents window shrink below 900x600.
- **TEST-F10-T2-03: `test_history_toolbar_zero_records_title_width`**
  - **Inputs**: `recent_rows.len() == 0`, title is `"🕒 最近播放流水账（最近 0 条）"`.
  - **Execution**: Solve layout at $612.0$px width.
  - **Expected Outcome**: Button group wraps gracefully without overlapping the shorter title.
- **TEST-F10-T2-04: `test_roaming_buttons_sub_row_wrap_at_narrow_bounds`**
  - **Inputs**: Button group inside roam card contains 3 buttons with total width $450.0$px. Test on extreme nested container width $400.0$px.
  - **Execution**: Check secondary `.flex_wrap().gap_2()` on the button group itself.
  - **Expected Outcome**: Buttons break into 2 rows rather than overflowing the right card border.
- **TEST-F10-T2-05: `test_sidebar_fixed_geometry_invariance`**
  - **Inputs**: Window resized from 900px to 1920px.
  - **Execution**: Measure sidebar width across all sizes.
  - **Expected Outcome**: Sidebar width remains invariant at exactly $216.0$px.

---

### 4.6 Detailed Test Case Specifications: F11 (Modal Backdrop & Esc Dismissal)

#### Tier 1: Feature Coverage
- **TEST-F11-T1-01: `test_poster_modal_initial_state_none`**
  - **Component**: `ConsoleView::new`
  - **Inputs**: Newly instantiated `ConsoleView`.
  - **Execution**: Inspect `view.wrapped_poster`.
  - **Expected Outcome**: Equals `None`.
- **TEST-F11-T1-02: `test_poster_modal_backdrop_click_dismisses`**
  - **Component**: Modal Dismissal State Machine
  - **Inputs**: `view.wrapped_poster = Some(vec![1, 2, 3])`. Mouse down event at $(10.0, 10.0)$ (outside modal dialog card).
  - **Execution**: Dispatch backdrop mouse down event handler.
  - **Expected Outcome**: `view.wrapped_poster` transitions to `None`.
- **TEST-F11-T1-03: `test_poster_modal_dialog_interior_click_retains_open`**
  - **Component**: Modal Dialog Click Consumer
  - **Inputs**: `view.wrapped_poster = Some(vec![1, 2, 3])`. Dialog card located at $[100, 100, 460, 600]$. Click at $(200.0, 200.0)$ (interior).
  - **Execution**: Dispatch dialog click handler.
  - **Expected Outcome**: Event is consumed; `view.wrapped_poster` remains `Some(vec![1, 2, 3])`.
- **TEST-F11-T1-04: `test_poster_modal_escape_key_dismisses`**
  - **Component**: `ConsoleView::on_key_down`
  - **Inputs**: `view.wrapped_poster = Some(vec![1, 2, 3])`. Key down event with `keystroke.key == "escape"`.
  - **Execution**: Invoke escape key handler.
  - **Expected Outcome**: `view.wrapped_poster` transitions to `None`.
- **TEST-F11-T1-05: `test_poster_modal_close_button_click_dismisses`**
  - **Component**: `#btn-close-poster`
  - **Inputs**: `view.wrapped_poster = Some(vec![1, 2, 3])`.
  - **Execution**: Click `#btn-close-poster`.
  - **Expected Outcome**: `view.wrapped_poster` transitions to `None`.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F11-T2-01: `test_escape_key_when_modal_not_open_is_noop`**
  - **Inputs**: `view.wrapped_poster = None`. Active tab = `NavTab::Radar`.
  - **Execution**: Dispatch `escape` key down event.
  - **Expected Outcome**: View state and active tab remain completely unaffected; no panics.
- **TEST-F11-T2-02: `test_non_escape_keys_do_not_dismiss_modal`**
  - **Inputs**: `view.wrapped_poster = Some(vec![1, 2, 3])`.
  - **Execution**: Dispatch key down events for `Enter`, `Space`, `Tab`, `ArrowDown`.
  - **Expected Outcome**: `view.wrapped_poster` remains `Some(vec![1, 2, 3])`.
- **TEST-F11-T2-03: `test_rapid_toggle_modal_cycles`**
  - **Inputs**: Rapid loop: set `Some(...)` $\to$ backdrop click $\to$ set `Some(...)` $\to$ Esc key $\to$ set `Some(...)` $\to$ close button.
  - **Execution**: Execute 50 successive cycles.
  - **Expected Outcome**: 100% deterministic dismissals without memory leak or state desynchronization.
- **TEST-F11-T2-04: `test_backdrop_right_mouse_down_ignored`**
  - **Inputs**: `view.wrapped_poster = Some(vec![1, 2, 3])`. Mouse down event with `button = MouseButton::Right`.
  - **Execution**: Dispatch backdrop event.
  - **Expected Outcome**: Modal is NOT dismissed; only primary `MouseButton::Left` dismisses.
- **TEST-F11-T2-05: `test_modal_dismissal_preserves_underlying_inputs`**
  - **Inputs**: `track_input` has text `"Hotel California"`, modal opens and closes via Esc.
  - **Execution**: Check `track_input.text()` after dismissal.
  - **Expected Outcome**: Retains `"Hotel California"`.

---

### 4.7 Detailed Test Case Specifications: F12 (History Record Deletion Safeguard)

#### Tier 1: Feature Coverage
- **TEST-F12-T1-01: `test_history_delete_first_click_sets_confirmation_state`**
  - **Component**: `ConsoleView` Inline Delete State Machine
  - **Inputs**: In-memory SQLite DB with play record $ID = 42$. `view.confirm_delete_id = None`.
  - **Execution**: User clicks delete button on row with $ID = 42$.
  - **Expected Outcome**: `view.confirm_delete_id` transitions to `Some(42)`. Database query `db.get_play(42)` confirms record is still present.
- **TEST-F12-T1-02: `test_history_delete_second_click_executes_permanent_deletion`**
  - **Component**: `ConsoleView` Inline Delete State Machine
  - **Inputs**: `view.confirm_delete_id = Some(42)`.
  - **Execution**: User clicks delete button on row $ID = 42$ a second time.
  - **Expected Outcome**: Record 42 is deleted from SQLite; `view.confirm_delete_id` resets to `None`; `recent_rows` no longer contains ID 42.
- **TEST-F12-T1-03: `test_history_delete_button_label_and_style_transition`**
  - **Component**: Delete Button Visual State Resolver
  - **Inputs**: Row ID 42. Compare visual state when `confirm_delete_id == None` vs `confirm_delete_id == Some(42)`.
  - **Execution**: Evaluate button label and background color.
  - **Expected Outcome**:
    - Unconfirmed: Label is `"✕"`, background is `rgb(0x21262d)`.
    - Confirming: Label is `"确认删除?"`, background is warning red `rgb(0xe84142)`.
- **TEST-F12-T1-04: `test_history_delete_generates_toast_notification`**
  - **Component**: Delete Action Toast Callback
  - **Inputs**: Execute successful 2nd click deletion on ID 42.
  - **Execution**: Inspect toast channel.
  - **Expected Outcome**: Toast message `"已永久删除该条播放记录"` is emitted.
- **TEST-F12-T1-05: `test_history_delete_row_count_decrements_by_one`**
  - **Component**: SQLite DB & View Row Synchronization
  - **Inputs**: DB initialized with 10 play rows.
  - **Execution**: Complete two-step deletion on row 3.
  - **Expected Outcome**: `db.recent(150).len()` decreases from 10 to 9.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F12-T2-01: `test_history_delete_target_switch_cancels_previous`**
  - **Inputs**: Row 10 clicked (`confirm_delete_id = Some(10)`). User immediately clicks delete on Row 20.
  - **Execution**: Process click on Row 20.
  - **Expected Outcome**: `confirm_delete_id` transitions to `Some(20)`. Neither Row 10 nor Row 20 is deleted.
- **TEST-F12-T2-02: `test_history_delete_tab_switch_clears_confirmation`**
  - **Inputs**: `confirm_delete_id = Some(10)`. User switches tab to `NavTab::Settings`.
  - **Execution**: Call `switch_tab(NavTab::Settings)`.
  - **Expected Outcome**: `confirm_delete_id` resets to `None`.
- **TEST-F12-T2-03: `test_history_delete_already_removed_record_handling`**
  - **Inputs**: `confirm_delete_id = Some(999)` where ID 999 does not exist in DB.
  - **Execution**: Confirm delete.
  - **Expected Outcome**: Database `DELETE` executes with 0 affected rows; resets `confirm_delete_id = None` without panic.
- **TEST-F12-T2-04: `test_history_delete_isolation_preserves_other_records`**
  - **Inputs**: 5 play records in DB with IDs 1..=5. Delete record 3.
  - **Execution**: Query all remaining rows.
  - **Expected Outcome**: Records 1, 2, 4, 5 remain completely intact with original metadata, play counts, and timestamps.
- **TEST-F12-T2-05: `test_history_delete_cancel_on_outside_click`**
  - **Inputs**: `confirm_delete_id = Some(10)`. User clicks on a table cell or backdrop.
  - **Execution**: Trigger outside click handler.
  - **Expected Outcome**: `confirm_delete_id` resets to `None`.

---

### 4.8 Detailed Test Case Specifications: F13 (Empty States & Click Feedback)

#### Tier 1: Feature Coverage
- **TEST-F13-T1-01: `test_roaming_initial_state_card_selection`**
  - **Component**: Roam View State Resolver
  - **Inputs**: `roam_result = None`, `roam_loading = false`.
  - **Execution**: Resolve view child state.
  - **Expected Outcome**: Selects `InitialRoamCard` containing `"开启你的全盘漫游"`.
- **TEST-F13-T1-02: `test_roaming_empty_result_card_selection`**
  - **Component**: Roam View State Resolver
  - **Inputs**: `roam_result = Some(RoamResult { tracks: vec![] })`, `roam_loading = false`.
  - **Execution**: Resolve view child state.
  - **Expected Outcome**: Selects `EmptyRoamCard` containing `"未找到符合条件的漫游曲目"`.
- **TEST-F13-T1-03: `test_radar_initial_state_card_selection`**
  - **Component**: Radar View State Resolver
  - **Inputs**: `radar_result = None`, `radar_loading = false`.
  - **Execution**: Resolve view child state.
  - **Expected Outcome**: Selects `InitialRadarCard` containing `"输入歌曲与歌手名"`.
- **TEST-F13-T1-04: `test_loved_play_action_toast_feedback`**
  - **Component**: Loved List Play Click Handler
  - **Inputs**: Track title `"夜曲"`, artist `"周杰伦"`, NCM ID 186016.
  - **Execution**: Trigger loved track play action.
  - **Expected Outcome**: `show_toast` is called with message containing `"▶ 已唤起客户端播放：夜曲"`.
- **TEST-F13-T1-05: `test_history_play_action_toast_feedback`**
  - **Component**: History List Play Click Handler
  - **Inputs**: Play row with title `"稻香"`, NCM ID 185924.
  - **Execution**: Trigger history row play action.
  - **Expected Outcome**: `show_toast` is called with message containing `"▶ 已唤起客户端播放：稻香"`.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F13-T2-01: `test_bottom_bar_inactive_cursor_not_allowed`**
  - **Inputs**: `state.now = None` (`has_song == false`).
  - **Execution**: Check cursor style attribute on `now-btn-radar` and `now-btn-love`.
  - **Expected Outcome**: Cursor is `CursorStyle::NotAllowed` (or `CursorStyle::Default`), NOT `CursorStyle::PointingHand`.
- **TEST-F13-T2-02: `test_bottom_bar_active_cursor_pointer`**
  - **Inputs**: `state.now = Some(PlaySnapshot { title: "Track", ... })` (`has_song == true`).
  - **Execution**: Check cursor style attribute.
  - **Expected Outcome**: Cursor is `CursorStyle::PointingHand`.
- **TEST-F13-T2-03: `test_top_artists_hover_style_defined`**
  - **Inputs**: Inspect top artist element definition in `render_stats_view`.
  - **Execution**: Verify presence of hover styling function.
  - **Expected Outcome**: Applies hover background color `rgb(0x1f2630)`.
- **TEST-F13-T2-04: `test_roaming_loading_state_takes_precedence`**
  - **Inputs**: `roam_loading = true`, previous `roam_result = None`.
  - **Execution**: Resolve view child state.
  - **Expected Outcome**: Selects `LoadingRoamCard` containing `"正在聚合多源曲库"`, not the empty or initial card.
- **TEST-F13-T2-05: `test_empty_roam_guidance_message_content`**
  - **Inputs**: Empty roaming result text.
  - **Execution**: Check guidance strings.
  - **Expected Outcome**: Includes actionable guidance suggesting unticking `"剔除已听"` or refreshing seeds.

---

### 4.9 Detailed Test Case Specifications: F14 (Poster Completion Rate & GDI Emojis)

#### Tier 1: Feature Coverage
- **TEST-F14-T1-01: `test_poster_completion_rate_calculation_standard`**
  - **Component**: `calculate_completion_rate(c, n)`
  - **Inputs**: Completed plays $c = 8$, total plays $n = 10$.
  - **Execution**: Compute completion rate badge.
  - **Expected Outcome**: Produces exactly `"80% 真爱完播"`.
- **TEST-F14-T1-02: `test_poster_completion_rate_perfect_100`**
  - **Component**: `calculate_completion_rate(c, n)`
  - **Inputs**: $c = 25, n = 25$.
  - **Execution**: Compute completion rate badge.
  - **Expected Outcome**: Produces exactly `"100% 真爱完播"`.
- **TEST-F14-T1-03: `test_poster_completion_rate_fractional_rounding`**
  - **Component**: `calculate_completion_rate(c, n)`
  - **Inputs**: $c = 2, n = 3$ (66.666...%) and $c = 1, n = 3$ (33.333...%).
  - **Execution**: Compute completion rate badge.
  - **Expected Outcome**: $c=2, n=3 \to "67\% 真爱完播"$; $c=1, n=3 \to "33\% 真爱完播"$.
- **TEST-F14-T1-04: `test_poster_medal_text_badges_replace_emojis`**
  - **Component**: Top 3 Artists Badge Generator
  - **Inputs**: Artists ranking indices $i \in \{0, 1, 2\}$.
  - **Execution**: Query medal badge for each rank.
  - **Expected Outcome**: Produces `["NO.1", "NO.2", "NO.3"]`, completely removing `🥇`, `🥈`, `🥉`.
- **TEST-F14-T1-05: `test_poster_gdi_text_sanitizer_cleans_section_headers`**
  - **Component**: `sanitize_gdi_text`
  - **Inputs**: `"🎤 灵魂常听音乐人 TOP 3"`, `"💚 本期真爱单曲（完播率最高）"`, `"🎯 智能推荐闭环满意度"`.
  - **Execution**: Pass through `sanitize_gdi_text`.
  - **Expected Outcome**: Emojis stripped/converted to clean typography: `"灵魂常听音乐人 TOP 3"`, `"本期真爱单曲（完播率最高）"`, `"智能推荐闭环满意度"`.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F14-T2-01: `test_poster_completion_rate_zero_total_plays`**
  - **Inputs**: $n = 0, c = 0$.
  - **Execution**: Compute completion rate.
  - **Expected Outcome**: Does not divide by zero; safely returns `"100% 真爱完播"` (or `"0% 真爱完播"`).
- **TEST-F14-T2-02: `test_poster_completion_rate_zero_completed_plays`**
  - **Inputs**: $n = 12, c = 0$.
  - **Execution**: Compute completion rate.
  - **Expected Outcome**: Produces `"0% 真爱完播"`.
- **TEST-F14-T2-03: `test_poster_completion_rate_anomaly_c_exceeds_n`**
  - **Inputs**: Corrupted record where $c = 15, n = 10$.
  - **Execution**: Compute completion rate.
  - **Expected Outcome**: Clamped to strictly $\le 100\%$, returning `"100% 真爱完播"`.
- **TEST-F14-T2-04: `test_gdi_sanitizer_removes_all_utf16_surrogate_pairs`**
  - **Inputs**: String containing various 4-byte SMP characters: `"\u{1F3B5}\u{1F3A4}\u{1F947}\u{1F319}\u{1F305}"`.
  - **Execution**: Call `sanitize_gdi_text`.
  - **Expected Outcome**: Zero UTF-16 surrogate pairs (`0xD800..=0xDFFF`) remain in encoded output; no tofu boxes in GDI.
- **TEST-F14-T2-05: `test_gdi_sanitizer_preserves_chinese_japanese_korean_and_punctuation`**
  - **Inputs**: `"🎵 [Live] 周杰伦 & 宇多田ヒカル · 晴天 (2024 Remaster) ★"`.
  - **Execution**: Call `sanitize_gdi_text`.
  - **Expected Outcome**: Strips `🎵`, cleanly preserves Chinese, Japanese, Latin, brackets, dot, and star `"[Live] 周杰伦 & 宇多田ヒカル · 晴天 (2024 Remaster) ★"`.
- **TEST-F14-T2-06: `test_poster_render_with_empty_loved_and_empty_artists`**
  - **Inputs**: Poster dataset where `top_artists` is empty and `loved` is empty.
  - **Execution**: Execute poster text preparation.
  - **Expected Outcome**: Fallback strings `"暂无足够播放记录"` and `"暂无真爱单曲记录"` are selected without panic.

---

### 4.10 Detailed Test Case Specifications: F15 (Win32 Dark Titlebar & DPI Scaling)

#### Tier 1: Feature Coverage
- **TEST-F15-T1-01: `test_dpi_scale_factor_calculations`**
  - **Component**: DPI Scaling Math
  - **Inputs**: Monitor DPI values: 96, 120, 144, 192.
  - **Execution**: Calculate $S = \text{DPI} / 96.0$.
  - **Expected Outcome**:
    - 96 DPI $\to S = 1.0$ (100%)
    - 120 DPI $\to S = 1.25$ (125%)
    - 144 DPI $\to S = 1.5$ (150%)
    - 192 DPI $\to S = 2.0$ (200%)
- **TEST-F15-T1-02: `test_widget_dimensions_at_100_percent_dpi`**
  - **Component**: Widget Geometry Resolver
  - **Inputs**: Baseline $W=380, H=72$, DPI = 96.
  - **Execution**: Compute scaled dimensions.
  - **Expected Outcome**: Scaled width $= 380$, scaled height $= 72$.
- **TEST-F15-T1-03: `test_widget_dimensions_at_150_percent_dpi`**
  - **Component**: Widget Geometry Resolver
  - **Inputs**: Baseline $W=380, H=72$, DPI = 144 ($S=1.5$).
  - **Execution**: Compute scaled dimensions.
  - **Expected Outcome**: Scaled width $= 570$, scaled height $= 108$.
- **TEST-F15-T1-04: `test_widget_dimensions_at_200_percent_dpi`**
  - **Component**: Widget Geometry Resolver
  - **Inputs**: Baseline $W=380, H=72$, DPI = 192 ($S=2.0$).
  - **Execution**: Compute scaled dimensions.
  - **Expected Outcome**: Scaled width $= 760$, scaled height $= 144$.
- **TEST-F15-T1-05: `test_dwm_colorref_conversion_for_dark_theme`**
  - **Component**: Win32 DWM Color Mask Calculator
  - **Inputs**: Hex RGB color `#0d1117` ($R = 13, G = 17, B = 23$).
  - **Execution**: Convert to Win32 `COLORREF` bit format: $(B \ll 16) | (G \ll 8) | R$.
  - **Expected Outcome**: Produces `COLORREF(0x0017110D)`.

#### Tier 2: Boundary Value Analysis & Corner Cases
- **TEST-F15-T2-01: `test_dpi_scaling_odd_non_standard_dpi`**
  - **Inputs**: Custom DPI 110 (114.58%) and 288 (300% 4K display).
  - **Execution**: Compute scaled widget dimensions with `.round() as i32`.
  - **Expected Outcome**:
    - 110 DPI: $W = (380 \times 110/96).round() = 435$, $H = (72 \times 110/96).round() = 83$.
    - 288 DPI: $W = 1140$, $H = 216$. Zero NaN or negative values.
- **TEST-F15-T2-02: `test_dpi_scaling_zero_or_negative_clamp`**
  - **Inputs**: Broken driver reporting DPI $= 0$ or $-50$.
  - **Execution**: Compute scale factor with `.max(96)`.
  - **Expected Outcome**: Clamps to 96 DPI ($S = 1.0$), avoiding division by zero or inversion.
- **TEST-F15-T2-03: `test_widget_reposition_on_dpi_changed_event`**
  - **Inputs**: Widget on monitor A (96 DPI) moved to monitor B (144 DPI) via `WM_DPICHANGED`. Suggested rect provided by Win32.
  - **Execution**: Parse `WM_DPICHANGED` rect and update internal scale.
  - **Expected Outcome**: Internal $S$ updates to $1.5$; fonts and bitmap recreation flag set.
- **TEST-F15-T2-04: `test_elimination_of_set_process_dpi_aware`**
  - **Inputs**: Inspect `src/widget.rs` spawn sequence.
  - **Execution**: Verify absence of deprecated `SetProcessDPIAware()` call.
  - **Expected Outcome**: PerMonitorV2 mode is not overridden by legacy process-wide DPI virtualization.
- **TEST-F15-T2-05: `test_dwm_immersive_dark_mode_attribute_fallback`**
  - **Inputs**: DWM attribute constants.
  - **Execution**: Verify primary constant is 20 (`DWMWA_USE_IMMERSIVE_DARK_MODE`), with fallback constant 19 for older Windows 10 versions.
  - **Expected Outcome**: Attribute sequence correctly orders $20 \to 19$.

---

### 4.11 Tier 3: Combinatorial Cross-Feature Test Specifications

- **TEST-T3-01 (F7 + F8): `test_tab_focus_switching_preserves_independent_scroll_offsets`**
  - **Features**: TextInput Horizontal Scrolling + Focus Navigation
  - **Scenario**: Form with 2 inputs: `track_input` has 150-char URL ($S = 400.0$px), `artist_input` has short name ($S = 0.0$px).
  - **Execution**: User is focused on `track_input`, presses `Tab` to jump to `artist_input`, then presses `Shift-Tab` to return to `track_input`.
  - **Expected Outcome**: `track_input` retains its $400.0$px scroll offset upon re-focus; `artist_input` retains $0.0$px offset; neither overwrites the other.
- **TEST-T3-02 (F7 + F9): `test_scrolling_masked_password_with_multibyte_emojis`**
  - **Features**: TextInput Horizontal Scrolling + Password Masking
  - **Scenario**: Password input with width $200.0$px receives 30 emoji characters (`"🔑".repeat(30)` = 120 bytes).
  - **Execution**: Content masks to 30 bullets (`"•".repeat(30)` = 90 bytes). Cursor moves to end. Calculate scroll offset.
  - **Expected Outcome**: Scroll offset calculates based on the 30-bullet shaped line width; cursor quad remains clamped inside visible viewport $[0, 200.0]$; zero UTF-8 boundary panics.
- **TEST-T3-03 (F8 + F9): `test_focus_navigation_selection_replacement_in_password_field`**
  - **Features**: Focus Navigation + Password Masking
  - **Scenario**: Tabbing into password field selects all characters (`0..content.len()`). User types a new character `'X'`.
  - **Execution**: `replace_text_in_range` replaces entire emoji password with `'X'`.
  - **Expected Outcome**: Password field safely updates to `"•"`, selection resets to $1..1$, cursor at offset 1.
- **TEST-T3-04 (F8 + F6/R2): `test_settings_enter_save_after_oauth_sync`**
  - **Features**: Focus & Enter Handling + OAuth State Synchronization
  - **Scenario**: Browser OAuth callback finishes, updates config username to `"authenticated_user"`, opens Settings tab and populates `username_input`.
  - **Execution**: User presses `Tab` to advance to `api_key_input`, types new key, presses `Enter`.
  - **Expected Outcome**: `save_credentials_from_inputs` executes; newly entered API key is saved without erasing or resetting the synchronized `"authenticated_user"`.
- **TEST-T3-05 (F10 + F11): `test_modal_backdrop_centered_within_900_bounds`**
  - **Features**: 900x600 Responsive Layout + Modal Backdrop Dismissal
  - **Scenario**: Window constrained to minimum size $900 \times 600$. Poster modal opened (`wrapped_poster = Some(...)`).
  - **Execution**: Verify modal dialog bounds $[(900-460)/2, (600-H)/2, 460, H]$. Click at $(10, 10)$ on backdrop.
  - **Expected Outcome**: Modal dialog is perfectly centered; backdrop covers entire $900 \times 600$; click dismisses modal; underlying wrapped toolbar remains cleanly rendered.
- **TEST-T3-06 (F10 + F13): `test_empty_state_cards_responsive_width_at_900`**
  - **Features**: 900x600 Responsive Layout + Empty State Cards
  - **Scenario**: Window at minimum size $900 \times 600$ ($W_{inner} = 612.0$px). Roam result is empty.
  - **Execution**: Measure empty state card bounding box.
  - **Expected Outcome**: Empty card conforms to $W \le 612.0$px without overflowing card boundaries or horizontal scroll clipping.
- **TEST-T3-07 (F11 + F8): `test_modal_escape_dismissal_preserves_background_input_focus`**
  - **Features**: Modal Esc Dismissal + TextInput Focus
  - **Scenario**: User was editing `track_input` (focused). A poster preview was opened. User presses `Escape`.
  - **Execution**: Process `Escape` key event.
  - **Expected Outcome**: Modal closes (`wrapped_poster = None`); focus remains on `track_input` without losing cursor or active selection.
- **TEST-T3-08 (F12 + F13): `test_history_delete_last_record_triggers_empty_state`**
  - **Features**: History Record Safeguard + Empty States & Feedback
  - **Scenario**: DB contains exactly 1 play record. User completes two-step deletion (click 1 $\to$ click 2).
  - **Execution**: Record is deleted from DB; view refreshes `recent_rows`.
  - **Expected Outcome**: Delete toast `"已永久删除该条播放记录"` is displayed; history view immediately transitions to display the empty history placeholder card.
- **TEST-T3-09 (F14 + F11): `test_poster_metrics_rendering_into_modal_preview`**
  - **Features**: Poster Completion Rate & GDI Emojis + Modal Poster View
  - **Scenario**: Generate annual stats with $c=8, n=10 \to "80\%"$, top artists with medals `["NO.1", "NO.2", "NO.3"]`, and clean GDI text.
  - **Execution**: Generate poster PNG bytes; assign to `wrapped_poster`.
  - **Expected Outcome**: PNG bytes are generated without GDI surrogate exceptions; modal displays the rendered poster preview; backdrop click dismisses cleanly.
- **TEST-T3-10 (F15 + F10): `test_dpi_scaling_preserves_gpui_logical_layout_constraints`**
  - **Features**: Win32 Dark Titlebar & DPI Scaling + 900x600 Window Constraints
  - **Scenario**: System DPI changes to 144 DPI (150% scaling). Physical window scales from $900 \times 600 \to 1350 \times 900$.
  - **Execution**: Measure GPUI logical pixels.
  - **Expected Outcome**: GPUI logical bounds remain $900.0 \times 600.0$; responsive wrapping triggers based on logical pixels while Win32 layered widget and titlebar scale by $1.5\times$ physical pixels.

---

## 5. Verification Method

### 5.1 Independent Test Execution Commands
The test suite designed above can be validated directly via `cargo test`:
```powershell
# 1. Verify project compiles cleanly with tests
cargo check --tests

# 2. Run existing baseline tests
cargo test --bin MusicHub

# 3. Run all newly created test suites once implemented
cargo test --test test_text_input
cargo test --test test_layout_responsive
cargo test --test test_safeguards
cargo test --test test_poster_metrics
cargo test --test test_dpi_and_platform
cargo test --test test_tier3_combinatorial

# 4. Run entire suite concurrently
cargo test
```

### 5.2 Verification Files to Inspect
- `tests/test_text_input.rs`: F7, F8, F9 unit tests.
- `tests/test_layout_responsive.rs`: F10 responsive layout and geometry solver tests.
- `tests/test_safeguards.rs`: F11, F12, F13 modal dismissals, history deletion, and empty states.
- `tests/test_poster_metrics.rs`: F14 completion rate formulas and GDI text sanitization.
- `tests/test_dpi_and_platform.rs`: F15 DPI scaling and Win32 DWM dark titlebar tests.
- `tests/test_tier3_combinatorial.rs`: Tier 3 cross-feature interactions.

### 5.3 Invalidation Conditions
The specifications in this report are invalidated if:
1. `TextInput` architecture is replaced with a third-party crate that handles scrolling and masking internally without custom GPUI layout code.
2. The minimum window size of Music Hub is increased beyond $980$px, rendering 900x600 responsive wrapping moot.
3. Win32 Dynamic Island is ported to GPUI instead of a native Win32 layered window, deprecating raw `WIDGET_WIDTH`/`WIDGET_HEIGHT` scaling.
