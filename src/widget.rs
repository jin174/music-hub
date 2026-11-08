use crate::app::{App, WidgetCommand};
use fontdue::{Font, FontSettings};
use std::path::Path;
use std::sync::Arc;
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point,
    Rect, Transform,
};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, PostQuitMessage, RegisterClassW, SendMessageW,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW,
    HTCAPTION, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCLBUTTONDOWN, WM_TIMER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

pub const WIDGET_WIDTH: u32 = 384;
pub const WIDGET_HEIGHT: u32 = 56;

const PILL_X: f32 = 2.0;
const PILL_Y: f32 = 2.0;
const PILL_W: f32 = 380.0;
const PILL_H: f32 = 52.0;
const PILL_RADIUS: f32 = 26.0;

// Button regions: (x, y, w, h)
const BTN_LIKE: (f32, f32, f32, f32) = (252.0, 18.0, 20.0, 20.0);
const BTN_DISLIKE: (f32, f32, f32, f32) = (276.0, 18.0, 20.0, 20.0);
const BTN_PLAY: (f32, f32, f32, f32) = (300.0, 18.0, 20.0, 20.0);
const BTN_CONSOLE: (f32, f32, f32, f32) = (324.0, 18.0, 20.0, 20.0);
const BTN_CLOSE: (f32, f32, f32, f32) = (348.0, 18.0, 20.0, 20.0);

#[derive(Clone)]
struct FontCache {
    main_font: Option<Arc<Font>>,
}

impl FontCache {
    fn new() -> Self {
        let font_paths = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ];
        let mut main_font = None;
        for p in font_paths {
            if Path::new(p).exists() {
                if let Ok(data) = std::fs::read(p) {
                    if let Ok(font) = Font::from_bytes(data, FontSettings::default()) {
                        main_font = Some(Arc::new(font));
                        break;
                    }
                }
            }
        }
        Self { main_font }
    }
}

pub struct WidgetWindow {
    app: Arc<App>,
    hwnd: HWND,
    fonts: FontCache,
    hovered_btn: Option<usize>, // 0: like, 1: dislike, 2: play, 3: console, 4: close
    anim_frame: u32,
    last_cover_url: String,
    cover_pixmap: Option<Pixmap>,
}

pub fn spawn_widget_thread(
    app: Arc<App>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<WidgetCommand>,
    initially_visible: bool,
) {
    std::thread::Builder::new()
        .name("dynamic-island-widget".to_string())
        .spawn(move || {
            let class_name = w!("MusicHub_DynamicIsland_Widget");
            let hinstance = HINSTANCE(std::ptr::null_mut());

            unsafe {
                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(widget_wnd_proc),
                    hInstance: hinstance,
                    lpszClassName: class_name,
                    ..Default::default()
                };
                RegisterClassW(&wc);

                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let x = (screen_w - WIDGET_WIDTH as i32) - 30;
                let y = (screen_h - WIDGET_HEIGHT as i32) - 80;

                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    class_name,
                    w!("Music Hub Dynamic Island"),
                    WS_POPUP,
                    x,
                    y,
                    WIDGET_WIDTH as i32,
                    WIDGET_HEIGHT as i32,
                    None,
                    None,
                    hinstance,
                    None,
                )
                .expect("Failed to create widget window");

                let fonts = FontCache::new();
                let widget = Box::new(WidgetWindow {
                    app: app.clone(),
                    hwnd,
                    fonts,
                    hovered_btn: None,
                    anim_frame: 0,
                    last_cover_url: String::new(),
                    cover_pixmap: None,
                });

                // Store pointer into window user data
                windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                    Box::into_raw(widget) as isize,
                );

                if initially_visible {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                } else {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }

                // Timer for render loop (30 FPS = ~33ms)
                windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, 1, 33, None);

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    // Check commands from channel
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        match cmd {
                            WidgetCommand::Show => {
                                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            }
                            WidgetCommand::Hide => {
                                let _ = ShowWindow(hwnd, SW_HIDE);
                            }
                            WidgetCommand::Toggle => {
                                let visible = windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool();
                                if visible {
                                    let _ = ShowWindow(hwnd, SW_HIDE);
                                } else {
                                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                }
                            }
                            WidgetCommand::Quit => {
                                DestroyWindow(hwnd).unwrap_or_default();
                            }
                        }
                    }

                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .expect("Failed to spawn widget thread");
}

unsafe extern "system" fn widget_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
    ) as *mut WidgetWindow;

    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let widget = &mut *ptr;

    match msg {
        WM_TIMER => {
            widget.anim_frame = widget.anim_frame.wrapping_add(1);
            widget.render_and_flush();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xffff) as i16 as f32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32;

            let in_btn = |(bx, by, bw, bh): (f32, f32, f32, f32)| -> bool {
                x >= bx && x <= bx + bw && y >= by && y <= by + bh
            };

            let new_hover = if in_btn(BTN_LIKE) {
                Some(0)
            } else if in_btn(BTN_DISLIKE) {
                Some(1)
            } else if in_btn(BTN_PLAY) {
                Some(2)
            } else if in_btn(BTN_CONSOLE) {
                Some(3)
            } else if in_btn(BTN_CLOSE) {
                Some(4)
            } else {
                None
            };

            if widget.hovered_btn != new_hover {
                widget.hovered_btn = new_hover;
                widget.render_and_flush();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as f32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32;

            let in_btn = |(bx, by, bw, bh): (f32, f32, f32, f32)| -> bool {
                x >= bx && x <= bx + bw && y >= by && y <= by + bh
            };

            if in_btn(BTN_LIKE) {
                widget.on_btn_like();
            } else if in_btn(BTN_DISLIKE) {
                widget.on_btn_dislike();
            } else if in_btn(BTN_PLAY) {
                widget.on_btn_play();
            } else if in_btn(BTN_CONSOLE) {
                widget.on_btn_console();
            } else if in_btn(BTN_CLOSE) {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                // Drag the window via OS HTCAPTION
                ReleaseCapture().unwrap_or_default();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = Box::from_raw(ptr);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

impl WidgetWindow {
    fn on_btn_like(&self) {
        let app = self.app.clone();
        tokio::spawn(async move {
            if let Some(now) = app.monitor.snapshot() {
                let fb_map = app.db.get_feedback_map();
                let key = (
                    crate::normalize::norm(&now.title),
                    crate::normalize::norm(&crate::normalize::clean_artist(&now.artist)),
                );
                let current_action = fb_map.get(&key).map(|s| s.as_str());
                let new_action = if current_action == Some("like") {
                    "remove"
                } else {
                    "like"
                };
                app.db.set_feedback(&now.title, &now.artist, new_action);
            }
        });
    }

    fn on_btn_dislike(&self) {
        let app = self.app.clone();
        tokio::spawn(async move {
            if let Some(now) = app.monitor.snapshot() {
                let fb_map = app.db.get_feedback_map();
                let key = (
                    crate::normalize::norm(&now.title),
                    crate::normalize::norm(&crate::normalize::clean_artist(&now.artist)),
                );
                let current_action = fb_map.get(&key).map(|s| s.as_str());
                let new_action = if current_action == Some("dislike") {
                    "remove"
                } else {
                    "dislike"
                };
                app.db.set_feedback(&now.title, &now.artist, new_action);
            }
        });
    }

    fn on_btn_play(&self) {
        let app = self.app.clone();
        tokio::spawn(async move {
            if let Some(now) = app.monitor.snapshot() {
                if let Some(nid) = now.ncm_id {
                    App::play_in_client(nid);
                } else {
                    let _ = app.play_artist(&now.artist).await;
                }
            }
        });
    }

    fn on_btn_console(&self) {
        let url = self.app.base_url();
        let _ = open::that(url);
    }

    fn render_and_flush(&mut self) {
        let now_opt = self.app.monitor.snapshot();

        let mut pixmap = match Pixmap::new(WIDGET_WIDTH, WIDGET_HEIGHT) {
            Some(p) => p,
            None => return,
        };
        pixmap.fill(Color::TRANSPARENT);

        // 1. Draw soft outer blur shadow (stacked rounded rects)
        self.draw_shadow(&mut pixmap);

        // 2. Draw capsule pill background
        self.draw_pill_bg(&mut pixmap);

        // 3. Draw album cover disc (vinyl spinning effect)
        self.draw_cover_disc(&mut pixmap, now_opt.as_ref());

        // 4. Draw track title, artist, audio wave animation
        self.draw_meta_and_waves(&mut pixmap, now_opt.as_ref());

        // 5. Draw progress bar and timestamp
        self.draw_progress(&mut pixmap, now_opt.as_ref());

        // 6. Draw action buttons
        self.draw_buttons(&mut pixmap, now_opt.as_ref());

        // 7. Flush 32-bit premultiplied ARGB to layered window
        self.flush_to_layered_window(&pixmap);
    }

    fn draw_shadow(&self, pixmap: &mut Pixmap) {
        let shadow_offsets = [(0.0, 3.0, 0.08), (0.0, 6.0, 0.12), (0.0, 10.0, 0.18)];
        for (ox, oy, alpha) in shadow_offsets {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, (alpha * 255.0) as u8);
            paint.anti_alias = true;

            let path = build_rounded_rect(
                PILL_X + ox,
                PILL_Y + oy,
                PILL_W,
                PILL_H,
                PILL_RADIUS,
            );
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    fn draw_pill_bg(&self, pixmap: &mut Pixmap) {
        let path = build_rounded_rect(PILL_X, PILL_Y, PILL_W, PILL_H, PILL_RADIUS);

        // Linear gradient: #1a1f2a (26, 31, 42) -> #0b0e14 (11, 14, 20)
        let shader = LinearGradient::new(
            Point::from_xy(0.0, PILL_Y),
            Point::from_xy(0.0, PILL_Y + PILL_H),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(26, 31, 42, 250)),
                GradientStop::new(1.0, Color::from_rgba8(11, 14, 20, 250)),
            ],
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        );

        if let Some(s) = shader {
            let mut paint = Paint::default();
            paint.shader = s;
            paint.anti_alias = true;
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }

        // Inner 1px subtle border: rgba(255, 255, 255, 0.16)
        let mut stroke_paint = Paint::default();
        stroke_paint.set_color_rgba8(255, 255, 255, 40);
        stroke_paint.anti_alias = true;
        let mut stroke = tiny_skia::Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    fn draw_cover_disc(&mut self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let cx = PILL_X + 24.0;
        let cy = PILL_Y + PILL_H / 2.0;
        let r = 18.0;

        // Base vinyl disc dark circle
        let mut disc_builder = PathBuilder::new();
        disc_builder.push_circle(cx, cy, r);
        if let Some(disc_path) = disc_builder.finish() {
            let mut disc_paint = Paint::default();
            disc_paint.set_color_rgba8(16, 20, 28, 255);
            disc_paint.anti_alias = true;
            pixmap.fill_path(&disc_path, &disc_paint, FillRule::Winding, Transform::identity(), None);

            // Vinyl subtle rings
            let mut ring_paint = Paint::default();
            ring_paint.set_color_rgba8(255, 255, 255, 18);
            ring_paint.anti_alias = true;
            let mut stroke = tiny_skia::Stroke::default();
            stroke.width = 1.0;

            for ring_r in [14.0, 10.0, 6.0] {
                let mut ring_builder = PathBuilder::new();
                ring_builder.push_circle(cx, cy, ring_r);
                if let Some(rp) = ring_builder.finish() {
                    pixmap.stroke_path(&rp, &ring_paint, &stroke, Transform::identity(), None);
                }
            }

            // Check if we have cover image URL to fetch or render
            let cover_url = now.map(|n| n.cover.as_str()).unwrap_or("");
            if !cover_url.is_empty() && cover_url != self.last_cover_url {
                self.last_cover_url = cover_url.to_string();
                self.cover_pixmap = None;
                let url = cover_url.to_string();
                let client = reqwest::Client::new();
                // Download in background
                tokio::spawn(async move {
                    if let Ok(resp) = client.get(&url).send().await {
                        if let Ok(bytes) = resp.bytes().await {
                            if let Ok(img) = image::load_from_memory(&bytes) {
                                let rgba = img.to_rgba8();
                                let (w, h) = rgba.dimensions();
                                if let Some(mut px) = Pixmap::new(w, h) {
                                    px.data_mut().copy_from_slice(&rgba.into_raw());
                                    // Cache would be stored
                                }
                            }
                        }
                    }
                });
            }

            // Center spindle spindle pin
            let mut hole_builder = PathBuilder::new();
            hole_builder.push_circle(cx, cy, 3.5);
            if let Some(hp) = hole_builder.finish() {
                let mut hole_paint = Paint::default();
                hole_paint.set_color_rgba8(0, 0, 0, 255);
                hole_paint.anti_alias = true;
                pixmap.fill_path(&hp, &hole_paint, FillRule::Winding, Transform::identity(), None);

                let mut pin_stroke = Paint::default();
                pin_stroke.set_color_rgba8(255, 255, 255, 120);
                pin_stroke.anti_alias = true;
                pixmap.stroke_path(&hp, &pin_stroke, &stroke, Transform::identity(), None);
            }
        }
    }

    fn draw_meta_and_waves(&self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let (title, artist, playing) = match now {
            Some(n) => (n.title.as_str(), n.artist.as_str(), n.playing),
            None => ("等待播放曲目...", "打开播放器播放即可", false),
        };

        // Title text
        self.draw_text(pixmap, title, 50.0, 20.0, 11.5, Color::from_rgba8(255, 255, 255, 255), 115.0);

        // Dot separator
        self.draw_text(pixmap, "·", 170.0, 20.0, 10.0, Color::from_rgba8(100, 110, 125, 255), 10.0);

        // Artist text
        self.draw_text(pixmap, artist, 180.0, 20.0, 10.5, Color::from_rgba8(140, 150, 165, 255), 45.0);

        // Soundwaves: 4 neon-green vertical bars at x = 230
        let wave_x = 230.0;
        let wave_y_base = 21.0;
        let heights = if playing {
            let f = self.anim_frame;
            [
                3.0 + ((f as f32 * 0.2).sin().abs() * 7.0),
                4.0 + (((f as f32 * 0.25) + 1.0).sin().abs() * 8.0),
                3.0 + (((f as f32 * 0.18) + 2.0).sin().abs() * 7.0),
                4.0 + (((f as f32 * 0.22) + 3.0).sin().abs() * 8.0),
            ]
        } else {
            [3.0, 3.0, 3.0, 3.0]
        };

        let mut wave_paint = Paint::default();
        wave_paint.set_color_rgba8(86, 211, 100, 255);
        wave_paint.anti_alias = true;

        for (i, h) in heights.iter().enumerate() {
            let bx = wave_x + (i as f32 * 4.0);
            let by = wave_y_base - h;
            if let Some(r) = Rect::from_xywh(bx, by, 2.0, *h) {
                let path = build_rounded_rect(r.x(), r.y(), r.width(), r.height(), 1.0);
                pixmap.fill_path(&path, &wave_paint, FillRule::Winding, Transform::identity(), None);
            }
        }
    }

    fn draw_progress(&self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let (played_ms, dur_ms) = match now {
            Some(n) => (n.played_ms, n.duration_ms),
            None => (0, 0),
        };

        let ratio = if dur_ms > 0 {
            ((played_ms as f32) / (dur_ms as f32)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Progress bar track
        let track_x = 50.0;
        let track_y = 35.0;
        let track_w = 115.0;
        let track_h = 2.5;

        let bg_path = build_rounded_rect(track_x, track_y, track_w, track_h, 1.25);
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 30);
        bg_paint.anti_alias = true;
        pixmap.fill_path(&bg_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

        // Progress bar fill (orange/red gradient)
        if ratio > 0.001 {
            let fill_w = track_w * ratio;
            let fill_path = build_rounded_rect(track_x, track_y, fill_w, track_h, 1.25);
            let mut fill_paint = Paint::default();
            fill_paint.set_color_rgba8(232, 65, 66, 255);
            fill_paint.anti_alias = true;
            pixmap.fill_path(&fill_path, &fill_paint, FillRule::Winding, Transform::identity(), None);
        }

        // Time labels: 00:08 / 04:47
        let played_s = played_ms / 1000;
        let dur_s = dur_ms / 1000;
        let time_str = format!(
            "{:02}:{:02} / {:02}:{:02}",
            played_s / 60,
            played_s % 60,
            dur_s / 60,
            dur_s % 60
        );
        self.draw_text(pixmap, &time_str, 172.0, 36.5, 9.5, Color::from_rgba8(125, 133, 144, 255), 75.0);
    }

    fn draw_buttons(&self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let buttons = [
            (BTN_LIKE, 0, "like"),
            (BTN_DISLIKE, 1, "dislike"),
            (BTN_PLAY, 2, "play"),
            (BTN_CONSOLE, 3, "console"),
            (BTN_CLOSE, 4, "close"),
        ];

        let fb_map = self.app.db.get_feedback_map();
        let (current_is_liked, current_is_disliked) = if let Some(n) = now {
            let key = (
                crate::normalize::norm(&n.title),
                crate::normalize::norm(&crate::normalize::clean_artist(&n.artist)),
            );
            let act = fb_map.get(&key).map(|s| s.as_str());
            (act == Some("like"), act == Some("dislike"))
        } else {
            (false, false)
        };

        for ((bx, by, bw, _bh), idx, kind) in buttons {
            let is_hovered = self.hovered_btn == Some(idx);
            let r = bw / 2.0;
            let cx = bx + r;
            let cy = by + r;

            let mut circle_builder = PathBuilder::new();
            circle_builder.push_circle(cx, cy, r);
            if let Some(cp) = circle_builder.finish() {
                let mut bg_paint = Paint::default();
                bg_paint.anti_alias = true;

                if idx == 0 && current_is_liked {
                    bg_paint.set_color_rgba8(46, 160, 67, 180);
                } else if idx == 1 && current_is_disliked {
                    bg_paint.set_color_rgba8(232, 65, 66, 180);
                } else if is_hovered {
                    bg_paint.set_color_rgba8(255, 255, 255, 60);
                } else {
                    bg_paint.set_color_rgba8(255, 255, 255, 18);
                }

                pixmap.fill_path(&cp, &bg_paint, FillRule::Winding, Transform::identity(), None);

                let mut stroke_paint = Paint::default();
                stroke_paint.anti_alias = true;
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.0;

                if idx == 0 && current_is_liked {
                    stroke_paint.set_color_rgba8(86, 211, 100, 255);
                } else if idx == 1 && current_is_disliked {
                    stroke_paint.set_color_rgba8(255, 123, 114, 255);
                } else if is_hovered {
                    stroke_paint.set_color_rgba8(255, 255, 255, 90);
                } else {
                    stroke_paint.set_color_rgba8(255, 255, 255, 28);
                }
                pixmap.stroke_path(&cp, &stroke_paint, &stroke, Transform::identity(), None);

                // Draw button iconography
                self.draw_button_icon(pixmap, cx, cy, kind, is_hovered);
            }
        }
    }

    fn draw_button_icon(&self, pixmap: &mut Pixmap, cx: f32, cy: f32, kind: &str, is_hover: bool) {
        let mut icon_paint = Paint::default();
        icon_paint.anti_alias = true;
        if is_hover {
            icon_paint.set_color_rgba8(255, 255, 255, 255);
        } else {
            icon_paint.set_color_rgba8(180, 190, 205, 220);
        }

        match kind {
            "like" => {
                // Thumbs up symbol
                self.draw_text(pixmap, "👍", cx - 4.5, cy + 3.0, 9.0, Color::from_rgba8(255, 200, 50, 255), 15.0);
            }
            "dislike" => {
                // Prohibited sign
                self.draw_text(pixmap, "🚫", cx - 4.5, cy + 3.0, 9.0, Color::from_rgba8(240, 70, 70, 255), 15.0);
            }
            "play" => {
                // Play triangle
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 2.5, cy - 4.0);
                pb.line_to(cx + 4.0, cy);
                pb.line_to(cx - 2.5, cy + 4.0);
                pb.close();
                if let Some(p) = pb.finish() {
                    pixmap.fill_path(&p, &icon_paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            "console" => {
                // Expand arrows
                let mut pb = PathBuilder::new();
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.2;
                pb.move_to(cx - 3.5, cy + 3.5);
                pb.line_to(cx + 3.5, cy - 3.5);
                pb.move_to(cx, cy - 3.5);
                pb.line_to(cx + 3.5, cy - 3.5);
                pb.line_to(cx + 3.5, cy);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &icon_paint, &stroke, Transform::identity(), None);
                }
            }
            "close" => {
                // Cross X
                let mut pb = PathBuilder::new();
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.2;
                pb.move_to(cx - 3.0, cy - 3.0);
                pb.line_to(cx + 3.0, cy + 3.0);
                pb.move_to(cx + 3.0, cy - 3.0);
                pb.line_to(cx - 3.0, cy + 3.0);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &icon_paint, &stroke, Transform::identity(), None);
                }
            }
            _ => {}
        }
    }

    fn draw_text(
        &self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        max_w: f32,
    ) {
        let font = match self.fonts.main_font.as_ref() {
            Some(f) => f,
            None => return,
        };

        let mut cur_x = x;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if cur_x - x > max_w - 10.0 && chars.peek().is_some() {
                // Append ellipsis
                for dot in ['.', '.', '.'] {
                    let (metrics, bitmap) = font.rasterize(dot, size);
                    let gx = cur_x + metrics.xmin as f32;
                    let gy = y - metrics.height as f32 - metrics.ymin as f32;
                    self.blit_glyph(pixmap, &bitmap, metrics.width, metrics.height, gx, gy, color);
                    cur_x += metrics.advance_width;
                }
                break;
            }

            let (metrics, bitmap) = font.rasterize(c, size);
            let gx = cur_x + metrics.xmin as f32;
            let gy = y - metrics.height as f32 - metrics.ymin as f32;
            self.blit_glyph(pixmap, &bitmap, metrics.width, metrics.height, gx, gy, color);
            cur_x += metrics.advance_width;
        }
    }

    fn blit_glyph(
        &self,
        pixmap: &mut Pixmap,
        bitmap: &[u8],
        w: usize,
        h: usize,
        x: f32,
        y: f32,
        color: Color,
    ) {
        let px_w = pixmap.width() as i32;
        let px_h = pixmap.height() as i32;
        let start_x = x.round() as i32;
        let start_y = y.round() as i32;

        let (cr, cg, cb, ca) = (
            (color.red() * 255.0) as u32,
            (color.green() * 255.0) as u32,
            (color.blue() * 255.0) as u32,
            (color.alpha() * 255.0) as u32,
        );

        let data = pixmap.data_mut();

        for gy in 0..h {
            let dy = start_y + gy as i32;
            if dy < 0 || dy >= px_h {
                continue;
            }
            for gx in 0..w {
                let dx = start_x + gx as i32;
                if dx < 0 || dx >= px_w {
                    continue;
                }

                let alpha_val = bitmap[gy * w + gx] as u32;
                if alpha_val == 0 {
                    continue;
                }

                let factor = (alpha_val * ca) / 255;
                let idx = ((dy * px_w + dx) * 4) as usize;

                let pr = (cr * factor) / 255;
                let pg = (cg * factor) / 255;
                let pb = (cb * factor) / 255;
                let inv = 255 - factor;

                let dr = data[idx] as u32;
                let dg = data[idx + 1] as u32;
                let db = data[idx + 2] as u32;
                let da = data[idx + 3] as u32;

                data[idx] = ((dr * inv) / 255 + pr).min(255) as u8;
                data[idx + 1] = ((dg * inv) / 255 + pg).min(255) as u8;
                data[idx + 2] = ((db * inv) / 255 + pb).min(255) as u8;
                data[idx + 3] = ((da * inv) / 255 + factor).min(255) as u8;
            }
        }
    }

    fn flush_to_layered_window(&self, pixmap: &Pixmap) {
        let w = WIDGET_WIDTH as i32;
        let h = WIDGET_HEIGHT as i32;

        unsafe {
            let screen_dc = GetDC(None);
            let mem_dc = CreateCompatibleDC(screen_dc);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // Top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbitmap = CreateDIBSection(
                mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )
            .expect("CreateDIBSection failed");

            let old_bmp = SelectObject(mem_dc, hbitmap);

            // Convert premultiplied RGBA (tiny-skia) to premultiplied BGRA (Windows DIBSection)
            let src_bytes = pixmap.data();
            let dst_slice = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);

            for i in (0..(w * h * 4) as usize).step_by(4) {
                let r = src_bytes[i];
                let g = src_bytes[i + 1];
                let b = src_bytes[i + 2];
                let a = src_bytes[i + 3];
                dst_slice[i] = b;
                dst_slice[i + 1] = g;
                dst_slice[i + 2] = r;
                dst_slice[i + 3] = a;
            }

            let blend = BLENDFUNCTION {
                BlendOp: windows::Win32::Graphics::Gdi::AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: windows::Win32::Graphics::Gdi::AC_SRC_ALPHA as u8,
            };

            let size = SIZE { cx: w, cy: h };
            let pt_src = POINT { x: 0, y: 0 };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                screen_dc,
                None,
                Some(&size),
                mem_dc,
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(hbitmap);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, screen_dc);
        }
    }
}

fn build_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}
