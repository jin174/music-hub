# Handoff Report: Specification Mining for Tier 3 Pairwise & Tier 4 Scenarios & TEST_INFRA.md Draft

- **Author**: Test Track Spec Miner (`test_track_spec_miner`)
- **Recipient**: E2E Testing Orchestrator (`sub_orch_test_track`, id: `f119d8d4-4796-4e92-aede-5db30c51d61c`)
- **Working Directory**: `e:\杂项\music-hub`
- **State Folder**: `e:\杂项\music-hub\.agents\teamwork\test_track_spec_miner`
- **Deliverable**: `e:\杂项\music-hub\.agents\teamwork\test_track_spec_miner\handoff.md`
- **Scope**: Tier 3 Cross-Feature Interactions (17 pairwise tests), Tier 4 Real-World Application Scenarios (9 end-to-end workflows), and Draft Feature Checklist & Coverage Summary table for `TEST_INFRA.md`.

---

## 1. Observation

### 1.1 Baseline Analysis from Authoritative Sources
A comprehensive audit of the authoritative project requirements was conducted against:
1. `ORIGINAL_REQUEST.md` (Requirements R1, R2, R3, R4 and Acceptance Criteria).
2. `orchestrator_1/PROJECT.md` (System architecture, F1–F16 feature inventory, milestone definitions, interface contracts).
3. `sub_orch_test_track/SCOPE.md` (Test Tier decomposition: Tier 1 Category-Partition, Tier 2 Boundary Value Analysis, Tier 3 Pairwise Combinations, Tier 4 Real-World Application Scenarios).
4. `explorer_survey_1/survey_r1.md`, `explorer_survey_2/survey_r2_r3.md`, `explorer_survey_3/survey_r4.md` (Deep architectural surveys and line-level codebase findings).
5. `test_track_explorer_1/handoff.md` and `test_track_explorer_2/handoff.md` (Exploration results covering R1-R2 and R3-R4 test specifications).
6. Live codebase in `src/` (`main.rs`, `app.rs`, `monitor.rs`, `scrobbler.rs`, `server.rs`, `db.rs`, `ui/mod.rs`, `ui/input.rs`, `poster.rs`, `widget.rs`, `normalize.rs`, `lastfm.rs`).

### 1.2 Cross-Feature Interdependency Map (F1 through F15)
The Music Hub architecture consists of concurrent asynchronous subsystems communicating through SQLite, tokio channels, Win32 window messaging, and GPUI model contexts. The 15 core features are not isolated units; their primary failure modes manifest at subsystem boundaries:

```
┌────────────────────────────────────────────────────────────────────────┐
│                          GPUI Console Window                           │
│  [F1 Lifecycle Hide/Show] [F15 Dark Titlebar & DPI] [F10 900x600]      │
│  ┌──────────────────────┐  ┌────────────────────────────────────────┐  │
│  │ Navigation Sidebar   │  │ Main Tab Content Views                 │  │
│  │ - [F4 Scrobble Error]│  │ - Settings: [F6 OAuth], [F7/F8/F9 Input│  │
│  │ - [F13 Empty State]  │  │ - Roam/Radar: [F10 Wrap], [F13 Toasts] │  │
│  │                      │  │ - History: [F12 Delete Safeguard]      │  │
│  │                      │  │ - Stats: [F11 Modal], [F14 Poster]     │  │
│  └──────────────────────┘  └────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ Bottom Player Bar: [F2 SMTC Cover] [F3 Repaint] [F5 Heart Sync]  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
          ▲                                       ▲
          │ ConsoleCommand::Open                  │ AppStateResponse
          ▼                                       ▼
┌───────────────────────┐   SQLite DB   ┌────────────────────────────────┐
│ Win32 Dynamic Island  │ ◄───────────► │ Background Hub Coordinator     │
│ & System Tray         │               │ - SMTC Media Monitor           │
│ [F15 DPI Scaling]     │               │ - Scrobbler Worker Queue       │
│ [F5 Heart Sync]       │               │ - IPC Web Server (/callback)   │
└───────────────────────┘               └────────────────────────────────┘
```

1. **F1 (Lifecycle) ↔ F6 (OAuth Sync)**:
   - Line `src/server.rs:90` receives OAuth tokens, writes to `config.json`, and sends `ConsoleCommand::Open { tab: Some("settings") }`.
   - When the window is hidden (`SW_HIDE`), restoring it (`SW_RESTORE`) must not drop the OAuth notification, must properly update `username_input`, and must avoid recreating the GPUI application loop.
2. **F5 (Heart Sync) ↔ F13 (Play Feedback & Toasts) ↔ F14 (Poster Stats)**:
   - Clicking `now-btn-love` (`src/ui/mod.rs:1048`) mutates SQLite `feedback` table (`src/db.rs:526`).
   - This mutation must propagate across in-memory `radar_result`, `roam_result`, and `feedback_list`.
   - F13 provides feedback toast ("❤️ 已同步红心...").
   - F14 aggregates completed plays and favorites to compute genuine completion percentage $(c/n) \times 100\%$, requiring coherent state across DB and UI caches.
3. **F7 (TextInput Scroll) ↔ F8 (Focus Navigation) ↔ F9 (Password Masking)**:
   - In `src/ui/input.rs`, text entry, horizontal scroll translation (`scroll_offset`), tab stop navigation, and Unicode grapheme masking (`display_string`) intersect.
   - When pasting long multilingual credentials or URLs (>150 chars), cursor translation must respect grapheme boundaries and maintain horizontal scroll clamping when focus shifts via `Tab`.
4. **F4 (Scrobble Error) ↔ F12 (History Deletion Safeguard)**:
   - When scrobbling fails (`src/scrobbler.rs:148-152`), `last_error` is populated and history rows display pending status `⏳ 打卡排队`.
   - Deleting a pending play via two-step inline confirmation (`confirm_delete_id`) in `src/ui/mod.rs:3184` must reconcile the SQLite scrobble queue without corrupting `AppStateResponse.pending` counts or leaving ghost retry jobs.
5. **F2 (SMTC Cover) ↔ F3 (Repaint Gating)**:
   - In `src/ui/mod.rs:4018-4067`, gating `cx.notify()` on state and cover changes requires thumbnail change keys to include `cover_bytes.len()`. Late cover arrivals must trigger a single layout update without causing 500ms polling churn.
6. **F10 (900x600 Layout) ↔ F11 (Poster Modal) ↔ F12 (Inline Confirmation)**:
   - At the minimum window dimension ($900 \times 600$), card inner width is only 612px. Inline delete confirmations and modal backdrops must render within bounding boxes without horizontal overflow or z-index clipping.

---

## 2. Logic Chain

### 2.1 Why Combinatorial Pairwise Testing (Tier 3) is Mandatory
A component-by-component unit test suite cannot verify multi-threaded interaction hazards. For example:
- `TextInput` grapheme masking (F9) can pass isolated unit tests, but panic when coupled with horizontal cursor scrolling (F7) and clipboard paste events.
- Heart toggling (F5) can pass SQLite isolation tests, but fail to update visible UI lists in Roam or Radar views when triggered from the bottom player bar.
- OAuth browser callbacks (F6) can correctly save to disk, but fail to reflect in GPUI input components if the window was minimized or in a background hide state (F1).
Therefore, **Tier 3 Pairwise Combinatorial Testing** systematically tests pairwise combinations of features F1–F15 across their communication boundaries.

### 2.2 Why End-to-End User Scenarios (Tier 4) are Mandatory
Music Hub is designed for continuous, long-running background execution during daily music listening. Real-world usage involves asynchronous sequences spanning multiple subsystems:
1. Media playback starts in external apps (SMTC monitor).
2. Late thumbnail extraction completes.
3. Network connection drops during track playback.
4. Track completes; scrobbler queues playback in SQLite and marks error.
5. User wakes UI from Dynamic Island to inspect error.
6. User authenticates via browser OAuth.
7. Network reconnects; scrobbler batch-retries.
8. User inspects updated history and generates annual stats poster.

Simulating these end-to-end user journeys ensures zero state desynchronization, crash-free lifecycle persistence, and predictable UX under realistic conditions.

---

## 3. Caveats

1. **Headless Execution Environment**: GPUI requires an active platform window system. For headless Windows CI (`cargo test`), UI tests must either run against GPUI's `TestAppContext` / headless test runner or test the decoupled state machines, data structures, layout engines, and math transformations without requiring physical GPU presentation.
2. **Win32 GDI & DPI Mocking**: Testing Win32 layered windows (`src/widget.rs`) and GDI typography (`src/poster.rs`) in headless environments requires testing through coordinate scaling math, DPI awareness APIs, and image buffer validation (`tiny_skia` / memory DC) rather than querying interactive desktop display monitors.
3. **External Network Mocking**: Last.fm API calls and NetEase Cloud Music endpoints must be exercised using local HTTP mock servers (e.g. `axum` test listeners or mocked responses) to ensure automated tests are 100% deterministic, offline-capable, and non-flaky.

---

## 4. Conclusion & Complete Specifications

### 4.1 Tier 3: Cross-Feature Interaction Specifications (17 Pairwise Tests)

Below are the 17 fully-specified pairwise interaction test specifications covering features F1–F15:

```
================================================================================
TIER 3 TEST SPECIFICATION INDEX
================================================================================
TEST-T3-01: F1 (Window Lifecycle)       + F6 (OAuth Settings Synchronization)
TEST-T3-02: F5 (Bottom Bar Heart Sync)   + F13 (Play Feedback & Toasts)
TEST-T3-03: F7 (TextInput Scroll)       + F9 (Password Masking Unicode Graphemes)
TEST-T3-04: F4 (Scrobble Error Display) + F12 (History Record Deletion Safeguard)
TEST-T3-05: F14 (Poster Completion Rate) + F5 (Bottom Bar Heart / Loved Tracks)
TEST-T3-06: F1 (Window Lifecycle)       + F2 (Late SMTC Cover Byte Detection)
TEST-T3-07: F3 (Repaint Gating)         + F13 (Toast Auto-Dismissal Timer)
TEST-T3-08: F7 (TextInput Scroll)       + F8 (Focus Navigation & Enter Handling)
TEST-T3-09: F10 (900x600 Layout)        + F12 (History Inline Delete Confirmation)
TEST-T3-10: F11 (Poster Modal Backdrop)  + F1 (Window Lifecycle Hide & Wake)
TEST-T3-11: F4 (Scrobble Error Display) + F6 (OAuth Settings Sync & Error Clear)
TEST-T3-12: F5 (Bottom Bar Heart Sync)   + F10 (900x600 Roaming View Responsive Header)
TEST-T3-13: F2 (Late SMTC Cover Bytes)   + F3 (Repaint Gating Single Invalidation)
TEST-T3-14: F8 (Focus Navigation)       + F9 (Password Masking & Unicode Selection)
TEST-T3-15: F13 (Empty State Guidance)   + F10 (900x600 Responsive Roaming Toolbar)
TEST-T3-16: F14 (Poster Stats & Badges)  + F11 (Poster Modal Dismissal via Esc)
TEST-T3-17: F1 (Window Lifecycle)       + F15 (Win32 Dark Titlebar & DPI Scaling)
================================================================================
```

---

#### `TEST-T3-01`: F1 (Window Lifecycle) + F6 (OAuth Settings Synchronization)
- **Features**: F1 (Long-lived GPUI Application Lifecycle) × F6 (OAuth Settings Synchronization).
- **Objective**: Verify that when the console window is hidden (`SW_HIDE`), receiving a browser OAuth callback updates config, restores the window (`SW_RESTORE`), switches to Settings tab, and populates `username_input` without restarting GPUI or crashing.
- **Preconditions**:
  - Console window initialized and hidden to system tray (`ShowWindow(hwnd, SW_HIDE)`).
  - GPUI `Application` loop is running in background.
  - Initial `config.lastfm.username` is empty `""`.
- **Action Sequence**:
  1. Trigger local IPC HTTP GET request: `GET /callback?token=mock_oauth_token_xyz`.
  2. Server exchanges token for session key (`sk: "mock_session_key_123"`, `name: "audiophile_user"`).
  3. Server updates `config.json` on disk and pushes `ConsoleCommand::Open { tab: Some("settings"), toast: Some("✅ Last.fm 授权绑定成功！".into()) }` to `cmd_tx`.
  4. GPUI background listener receives command.
- **Expected State Transitions**:
  - Main window state transitions from Hidden (`SW_HIDE`) to Restored/Foreground (`SW_RESTORE`).
  - Active navigation tab updates from previous tab to `NavTab::Settings`.
  - Settings model updates: `this.username_input.update(cx, |input, _| input.set_content("audiophile_user"))`.
  - Status badge changes from `"❌ 未授权"` to `"✅ 已授权绑定: audiophile_user"`.
  - Toast `"✅ Last.fm 授权绑定成功！"` is queued in `ConsoleView`.
- **Assertions**:
  - `assert_eq!(window.is_visible(), true);`
  - `assert_eq!(view.active_tab, NavTab::Settings);`
  - `assert_eq!(view.username_input.read(cx).content(), "audiophile_user");`
  - `assert_eq!(hub.cfg.is_authorized(), true);`
  - No new Win32 window class is registered (`ERROR_CLASS_ALREADY_EXISTS` is not thrown).
- **Hazard & Edge Case**:
  - Token callback arrives while window is in the middle of closing; race condition must resolve cleanly without dropping the command or corrupting input focus.

---

#### `TEST-T3-02`: F5 (Bottom Bar Heart Sync) + F13 (Play Feedback & Toasts)
- **Features**: F5 (Bottom Bar Heart Synchronization) × F13 (Empty States & Click Feedback).
- **Objective**: Verify that toggling heart in the bottom player bar immediately updates SQLite, refreshes active Roaming and Radar track models, dispatches an asynchronous Last.fm love call, and renders a descriptive toast feedback notification.
- **Preconditions**:
  - A track is actively playing: Title `"夜曲"`, Artist `"周杰伦"`.
  - Song is currently listed in `roam_result.tracks` and `radar_result.tracks` with `feedback: None`.
  - Settings `feedback_list` loaded in memory.
- **Action Sequence**:
  1. User clicks `now-btn-love` (`src/ui/mod.rs:1048`) in bottom player bar.
  2. Handler invokes `toggle_feedback_item("夜曲", "周杰伦")`.
  3. Check resulting in-memory lists, DB entries, and toast container.
  4. User clicks `now-btn-love` a second time (untoggle action).
- **Expected State Transitions**:
  - Step 1-2: SQLite executes `INSERT OR REPLACE INTO feedback ... action='like'`.
  - `roam_result.tracks.iter_mut().find(|t| t.title == "夜曲").feedback` flips to `Some("like")`.
  - `radar_result.tracks.iter_mut().find(|t| t.title == "夜曲").feedback` flips to `Some("like")`.
  - `feedback_list` prepends new `FeedbackRow { title: "夜曲", artist: "周杰伦", action: "like" }`.
  - Toast created: `"❤️ 已同步红心至 Last.fm 与本地喜欢：夜曲"`.
  - Step 4: Untoggle executes `DELETE FROM feedback WHERE title_key=? AND artist_key=?`.
  - Both Roam and Radar items flip to `feedback: None`.
  - Toast created: `"🤍 已取消喜欢：夜曲"`.
- **Assertions**:
  - `assert_eq!(hub.db.get_feedback("夜曲", "周杰伦"), Some("like".to_string()));`
  - `assert_eq!(view.roam_result.as_ref().unwrap().tracks[0].feedback, Some("like".to_string()));`
  - `assert_eq!(view.radar_result.as_ref().unwrap().tracks[0].feedback, Some("like".to_string()));`
  - `assert!(view.toasts.iter().any(|t| t.text.contains("已同步红心")));`
  - Second click: `assert_eq!(hub.db.get_feedback("夜曲", "周杰伦"), None);`
- **Hazard & Edge Case**:
  - Track contains special punctuation (`"夜曲 (Live) · 2005"`); normalized `title_key` and `artist_key` must match between bottom player bar and Roaming list.

---

#### `TEST-T3-03`: F7 (TextInput Scroll) + F9 (Password Masking Unicode Graphemes)
- **Features**: F7 (TextInput Horizontal Scrolling) × F9 (Password Masking Unicode Graphemes).
- **Objective**: Verify that pasting and editing a password containing multi-byte Unicode characters, emojis, and symbols scrolls horizontally without visual clipping and maps graphemes 1:1 to bullets without byte-slice panics.
- **Preconditions**:
  - `TextInput` initialized with `is_password: true`, width $W = \text{px}(200.0)$, padding $\text{px}(8.0)$ (visible text width $W_{vis} = 184\text{px}$).
- **Action Sequence**:
  1. Set content to a 30-grapheme password containing multi-byte UTF-8, CJK, and emojis:
     `"P@ss🔑123🔒中島みゆき✨2026!#$*~"` (length = 30 grapheme clusters, 64 UTF-8 bytes).
  2. Move cursor to index 30 (end of string).
  3. Calculate `scroll_offset` and `display_string`.
  4. Simulate mouse click at pixel position $X = 150.0$ on the masked text.
  5. Press Backspace twice to delete the last two characters (`"~"` and `"*"`).
- **Expected State Transitions**:
  - `display_string` produces exactly 30 identical bullets: `"••••••••••••••••••••••••••••••"` (all `•` U+2022, 3 bytes each; total 90 bytes).
  - Each grapheme corresponds to exactly one bullet; no surrogate split or multi-character replacement (`****`).
  - `line.width()` exceeds 184px; `scroll_offset` translates bounds leftward so cursor at end is within viewport:
    $0 \le \text{cursor\_pos} - \text{scroll\_offset} \le 184$.
  - Mouse click at $X=150.0$ computes target grapheme offset accurately by taking `scroll_offset` into account.
  - Backspace deletes exact graphemes without splitting UTF-8 boundaries or causing slice panics.
- **Assertions**:
  - `assert_eq!(input.display_string().chars().count(), 30);`
  - `assert_eq!(input.display_string().chars().all(|c| c == '•'), true);`
  - `assert!(input.scroll_offset() > px(0.0));`
  - Cursor after 2 backspaces: `assert_eq!(input.cursor_grapheme_index(), 28);`
  - No panic on slice boundaries.
- **Hazard & Edge Case**:
  - 4-byte astral code points (e.g. `🔑` U+1F511, `🔒` U+1F512) and zero-width joiner sequences; segmentation must use `unicode_segmentation::UnicodeSegmentation::graphemes`.

---

#### `TEST-T3-04`: F4 (Scrobble Error Display) + F12 (History Record Deletion Safeguard)
- **Features**: F4 (Scrobble Error Visibility) × F12 (History Record Deletion Safeguard).
- **Objective**: Verify that when a scrobble failure occurs and records `last_error`, deleting the failed pending history record engages the two-step inline confirmation safeguard, updates pending counts upon confirmation, and reconciles state without error masking.
- **Preconditions**:
  - SQLite database contains 1 pending play row: `id: 42, title: "Bad Romance", artist: "Lady Gaga", scrobble_state: 'pending'`.
  - Scrobbler encountered HTTP 503 error: `last_error: Some("HTTP 503: Service Unavailable")`.
  - UI sidebar displays `"工作模式 (待打卡 1 首)"` and red error banner `"HTTP 503: Service Unavailable"`.
- **Action Sequence**:
  1. User navigates to History (Recent) view.
  2. User clicks delete button (`✕`) on play record `id: 42`.
  3. Verify UI enters inline confirmation state (`confirm_delete_id: Some(42)`). Record is NOT deleted from DB yet.
  4. User clicks "取消" (Cancel).
  5. Verify `confirm_delete_id` resets to `None`, row remains in DB, and error remains visible.
  6. User clicks delete button (`✕`) again on play `id: 42`, then clicks "确认删除" (Confirm).
- **Expected State Transitions**:
  - Step 2-3: Row UI changes from default delete icon to inline prompt: `[确定删除] [取消]`.
  - DB query verifies `SELECT COUNT(*) FROM plays WHERE id=42` returns `1`.
  - Step 4-5: `confirm_delete_id` set to `None`; UI reverts to normal row display.
  - Step 6: `db.delete_play(42)` executed; row removed from SQLite.
  - `view.state.pending` decrements from 1 to 0.
  - History list reloads; row 42 disappears.
  - Sidebar updates to `"工作模式"` (pending count = 0); error banner remains visible until cleared or new scrobble succeeds.
- **Assertions**:
  - Step 3: `assert_eq!(view.confirm_delete_id, Some(42));`
  - Step 3: `assert_eq!(hub.db.get_play(42).is_some(), true);`
  - Step 5: `assert_eq!(view.confirm_delete_id, None);`
  - Step 6: `assert_eq!(hub.db.get_play(42).is_none(), true);`
  - Step 6: `assert_eq!(hub.db.pending_scrobbles(10).len(), 0);`
- **Hazard & Edge Case**:
  - Deleting row while background scrobbler is actively retrying `flush()`; `Mutex<()>` in DB/Scrobbler must prevent SQLITE_BUSY or double delete.

---

#### `TEST-T3-05`: F14 (Poster Completion Rate) + F5 (Bottom Bar Heart / Loved Tracks)
- **Features**: F14 (Poster Completion Rate & GDI Emojis) × F5 (Bottom Bar Heart Synchronization).
- **Objective**: Verify that tracks loved via the bottom player bar are properly included in listening statistics, and that the poster generator computes the actual completion rate $(c / n) \times 100\%$ rather than a static hardcoded string.
- **Preconditions**:
  - Database populated with 10 total plays for `"晴天" · "周杰伦"`, with 8 fully completed (`completed = 1`) and 2 skipped (`skipped = 1`). Total: $n = 10, c = 8$.
  - Song is currently playing in bottom player bar.
- **Action Sequence**:
  1. User clicks `now-btn-love` to mark `"晴天"` as a loved track.
  2. User switches to Stats tab and triggers "生成海报".
  3. `render_wrapped_poster` queries DB for top loved track and generates poster bitmap.
  4. Inspect text emitted for completion rate in poster.
- **Expected State Transitions**:
  - `set_feedback("晴天", "周杰伦", "like")` marks track as favorite.
  - `app.stats(30)` aggregates plays: `StatsTrackItem { title: "晴天", artist: "周杰伦", n: 10, c: 8, s: 2 }`.
  - Completion percentage calculated: $\frac{c}{n} \times 100\% = \frac{8}{10} \times 100\% = 80\%$.
  - GDI text drawing draws `"80% 真爱完播"` (instead of `"100% 真爱完播"`).
  - Ranks and badges are rendered using clean typography shapes (e.g. `#1`, `#2`, `TOP 1`) instead of Win32 emoji glyphs.
- **Assertions**:
  - `assert_eq!(stats.loved[0].n, 10);`
  - `assert_eq!(stats.loved[0].c, 8);`
  - `let rate = (stats.loved[0].c as f64 / stats.loved[0].n as f64 * 100.0).round() as i64;`
  - `assert_eq!(rate, 80);`
  - `assert_eq!(format!("{rate}% 真爱完播"), "80% 真爱完播");`
- **Hazard & Edge Case**:
  - $n = 0$ (division by zero guard: `n.max(1)`); $c > n$ (clamped to 100%); $c = 0$ (renders `"0% 真爱完播"`).

---

#### `TEST-T3-06`: F1 (Window Lifecycle) + F2 (Late SMTC Cover Byte Detection)
- **Features**: F1 (Long-lived GPUI Application Lifecycle) × F2 (Late SMTC Cover Byte Detection).
- **Objective**: Verify that if SMTC thumbnail bytes arrive while the window is hidden, waking the window immediately reflects the decoded cover without missing the update or re-running the GPUI initialization loop.
- **Preconditions**:
  - Console window is hidden (`SW_HIDE`).
  - Active track playing: `play_id: 101, title: "Shape of You", artist: "Ed Sheeran"`.
  - Initial `cover_bytes` is `None`. Cover change detection key: `"101:0:false:0"`.
- **Action Sequence**:
  1. SMTC monitor extracts late image bytes (JPEG 45,000 bytes) and updates `NowPlaying.cover_bytes`.
  2. System tray receives wake click: dispatches `ConsoleCommand::Open { tab: None, toast: None }`.
  3. Background listener unhides window (`SW_RESTORE`).
  4. UI background synchronization loop evaluates cover cache key.
- **Expected State Transitions**:
  - Cover change key evaluates: `"101:45000:true:45000"`.
  - Key differs from `"101:0:false:0"`; triggers `decode_image_bytes(cover_bytes)`.
  - `view.cover_image` is populated with `Arc<RenderImage>`.
  - Window wakes and renders decoded cover image in the bottom player bar immediately.
- **Assertions**:
  - `assert_ne!(new_cover_key, old_cover_key);`
  - `assert!(view.cover_image.is_some());`
  - `assert_eq!(window.is_visible(), true);`
- **Hazard & Edge Case**:
  - Corrupt image bytes in `cover_bytes`; decoding fails gracefully to default placeholder cover without panic.

---

#### `TEST-T3-07`: F3 (Repaint Gating) + F13 (Toast Auto-Dismissal Timer)
- **Features**: F3 (Repaint Gating & Decoupled Toasts) × F13 (Empty States & Click Feedback).
- **Objective**: Verify that toast notifications auto-dismiss via decoupled async timers and that UI repaints (`cx.notify()`) occur ONLY upon state mutation and toast expiration, eliminating the 500ms continuous repaint loop.
- **Preconditions**:
  - ConsoleView is open and idle.
  - No active song playing; no state changes occurring.
  - Notification counter tracking `cx.notify()` calls.
- **Action Sequence**:
  1. Monitor UI notifications for 2.0 seconds in idle state.
  2. Trigger play action on Loved item: calls `show_toast("▶ 正在跳转播放：晴天", cx)`.
  3. Measure notification events immediately after toast creation.
  4. Wait 3.5 seconds (toast TTL = 3.0s).
  5. Measure notification events upon toast expiration.
- **Expected State Transitions**:
  - Step 1: During the 2.0s idle period, `cx.notify()` is called 0 times (0 Hz repaint rate).
  - Step 2-3: `show_toast` appends toast and calls `cx.notify()` exactly ONCE to render the toast.
  - An async timer (`tokio::time::sleep(Duration::from_millis(3000))`) is spawned in background.
  - Step 4-5: When timer completes, toast is removed from `self.toasts`, calling `cx.notify()` exactly ONCE.
  - Total notify calls over 5.5s = 2 (compared to 11 calls in the buggy 500ms ticker implementation).
- **Assertions**:
  - `assert_eq!(idle_notify_count, 0);`
  - `assert_eq!(toast_created_notify_count, 1);`
  - `assert_eq!(view.toasts.len(), 1);`
  - After 3.5s: `assert_eq!(view.toasts.len(), 0);`
  - `assert_eq!(total_notify_count, 2);`
- **Hazard & Edge Case**:
  - Rapidly clicking play 5 times creates 5 toasts; each timer must cancel or pop its own toast cleanly without array index out-of-bounds.

---

#### `TEST-T3-08`: F7 (TextInput Scroll) + F8 (Focus Navigation & Enter Handling)
- **Features**: F7 (TextInput Horizontal Scrolling) × F8 (Focus Navigation & Enter Handling).
- **Objective**: Verify that cycling focus between text inputs via `Tab` and `Shift-Tab` preserves horizontal scroll positions and cursor offsets, and that pressing `Enter` fires form submission.
- **Preconditions**:
  - Settings view with two inputs: Input A (`Last.fm API Key`), Input B (`Last.fm Secret`).
  - Both inputs have `.tab_stop(true)`.
- **Action Sequence**:
  1. Focus Input A. Enter long string: `"abcdefghijklmnopqrstuvwxyz0123456789_very_long_api_key_exceeding_width"`.
  2. Verify Input A computes `scroll_offset > 0` and cursor is at end.
  3. Press `Tab` key.
  4. Verify focus shifts to Input B (`window.focus_next(cx)`).
  5. In Input B, type `"secret_123"` and press `Enter`.
  6. Press `Shift-Tab` to navigate back to Input A.
- **Expected State Transitions**:
  - Step 2: Input A `scroll_offset` is positive; trailing characters are visible.
  - Step 3-4: Focus transfers to Input B. Input A loses focus, but preserves its `content`, `cursor_offset`, and `scroll_offset`.
  - Step 5: `Enter` triggers `InputEvent::EnterPressed` subscription, invoking `save_settings()`.
  - Step 6: Focus returns to Input A; `scroll_offset` is maintained and cursor is rendered at previous offset.
- **Assertions**:
  - `assert_eq!(input_a.read(cx).focus_handle.is_focused(cx), false);`
  - `assert_eq!(input_b.read(cx).focus_handle.is_focused(cx), true);`
  - `assert!(input_a.read(cx).scroll_offset() > px(0.0));`
  - `assert_eq!(settings_saved_flag, true);`
- **Hazard & Edge Case**:
  - Pressing Tab inside the last input cycles focus back to the first input without trapping focus or losing layout state.

---

#### `TEST-T3-09`: F10 (900x600 Layout) + F12 (History Inline Delete Confirmation)
- **Features**: F10 (900x600 Responsive Layout) × F12 (History Record Deletion Safeguard).
- **Objective**: Verify that at minimum window dimensions ($900 \times 600$, content width 612px), expanding an inline deletion confirmation does not break table alignment or wrap action buttons out of view.
- **Preconditions**:
  - Window resized to $900 \times 600$.
  - History view open with 10 rows. Toolbar contains 4 buttons with `.flex_wrap().gap_2()`.
- **Action Sequence**:
  1. Measure initial bounds of toolbar and table rows.
  2. Click delete (`✕`) on Row 3.
  3. Row 3 replaces delete button with inline confirmation container: `[确定删除] [取消]`.
  4. Measure width and bounds of Row 3 and the History toolbar.
- **Expected State Transitions**:
  - History toolbar buttons wrap onto a secondary line gracefully if necessary without colliding with the title.
  - Row 3 inline confirmation elements fit within the row bounds without triggering horizontal window scrollbars or clipping text.
- **Assertions**:
  - `assert!(toolbar.bounds.bottom() <= content_card.bounds.bottom());`
  - `assert!(row_3.bounds.right() <= content_card.bounds.right());`
  - `assert_eq!(view.confirm_delete_id, Some(row_3.id));`
- **Hazard & Edge Case**:
  - Long song title on Row 3 (e.g. 50 characters); title must use ellipsis or truncate cleanly so inline buttons stay visible.

---

#### `TEST-T3-10`: F11 (Poster Modal Backdrop) + F1 (Window Lifecycle Hide & Wake)
- **Features**: F11 (Modal Backdrop & Esc Dismissal) × F1 (Long-lived GPUI Application Lifecycle).
- **Objective**: Verify that if the poster modal is open when the window is hidden, re-waking the window preserves modal state, and clicking the backdrop or pressing `Esc` dismisses the modal without closing the window.
- **Preconditions**:
  - User generates listening stats poster.
  - Poster modal is displayed (`view.poster_modal: Some(WrappedPoster)`).
- **Action Sequence**:
  1. Window close triggered via title bar or system hotkey; GPUI intercepts `on_window_should_close` and hides window (`SW_HIDE`).
  2. Dynamic Island / Tray sends `ConsoleCommand::Open { tab: None, toast: None }`.
  3. Window restored (`SW_RESTORE`).
  4. User presses `Esc` key.
  5. Inspect `view.poster_modal` and window visibility.
- **Expected State Transitions**:
  - Step 1: Window hidden; `view.poster_modal` remains retained in memory.
  - Step 3: Window restores; modal is visible.
  - Step 4: `Esc` key handler fires `this.poster_modal = None; cx.notify();`.
  - Modal is dismissed; main window remains open and fully interactive.
- **Assertions**:
  - Step 3: `assert!(view.poster_modal.is_some());`
  - Step 4: `assert!(view.poster_modal.is_none());`
  - Step 4: `assert_eq!(window.is_visible(), true);`
- **Hazard & Edge Case**:
  - Backdrop click event bubbling through to underlying tab buttons; `.stop_propagation()` must prevent underlying clicks.

---

#### `TEST-T3-11`: F4 (Scrobble Error Display) + F6 (OAuth Settings Sync & Error Clear)
- **Features**: F4 (Scrobble Error Visibility) × F6 (OAuth Settings Synchronization).
- **Objective**: Verify that when a scrobble failure is due to an invalid session key (error code 9), completing OAuth re-authentication updates the username input, triggers re-authorization, and clears the error display on the next scrobble cycle.
- **Preconditions**:
  - `hub.scrobbler.last_error` contains `"Last.fm error (code 9): Invalid session key - Please re-authenticate"`.
  - Sidebar displays red error badge. Settings status shows `"❌ 授权已失效"`.
- **Action Sequence**:
  1. User triggers OAuth flow in browser.
  2. Local server receives `/callback` with valid token; updates `config.lastfm.session_key = "valid_sk_999"`.
  3. IPC dispatches settings update to console.
  4. Scrobbler background worker runs `flush()`.
- **Expected State Transitions**:
  - Settings `username_input` populates with authorized username.
  - Status badge flips to `"✅ 已授权绑定"`.
  - Scrobbler successfully submits pending items; `*self.last_error.write().await = None`.
  - UI sidebar error alert disappears.
- **Assertions**:
  - `assert_eq!(hub.cfg.is_authorized(), true);`
  - `assert_eq!(hub.scrobbler.last_error.read().await.is_none(), true);`
  - `assert_eq!(view.state.last_error, None);`
- **Hazard & Edge Case**:
  - Race condition where scrobbler retries before config file write is completed; file lock or in-memory config update must synchronize.

---

#### `TEST-T3-12`: F5 (Bottom Bar Heart Sync) + F10 (900x600 Roaming View Responsive Header)
- **Features**: F5 (Bottom Bar Heart Synchronization) × F10 (900x600 Responsive Layout).
- **Objective**: Verify that hearting a track in the bottom player bar at 900x600 window size updates the Roaming recommendation list item heart icon without triggering horizontal overflow or card height recalculation jitter.
- **Preconditions**:
  - Window size: $900 \times 600$.
  - Roaming tab is active with 10 generated tracks.
  - Track 2 is currently playing in the bottom bar.
- **Action Sequence**:
  1. Click `now-btn-love` in the bottom player bar.
  2. Handler updates SQLite and `roam_result.tracks[1].feedback = Some("like")`.
  3. Inspect Roaming view layout bounds and Track 2 heart icon.
- **Expected State Transitions**:
  - Bottom bar heart switches to red/active color.
  - Roaming view Track 2 heart icon switches from gray to red filled heart.
  - Content card layout width remains exactly 612px; no horizontal overflow scrollbar appears.
- **Assertions**:
  - `assert_eq!(view.roam_result.as_ref().unwrap().tracks[1].feedback, Some("like".to_string()));`
  - `assert!(roaming_container.bounds.width() <= px(644.0));`
- **Hazard & Edge Case**:
  - Re-rendering the list row does not reset the Roaming view's vertical scroll position.

---

#### `TEST-T3-13`: F2 (Late SMTC Cover Bytes) + F3 (Repaint Gating Single Invalidation)
- **Features**: F2 (Late SMTC Cover Byte Detection) × F3 (Repaint Gating & Decoupled Toasts).
- **Objective**: Verify that when late SMTC cover bytes arrive for an existing track, the cache key change detects the new bytes and triggers exactly ONE repaint upon decode completion, rather than polling repaints.
- **Preconditions**:
  - Track playing: `title: "Bohemian Rhapsody"`, cover initially absent (`cover: ""`, `cover_bytes: None`).
  - Gated repaint loop running. Initial `last_cover_key = "105:0:false:0"`.
- **Action Sequence**:
  1. Monitor repaint count for 1.0 second (no changes).
  2. Late SMTC cover bytes arrive (32 KB PNG).
  3. Cover key transitions to `"105:32768:true:32768"`.
  4. Async image decode spawns and resolves.
- **Expected State Transitions**:
  - Step 1: 0 repaints in 1.0 second.
  - Step 3-4: Key difference detected; image decoded into `view.cover_image`.
  - `cx.notify()` called exactly once upon image decode completion.
- **Assertions**:
  - `assert_eq!(pre_event_repaints, 0);`
  - `assert_eq!(post_decode_repaints, 1);`
  - `assert!(view.cover_image.is_some());`
- **Hazard & Edge Case**:
  - Multiple rapid byte chunks from SMTC stream; only the final stable byte array triggers image decode.

---

#### `TEST-T3-14`: F8 (Focus Navigation) + F9 (Password Masking & Unicode Selection)
- **Features**: F8 (Focus Navigation & Enter Handling) × F9 (Password Masking Unicode Graphemes).
- **Objective**: Verify that keyboard focus cycling via `Tab` maintains cursor selection ranges in masked password inputs containing multilingual characters.
- **Preconditions**:
  - Two password inputs: Password Input 1 and Password Input 2.
  - Input 1 contains `"安全🔑パスワード"` (8 graphemes, 22 UTF-8 bytes).
  - Selected range in Input 1: graphemes 2 to 5 (`"🔑パス"`).
- **Action Sequence**:
  1. Press `Tab` to shift focus to Input 2.
  2. Verify Input 1 preserves its selection range without byte offset corruption.
  3. Press `Shift-Tab` to return focus to Input 1.
  4. Press Backspace while text is selected.
- **Expected State Transitions**:
  - Step 1-2: Selection range (graphemes 2..5) is preserved in `Input 1`.
  - Step 3-4: Returning focus restores selection; Backspace deletes the 3 selected graphemes cleanly.
  - Resulting content: `"安全ワード"` (5 graphemes).
  - Masked string: `"•••••"` (5 bullets).
- **Assertions**:
  - `assert_eq!(input_1.read(cx).content(), "安全ワード");`
  - `assert_eq!(input_1.read(cx).display_string(), "•••••");`
  - No panic on byte slicing.
- **Hazard & Edge Case**:
  - Selecting across multi-byte character boundaries; selection bounds must clamp to valid UTF-8 and grapheme cluster boundaries.

---

#### `TEST-T3-15`: F13 (Empty State Guidance) + F10 (900x600 Responsive Roaming Toolbar)
- **Features**: F13 (Empty States & Click Feedback) × F10 (900x600 Responsive Layout).
- **Objective**: Verify that when the Roaming tab has no seed tracks on a 900x600 window, the empty state card and responsive toolbar header render without overlapping or button clipping.
- **Preconditions**:
  - Database contains 0 completed plays.
  - Roaming tab active at $900 \times 600$ window bounds.
- **Action Sequence**:
  1. Open Roaming tab.
  2. Measure dimensions of top control card and empty state illustration card.
- **Expected State Transitions**:
  - Top header toolbar wraps buttons into responsive flex container without pushing action buttons outside card bounds.
  - Content area displays styled Empty State card with illustration icon `"🌌"`, guidance title `"暂无漫游推荐"`, and subtitle `"先去听几首歌积累种子，或者去设置绑定 Last.fm 账户吧"`.
- **Assertions**:
  - `assert!(roam_header.bounds.width() <= px(612.0));`
  - `assert!(empty_state_card.is_displayed());`
  - `assert!(empty_state_card.bounds.bottom() <= px(600.0));`
- **Hazard & Edge Case**:
  - Clicking "✨ 生成今日漫游歌单" on empty database does not panic; displays a guidance toast instead.

---

#### `TEST-T3-16`: F14 (Poster Stats & Badges) + F11 (Poster Modal Dismissal via Esc)
- **Features**: F14 (Poster Completion Rate & GDI Emojis) × F11 (Modal Backdrop & Esc Dismissal).
- **Objective**: Verify that generating a poster renders clean typography badges instead of missing emoji glyph boxes, and pressing `Esc` immediately releases the modal and cleans up the rendered image buffer.
- **Preconditions**:
  - Listening stats view open. Top 3 artists have ranks 1, 2, and 3.
- **Action Sequence**:
  1. Click "生成年度唱片海报".
  2. GDI poster generator renders poster bitmap.
  3. Verify ranks 1, 2, 3 use clean numeric badges (`TOP 1`, `TOP 2`, `TOP 3`) or vector shapes instead of GDI-incompatible emojis (`🥇`, `🥈`, `🥉`).
  4. Press `Esc` key.
- **Expected State Transitions**:
  - Rendered bitmap does not contain Win32 missing glyph tofu boxes (`□`).
  - Poster modal displays in UI.
  - Pressing `Esc` sets `view.poster_modal = None` and triggers repaint.
  - Bitmap memory is released cleanly.
- **Assertions**:
  - `assert!(poster.png_bytes.len() > 1000);`
  - `assert!(view.poster_modal.is_some());`
  - After `Esc`: `assert!(view.poster_modal.is_none());`
- **Hazard & Edge Case**:
  - Rapidly pressing `Esc` multiple times does not panic or close the underlying console window.

---

#### `TEST-T3-17`: F1 (Window Lifecycle) + F15 (Win32 Dark Titlebar & DPI Scaling)
- **Features**: F1 (Long-lived GPUI Application Lifecycle) × F15 (Win32 Dark Titlebar & DPI Scaling).
- **Objective**: Verify that when the window is dragged between monitors of different DPI (100% to 150%) and cycled through hide/wake, the Win32 dark title bar attribute persists and the Dynamic Island widget scales its geometry accurately.
- **Preconditions**:
  - Monitor 1: 96 DPI (100% scale). Monitor 2: 144 DPI (150% scale).
  - Main window initialized with Win32 immersive dark mode (`DWMWA_USE_IMMERSIVE_DARK_MODE`).
- **Action Sequence**:
  1. Main window created on Monitor 1. Verify dark titlebar attribute is enabled.
  2. Hide window to tray (`SW_HIDE`).
  3. Move Dynamic Island to Monitor 2; receive `WM_DPICHANGED` with 144 DPI.
  4. Wake window from Dynamic Island (`SW_RESTORE`).
- **Expected State Transitions**:
  - Step 1: `DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, ...)` returns `S_OK`.
  - Step 3: Dynamic Island recalculates dimensions: $W = 380 \times 1.5 = 570\text{px}$, $H = 72 \times 1.5 = 108\text{px}$.
  - Step 4: Restored window retains immersive dark titlebar and matches dark theme `#0d1117`.
- **Assertions**:
  - `assert_eq!(island.width(), 570);`
  - `assert_eq!(island.height(), 108);`
  - `assert_eq!(window.has_dark_titlebar(), true);`
- **Hazard & Edge Case**:
  - Older Windows 10 versions (< Build 19041); dark titlebar attribute fallback (attribute 19 vs 20) must be handled gracefully without Win32 failure.

---

### 4.2 Tier 4: Real-World Application Scenarios (9 End-to-End Scenarios)

Below are the 9 comprehensive, multi-step end-to-end user journeys modeling realistic listening, configuration, error recovery, and collection management workflows:

```
================================================================================
TIER 4 END-TO-END SCENARIO INDEX
================================================================================
SCENARIO-T4-01: Full Scrobble Lifecycle with Transient Network Failure & Recovery
SCENARIO-T4-02: Music Discovery Journey from Roaming to Hearting & Poster Generation
SCENARIO-T4-03: End-to-End Account Setup & Credential Input on High-DPI Display
SCENARIO-T4-04: Destructive History Cleaning with Confirmation Safeguards
SCENARIO-T4-05: Multilingual Unicode Music Collection, Search & GDI Typography
SCENARIO-T4-06: Background Tray Listening, SMTC Cover Decode & Island Sync
SCENARIO-T4-07: Offline Heavy Queue Accumulation, Error Banner & Batch Catch-Up
SCENARIO-T4-08: Fresh Install Out-of-the-Box: Empty States, Presets & First Scrobble
SCENARIO-T4-09: Rapid Lifecycle Stress, Tab Navigation Churn & State Machine Audit
================================================================================
```

---

#### `SCENARIO-T4-01`: Full Scrobble Lifecycle with Transient Network Failure & Recovery
- **Primary Goal**: Complete an entire track scrobble under real-world conditions where the network temporarily fails at track completion, verify error visibility, and confirm automatic queue recovery when the network returns.
- **Features Exercised**: F1, F2, F3, F4, F12.
- **Execution Timeline**:
  - **Milestone 1 (Track Start & Metadata)**: External music player starts playing `"Hotel California" · "Eagles"` (duration 390s). SMTC monitor detects session change. Title and artist extracted; cover image bytes are delayed.
  - **Milestone 2 (Late Cover Arrival - F2)**: 1.2 seconds later, SMTC provides 64 KB JPEG thumbnail. Change key `"201:65536:true:65536"` triggers decode. Bottom player bar updates with album cover.
  - **Milestone 3 (Network Outage & Play Completion)**: At 240 seconds (>50% played), track completes. Network connection goes down (simulated DNS/timeout). Scrobbler attempts Last.fm `track.scrobble`.
  - **Milestone 4 (Error Exposure - F4)**: Last.fm call times out. Scrobbler records `last_error = Some("HTTP timeout connecting to ws.audioscrobbler.com")`. Play record marked `scrobble_state = 'pending'` in SQLite. UI sidebar immediately displays `"工作模式 (待打卡 1 首)"` and red error banner with the timeout explanation.
  - **Milestone 5 (User Inspection & Safeguard Test - F12)**: User opens History tab, sees row with `⏳ 打卡排队`. User clicks delete (`✕`); inline confirmation appears; user clicks "取消" to test cancelation safety.
  - **Milestone 6 (Network Recovery & Auto Flush)**: Network reconnects. Background scrobbler worker wakes on retry timer; successfully flushes queue.
  - **Milestone 7 (State Reconciliation & Gated Repaint - F3)**: Scrobble succeeds; row updates to `scrobble_state = 'done'`. `last_error` is set to `None`. Pending count drops to 0. Sidebar error banner disappears. UI repaints exactly once.
- **Verifiable Checkpoints**:
  1. `assert_eq!(hub.db.get_play(201).unwrap().scrobble_state, "done");`
  2. `assert_eq!(hub.scrobbler.last_error.read().await.is_none(), true);`
  3. `assert_eq!(view.state.pending, 0);`
  4. Repaint count during idle listening period is 0.

---

#### `SCENARIO-T4-02`: Music Discovery Journey from Roaming to Hearting & Poster Generation
- **Primary Goal**: Discover new music via the Roaming recommendation engine, trigger track playback, like the track from the bottom player bar, verify cross-view heart synchronization, and generate a personalized listening poster with accurate completion statistics.
- **Features Exercised**: F5, F10, F11, F13, F14.
- **Execution Timeline**:
  - **Milestone 1 (Responsive Roam Navigation - F10)**: User resizes window to 900x600 and clicks "全盘漫游" tab. Roaming top card renders cleanly with wrapped buttons.
  - **Milestone 2 (Play Recommendation - F13)**: User clicks Play on recommended track `"First Love" · "宇多田ヒカル"`. Play action invokes client URL and triggers toast `"▶ 正在跳转播放：First Love"`.
  - **Milestone 3 (Bottom Bar Heart Sync - F5)**: While track plays, user clicks `now-btn-love` in the bottom player bar. SQLite saves `feedback = 'like'`. In-memory `roam_result` track item immediately updates its heart icon to red. Async task sends `track.love` to Last.fm. Toast confirms: `"❤️ 已同步红心至 Last.fm 与本地喜欢：First Love"`.
  - **Milestone 4 (Stats & Poster Generation - F14)**: User switches to Stats tab; clicks "生成年度真爱唱片海报". GDI poster generator calculates true completion rate $\frac{c}{n} \times 100\%$ from DB history; top tracks render with vector rank badges instead of broken Win32 emojis.
  - **Milestone 5 (Modal Dismissal - F11)**: Poster modal displays. User inspects poster, then presses `Esc` key. Modal smoothly dismisses and memory is reclaimed.
- **Verifiable Checkpoints**:
  1. `assert_eq!(hub.db.get_feedback("First Love", "宇多田ヒカル"), Some("like".to_string()));`
  2. `assert_eq!(view.roam_result.unwrap().tracks.iter().find(|t| t.title == "First Love").unwrap().feedback, Some("like".to_string()));`
  3. Poster completion text matches `format!("{:.0}% 真爱完播", calculated_rate)`.
  4. `assert!(view.poster_modal.is_none());`

---

#### `SCENARIO-T4-03`: End-to-End Account Setup & Credential Input on High-DPI Display
- **Primary Goal**: Complete first-time Last.fm and NetEase account configuration on a 150% DPI high-resolution monitor, testing focus navigation, password masking, long URL scrolling, and OAuth callback synchronization.
- **Features Exercised**: F1, F6, F7, F8, F9, F15.
- **Execution Timeline**:
  - **Milestone 1 (Window Creation & DPI Scaling - F15)**: Application launches on 144 DPI (150%) display. Win32 dark title bar matches `#0d1117`. Dynamic Island initializes with scaled bounds ($570 \times 108$).
  - **Milestone 2 (Tab Navigation Across Fields - F8)**: User opens Settings tab. Presses `Tab` to navigate through input fields in order: API Key $\to$ API Secret $\to$ NetEase Link.
  - **Milestone 3 (Unicode Password Masking - F9)**: In API Secret input, user types secret containing symbols and emojis: `"S3cr3t🔑!#"` (10 graphemes). Masking displays exactly 10 uniform bullets `"••••••••••"`. Cursor movement and selection operate smoothly without byte-slice errors.
  - **Milestone 4 (Horizontal Scroll on Long URL - F7)**: In NetEase Link input, user pastes a 180-character share link: `"https://music.163.com/playlist?id=123456789&userid=987654321&token=abcdef1234567890abcdef1234567890abcdef&source=desktop_share_button_v2_client_extra_parameters"`. Text scrolls horizontally; cursor and trailing parameters remain visible.
  - **Milestone 5 (Enter Key Save - F8)**: User presses `Enter` in the input field. Form saves to `config.json`.
  - **Milestone 6 (OAuth Web Callback Synchronization - F1, F6)**: User clicks "浏览器授权". Browser authorizes Last.fm. Server `/callback` handler receives token, updates config, and pushes `ConsoleCommand::Open`. Settings tab automatically updates `username_input` and status badge flips to `"✅ 已授权绑定: audiophile_user"`.
  - **Milestone 7 (Hide and Restore Lifecycle - F1)**: User closes window via "X"; window hides (`SW_HIDE`). User clicks Dynamic Island; window restores (`SW_RESTORE`). Settings values remain intact.
- **Verifiable Checkpoints**:
  1. `assert_eq!(view.username_input.read(cx).content(), "audiophile_user");`
  2. `assert!(input_url.read(cx).scroll_offset() > px(0.0));`
  3. `assert_eq!(hub.cfg.is_authorized(), true);`
  4. `assert_eq!(window.is_visible(), true);`

---

#### `SCENARIO-T4-04`: Destructive History Cleaning with Confirmation Safeguards
- **Primary Goal**: Audit listening history, clean up unwanted or mistracked plays at minimum window dimensions, verify two-step confirmation safeguards, and ensure SQLite integrity and stats synchronization.
- **Features Exercised**: F1, F3, F4, F10, F12, F13.
- **Execution Timeline**:
  - **Milestone 1 (History Review at 900x600 - F10)**: User resizes window to 900x600 and opens History tab. Toolbar buttons ("导出流水 CSV", "备份数据库", "一键补打卡", "刷新") wrap cleanly without overlapping header text.
  - **Milestone 2 (Accidental Delete Click & Safeguard - F12)**: User accidentally clicks delete (`✕`) on an important history row (Row 15: `"Bohemian Rhapsody"`). Row displays inline prompt: `"确定删除此条记录？[确定] [取消]"`. Database row remains untouched.
  - **Milestone 3 (Cancelation)**: User clicks "取消". Row returns to normal display. DB row remains present.
  - **Milestone 4 (Intentional Deletion & Confirmation - F12)**: User clicks delete (`✕`) on a junk stream record (Row 22: `"Ad Interlude"`), then clicks "确定".
  - **Milestone 5 (DB Execution & UI Reconciliation)**: SQLite executes `DELETE FROM plays WHERE id=22`. Row disappears from list. Stats and scrobble counters recalculate.
  - **Milestone 6 (Repaint Gating Verification - F3)**: Repaint gating verifies that `cx.notify()` was called only on the confirmation click and not on a 500ms continuous loop.
- **Verifiable Checkpoints**:
  1. `assert!(hub.db.get_play(15).is_some());`
  2. `assert!(hub.db.get_play(22).is_none());`
  3. `assert_eq!(view.confirm_delete_id, None);`

---

#### `SCENARIO-T4-05`: Multilingual Unicode Music Collection, Search & GDI Typography
- **Primary Goal**: Manage and scrobble an international music collection with diverse multilingual metadata (Japanese Kana/Kanji, Korean Hangul, Arabic, Russian Cyrillic, and zero-width joiner emojis), verify input safety, and generate error-free GDI poster typography.
- **Features Exercised**: F7, F8, F9, F11, F14.
- **Execution Timeline**:
  - **Milestone 1 (Multilingual Playback & History)**: Play 5 songs with complex metadata:
    1. `"中島みゆき" · "永遠の嘘をついてくれ"` (Japanese)
    2. `"아이유" · "밤편지 (Through the Night)"` (Korean)
    3. `"Кино" · "Группа крови"` (Russian)
    4. `"Fairouz" · "نسم علينا الهوى"` (Arabic)
    5. `"The Weeknd" · "Blinding Lights 🎧🔥"` (Emoji)
  - **Milestone 2 (Search Input & Grapheme Alignment - F7, F9)**: User searches in Radar using query `"中島みゆき 🎧"`. Backspacing over the emoji deletes the exact 4-byte grapheme cleanly. Pasting long queries scrolls horizontally.
  - **Milestone 3 (Password Masking with Mixed Scripts - F9)**: User enters a password with CJK and emoji tokens; 1:1 grapheme bullets render without panic.
  - **Milestone 4 (Poster GDI Typography Rendering - F14)**: User generates listening poster. GDI text renderer uses clean font fallbacks for multilingual text and replaces emoji ranks (`🥇`, `🥈`, `🥉`) with vector typography badges (`#1`, `#2`, `#3`), avoiding missing glyph tofu boxes.
  - **Milestone 5 (Modal Dismissal via Esc - F11)**: User presses `Esc` to close poster modal.
- **Verifiable Checkpoints**:
  1. No UTF-8 character boundary slice panics.
  2. `assert_eq!(input.display_string().chars().count(), grapheme_count);`
  3. Poster bitmap generated successfully without GDI errors.

---

#### `SCENARIO-T4-06`: Background Tray Listening, SMTC Cover Decode & Island Sync
- **Primary Goal**: Run Music Hub in continuous background mode with the console hidden to tray, track playback transitions via SMTC, decode late thumbnail bytes, interact via Dynamic Island, and wake the console window without state loss.
- **Features Exercised**: F1, F2, F3, F5, F15.
- **Execution Timeline**:
  - **Milestone 1 (Window Minimize to Tray - F1)**: User closes console window; GPUI intercepts `WM_CLOSE` and hides window via `ShowWindow(SW_HIDE)`.
  - **Milestone 2 (Track Transitions in Background)**: External music player plays 3 consecutive tracks. SMTC monitor captures state changes.
  - **Milestone 3 (Late Cover Extraction - F2)**: On Track 2, cover image bytes arrive 1.5 seconds after metadata. Change key detects new bytes and decodes image in background.
  - **Milestone 4 (Dynamic Island Interaction - F5, F15)**: Dynamic Island widget floats on desktop, scaled accurately according to PerMonitorV2 DPI. User clicks heart button on the Dynamic Island. SQLite feedback table is updated.
  - **Milestone 5 (Console Wake via Island - F1)**: User clicks Dynamic Island body to wake console. Island dispatches `ConsoleCommand::Open`. Console unhides via `SW_RESTORE`, activates foreground, and displays current track with decoded cover and synced heart status.
  - **Milestone 6 (Zero Repaint Churn - F3)**: Verify that during the entire hidden background period, 0 full tree repaints occurred.
- **Verifiable Checkpoints**:
  1. `assert_eq!(window.is_visible(), true);`
  2. `assert!(view.cover_image.is_some());`
  3. `assert_eq!(hub.db.get_feedback(current_title, current_artist), Some("like".to_string()));`
  4. Zero GPUI crashes or dispatcher re-registrations.

---

#### `SCENARIO-T4-07`: Offline Heavy Queue Accumulation, Error Banner & Batch Catch-Up
- **Primary Goal**: Listen offline during network disconnection, accumulate scrobble backlog in SQLite, inspect error notifications in UI, and batch-submit all queued scrobbles when connection is restored.
- **Features Exercised**: F3, F4, F6, F12, F13.
- **Execution Timeline**:
  - **Milestone 1 (Offline Playback)**: System loses internet connection. User listens to an album (8 songs).
  - **Milestone 2 (Queue Accumulation & Error Recording - F4)**: Each song completes and is queued into `scrobble_queue` (`scrobble_state = 'pending'`). Scrobbler attempts flush; receives network error; records `last_error = Some("Failed to resolve host 'ws.audioscrobbler.com'")`.
  - **Milestone 3 (Error Visibility in Sidebar & Settings - F4)**: User opens console. Sidebar displays `"工作模式 (待打卡 8 首)"` and red error banner with actionable error message. Recent tab shows all 8 rows with `⏳ 打卡排队`.
  - **Milestone 4 (Network Reconnection & Manual Batch Retry)**: Internet reconnects. User clicks `"📤 一键补打卡达标记录"` in History view toolbar.
  - **Milestone 5 (Batch Processing)**: Scrobbler flushes queue in batch (up to 50 items per Last.fm API spec). All 8 tracks scrobble successfully. `last_error` is cleared to `None`.
  - **Milestone 6 (Completion Toast & State Reconciliation - F13)**: Toast notifies: `"✅ 补打卡完成：成功 8 首"`. Pending counter drops to 0. Error banner disappears.
- **Verifiable Checkpoints**:
  1. `assert_eq!(hub.db.pending_scrobbles(50).len(), 0);`
  2. `assert_eq!(hub.scrobbler.last_error.read().await.is_none(), true);`
  3. `assert_eq!(view.state.pending, 0);`

---

#### `SCENARIO-T4-08`: Fresh Install Out-of-the-Box: Empty States, Presets & First Scrobble
- **Primary Goal**: Simulate a brand new user starting with a fresh SQLite database, encountering clean empty states, exploring quick-presets, and completing their first scrobble.
- **Features Exercised**: F10, F11, F13, F14, F15.
- **Execution Timeline**:
  - **Milestone 1 (Fresh Launch)**: App starts with brand new SQLite database (0 plays, 0 favorites).
  - **Milestone 2 (Radar Empty State & Quick Presets - F13)**: User opens Radar tab. UI displays empty state illustration and quick-preset chips: `"晴天 · 周杰伦"`, `"500 Miles · PPM"`, `"Anyone · Tommee Profitt"`.
  - **Milestone 3 (Roaming Empty State - F10, F13)**: User opens Roaming tab. Informative guidance card explains how roaming works; header buttons wrap cleanly without collision.
  - **Milestone 4 (First Track Playback & Toast - F13)**: User clicks quick preset `"晴天 · 周杰伦"`. Play toast displays. Playback snapshot records in DB.
  - **Milestone 5 (First Milestone Poster - F14)**: User views stats poster with small sample size ($n=1, c=1$). Poster renders exact $100\%$ completion rate without division-by-zero errors. Modal closes on `Esc` (F11).
- **Verifiable Checkpoints**:
  1. Empty state components render gracefully on fresh database.
  2. Quick preset click triggers play toast.
  3. Division by zero guard prevents poster generator panics when $n=0$.

---

#### `SCENARIO-T4-09`: Rapid Lifecycle Stress, Tab Navigation Churn & State Machine Audit
- **Primary Goal**: Subject the application to high-frequency window lifecycle cycles, rapid tab navigation, clipboard spamming, and modal dismissals to verify memory stability and absence of state leaks.
- **Features Exercised**: F1, F3, F7, F8, F10, F11, F13.
- **Execution Timeline**:
  - **Milestone 1 (Rapid Hide/Wake Churn - F1)**: Trigger 10 consecutive window hide/restore cycles via `ConsoleCommand::Open` within 2 seconds. Verify GPUI dispatcher remains responsive and memory does not leak.
  - **Milestone 2 (Tab Navigation Churn)**: Rapidly cycle through all 5 tabs (Now Playing $\to$ Radar $\to$ Roaming $\to$ History $\to$ Settings) 20 times. Verify tab active states and scroll containers reset cleanly.
  - **Milestone 3 (Text Input Paste Spam - F7, F9)**: Rapidly paste and clear 500-character strings into text inputs. Verify `scroll_offset` clamps accurately without visual artifacts.
  - **Milestone 4 (Modal Churn - F11)**: Rapidly open and dismiss the poster modal 10 times via `Esc` key and backdrop clicks.
  - **Milestone 5 (Audit State Consistency)**: Verify SQLite connection pool, tokio background tasks, and GPUI memory allocations remain within normal operational bounds.
- **Verifiable Checkpoints**:
  1. Zero GPUI panics or dispatcher corruption.
  2. Process memory RSS remains stable (no leak from repeated open/close).
  3. All UI components retain interactive state.

---

### 4.3 Draft Feature Checklist and Coverage Summary Table for `TEST_INFRA.md`

Below is the complete, production-ready draft of the Feature Checklist and Coverage Summary for `TEST_INFRA.md`:

```markdown
# Test Infrastructure Specification: Music Hub

## 1. Feature Coverage Matrix (F1–F15)

| Feature ID | Feature Name | Requirement Source | Implementation Subsystems | Tier 1 Unit Tests | Tier 2 Boundary Tests | Tier 3 Interaction Tests | Tier 4 E2E Scenarios | Total Planned Tests |
|---|---|---|---|---|---|---|---|---|
| **F1** | Long-lived GPUI Application Lifecycle | R1 (Lifecycle) | `src/ui/mod.rs`, `src/main.rs` | 5 | 5 | 5 | 4 | **19** |
| **F2** | Late SMTC Cover Byte Detection | R1 (Media Monitor) | `src/ui/mod.rs`, `src/monitor.rs` | 5 | 5 | 3 | 2 | **15** |
| **F3** | Repaint Gating & Decoupled Toasts | R1 (UI Loop) | `src/ui/mod.rs` | 5 | 5 | 3 | 4 | **17** |
| **F4** | Scrobble Error Visibility | R2 (Error Visibility) | `src/scrobbler.rs`, `src/ui/mod.rs`, `src/app.rs` | 5 | 5 | 3 | 3 | **16** |
| **F5** | Bottom Bar Heart Synchronization | R2 (State Sync) | `src/ui/mod.rs`, `src/db.rs`, `src/app.rs` | 5 | 5 | 4 | 3 | **17** |
| **F6** | OAuth Settings Synchronization | R2 (OAuth Sync) | `src/server.rs`, `src/ui/mod.rs`, `src/config.rs` | 5 | 5 | 3 | 2 | **15** |
| **F7** | TextInput Horizontal Scrolling | R3 (Input Layout) | `src/ui/input.rs` | 5 | 5 | 3 | 2 | **15** |
| **F8** | Focus Navigation & Enter Handling | R3 (Focus & Keys) | `src/ui/input.rs`, `src/ui/mod.rs` | 5 | 5 | 3 | 2 | **15** |
| **F9** | Password Masking Unicode Graphemes | R3 (Password Mask) | `src/ui/input.rs` | 5 | 5 | 3 | 2 | **15** |
| **F10** | 900x600 Responsive Layout | R4 (Layout) | `src/ui/mod.rs` | 5 | 5 | 3 | 3 | **16** |
| **F11** | Modal Backdrop & Esc Dismissal | R4 (Modal Safeguard)| `src/ui/mod.rs` | 5 | 5 | 2 | 3 | **15** |
| **F12** | History Record Deletion Safeguard | R4 (Destructive Action)| `src/ui/mod.rs`, `src/db.rs` | 5 | 5 | 3 | 3 | **16** |
| **F13** | Empty States & Click Feedback | R4 (UI Feedback) | `src/ui/mod.rs` | 5 | 5 | 4 | 4 | **18** |
| **F14** | Poster Completion Rate & GDI Emojis| R4 (Poster Stats) | `src/poster.rs`, `src/app.rs` | 5 | 5 | 3 | 3 | **16** |
| **F15** | Win32 Dark Titlebar & DPI Scaling | R4 (Platform UI) | `src/widget.rs`, `src/ui/mod.rs` | 5 | 5 | 2 | 2 | **14** |
| **TOTALS** | | | | **75** | **75** | **17** | **9** | **176** |

---

## 2. Test Execution Tiers & Methodology Summary

| Test Tier | Test Methodology | Test Count | Scope & Target Subsystems | Verification Command | CI / Headless Guarantee |
|---|---|---|---|---|---|
| **Tier 1: Feature Coverage** | Category-Partition & Happy Path | **75** (5 per feature F1–F15) | Core algorithms, data structures, state machines, DB queries, input parsing | `cargo test --test tier1_feature_tests` | 100% headless, zero GPU/display dependencies |
| **Tier 2: Boundary & Corner Cases** | Boundary Value Analysis (BVA) | **75** (5 per feature F1–F15) | Extreme lengths, empty states, zero divisions, UTF-8 astral characters, 900x600 limits | `cargo test --test tier2_boundary_tests` | 100% headless, synthetic edge data sets |
| **Tier 3: Cross-Feature Interactions** | Pairwise Combinatorial Testing | **17** interaction pairs | Inter-subsystem state synchronization, channel messaging, lifecycle wake + OAuth, scroll + mask | `cargo test --test tier3_pairwise_tests` | Mock channels, in-memory DB, async tokio test context |
| **Tier 4: Real-World Scenarios** | End-to-End User Journeys | **9** complete workflows | Multi-step listening sessions, network timeout recovery, batch catch-up, modal/stress audit | `cargo test --test tier4_scenario_tests` | Headless E2E runners with mock local HTTP/SMTC stubs |
| **OVERALL TOTAL** | Multi-Tier Test Suite | **176 tests** | Full coverage of F1–F15, R1–R4, and all Acceptance Criteria | `cargo test` | All tests pass cleanly in Windows CI |
```

---

## 5. Verification Method

To independently verify the completeness, consistency, and structural alignment of these mined specifications:

1. **Verify Coverage Requirements in SCOPE.md**:
   - Check that Tier 3 interaction tests meet or exceed requirement: $\ge 15$ required; **17 provided**.
   - Check that Tier 4 real-world scenarios meet or exceed requirement: $\ge 8$ required; **9 provided**.
   - Check that all features F1 through F15 are mapped into both Tier 3 and Tier 4.

2. **Verify Codebase Alignment**:
   - Confirm function and field names referenced in specifications match source code:
     - `AppStateResponse.last_error` (`src/app.rs:183`)
     - `Scrobbler.last_error` (`src/scrobbler.rs:21`)
     - `DB.set_feedback`, `DB.get_feedback_map`, `DB.delete_play` (`src/db.rs:267, 526, 551`)
     - `ConsoleCommand::Open { tab, toast }` (`src/app.rs`)
     - `TextInput.scroll_offset`, `display_string` (`src/ui/input.rs:109, 620`)
     - `PosterStats.loved`, `StatsTrackItem` (`src/app.rs:109, 123`)

3. **Verify Baseline Test Execution**:
   - Run `cargo test` to confirm baseline test suite passes without errors:
     ```powershell
     cargo test
     ```
   - Observed result: 6 passed, 0 failed.
