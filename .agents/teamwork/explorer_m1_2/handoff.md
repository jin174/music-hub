# Handoff Report: Feature 2 (Late SMTC Cover Byte Detection)

## 1. Observation

### 1.1 SMTC Thumbnail Extraction in `src/monitor.rs`
- **Definition of `extract_smtc_thumbnail` (`src/monitor.rs:266–282`)**:
  ```rust
  fn extract_smtc_thumbnail(
      props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
  ) -> Option<Arc<Vec<u8>>> {
      use windows::Storage::Streams::DataReader;
      let thumb_ref = props.Thumbnail().ok()?;
      let stream = thumb_ref.OpenReadAsync().ok()?.get().ok()?;
      let size = stream.Size().ok()? as u32;
      if size == 0 || size > 8 * 1024 * 1024 {
          return None;
      }
      let input = stream.GetInputStreamAt(0).ok()?;
      let reader = DataReader::CreateDataReader(&input).ok()?;
      reader.LoadAsync(size).ok()?.get().ok()?;
      let mut buf = vec![0u8; size as usize];
      reader.ReadBytes(&mut buf).ok()?;
      Some(Arc::new(buf))
  }
  ```
- **Initial Play Transition Call Site (`src/monitor.rs:320–344`)**:
  ```rust
  let needs_start = match &*guard {
      None => true,
      Some(cur) => cur.title != title || cur.artist_raw != artist,
  };

  if needs_start {
      if guard.is_some() {
          self.finalize(&mut guard, "changed").await;
      }
      let mut new_play = self.start_play(&title, &artist, &app, now, playing, None).await;
      if let Some((ref s, ref props)) = smtc_session_props {
          new_play.cover_bytes = Self::extract_smtc_thumbnail(props);
  ...
          *guard = Some(new_play);
          return;
  }
  ```
- **Subsequent Tick Polling Call Site (`src/monitor.rs:347–352`)**:
  ```rust
  if let Some(ref mut cur) = *guard {
      if cur.cover_bytes.is_none() {
          if let Some((_, ref props)) = smtc_session_props {
              cur.cover_bytes = Self::extract_smtc_thumbnail(props);
          }
      }
  ```
- **Data Model (`src/monitor.rs:69–88`)**:
  `PlaySnapshot` stores:
  ```rust
  pub play_id: i64,
  pub title: String,
  ...
  pub cover: String,
  #[serde(skip)]
  pub cover_bytes: Option<Arc<Vec<u8>>>,
  ```

### 1.2 Cover Key Construction and Comparison in `src/ui/mod.rs`
- **Polling Loop & Key Generation (`src/ui/mod.rs:4035–4071`)**:
  ```rust
  let prev_key = last_cover_key.clone();
  let (st_tx, st_rx) = tokio::sync::oneshot::channel();
  rt.spawn(async move {
      let st = hub_c.state().await;
      let cur_cover = st
          .now
          .as_ref()
          .map(|n| format!("{}:{}", n.play_id, n.cover.len()))
          .unwrap_or_default();
      let new_img = if cur_cover != prev_key {
          if let Some(bytes) = st.now.as_ref().and_then(|n| n.cover_bytes.as_ref()) {
              decode_bytes_to_render_image(bytes)
          } else if let Some(cov) = st.now.as_ref().map(|n| n.cover.as_str()) {
              fetch_cover_render_image(cov).await
          } else {
              None
          }
      } else {
          None
      };
      let _ = st_tx.send((st, cur_cover, new_img));
  });

  let Ok((new_state, cur_cover, new_img)) = st_rx.await else {
      break;
  };
  let cover_changed = cur_cover != last_cover_key;
  if cover_changed {
      last_cover_key = cur_cover.clone();
  }

  let update_res = window_handle.update(cx, |view, window, cx| {
      view.state = new_state;
      if cover_changed {
          view.now_cover_key = cur_cover;
          view.now_cover_img = new_img;
      }
  ```

### 1.3 Image Decoding Implementation in `src/ui/mod.rs`
- **`decode_bytes_to_render_image` (`src/ui/mod.rs:100–109`)**:
  ```rust
  fn decode_bytes_to_render_image(bytes: &[u8]) -> Option<Arc<RenderImage>> {
      let mut rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
      for pixel in rgba.chunks_exact_mut(4) {
          pixel.swap(0, 2);
      }
      Some(Arc::new(RenderImage::new(SmallVec::from_elem(
          image::Frame::new(rgba),
          1,
      ))))
  }
  ```
- **`fetch_cover_render_image` (`src/ui/mod.rs:111–133`)**:
  ```rust
  async fn fetch_cover_render_image(cover_str: &str) -> Option<Arc<RenderImage>> {
      if cover_str.is_empty() {
          return None;
      }
      if let Some(idx) = cover_str.find("base64,") {
          let b64 = &cover_str[idx + 7..];
          let bytes = base64::engine::general_purpose::STANDARD
              .decode(b64.trim())
              .ok()?;
          return decode_bytes_to_render_image(&bytes);
      }
      if cover_str.starts_with("http://") || cover_str.starts_with("https://") {
          let url = if cover_str.contains("music.126.net") && !cover_str.contains("?param=") {
              format!("{}?param=140y140", cover_str)
          } else {
              cover_str.to_string()
          };
          let resp = reqwest::get(&url).await.ok()?;
          let bytes = resp.bytes().await.ok()?;
          return decode_bytes_to_render_image(&bytes);
      }
      None
  }
  ```

### 1.4 Dynamic Island Contrast in `src/widget.rs:536–576`
In contrast to `src/ui/mod.rs`, the Win32 Dynamic Island widget explicitly handles `n.cover_bytes`:
```rust
let cover_key = match now {
    Some(n) if !n.cover.is_empty() => format!("url:{}", n.cover),
    Some(n) if n.cover_bytes.is_some() => format!("smtc:{}", n.play_id),
    _ => String::new(),
};
```
When `n.cover_bytes` transitions from `None` to `Some(...)`, `cover_key` changes from `""` to `"smtc:{play_id}"`, which triggers decoding via `decode_cover_to_pixmap`.

---

## 2. Logic Chain

### 2.1 Why SMTC Cover Bytes Arrive on Subsequent Ticks (Question 1)
1. **Decoupled WinRT Property Delivery**: External media applications (Spotify, NetEase Cloud Music, QQ Music, Apple Music, web browsers) publish SMTC updates asynchronously. Metadata strings (`Title`, `Artist`) are trivial and dispatched instantaneously when track playback begins.
2. **Artwork Buffering Latency**: Thumbnail artwork involves disk reading of local ID3 APIC / Vorbis picture frames or network image downloads. When `s.TryGetMediaPropertiesAsync()` first returns at track start (`needs_start == true`), `props.Thumbnail()` is frequently uninitialized (`None`) or returns a zero-byte stream (`size == 0`).
3. **Explicit Tick Retry**: In `src/monitor.rs:348–352`, the monitor worker loop executes every 1 second (`TICK_S = 1.0`). If `cur.cover_bytes.is_none()`, it re-attempts `Self::extract_smtc_thumbnail(props)`. On tick 1 or 2 (1–2 seconds into playback), WinRT has populated the thumbnail stream, and `cur.cover_bytes` is successfully updated to `Some(Arc<Vec<u8>>)`.

### 2.2 Why Current Key Comparison Fails (Question 2)
1. At track onset (Tick 0), `n.play_id` is assigned (e.g., `42`), `n.cover` is `""` (length `0`), and `n.cover_bytes` is `None`.
2. Current code in `src/ui/mod.rs:4042` computes `cur_cover = format!("{}:{}", n.play_id, n.cover.len())`, resulting in `"42:0"`.
3. `prev_key` transitions to `"42:0"`. Since `n.cover_bytes` is `None` and `n.cover` is `""`, `new_img` is `None`, and `view.now_cover_img = None`.
4. At Tick 1 (~1 second later), SMTC thumbnail arrives: `n.cover_bytes` becomes `Some(65536 bytes)`. `n.play_id` remains `42`, and `n.cover` remains `""` (NCM lookup still pending, failed, or unmatched).
5. At `src/ui/mod.rs:4042`, `cur_cover` is computed again:
   `format!("{}:{}", 42, 0)` -> `"42:0"`.
6. Comparison `cur_cover != prev_key` evaluates `"42:0" != "42:0"` -> **`false`**!
7. Consequently:
   - `new_img` branches to `else { None }`.
   - `cover_changed` evaluates to `false`.
   - `view.now_cover_img` is never assigned the decoded image.
   - The cover displays the fallback note "🎵" indefinitely.

### 2.3 Exact Proposed Key Formula (Question 3)
To ensure the comparison diverges whenever either the URL string or the SMTC binary thumbnail changes, the key must incorporate `n.cover_bytes` length:
```rust
let cur_cover = st
    .now
    .as_ref()
    .map(|n| {
        let b_len = n.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0);
        format!("{}:{}:{}", n.play_id, n.cover, b_len)
    })
    .unwrap_or_default();
```
Or encapsulated as a method on `PlaySnapshot`:
```rust
impl PlaySnapshot {
    pub fn cover_key(&self) -> String {
        let b_len = self.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0);
        format!("{}:{}:{}", self.play_id, self.cover, b_len)
    }
}
```
**State Transition Verification**:
- Tick 0: `play_id = 42, cover = "", cover_bytes = None` -> `"42::0"`
- Tick 1: `play_id = 42, cover = "", cover_bytes = Some(65536 bytes)` -> `"42::65536"`
- `"42::65536" != "42::0"` evaluates to **`true`**! Divergence triggers decode immediately.
- Tick 2: `play_id = 42, cover = "", cover_bytes = Some(65536 bytes)` -> `"42::65536" == "42::65536"` -> **`false`** (no redundant decode).
- Tick 3 (NCM matched): `cover = "https://p1..."` -> `"42:https://p1...:65536"` -> evaluates to **`true`**.
- Track end: `st.now = None` -> `""` -> evaluates to **`true`**, clearing `now_cover_img`.

### 2.4 Decoding Priority & Execution Model (Question 4)
1. **Priority Hierarchy**:
   - `n.cover_bytes` via `decode_bytes_to_render_image` takes **Priority 1**: Embedded local album art directly reflects what the playing music player is rendering, with 0 network latency.
   - `n.cover` via `fetch_cover_render_image` takes **Priority 2**: Fallback when media player publishes no thumbnail bytes (or fallback window scrapers like `QQMusic_Daemon_Wnd`), but NCM online match resolves a URL.
2. **Defensive Fallback**:
   If `decode_bytes_to_render_image` returns `None` (corrupted stream/unsupported codec from SMTC), the code should gracefully fallback to `fetch_cover_render_image(cov)`.
3. **Threading Architecture**:
   - Both `decode_bytes_to_render_image` (CPU-bound JPEG/PNG decompression and RGBA/BGRA byte swap) and `fetch_cover_render_image` (async HTTP GET) must execute inside `rt.spawn(async move { ... })` on Tokio worker threads.
   - Only the resulting `Arc<RenderImage>` is sent through `st_tx` to GPUI's thread, preventing frame jitter or UI latency.

### 2.5 Unit Test Design (Question 5)
Because `Cargo.toml` specifies a binary target `[[bin]] name = "MusicHub"` without a `[lib]`, unit tests must reside within `src/*.rs` under `#[cfg(test)] mod tests`.
Adding `pub fn cover_key(&self) -> String` to `PlaySnapshot` allows testing key divergence entirely in `src/monitor.rs` without requiring GPUI window handles or WinRT hardware hooks.

---

## 3. Caveats
1. **Hash vs Length**: Using `b_len` (`n.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0)`) is $O(1)$ and avoids hashing tens or hundreds of kilobytes of image data on every 500ms tick. In SMTC sessions, thumbnail bytes for a single `play_id` transition from `None` (0 bytes) to `Some(N)` once, making length divergence 100% reliable.
2. **Serde Serialization**: In `PlaySnapshot`, `cover_bytes` has `#[serde(skip)]`. IPC JSON responses do not transmit raw bytes (preserving IPC performance), but within the local process `hub.state().await` clones the `Arc<Vec<u8>>` reference directly, so `st.now.cover_bytes` is fully available to `src/ui/mod.rs`.

---

## 4. Conclusion
1. **Root Cause**: The current key formula `format!("{}:{}", n.play_id, n.cover.len())` in `src/ui/mod.rs:4042` completely ignores `cover_bytes`. When WinRT thumbnail streams load after track start (the standard behavior for Spotify, NetEase, and QQ Music), `cur_cover != prev_key` evaluates to `false`, silently dropping late-arriving thumbnails.
2. **Solution**:
   - Implement `PlaySnapshot::cover_key(&self) -> String` in `src/monitor.rs` returning `format!("{}:{}:{}", self.play_id, self.cover, self.cover_bytes.as_ref().map(|b| b.len()).unwrap_or(0))`.
   - Update `src/ui/mod.rs:4039` to invoke `n.cover_key()`.
   - Ensure `new_img` decodes `cover_bytes` on Tokio background pool with fallback to `fetch_cover_render_image`.
   - Add unit test `test_cover_key_late_bytes_divergence` in `src/monitor.rs`.

---

## 5. Verification Method

### 5.1 Automated Unit Test Verification
Implement the following unit test in `src/monitor.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_cover_key_late_bytes_divergence() {
        let mut snapshot = PlaySnapshot {
            play_id: 101,
            title: "Sample Song".to_string(),
            artist: "Artist".to_string(),
            artists: vec!["Artist".to_string()],
            album: "Album".to_string(),
            cover: String::new(),
            cover_bytes: None,
            ncm_id: None,
            duration_ms: 180_000,
            played_ms: 0,
            threshold_ms: Some(90_000),
            threshold_reached: false,
            scrobble_state: "none".to_string(),
            playing: true,
            lookup: "pending".to_string(),
            started_at: 1000,
            app: "spotify.exe".to_string(),
        };

        // Tick 0: Initial track start (no thumbnail bytes)
        let key_0 = snapshot.cover_key();
        assert_eq!(key_0, "101::0");

        // Tick 1: SMTC thumbnail arrives
        snapshot.cover_bytes = Some(Arc::new(vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4]));
        let key_1 = snapshot.cover_key();
        assert_eq!(key_1, "101::8");
        assert_ne!(key_0, key_1, "Late SMTC bytes MUST trigger key divergence!");

        // Tick 2: Stable tick (identical key, prevents redundant decode)
        let key_2 = snapshot.cover_key();
        assert_eq!(key_1, key_2);

        // Tick 3: NCM online URL arrives
        snapshot.cover = "https://p1.music.126.net/xyz.jpg".to_string();
        let key_3 = snapshot.cover_key();
        assert_eq!(key_3, "101:https://p1.music.126.net/xyz.jpg:8");
        assert_ne!(key_1, key_3);
    }
}
```

### 5.2 Verification Commands
- `cargo test monitor::tests::test_cover_key_late_bytes_divergence`
- `cargo check`

### 5.3 Invalidation Conditions
- If `key_0 == key_1`, the fix has failed and late-arriving SMTC covers will not decode.
- If `key_1 != key_2`, spurious decodes will execute every tick, causing unnecessary CPU load.
