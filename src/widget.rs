use crate::app::{App, WidgetCommand};
use fontdue::{Font, FontSettings};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LinearGradient, Paint, PathBuilder, Pattern,
    Pixmap, Point, RadialGradient, Rect, SpreadMode, Transform,
};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, HTCAPTION,
    IDC_ARROW, IDC_HAND, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCLBUTTONDOWN, WM_SETCURSOR, WM_TIMER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const WM_MOUSELEAVE: u32 = 0x02A3;

pub const WIDGET_WIDTH: u32 = 396;
pub const WIDGET_HEIGHT: u32 = 66;

const PILL_X: f32 = 8.0;
const PILL_Y: f32 = 5.0;
const PILL_W: f32 = 380.0;
const PILL_H: f32 = 52.0;
const PILL_RADIUS: f32 = 26.0;

// Button regions: (cx, cy, r)
const BTN_LIKE: (f32, f32, f32) = (260.0, 31.0, 10.5);
const BTN_DISLIKE: (f32, f32, f32) = (284.0, 31.0, 10.5);
const BTN_PLAY: (f32, f32, f32) = (308.0, 31.0, 10.5);
const BTN_CONSOLE: (f32, f32, f32) = (332.0, 31.0, 10.5);
const BTN_CLOSE: (f32, f32, f32) = (356.0, 31.0, 10.5);

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
    cover_tx: Sender<(String, Pixmap)>,
    cover_rx: Receiver<(String, Pixmap)>,
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
                let default_cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(widget_wnd_proc),
                    hInstance: hinstance,
                    hCursor: default_cursor,
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
                let (cover_tx, cover_rx) = channel();

                let widget = Box::new(WidgetWindow {
                    app: app.clone(),
                    hwnd,
                    fonts,
                    hovered_btn: None,
                    anim_frame: 0,
                    last_cover_url: String::new(),
                    cover_pixmap: None,
                    cover_tx,
                    cover_rx,
                });

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

            // Pull any downloaded album cover artwork
            while let Ok((url, px)) = widget.cover_rx.try_recv() {
                if url == widget.last_cover_url {
                    widget.cover_pixmap = Some(px);
                }
            }

            widget.render_and_flush();
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // Fix busy spinning wheel cursor!
            let cursor_res = if widget.hovered_btn.is_some() {
                LoadCursorW(None, IDC_HAND)
            } else {
                LoadCursorW(None, IDC_ARROW)
            };
            if let Ok(c) = cursor_res {
                SetCursor(c);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEMOVE => {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);

            let x = (lparam.0 & 0xffff) as i16 as f32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32;

            let in_btn = |(cx, cy, r): (f32, f32, f32)| -> bool {
                let dx = x - cx;
                let dy = y - cy;
                (dx * dx + dy * dy) <= (r * r)
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
                let cur = if new_hover.is_some() {
                    LoadCursorW(None, IDC_HAND)
                } else {
                    LoadCursorW(None, IDC_ARROW)
                };
                if let Ok(c) = cur {
                    SetCursor(c);
                }
                widget.render_and_flush();
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if widget.hovered_btn.is_some() {
                widget.hovered_btn = None;
                if let Ok(c) = LoadCursorW(None, IDC_ARROW) {
                    SetCursor(c);
                }
                widget.render_and_flush();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as f32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32;

            let in_btn = |(cx, cy, r): (f32, f32, f32)| -> bool {
                let dx = x - cx;
                let dy = y - cy;
                (dx * dx + dy * dy) <= (r * r)
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
                // Drag window via OS hardware-accelerated HTCAPTION
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
        self.app.rt.spawn(async move {
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
        self.app.rt.spawn(async move {
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
        self.app.rt.spawn(async move {
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

        // 1. Soft layered glow shadow
        self.draw_shadow(&mut pixmap);

        // 2. Glassmorphism pill background with crisp 1px highlight border
        self.draw_pill_bg(&mut pixmap);

        // 3. High-res spinning vinyl disc / album artwork
        self.draw_cover_disc(&mut pixmap, now_opt.as_ref());

        // 4. Track title, artist, audio wave animation
        self.draw_meta_and_waves(&mut pixmap, now_opt.as_ref());

        // 5. Progress bar and timestamp
        self.draw_progress(&mut pixmap, now_opt.as_ref());

        // 6. Action buttons with vector icons
        self.draw_buttons(&mut pixmap, now_opt.as_ref());

        // 7. Flush 32-bit premultiplied ARGB to layered window
        self.flush_to_layered_window(&pixmap);
    }

    fn draw_shadow(&self, pixmap: &mut Pixmap) {
        let shadow_steps = [
            (0.0, 1.5, 0.05),
            (0.0, 3.5, 0.09),
            (0.0, 6.0, 0.15),
            (0.0, 9.0, 0.22),
        ];
        for (ox, oy, alpha) in shadow_steps {
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

        let shader = LinearGradient::new(
            Point::from_xy(0.0, PILL_Y),
            Point::from_xy(0.0, PILL_Y + PILL_H),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(26, 31, 42, 252)),
                GradientStop::new(1.0, Color::from_rgba8(11, 14, 20, 252)),
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

        // Inner 1px top highlight border
        let mut stroke_paint = Paint::default();
        stroke_paint.set_color_rgba8(255, 255, 255, 38);
        stroke_paint.anti_alias = true;
        let mut stroke = tiny_skia::Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    fn draw_cover_disc(&mut self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let cx = PILL_X + 26.0;
        let cy = PILL_Y + PILL_H / 2.0;
        let r = 19.0;

        // Base vinyl disc dark circle
        let mut disc_builder = PathBuilder::new();
        disc_builder.push_circle(cx, cy, r);
        let disc_path = match disc_builder.finish() {
            Some(p) => p,
            None => return,
        };

        // Radial gradient vinyl texture fallback
        let shader = RadialGradient::new(
            Point::from_xy(cx, cy),
            Point::from_xy(cx, cy),
            r,
            vec![
                GradientStop::new(0.0, Color::from_rgba8(27, 33, 44, 255)),
                GradientStop::new(0.30, Color::from_rgba8(27, 33, 44, 255)),
                GradientStop::new(0.31, Color::from_rgba8(5, 7, 10, 255)),
                GradientStop::new(0.34, Color::from_rgba8(38, 44, 54, 255)),
                GradientStop::new(0.65, Color::from_rgba8(18, 22, 31, 255)),
                GradientStop::new(1.0, Color::from_rgba8(5, 7, 10, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        );

        let mut disc_paint = Paint::default();
        if let Some(s) = shader {
            disc_paint.shader = s;
        } else {
            disc_paint.set_color_rgba8(15, 18, 25, 255);
        }
        disc_paint.anti_alias = true;
        pixmap.fill_path(&disc_path, &disc_paint, FillRule::Winding, Transform::identity(), None);

        // Check if cover URL has changed
        let cover_url = now.map(|n| n.cover.as_str()).unwrap_or("");
        if !cover_url.is_empty() && cover_url != self.last_cover_url {
            self.last_cover_url = cover_url.to_string();
            self.cover_pixmap = None;
            let url = cover_url.to_string();
            let tx = self.cover_tx.clone();

            self.app.rt.spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(8))
                    .build()
                    .unwrap_or_default();
                let req = client
                    .get(&url)
                    .header(
                        "User-Agent",
                        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36",
                    )
                    .header("Referer", "https://music.163.com/");

                if let Ok(resp) = req.send().await {
                    if let Ok(bytes) = resp.bytes().await {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            // High quality resize to 38x38
                            let resized = img.resize_exact(38, 38, image::imageops::FilterType::Lanczos3);
                            let rgba = resized.to_rgba8();
                            if let Some(mut px) = Pixmap::new(38, 38) {
                                px.data_mut().copy_from_slice(&rgba.into_raw());
                                let _ = tx.send((url, px));
                            }
                        }
                    }
                }
            });
        }

        let is_playing = now.map(|n| n.playing).unwrap_or(false);

        // Draw album art or vinyl grooves
        if let Some(ref cover) = self.cover_pixmap {
            // Gentle spinning angle when playing (~14 seconds per revolution)
            let angle = if is_playing {
                (self.anim_frame as f32 * 1.2) % 360.0
            } else {
                0.0
            };

            let mut trans = Transform::identity();
            trans = trans.pre_translate(cx, cy);
            trans = trans.pre_rotate(angle);
            trans = trans.pre_translate(-19.0, -19.0);

            let mut cover_paint = Paint::default();
            cover_paint.shader = Pattern::new(
                cover.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                1.0,
                trans,
            );
            cover_paint.anti_alias = true;
            pixmap.fill_path(&disc_path, &cover_paint, FillRule::Winding, Transform::identity(), None);
        } else {
            // Vinyl grooves fallback
            let mut ring_paint = Paint::default();
            ring_paint.set_color_rgba8(255, 255, 255, 20);
            ring_paint.anti_alias = true;
            let mut stroke = tiny_skia::Stroke::default();
            stroke.width = 0.8;

            for ring_r in [14.0, 11.0, 8.0] {
                let mut ring_builder = PathBuilder::new();
                ring_builder.push_circle(cx, cy, ring_r);
                if let Some(rp) = ring_builder.finish() {
                    pixmap.stroke_path(&rp, &ring_paint, &stroke, Transform::identity(), None);
                }
            }
        }

        // Disc outer rim border
        let mut rim_paint = Paint::default();
        rim_paint.set_color_rgba8(255, 255, 255, 45);
        rim_paint.anti_alias = true;
        let mut rim_stroke = tiny_skia::Stroke::default();
        rim_stroke.width = 1.0;
        pixmap.stroke_path(&disc_path, &rim_paint, &rim_stroke, Transform::identity(), None);

        // Center spindle hole
        let mut hole_builder = PathBuilder::new();
        hole_builder.push_circle(cx, cy, 3.5);
        if let Some(hp) = hole_builder.finish() {
            let mut hole_paint = Paint::default();
            hole_paint.set_color_rgba8(5, 7, 10, 255);
            hole_paint.anti_alias = true;
            pixmap.fill_path(&hp, &hole_paint, FillRule::Winding, Transform::identity(), None);

            let mut pin_stroke = Paint::default();
            pin_stroke.set_color_rgba8(255, 255, 255, 140);
            pin_stroke.anti_alias = true;
            let mut stroke = tiny_skia::Stroke::default();
            stroke.width = 1.2;
            pixmap.stroke_path(&hp, &pin_stroke, &stroke, Transform::identity(), None);
        }
    }

    fn draw_meta_and_waves(&self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let (title, artist, playing) = match now {
            Some(n) => (n.title.as_str(), n.artist.as_str(), n.playing),
            None => ("等待播放曲目...", "打开播放器播放即可", false),
        };

        let text_start_x = PILL_X + 50.0;
        let text_y = 25.0;

        // Title text
        let title_w = self.draw_text(pixmap, title, text_start_x, text_y, 12.0, Color::from_rgba8(255, 255, 255, 255), 115.0);

        // Dot separator
        let dot_x = text_start_x + title_w + 4.0;
        let dot_w = self.draw_text(pixmap, "·", dot_x, text_y, 10.0, Color::from_rgba8(88, 96, 105, 255), 10.0);

        // Artist text
        let artist_x = dot_x + dot_w + 4.0;
        let artist_w = self.draw_text(pixmap, artist, artist_x, text_y, 11.0, Color::from_rgba8(139, 148, 158, 255), 52.0);

        // Soundwaves: 4 neon-green vertical bars
        let wave_x = (artist_x + artist_w + 5.0).min(PILL_X + 225.0);
        let wave_y_base = text_y + 1.0;

        let heights = if playing {
            let f = self.anim_frame as f32;
            [
                3.0 + ((f * 0.16).sin().abs() * 7.5),
                4.0 + ((f * 0.23 + 1.2).sin().abs() * 8.5),
                3.0 + ((f * 0.19 + 2.4).sin().abs() * 7.0),
                4.0 + ((f * 0.25 + 3.6).sin().abs() * 8.0),
            ]
        } else {
            [2.5, 2.5, 2.5, 2.5]
        };

        let mut wave_paint = Paint::default();
        wave_paint.set_color_rgba8(86, 211, 100, 255);
        wave_paint.anti_alias = true;

        for (i, h) in heights.iter().enumerate() {
            let bx = wave_x + (i as f32 * 3.5);
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

        let track_x = PILL_X + 50.0;
        let track_y = 35.0;
        let track_w = 95.0;
        let track_h = 2.5;

        // Progress background track
        let bg_path = build_rounded_rect(track_x, track_y, track_w, track_h, 1.25);
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 25);
        bg_paint.anti_alias = true;
        pixmap.fill_path(&bg_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

        // Progress bar fill (orange/red gradient)
        if ratio > 0.005 {
            let fill_w = (track_w * ratio).max(3.0);
            let fill_path = build_rounded_rect(track_x, track_y, fill_w, track_h, 1.25);
            let mut fill_paint = Paint::default();
            fill_paint.set_color_rgba8(232, 65, 66, 255);
            fill_paint.anti_alias = true;
            pixmap.fill_path(&fill_path, &fill_paint, FillRule::Winding, Transform::identity(), None);

            // Glowing progress thumb head dot
            let thumb_cx = track_x + fill_w;
            let thumb_cy = track_y + track_h / 2.0;
            let mut thumb_builder = PathBuilder::new();
            thumb_builder.push_circle(thumb_cx, thumb_cy, 2.0);
            if let Some(tp) = thumb_builder.finish() {
                let mut thumb_paint = Paint::default();
                thumb_paint.set_color_rgba8(255, 123, 114, 255);
                thumb_paint.anti_alias = true;
                pixmap.fill_path(&tp, &thumb_paint, FillRule::Winding, Transform::identity(), None);
            }
        }

        // Time labels: 00:15 / 04:47
        let played_s = played_ms / 1000;
        let time_str = if dur_ms > 0 {
            let dur_s = dur_ms / 1000;
            format!(
                "{:02}:{:02} / {:02}:{:02}",
                played_s / 60,
                played_s % 60,
                dur_s / 60,
                dur_s % 60
            )
        } else {
            format!("{:02}:{:02}", played_s / 60, played_s % 60)
        };

        self.draw_text(pixmap, &time_str, track_x + track_w + 8.0, 40.0, 9.5, Color::from_rgba8(125, 133, 144, 255), 75.0);
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

        for ((cx, cy, r), idx, kind) in buttons {
            let is_hovered = self.hovered_btn == Some(idx);

            let mut circle_builder = PathBuilder::new();
            circle_builder.push_circle(cx, cy, r);
            let cp = match circle_builder.finish() {
                Some(p) => p,
                None => continue,
            };

            let mut bg_paint = Paint::default();
            bg_paint.anti_alias = true;

            if idx == 0 && current_is_liked {
                bg_paint.set_color_rgba8(46, 160, 67, 100);
            } else if idx == 1 && current_is_disliked {
                bg_paint.set_color_rgba8(232, 65, 66, 100);
            } else if idx == 4 && is_hovered {
                bg_paint.set_color_rgba8(232, 65, 66, 90);
            } else if is_hovered {
                bg_paint.set_color_rgba8(255, 255, 255, 55);
            } else {
                bg_paint.set_color_rgba8(255, 255, 255, 16);
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
            } else if idx == 4 && is_hovered {
                stroke_paint.set_color_rgba8(255, 123, 114, 255);
            } else if is_hovered {
                stroke_paint.set_color_rgba8(255, 255, 255, 90);
            } else {
                stroke_paint.set_color_rgba8(255, 255, 255, 26);
            }
            pixmap.stroke_path(&cp, &stroke_paint, &stroke, Transform::identity(), None);

            // Highlight play button as primary action
            if idx == 2 {
                let mut play_ring = tiny_skia::Stroke::default();
                play_ring.width = 1.2;
                let mut play_stroke = Paint::default();
                play_stroke.anti_alias = true;
                if is_hovered {
                    play_stroke.set_color_rgba8(255, 255, 255, 230);
                } else {
                    play_stroke.set_color_rgba8(255, 255, 255, 110);
                }
                pixmap.stroke_path(&cp, &play_stroke, &play_ring, Transform::identity(), None);
            }

            // Draw crisp vector icon geometry
            self.draw_button_icon(pixmap, cx, cy, kind, is_hovered, current_is_liked, current_is_disliked);
        }
    }

    fn draw_button_icon(
        &self,
        pixmap: &mut Pixmap,
        cx: f32,
        cy: f32,
        kind: &str,
        is_hover: bool,
        is_liked: bool,
        is_disliked: bool,
    ) {
        let mut stroke_paint = Paint::default();
        stroke_paint.anti_alias = true;

        let mut fill_paint = Paint::default();
        fill_paint.anti_alias = true;

        if is_hover {
            stroke_paint.set_color_rgba8(255, 255, 255, 255);
            fill_paint.set_color_rgba8(255, 255, 255, 255);
        } else {
            stroke_paint.set_color_rgba8(170, 180, 195, 230);
            fill_paint.set_color_rgba8(170, 180, 195, 230);
        }

        match kind {
            "like" => {
                // Vector thumbs up
                if is_liked {
                    stroke_paint.set_color_rgba8(86, 211, 100, 255);
                    fill_paint.set_color_rgba8(86, 211, 100, 255);
                }

                // Vertical wrist bar
                let mut wrist = PathBuilder::new();
                wrist.move_to(cx - 4.8, cy - 1.0);
                wrist.line_to(cx - 4.8, cy + 4.5);
                let mut wrist_stroke = tiny_skia::Stroke::default();
                wrist_stroke.width = 1.4;
                wrist_stroke.line_cap = tiny_skia::LineCap::Round;
                if let Some(p) = wrist.finish() {
                    pixmap.stroke_path(&p, &stroke_paint, &wrist_stroke, Transform::identity(), None);
                }

                // Hand body & thumb
                let mut hand = PathBuilder::new();
                hand.move_to(cx - 3.2, cy + 4.5);
                hand.line_to(cx + 3.0, cy + 4.5);
                hand.quad_to(cx + 4.5, cy + 4.5, cx + 4.5, cy + 2.5);
                hand.line_to(cx + 4.5, cy - 0.2);
                hand.line_to(cx - 0.5, cy - 0.2);
                hand.line_to(cx - 0.5, cy - 4.2);
                hand.quad_to(cx - 1.5, cy - 5.2, cx - 2.5, cy - 4.2);
                hand.line_to(cx - 3.2, cy - 1.0);
                hand.close();
                if let Some(p) = hand.finish() {
                    pixmap.fill_path(&p, &fill_paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            "dislike" => {
                // Vector prohibited / ban sign
                if is_disliked {
                    stroke_paint.set_color_rgba8(255, 123, 114, 255);
                }
                let mut pb = PathBuilder::new();
                pb.push_circle(cx, cy, 4.8);
                pb.move_to(cx - 3.2, cy - 3.2);
                pb.line_to(cx + 3.2, cy + 3.2);

                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &stroke_paint, &stroke, Transform::identity(), None);
                }
            }
            "play" => {
                // Vector play triangle
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 2.5, cy - 4.2);
                pb.line_to(cx + 4.2, cy);
                pb.line_to(cx - 2.5, cy + 4.2);
                pb.close();
                if let Some(p) = pb.finish() {
                    pixmap.fill_path(&p, &fill_paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            "console" => {
                // Vector corner expand arrows
                let mut pb = PathBuilder::new();
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;
                stroke.line_join = tiny_skia::LineJoin::Round;

                pb.move_to(cx - 3.5, cy + 3.5);
                pb.line_to(cx + 3.5, cy - 3.5);
                // Arrow head
                pb.move_to(cx + 1.2, cy - 3.5);
                pb.line_to(cx + 3.5, cy - 3.5);
                pb.line_to(cx + 3.5, cy - 1.2);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &stroke_paint, &stroke, Transform::identity(), None);
                }
            }
            "close" => {
                // Vector clean cross
                let mut pb = PathBuilder::new();
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;

                pb.move_to(cx - 3.2, cy - 3.2);
                pb.line_to(cx + 3.2, cy + 3.2);
                pb.move_to(cx + 3.2, cy - 3.2);
                pb.line_to(cx - 3.2, cy + 3.2);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &stroke_paint, &stroke, Transform::identity(), None);
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
    ) -> f32 {
        let font = match self.fonts.main_font.as_ref() {
            Some(f) => f,
            None => return 0.0,
        };

        let mut cur_x = x;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if cur_x - x > max_w - 8.0 && chars.peek().is_some() {
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

        cur_x - x
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
