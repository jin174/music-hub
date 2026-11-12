# In-Depth Survey Report: Requirements 2 & 3 (R2 & R3)

**Date**: 2026-11-12  
**Agent**: teamwork_explorer (`explorer_survey_2`)  
**Scope**: 
- **R2: State Synchronization & Error Visibility** (`src/ui/mod.rs`, `src/server.rs`, `src/app.rs`, `src/scrobbler.rs`)
- **R3: TextInput Component Boundary & Focus Fixes** (`src/ui/input.rs`, `src/ui/mod.rs`)

---

## 1. Executive Summary

This survey provides a line-level investigation of the Music Hub codebase covering State Synchronization & Error Visibility (R2) and TextInput Component Boundary & Focus Fixes (R3).

### Key Findings at a Glance
| Requirement | Sub-area | Root Cause | Impact |
|---|---|---|---|
| **R2.1** | Last.fm / Scrobble Error Visibility | `AppStateResponse.last_error` is tracked in `Scrobbler` and polled by the UI loop, but `ConsoleView` completely ignores it during rendering. | Users experience silent scrobble failures; the pending counter climbs without any indication of network/auth/API failure reasons. |
| **R2.2** | Bottom Player Bar Favorite Toggle | `now-btn-love` directly executes `hub.db.set_feedback(..., "like")` without checking existing state, without updating in-memory `radar_result` or `roam_result`, and without refreshing `feedback_list`. | Liking a currently playing song leaves the active Radar and Roam table heart icons stale; cannot untoggle/remove; settings blacklist/favorites view does not reflect the update until tab switch/restart. |
| **R2.3** | OAuth Settings Synchronization | `server.rs::handle_callback` saves credentials to config and sends `ConsoleCommand::Open { tab: "settings" }`, but `ConsoleView::switch_tab` does not update `username_input` (`Entity<TextInput>`). | After authorizing in the browser, the status badge updates, but the username input field still displays the old text or remains blank. |
| **R3.1** | TextInput Horizontal Scrolling | `TextElement` has fixed width with `.overflow_hidden()`, but paints the shaped line at `bounds.origin` without a horizontal scroll offset. | Long strings (e.g. 150+ char NetEase Cloud Music share links) clip; cursor moves off-screen to the right and becomes invisible; user cannot see what is being typed or pasted. |
| **R3.2** | Focus Navigation & Shortcuts | `TextInput` lacks `Tab`/`Shift-Tab` key bindings and does not enable `.tab_stop(true)` on `focus_handle`; settings form text inputs have no `InputEvent::EnterPressed` subscriptions. | Cannot cycle focus between inputs using Tab key; pressing Enter in settings username/API key fields does not submit the form. |
| **R3.3** | Password Masking & Grapheme Alignment | `display_string` uses an ad-hoc byte-length match (`1 => '*', 2 => '·', 3 => '•', _ => "****"`) iterating over `chars()`. | Mixed, inconsistent glyphs for passwords; clicking on 4-byte characters (emojis) produces byte offsets that split UTF-8 code points, causing immediate Rust panic on slice. |

---

## 2. Requirement 2: State Synchronization & Error Visibility

### 2.1 Last.fm / Scrobble Failure Visibility

#### 2.1.1 Code Path & Origin
1. **Scrobbler Execution & Error Recording**:
   - In `src/scrobbler.rs:21`, `Scrobbler` declares `pub last_error: Arc<RwLock<Option<String>>>`.
   - In `src/scrobbler.rs:144-153`, during `scrobble_pending()`:
     ```rust
     match self.lf.scrobble(&items).await {
         Ok(accepted) => {
             ...
             *self.last_error.write().await = None;
             *self.last_ok.write().await = Some(now_sec());
         }
         Err(err) => {
             tracing::warn!("Scrobble failed: {err}");
             *self.last_error.write().await = Some(err.clone());
             return Err(err);
         }
     }
     ```
2. **App State Aggregation**:
   - In `src/app.rs:15-23`, `AppStateResponse` includes `pub last_error: Option<String>`.
   - In `src/app.rs:183-199`, `hub.state().await` retrieves `let last_error = self.scrobbler.last_error.read().await.clone();` and packs it into the response.
3. **UI State Retrieval**:
   - In `src/ui/mod.rs:194-202` (`ConsoleView::new`), `last_error` is hardcoded as `None`:
     ```rust
     let state = AppStateResponse {
         now,
         config: pub_cfg.clone(),
         scrobbling: hub.cfg.is_scrobbling(),
         pending,
         last_error: None,
         last_ok: None,
         session_app,
     };
     ```
   - In `src/ui/mod.rs:4038-4067`, the 500ms background loop executes:
     ```rust
     let st = hub_c.state().await;
     ...
     view.state = new_state;
     ```
   - Thus, `view.state.last_error` is continually kept up-to-date in memory!

#### 2.1.2 The Bug & Gap
Despite `view.state.last_error` being actively populated with error strings like `"Last.fm error (code 9): Invalid session key - Please re-authenticate"` or `"HTTP request failed: timed out"`, **it is never referenced anywhere in `src/ui/mod.rs`**:
- In `render_sidebar` (lines 711-721), only `self.state.pending` is displayed:
  ```rust
  child(if self.state.pending > 0 {
      format!("工作模式 (待打卡 {} 首)", self.state.pending)
  } else {
      "工作模式".to_string()
  })
  ```
- In `render_recent_view` (lines 2870-3050), pending tracks display `⏳ 打卡排队` badge, but there is no explanation of why the queue is not clearing.
- In `render_settings_view` (lines 3370-3400), the status badge shows `✅ 已授权绑定`, giving the false impression that scrobbling is functioning when the session key might actually be invalidated.

#### 2.1.3 Proposed Changes
1. **Initial State in `ConsoleView::new`**:
   - Read initial error state using `hub.scrobbler.last_error.try_read().ok().and_then(|g| g.clone())`.
2. **Sidebar Warning Indicator (`render_sidebar`, line 721)**:
   - When `let Some(ref err) = self.state.last_error`:
     Render a compact error badge or alert box below the mode switcher:
     ```rust
     .children(self.state.last_error.as_ref().map(|err| {
         div()
             .w_full()
             .px_2()
             .py_1()
             .rounded_sm()
             .bg(rgba(0xf8514922))
             .border_1()
             .border_color(rgb(0xf85149))
             .text_size(px(10.))
             .text_color(rgb(0xf85149))
             .truncate()
             .child(format!("⚠️ 打卡失败: {}", err))
     }))
     ```
3. **Recent Plays View Banner (`render_recent_view`, line 3001)**:
   - If `self.state.last_error.is_some()`, render a prominent callout bar above the recent plays table:
     ```rust
     div()
         .w_full()
         .p_3()
         .rounded_md()
         .bg(rgba(0xf8514918))
         .border_1()
         .border_color(rgba(0xf8514944))
         .flex()
         .items_center()
         .justify_between()
         .child(
             div()
                 .flex()
                 .items_center()
                 .gap_2()
                 .child(div().text_size(px(14.)).child("⚠️"))
                 .child(
                     div()
                         .text_size(px(12.))
                         .text_color(rgb(0xff7b7c))
                         .child(format!("Last.fm 打卡异常: {} (积压 {} 首待打卡)", err, self.state.pending))
                 )
         )
     ```
4. **Settings View OAuth Card (`render_settings_view`, line 3397)**:
   - If `self.state.last_error.is_some()`, show an error note underneath the authorization badge suggesting re-authentication if the error is session-related.

---

### 2.2 Bottom Player Bar Favorite Toggle & Multi-View Sync

#### 2.2.1 Code Path & The Bug
In `src/ui/mod.rs:1048-1087`:
```rust
div()
    .id("now-btn-love")
    ...
    .on_click(cx.listener(|this, _, _, cx| {
        let Some(n) = this.state.now.clone() else {
            return;
        };
        this.hub.db.set_feedback(&n.title, &n.artist, "like");
        if this.hub.cfg.is_authorized() {
            let lf = this.hub.lf.clone();
            let art = n.artist.clone();
            let trk = n.title.clone();
            this.hub.rt.spawn(async move {
                let _ = lf.love(&art, &trk).await;
            });
            this.show_toast(format!("❤️ 已同步红心至 Last.fm 与本地喜欢：{}", n.title), cx);
        } else {
            this.show_toast(format!("❤️ 已加入本地真爱喜欢：{}", n.title), cx);
        }
    }))
    .child("❤️ 红心")
```
Observations:
1. **Unilateral Action**: It always writes `"like"`. Clicking it a second time does not toggle or remove the like.
2. **In-Memory Desynchronization**:
   - `this.radar_result`: If the currently playing song is in the active Radar list, its `feedback` property is NOT updated. The user still sees the neutral button.
   - `this.roam_result`: If the currently playing song is in the active Roam list, its `feedback` property is NOT updated.
   - `this.feedback_list`: In Settings -> "真爱标记与避雷黑名单", the table is NOT refreshed. The song does not appear until the user leaves the tab and comes back or restarts the app.
3. **Comparison with Existing Method**:
   In `src/ui/mod.rs:412-472`, `ConsoleView` has `toggle_feedback_item(title, artist, target_action, cx)`. This method updates `radar_result`, `roam_result`, and `self.feedback_list = self.hub.db.get_feedback_list(200)`.
   However, `toggle_feedback_item` has an omission: at line 419-435, it only looks inside `radar_result` and `roam_result` to check the current feedback state; it does not query `self.hub.db.get_feedback(&title, &artist)`.

#### 2.2.2 Proposed Solution & Implementation Design
1. **Enhance `toggle_feedback_item`**:
   Query `self.hub.db.get_feedback(&title, &artist)` (or `self.feedback_list.iter().find(...)`) so the authoritative feedback state is always resolved even if the song is not in the active Radar/Roam table.
2. **Support Last.fm Sync in Toggle**:
   When toggling to `"like"`, if authorized, invoke `lf.love(&artist, &title)`.
3. **Refactor `now-btn-love` Click Handler**:
   In `now-btn-love`, delegate directly to `this.toggle_feedback_item(n.title.clone(), n.artist.clone(), "like", cx)`.
4. **Visual Feedback on Bottom Player Bar**:
   Check if `n.title` / `n.artist` is currently liked in `self.feedback_list` or SQLite. If liked:
   - Highlight the button with red background (`rgba(0xe8414233)`), red border (`rgb(0xe84142)`), and text `❤️ 已喜欢`.
   - If not liked, display `🤍 红心` or `❤️ 红心` with default dark background.

---

### 2.3 OAuth Browser Flow & Settings State Synchronization

#### 2.3.1 Code Path & The Bug
1. **Browser Auth Completion**:
   In `src/server.rs:118-128`:
   ```rust
   match app.lf.get_session(token).await {
       Ok((sk, name)) => {
           app.cfg.update(|d| {
               d.session_key = sk;
               d.username = name.clone();
           });
           tracing::info!("Authorized Last.fm session for user {}", name);
           app.open_console_with_toast(
               Some("settings"),
               Some(format!("🎉 Last.fm 授权成功！已绑定账号：{}", name)),
           );
   ...
   ```
2. **App Window Notification**:
   In `src/app.rs:245-256`, `open_console_with_toast` posts `ConsoleCommand::Open { tab: Some("settings"), toast }` to `console_tx`.
3. **UI Command Consumption**:
   In `src/ui/mod.rs:4027, 4072-4080`:
   ```rust
   if let Some((tab_opt, toast_opt)) = focus_req {
       window.activate_window();
       if let Some(t) = tab_opt {
           view.switch_tab(NavTab::from_str(&t), cx);
       }
       if let Some(msg) = toast_opt {
           view.show_toast(msg, cx);
       }
   }
   ```
4. **The Synchronization Gap**:
   In `src/ui/mod.rs:291-307` (`switch_tab`):
   ```rust
   NavTab::Settings => {
       self.feedback_list = self.hub.db.get_feedback_list(200);
       self.autostart_enabled = is_autostart_enabled();
   }
   ```
   Notice that `self.username_input` (`Entity<TextInput>`) was initialized once at startup with `cfg_data.username` (`src/ui/mod.rs:217`).
   `switch_tab` **never updates the text in `self.username_input`**!
   As a result:
   - The status badge (line 3392) reads directly from `self.hub.cfg.public()` and updates to `✅ 已授权绑定 (<name>)`.
   - But Section 1's "Last.fm 用户名 (Username)" input field (`self.username_input`) remains blank or displays the previous unsaved value!

#### 2.3.2 Proposed Solution
1. **Add `sync_settings_from_config` helper to `ConsoleView`**:
   ```rust
   fn sync_settings_from_config(&mut self, cx: &mut Context<Self>) {
       let cfg = self.hub.cfg.get_data();
       self.username_input.update(cx, |input, cx| {
           input.set_text(cfg.username, cx);
       });
       self.api_key_input.update(cx, |input, cx| {
           input.set_text(cfg.api_key, cx);
       });
       self.api_secret_input.update(cx, |input, cx| {
           input.set_text(cfg.api_secret, cx);
       });
       self.scrobble_unmatched = cfg.scrobble_unmatched;
   }
   ```
2. **Call in `switch_tab(NavTab::Settings)`**:
   When entering the Settings tab, always synchronize the inputs with the latest stored configuration.
3. **Call when `ConsoleCommand::Open` is handled**:
   When the OAuth callback opens the console window to the settings tab, trigger the sync so the user immediately sees their authenticated username in the input box.

---

## 3. Requirement 3: TextInput Component Boundary & Focus Fixes

### 3.1 Horizontal Scrolling Implementation in `TextInput`

#### 3.1.1 Current Architecture (`src/ui/input.rs`)
- `TextInput` struct (lines 57-68) stores:
  ```rust
  pub struct TextInput {
      pub focus_handle: FocusHandle,
      pub content: SharedString,
      pub placeholder: SharedString,
      pub is_password: bool,
      selected_range: Range<usize>,
      selection_reversed: bool,
      marked_range: Option<Range<usize>>,
      last_layout: Option<ShapedLine>,
      last_bounds: Option<Bounds<Pixels>>,
      is_selecting: bool,
  }
  ```
- In `TextElement::request_layout` (lines 496-500):
  ```rust
  let mut style = Style::default();
  style.size.width = relative(1.).into();
  style.size.height = window.line_height().into();
  (window.request_layout(style, [], cx), ())
  ```
- In `TextElement::prepaint` (lines 559-574):
  ```rust
  let line = window.text_system().shape_line(display_text, font_size, &runs, None);
  let cursor_pos = line.x_for_index(cursor);
  let (selection, cursor) = if selected_range.is_empty() {
      (None, Some(fill(
          Bounds::new(
              point(bounds.left() + cursor_pos, bounds.top()),
              size(px(1.5), bounds.bottom() - bounds.top()),
          ),
          rgb(0xe84142),
      )))
  } else ...
  ```
- In `TextElement::paint` (lines 616-626):
  ```rust
  let line = prepaint.line.take().unwrap();
  let _ = line.paint(bounds.origin, window.line_height(), window, cx);
  ```

#### 3.1.2 The Problem
1. When text exceeds the input container width (e.g. pasting long NetEase song URLs such as `https://music.163.com/song?id=12345678&userid=1234567` or long API secrets):
   - `line.width` exceeds `bounds.size.width`.
   - The outer `div` has `.overflow_hidden()`, so all content beyond `bounds.right()` is clipped.
   - `line.paint` always draws from `bounds.origin` (X = 0 relative to bounds).
   - `cursor_pos` is positioned at `bounds.left() + line.x_for_index(cursor)`. Because cursor is at the end, it is painted far beyond the right boundary of the window element.
   - The user cannot see the cursor or any characters past the initial ~30 characters.
2. Mouse click position mapping (`index_for_mouse_position`, lines 250-266):
   - `line.closest_index_for_x(position.x - bounds.left())` only inspects the first `[0, bounds.width]` slice of the text, making it impossible to click anywhere into the clipped trailing text.

#### 3.1.3 Solution & Architectural Design
1. **State Addition**:
   Add `scroll_offset: Pixels` to `TextInput` (initialized to `px(0.)`).
2. **Dynamic Scroll Calculation in `prepaint`**:
   Let `container_width = bounds.size.width`.
   Let `cursor_x = line.x_for_index(cursor)`.
   Let `total_width = line.width`.
   Compute new `scroll_offset`:
   - If `cursor_x < self.scroll_offset`:
     `scroll_offset = (cursor_x - px(12.)).max(px(0.))`
   - Else if `cursor_x > self.scroll_offset + container_width`:
     `scroll_offset = (cursor_x - container_width + px(12.)).max(px(0.))`
   - If `total_width <= container_width`:
     `scroll_offset = px(0.)`
   - Clamp: `scroll_offset = scroll_offset.clamp(px(0.), (total_width - container_width).max(px(0.)))`
   - Store back into `self.scroll_offset`.
3. **Coordinate Translations**:
   - `line.paint(point(bounds.origin.x - scroll_offset, bounds.origin.y), window.line_height(), window, cx)`
   - Cursor quad X: `bounds.left() + cursor_pos - scroll_offset`
   - Selection quad X: `bounds.left() + x_start - scroll_offset` to `bounds.left() + x_end - scroll_offset`
   - Mouse click: `line.closest_index_for_x(position.x - bounds.left() + scroll_offset)`
   - `bounds_for_range`: adjust corners by `- scroll_offset`
   - `character_index_for_point`: adjust point by `+ scroll_offset`

---

### 3.2 Focus Navigation (`Tab`) & Form Shortcuts (`Enter`)

#### 3.2.1 Current Implementation & Missing Hooks
1. **Missing Key Bindings**:
   In `src/ui/input.rs:32-48` (`bind_input_keys`), there are no bindings for `"tab"` or `"shift-tab"`.
2. **Missing `tab_stop` Flag**:
   In `src/ui/input.rs:80`, `TextInput::new` creates the focus handle with:
   `focus_handle: cx.focus_handle()`
   In GPUI (`gpui-0.2.2/src/window.rs:286`), `tab_stop` is `false` by default. Elements without `tab_stop: true` are excluded from the window's tab order (`self.rendered_frame.tab_stops`).
3. **Missing Form Submissions on Enter**:
   In `src/ui/mod.rs:241-250`, only `track_input` and `artist_input` have `InputEvent::EnterPressed` event subscriptions:
   ```rust
   let sub1 = cx.subscribe(&track_input, |this, _, ev: &InputEvent, cx| {
       if matches!(ev, InputEvent::EnterPressed) {
           this.run_radar(cx);
       }
   });
   ```
   In `render_settings_view` (lines 3240-3460), `username_input`, `api_key_input`, and `api_secret_input` have **no listeners** for `EnterPressed`. Pressing Enter while editing username or credentials does nothing.

#### 3.2.2 Solution & Architectural Design
1. **In `src/ui/input.rs`**:
   - Declare actions:
     ```rust
     actions!(
         music_hub_input,
         [
             ...
             Tab,
             PrevTab,
         ]
     );
     ```
   - Bind in `bind_input_keys`:
     ```rust
     KeyBinding::new("tab", Tab, Some("TextInput")),
     KeyBinding::new("shift-tab", PrevTab, Some("TextInput")),
     ```
   - In `TextInput::new`:
     ```rust
     focus_handle: cx.focus_handle().tab_stop(true),
     ```
   - In `TextInput` action handlers:
     ```rust
     fn tab(&mut self, _: &Tab, window: &mut Window, _: &mut Context<Self>) {
         window.focus_next();
     }
     fn prev_tab(&mut self, _: &PrevTab, window: &mut Window, _: &mut Context<Self>) {
         window.focus_prev();
     }
     ```
   - Register on root element in `TextInput::render`:
     ```rust
     .on_action(cx.listener(Self::tab))
     .on_action(cx.listener(Self::prev_tab))
     ```
   Because GPUI's `window.focus_next()` automatically wraps around the rendered tab stops in document order, pressing `Tab` cleanly cycles between `track_input` and `artist_input` on the Radar view, and cycles `username_input` -> `api_key_input` -> `api_secret_input` -> `username_input` on the Settings view.
2. **In `src/ui/mod.rs` (Settings Form Enter Submissions)**:
   Subscribe to `username_input`, `api_key_input`, and `api_secret_input` in `ConsoleView::new`:
   ```rust
   let sub3 = cx.subscribe(&username_input, |this, _, ev: &InputEvent, cx| {
       if matches!(ev, InputEvent::EnterPressed) {
           let u = this.username_input.read(cx).text().trim().to_string();
           this.hub.cfg.update(|d| d.username = u.clone());
           this.show_toast(format!("✅ 已保存 Last.fm 用户名：{}", u), cx);
       }
   });
   let sub4 = cx.subscribe(&api_key_input, |this, _, ev: &InputEvent, cx| {
       if matches!(ev, InputEvent::EnterPressed) {
           this.save_credentials_from_inputs(cx);
       }
   });
   let sub5 = cx.subscribe(&api_secret_input, |this, _, ev: &InputEvent, cx| {
       if matches!(ev, InputEvent::EnterPressed) {
           this.save_credentials_from_inputs(cx);
       }
   });
   ```
   Add `sub3`, `sub4`, `sub5` to `_subscriptions`.

---

### 3.3 Password Masking & Unicode Grapheme Cluster Alignment

#### 3.3.1 Current Implementation & Critical Flaws
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
**Why this implementation is defective**:
1. **Multi-byte Characters and Slicing Panics**:
   - For any 4-byte character (such as emojis or rare CJK extensions), it pushes four asterisks `"****"`.
   - `cursor` and `selected_range` are byte offsets into `self.content`.
   - However, `line.x_for_index` and `line.closest_index_for_x` operate on the shaped `display_string`.
   - If the user clicks between the second and third asterisk of a 4-byte character, `closest_index_for_x` returns `offset + 2`.
   - When this offset is assigned to `selected_range` and subsequently used in `replace_text_in_range` (`self.content[0..range.start]`), **Rust panics immediately**:
     `byte index 2 is not a char boundary; it is inside '...'`
2. **Grapheme Cluster Incoherence**:
   - `chars()` iterates over Unicode scalar values, not grapheme clusters.
   - Characters with combining marks (e.g. `e` + `\u{0301}`) or emoji sequences with zero-width joiners (ZWJ) are split into multiple distinct characters, displaying a mixture of dots and asterisks.
3. **Visual Inconsistency**:
   - A single password entered with mixed characters displays a jagged sequence of `*`, `·`, `•`, and `****`. Standard GUI password fields uniformly display `•` (bullet) or `*`.

#### 3.3.2 Solution & Architectural Design
1. **Use Uniform Password Mask**:
   Each grapheme cluster in `content` should correspond to exactly one visual bullet (e.g. `'•'`, U+2022, 3 UTF-8 bytes).
2. **Bi-directional Grapheme Index Mapping**:
   Maintain mapping between `content` byte offsets and `display_string` byte offsets:
   ```rust
   impl TextInput {
       fn content_to_display_byte_offset(&self, content_offset: usize) -> usize {
           if !self.is_password || self.content.is_empty() {
               return content_offset;
           }
           let bullet_len = '•'.len_utf8(); // 3 bytes
           let mut count = 0;
           for (offset, _) in self.content.grapheme_indices(true) {
               if offset >= content_offset {
                   break;
               }
               count += 1;
           }
           count * bullet_len
       }

       fn display_to_content_byte_offset(&self, display_offset: usize) -> usize {
           if !self.is_password || self.content.is_empty() {
               return display_offset;
           }
           let bullet_len = '•'.len_utf8();
           let grapheme_target_index = display_offset / bullet_len;
           let mut current_grapheme = 0;
           for (offset, _) in self.content.grapheme_indices(true) {
               if current_grapheme >= grapheme_target_index {
                   return offset;
               }
               current_grapheme += 1;
           }
           self.content.len()
       }
   }
   ```
3. **Usage in Prepaint and Event Handlers**:
   - In `display_string()`:
     ```rust
     if self.is_password && !self.content.is_empty() {
         let count = self.content.graphemes(true).count();
         "•".repeat(count).into()
     } else {
         self.content.clone()
     }
     ```
   - In `prepaint`:
     ```rust
     let display_cursor = input.content_to_display_byte_offset(cursor);
     let cursor_pos = line.x_for_index(display_cursor);
     let display_sel_start = input.content_to_display_byte_offset(selected_range.start);
     let display_sel_end = input.content_to_display_byte_offset(selected_range.end);
     ```
   - In `index_for_mouse_position`:
     ```rust
     let display_idx = line.closest_index_for_x(position.x - bounds.left() + self.scroll_offset);
     let content_idx = self.display_to_content_byte_offset(display_idx);
     content_idx.min(self.content.len())
     ```
4. **Safety Verification**:
   Because `display_to_content_byte_offset` strictly returns offsets yielded by `grapheme_indices(true)`, the cursor and selection ranges are mathematically guaranteed to land on valid UTF-8 char boundaries and grapheme boundaries, entirely preventing slice panics.

---

## 4. Synthesis & Recommendations for Implementation

### File Modification Checklist
1. **`src/ui/input.rs`**:
   - Add `Tab` and `PrevTab` actions to `actions!`.
   - Add `"tab"` and `"shift-tab"` to `bind_input_keys`.
   - Update `TextInput::new` to enable `.tab_stop(true)`.
   - Implement `tab()` and `prev_tab()` handlers delegating to `window.focus_next()` / `window.focus_prev()`.
   - Add `scroll_offset: Pixels` to `TextInput`.
   - Implement dynamic scroll adjustment and coordinate shifting in `TextElement::prepaint` and `TextElement::paint`.
   - Replace character-length hacking in `display_string` with grapheme-cluster-based uniform `'•'` masking and bi-directional offset mapping helpers.
2. **`src/ui/mod.rs`**:
   - Render `self.state.last_error` in `render_sidebar` (below mode switcher) and `render_recent_view` (above recent rows table).
   - In `ConsoleView::new`, initialize `last_error` from `hub.scrobbler.last_error`.
   - Add `InputEvent::EnterPressed` subscriptions for `username_input`, `api_key_input`, and `api_secret_input` to trigger form save.
   - Refactor bottom player bar `now-btn-love` to toggle like state, sync in-memory `radar_result`, `roam_result`, and `feedback_list`, and reflect active styling.
   - Add `sync_settings_from_config` and call it in `switch_tab(NavTab::Settings)` and on `ConsoleCommand::Open`.
3. **`src/server.rs`**:
   - Ensure `handle_callback` continues sending `ConsoleCommand::Open { tab: Some("settings"), toast }` which `ConsoleView` will now handle with full field synchronization.

---

## 5. Verification Protocol
- **Compilation Check**: `cargo check`
- **Unit Test Check**: `cargo test`
- **Verification Tests to Add**:
  - Test `display_string` and offset mappings with multi-byte characters, CJK ideographs, and emojis.
  - Test `toggle_feedback_item` correctly toggles state and updates in-memory radar/roaming structures.
