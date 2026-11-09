use crate::app::{App, WidgetCommand};
use fontdue::{Font, FontSettings};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LinearGradient, Paint, PathBuilder, Pattern,
    Pixmap, Point, SpreadMode, Transform,
};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, BLENDFUNCTION,
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

pub const WIDGET_WIDTH: u32 = 364;
pub const WIDGET_HEIGHT: u32 = 68;

const PILL_X: f32 = 14.0;
const PILL_Y: f32 = 8.0;
const PILL_W: f32 = 336.0;
const PILL_H: f32 = 48.0;
const PILL_RADIUS: f32 = 24.0;
const PILL_CY: f32 = PILL_Y + PILL_H * 0.5; // 32.0

// Hover action button hit regions: (cx, cy, r)
const BTN_LIKE: (f32, f32, f32) = (PILL_X + 216.0, PILL_CY, 10.5);
const BTN_DISLIKE: (f32, f32, f32) = (PILL_X + 240.0, PILL_CY, 10.5);
const BTN_PLAY: (f32, f32, f32) = (PILL_X + 264.0, PILL_CY, 10.5);
const BTN_CONSOLE: (f32, f32, f32) = (PILL_X + 288.0, PILL_CY, 10.5);
const BTN_CLOSE: (f32, f32, f32) = (PILL_X + 312.0, PILL_CY, 10.5);

#[derive(Clone)]
struct FontCache {
    regular_font: Option<Arc<Font>>,
    bold_font: Option<Arc<Font>>,
}

impl FontCache {
    fn new() -> Self {
        let regular_paths = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ];
        let bold_paths = [
            r"C:\Windows\Fonts\msyhbd.ttc",
            r"C:\Windows\Fonts\msyhbd.ttf",
            r"C:\Windows\Fonts\segoeuib.ttf",
        ];

        let mut regular_font = None;
        for p in regular_paths {
            if Path::new(p).exists() {
                if let Ok(data) = std::fs::read(p) {
                    if let Ok(font) = Font::from_bytes(data, FontSettings::default()) {
                        regular_font = Some(Arc::new(font));
                        break;
                    }
                }
            }
        }

        let mut bold_font = None;
        for p in bold_paths {
            if Path::new(p).exists() {
                if let Ok(data) = std::fs::read(p) {
                    if let Ok(font) = Font::from_bytes(data, FontSettings::default()) {
                        bold_font = Some(Arc::new(font));
                        break;
                    }
                }
            }
        }
        if bold_font.is_none() {
            bold_font = regular_font.clone();
        }

        Self {
            regular_font,
            bold_font,
        }
    }
}

pub struct WidgetWindow {
    app: Arc<App>,
    hwnd: HWND,
    fonts: FontCache,
    shadow_pixmap: Pixmap,
    is_mouse_inside: bool,
    hover_anim: f32,            // 0.0 (breathing waveform) -> 1.0 (minimalist controls)
    hovered_btn: Option<usize>, // 0: like, 1: dislike, 2: play, 3: console, 4: close
    anim_frame: u32,
    last_cover_key: String,
    cover_pixmap: Option<Pixmap>,
    cover_accent: (u8, u8, u8),
    cover_tx: Sender<(String, Pixmap, (u8, u8, u8))>,
    cover_rx: Receiver<(String, Pixmap, (u8, u8, u8))>,
}

pub fn spawn_widget_thread(
    app: Arc<App>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<WidgetCommand>,
    show_initially: bool,
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
                let x = (screen_w - WIDGET_WIDTH as i32) - 36;
                let y = (screen_h - WIDGET_HEIGHT as i32) - 84;

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
                let shadow_pixmap = precompute_gaussian_shadow();
                let (cover_tx, cover_rx) = channel();

                let mut widget = Box::new(WidgetWindow {
                    app: app.clone(),
                    hwnd,
                    fonts,
                    shadow_pixmap,
                    is_mouse_inside: false,
                    hover_anim: 0.0,
                    hovered_btn: None,
                    anim_frame: 0,
                    last_cover_key: String::new(),
                    cover_pixmap: None,
                    cover_accent: (245, 245, 247),
                    cover_tx,
                    cover_rx,
                });

                widget.render_and_flush();

                windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                    Box::into_raw(widget) as isize,
                );

                if show_initially {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }

                // 33 FPS (30ms) timer for smooth waveform & hover fade transitions
                windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, 1, 30, None);

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
                                let visible =
                                    windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd)
                                        .as_bool();
                                if visible {
                                    let _ = ShowWindow(hwnd, SW_HIDE);
                                } else {
                                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                }
                            }
                            WidgetCommand::Quit => {
                                let _ = DestroyWindow(hwnd);
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

            // Smoothly animate hover state (breathing <-> controls)
            if widget.is_mouse_inside {
                widget.hover_anim = (widget.hover_anim + 0.22).min(1.0);
            } else {
                widget.hover_anim = (widget.hover_anim - 0.16).max(0.0);
            }

            // Pull any decoded/downloaded album cover artwork
            while let Ok((key, px, accent)) = widget.cover_rx.try_recv() {
                if key == widget.last_cover_key {
                    widget.cover_pixmap = Some(px);
                    widget.cover_accent = accent;
                }
            }

            widget.render_and_flush();
            LRESULT(0)
        }
        WM_SETCURSOR => {
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

            let in_pill = x >= PILL_X
                && x <= PILL_X + PILL_W
                && y >= PILL_Y
                && y <= PILL_Y + PILL_H;
            widget.is_mouse_inside = in_pill;

            let in_btn = |(cx, cy, r): (f32, f32, f32)| -> bool {
                let dx = x - cx;
                let dy = y - cy;
                (dx * dx + dy * dy) <= (r * r)
            };

            let new_hover = if widget.hover_anim > 0.3 {
                if in_btn(BTN_LIKE) {
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
                }
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
            widget.is_mouse_inside = false;
            if widget.hovered_btn.is_some() {
                widget.hovered_btn = None;
                if let Ok(c) = LoadCursorW(None, IDC_ARROW) {
                    SetCursor(c);
                }
            }
            widget.render_and_flush();
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

            if widget.hover_anim > 0.3 && in_btn(BTN_LIKE) {
                widget.on_btn_like();
            } else if widget.hover_anim > 0.3 && in_btn(BTN_DISLIKE) {
                widget.on_btn_dislike();
            } else if widget.hover_anim > 0.3 && in_btn(BTN_PLAY) {
                widget.on_btn_play();
            } else if widget.hover_anim > 0.3 && in_btn(BTN_CONSOLE) {
                widget.on_btn_console();
            } else if widget.hover_anim > 0.3 && in_btn(BTN_CLOSE) {
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

        // Start with the precomputed true Gaussian blur shadow
        let mut pixmap = self.shadow_pixmap.clone();

        // 1. Apple Dynamic Island pure pitch-black pill (#08080A) + whisper inner rim
        self.draw_pill_bg(&mut pixmap);

        // 2. Crisp circular album art (instant SMTC thumbnail or NCM) or minimalist note fallback
        self.draw_cover_disc(&mut pixmap, now_opt.as_ref());

        // 3. Two-line typography (Bold Title + Muted Artist)
        self.draw_meta_text(&mut pixmap, now_opt.as_ref());

        // 4. Whisper-thin ambient bottom progress line (no 00:00 text clutter)
        self.draw_ambient_progress(&mut pixmap, now_opt.as_ref());

        // 5. Right side: Smoothly cross-fade between symmetric equalizer waveform & minimalist borderless controls
        if self.hover_anim < 0.99 {
            let wave_alpha = (1.0 - self.hover_anim).clamp(0.0, 1.0);
            self.draw_symmetric_waveform(&mut pixmap, now_opt.as_ref(), wave_alpha);
        }
        if self.hover_anim > 0.01 {
            let ctrl_alpha = self.hover_anim.clamp(0.0, 1.0);
            self.draw_minimal_controls(&mut pixmap, now_opt.as_ref(), ctrl_alpha);
        }

        // 6. Flush 32-bit premultiplied ARGB to layered window
        self.flush_to_layered_window(&pixmap);
    }

    fn draw_pill_bg(&self, pixmap: &mut Pixmap) {
        let path = build_rounded_rect(PILL_X, PILL_Y, PILL_W, PILL_H, PILL_RADIUS);

        // Pure OLED Pitch Black (#0A0A0D -> #050507)
        let shader = LinearGradient::new(
            Point::from_xy(0.0, PILL_Y),
            Point::from_xy(0.0, PILL_Y + PILL_H),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(13, 13, 16, 254)),
                GradientStop::new(1.0, Color::from_rgba8(5, 5, 7, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        );

        if let Some(s) = shader {
            let mut paint = Paint::default();
            paint.shader = s;
            paint.anti_alias = true;
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }

        // Ultra-subtle specular inner rim (fades from 11% white at top to 3% at bottom)
        let rim_shader = LinearGradient::new(
            Point::from_xy(0.0, PILL_Y),
            Point::from_xy(0.0, PILL_Y + PILL_H),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 28)),
                GradientStop::new(1.0, Color::from_rgba8(255, 255, 255, 8)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        );
        if let Some(rs) = rim_shader {
            let mut stroke_paint = Paint::default();
            stroke_paint.shader = rs;
            stroke_paint.anti_alias = true;
            let mut stroke = tiny_skia::Stroke::default();
            stroke.width = 0.85;
            pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
        }
    }

    fn draw_cover_disc(&mut self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let cx = PILL_X + 24.0;
        let cy = PILL_CY;
        let r = 17.0; // 34x34 diameter -> 7px uniform margin around inside 48px pill

        let mut disc_builder = PathBuilder::new();
        disc_builder.push_circle(cx, cy, r);
        let disc_path = match disc_builder.finish() {
            Some(p) => p,
            None => return,
        };

        // Determine unique cover key (prefer high-res NCM URL if ready, otherwise SMTC local bytes by play_id)
        let cover_key = match now {
            Some(n) if !n.cover.is_empty() => format!("url:{}", n.cover),
            Some(n) if n.cover_bytes.is_some() => format!("smtc:{}", n.play_id),
            _ => String::new(),
        };

        if !cover_key.is_empty() && cover_key != self.last_cover_key {
            self.last_cover_key = cover_key.clone();
            let tx = self.cover_tx.clone();

            if let Some(n) = now {
                if !n.cover.is_empty() {
                    let url = n.cover.clone();
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
                                if let Some((px, accent)) = decode_cover_to_pixmap(&bytes, 34) {
                                    let _ = tx.send((cover_key, px, accent));
                                }
                            }
                        }
                    });
                } else if let Some(ref raw_bytes) = n.cover_bytes {
                    let bytes_arc = raw_bytes.clone();
                    if let Some((px, accent)) = decode_cover_to_pixmap(&bytes_arc, 34) {
                        self.cover_pixmap = Some(px);
                        self.cover_accent = accent;
                    }
                }
            }
        } else if cover_key.is_empty() {
            self.last_cover_key.clear();
            self.cover_pixmap = None;
            self.cover_accent = (245, 245, 247);
        }

        if let Some(ref cover) = self.cover_pixmap {
            // Clean, unobstructed album cover (no tacky center bullseye hole!)
            let mut trans = Transform::identity();
            trans = trans.pre_translate(cx - r, cy - r);

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

            // Whisper-thin inner border on the cover
            let mut rim_paint = Paint::default();
            rim_paint.set_color_rgba8(255, 255, 255, 22);
            rim_paint.anti_alias = true;
            let mut rim_stroke = tiny_skia::Stroke::default();
            rim_stroke.width = 0.75;
            pixmap.stroke_path(&disc_path, &rim_paint, &rim_stroke, Transform::identity(), None);
        } else {
            // Minimalist dark graphite circle + single sleek musical note (no concentric bullseye lines!)
            let mut bg_paint = Paint::default();
            bg_paint.set_color_rgba8(22, 22, 27, 255);
            bg_paint.anti_alias = true;
            pixmap.fill_path(&disc_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

            let mut note_paint = Paint::default();
            note_paint.set_color_rgba8(118, 118, 128, 255);
            note_paint.anti_alias = true;

            // Note head (filled small circle at bottom-left of center)
            let mut head_pb = PathBuilder::new();
            head_pb.push_circle(cx - 1.5, cy + 3.2, 2.4);
            if let Some(hp) = head_pb.finish() {
                pixmap.fill_path(&hp, &note_paint, FillRule::Winding, Transform::identity(), None);
            }

            // Note stem & flag
            let mut stem_pb = PathBuilder::new();
            stem_pb.move_to(cx + 0.6, cy + 3.0);
            stem_pb.line_to(cx + 0.6, cy - 4.5);
            stem_pb.quad_to(cx + 3.8, cy - 3.8, cx + 4.0, cy - 1.2);
            let mut stem_stroke = tiny_skia::Stroke::default();
            stem_stroke.width = 1.35;
            stem_stroke.line_cap = tiny_skia::LineCap::Round;
            stem_stroke.line_join = tiny_skia::LineJoin::Round;
            if let Some(sp) = stem_pb.finish() {
                pixmap.stroke_path(&sp, &note_paint, &stem_stroke, Transform::identity(), None);
            }
        }
    }

    fn draw_meta_text(&self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let (title, artist) = match now {
            Some(n) => (
                n.title.as_str(),
                if n.artist.is_empty() { "未知艺术家" } else { n.artist.as_str() },
            ),
            None => ("等待音乐播放", "打开任意音乐软件即可联动"),
        };

        let text_x = PILL_X + 49.0;
        // Dynamically narrow max text width when hover controls appear
        let max_w = 230.0 - (self.hover_anim * 75.0);

        // Line 1: Bold crisp track title (#F5F5F7)
        self.draw_text(
            pixmap,
            title,
            text_x,
            PILL_Y + 21.5,
            12.5,
            Color::from_rgba8(245, 245, 247, 255),
            max_w,
            true,
        );

        // Line 2: Subtle secondary artist label (#86868B)
        self.draw_text(
            pixmap,
            artist,
            text_x,
            PILL_Y + 36.5,
            10.5,
            Color::from_rgba8(134, 134, 139, 255),
            max_w,
            false,
        );
    }

    fn draw_ambient_progress(&self, pixmap: &mut Pixmap, now: Option<&crate::monitor::PlaySnapshot>) {
        let (played_ms, dur_ms) = match now {
            Some(n) => (n.played_ms, n.duration_ms),
            None => (0, 0),
        };

        let ratio = if dur_ms > 0 {
            ((played_ms as f32) / (dur_ms as f32)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        if ratio <= 0.002 {
            return;
        }

        let track_x = PILL_X + 49.0;
        let track_y = PILL_Y + PILL_H - 2.6;
        let track_w = PILL_W - 73.0;
        let track_h = 1.4;

        // Whisper-faint track
        let bg_path = build_rounded_rect(track_x, track_y, track_w, track_h, 0.7);
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 16);
        bg_paint.anti_alias = true;
        pixmap.fill_path(&bg_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

        // Streamline active fill tinted by album cover accent
        let fill_w = (track_w * ratio).max(2.0);
        let fill_path = build_rounded_rect(track_x, track_y, fill_w, track_h, 0.7);
        let (ar, ag, ab) = self.cover_accent;

        let shader = LinearGradient::new(
            Point::from_xy(track_x, track_y),
            Point::from_xy(track_x + fill_w, track_y),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(ar, ag, ab, 120)),
                GradientStop::new(1.0, Color::from_rgba8(ar, ag, ab, 235)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        );
        let mut fill_paint = Paint::default();
        if let Some(s) = shader {
            fill_paint.shader = s;
        } else {
            fill_paint.set_color_rgba8(ar, ag, ab, 210);
        }
        fill_paint.anti_alias = true;
        pixmap.fill_path(&fill_path, &fill_paint, FillRule::Winding, Transform::identity(), None);
    }

    fn draw_symmetric_waveform(
        &self,
        pixmap: &mut Pixmap,
        now: Option<&crate::monitor::PlaySnapshot>,
        alpha: f32,
    ) {
        let playing = now.map(|n| n.playing).unwrap_or(false);
        let f = self.anim_frame as f32;

        // Apple Dynamic Island style: 4 vertical capsule bars centered at PILL_CY
        let heights = if playing {
            [
                4.0 + ((f * 0.14).sin().abs() * 8.5),
                4.5 + ((f * 0.21 + 1.1).sin().abs() * 11.5),
                4.0 + ((f * 0.17 + 2.3).sin().abs() * 9.5),
                3.5 + ((f * 0.24 + 3.7).sin().abs() * 7.5),
            ]
        } else {
            [3.2, 3.2, 3.2, 3.2]
        };

        let (ar, ag, ab) = self.cover_accent;
        let bar_alpha = ((if playing { 235.0 } else { 85.0 }) * alpha) as u8;

        let mut wave_paint = Paint::default();
        wave_paint.set_color_rgba8(ar, ag, ab, bar_alpha);
        wave_paint.anti_alias = true;

        let start_x = PILL_X + PILL_W - 33.0;
        for (i, h) in heights.iter().enumerate() {
            let bx = start_x + (i as f32 * 4.5);
            let by = PILL_CY - (*h * 0.5);
            let path = build_rounded_rect(bx, by, 2.2, *h, 1.1);
            pixmap.fill_path(&path, &wave_paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    fn draw_minimal_controls(
        &self,
        pixmap: &mut Pixmap,
        now: Option<&crate::monitor::PlaySnapshot>,
        alpha: f32,
    ) {
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

            // Minimalist philosophy: NO permanent circles or borders!
            // Only draw a soft translucent backdrop when the cursor is directly hovering this icon.
            if is_hovered {
                let mut circle_builder = PathBuilder::new();
                circle_builder.push_circle(cx, cy, r);
                if let Some(cp) = circle_builder.finish() {
                    let mut bg_paint = Paint::default();
                    bg_paint.anti_alias = true;
                    if idx == 4 {
                        bg_paint.set_color_rgba8(255, 69, 58, (55.0 * alpha) as u8);
                    } else {
                        bg_paint.set_color_rgba8(255, 255, 255, (32.0 * alpha) as u8);
                    }
                    pixmap.fill_path(&cp, &bg_paint, FillRule::Winding, Transform::identity(), None);
                }
            }

            self.draw_minimal_icon(
                pixmap,
                cx,
                cy,
                kind,
                is_hovered,
                current_is_liked,
                current_is_disliked,
                alpha,
            );
        }
    }

    fn draw_minimal_icon(
        &self,
        pixmap: &mut Pixmap,
        cx: f32,
        cy: f32,
        kind: &str,
        is_hover: bool,
        is_liked: bool,
        is_disliked: bool,
        alpha: f32,
    ) {
        let mut paint = Paint::default();
        paint.anti_alias = true;

        let base_a = ((if is_hover { 255.0 } else { 165.0 }) * alpha) as u8;
        paint.set_color_rgba8(245, 245, 247, base_a);

        match kind {
            "like" => {
                // Clean symmetric Bezier Heart (♥ / ♡)
                if is_liked {
                    paint.set_color_rgba8(255, 55, 95, (255.0 * alpha) as u8);
                }
                let mut pb = PathBuilder::new();
                pb.move_to(cx, cy + 4.2);
                pb.cubic_to(cx - 5.8, cy + 0.6, cx - 5.2, cy - 4.2, cx - 2.4, cy - 4.2);
                pb.cubic_to(cx - 0.9, cy - 4.2, cx, cy - 2.8, cx, cy - 2.2);
                pb.cubic_to(cx, cy - 2.8, cx + 0.9, cy - 4.2, cx + 2.4, cy - 4.2);
                pb.cubic_to(cx + 5.2, cy - 4.2, cx + 5.8, cy + 0.6, cx, cy + 4.2);
                pb.close();

                if let Some(p) = pb.finish() {
                    if is_liked {
                        pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                    } else {
                        let mut stroke = tiny_skia::Stroke::default();
                        stroke.width = 1.3;
                        stroke.line_join = tiny_skia::LineJoin::Round;
                        pixmap.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                    }
                }
            }
            "dislike" => {
                if is_disliked {
                    paint.set_color_rgba8(255, 69, 58, (255.0 * alpha) as u8);
                }
                let mut pb = PathBuilder::new();
                pb.push_circle(cx, cy, 4.4);
                pb.move_to(cx - 3.0, cy - 3.0);
                pb.line_to(cx + 3.0, cy + 3.0);

                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                }
            }
            "play" => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 2.4, cy - 4.2);
                pb.line_to(cx + 4.2, cy);
                pb.line_to(cx - 2.4, cy + 4.2);
                pb.close();
                if let Some(p) = pb.finish() {
                    pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            "console" => {
                let mut pb = PathBuilder::new();
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;
                stroke.line_join = tiny_skia::LineJoin::Round;

                pb.move_to(cx - 3.3, cy + 3.3);
                pb.line_to(cx + 3.3, cy - 3.3);
                pb.move_to(cx - 0.5, cy - 3.3);
                pb.line_to(cx + 3.3, cy - 3.3);
                pb.line_to(cx + 3.3, cy + 0.5);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                }
            }
            "close" => {
                if is_hover {
                    paint.set_color_rgba8(255, 105, 97, (255.0 * alpha) as u8);
                }
                let mut pb = PathBuilder::new();
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;

                pb.move_to(cx - 3.0, cy - 3.0);
                pb.line_to(cx + 3.0, cy + 3.0);
                pb.move_to(cx + 3.0, cy - 3.0);
                pb.line_to(cx - 3.0, cy + 3.0);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
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
        bold: bool,
    ) -> f32 {
        let font_opt = if bold {
            self.fonts.bold_font.as_ref().or(self.fonts.regular_font.as_ref())
        } else {
            self.fonts.regular_font.as_ref()
        };
        let font = match font_opt {
            Some(f) => f,
            None => return 0.0,
        };

        let mut cur_x = x;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if cur_x - x > max_w - 10.0 && chars.peek().is_some() {
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

                let raw_a = bitmap[gy * w + gx] as f32 / 255.0;
                if raw_a <= 0.0 {
                    continue;
                }
                // Slight gamma boost (1/1.25) so anti-aliased CJK strokes look crisp on dark OLED bg
                let alpha_val = (raw_a.powf(0.82) * 255.0).round() as u32;

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

/// Precomputes a true 3-pass separable Gaussian-blurred ambient shadow once at startup.
/// Eliminates all stepped banding ("double-chin") on light desktop backgrounds.
fn precompute_gaussian_shadow() -> Pixmap {
    let w = WIDGET_WIDTH as usize;
    let h = WIDGET_HEIGHT as usize;
    let mut px = Pixmap::new(WIDGET_WIDTH, WIDGET_HEIGHT).unwrap();
    px.fill(Color::TRANSPARENT);

    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 56);
    paint.anti_alias = true;
    let path = build_rounded_rect(PILL_X + 1.0, PILL_Y + 3.0, PILL_W - 2.0, PILL_H, PILL_RADIUS);
    px.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);

    let mut alpha_buf: Vec<f32> = px.data().iter().skip(3).step_by(4).map(|&a| a as f32).collect();
    let mut temp_buf = vec![0.0f32; w * h];
    let radius: isize = 4;
    let kernel_len = (radius * 2 + 1) as f32;

    // 3 iterations of separable 1D box blur -> smooth Gaussian kernel
    for _ in 0..3 {
        // Horizontal pass
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for k in -radius..=radius {
                    let nx = (x as isize + k).clamp(0, w as isize - 1) as usize;
                    sum += alpha_buf[y * w + nx];
                }
                temp_buf[y * w + x] = sum / kernel_len;
            }
        }
        // Vertical pass
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for k in -radius..=radius {
                    let ny = (y as isize + k).clamp(0, h as isize - 1) as usize;
                    sum += temp_buf[ny * w + x];
                }
                alpha_buf[y * w + x] = sum / kernel_len;
            }
        }
    }

    let data = px.data_mut();
    for i in 0..(w * h) {
        let a = alpha_buf[i].round().clamp(0.0, 255.0) as u8;
        data[i * 4] = 0;
        data[i * 4 + 1] = 0;
        data[i * 4 + 2] = 0;
        data[i * 4 + 3] = a;
    }
    px
}

/// Decodes raw image bytes into a resized Pixmap and extracts a vibrant, readable accent color.
fn decode_cover_to_pixmap(bytes: &[u8], size: u32) -> Option<(Pixmap, (u8, u8, u8))> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut weight_sum: u64 = 0;

    for pixel in rgba.pixels() {
        let (r, g, b) = (pixel[0] as u64, pixel[1] as u64, pixel[2] as u64);
        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        let sat = max_c - min_c;
        // Weight colorful, non-black, non-white pixels higher
        let w = if max_c > 35 && max_c < 245 {
            1 + sat * 3
        } else {
            1
        };
        r_sum += r * w;
        g_sum += g * w;
        b_sum += b * w;
        weight_sum += w;
    }

    let accent = if weight_sum > 0 {
        let mut ar = (r_sum / weight_sum) as f32;
        let mut ag = (g_sum / weight_sum) as f32;
        let mut ab = (b_sum / weight_sum) as f32;
        // Ensure high luminance so the waveform & progress bar pop against #08080A black
        let max_v = ar.max(ag).max(ab).max(1.0);
        if max_v < 215.0 {
            let scale = 215.0 / max_v;
            ar = (ar * scale).min(255.0);
            ag = (ag * scale).min(255.0);
            ab = (ab * scale).min(255.0);
        }
        // Softly blend 20% white so extremely saturated colors stay refined
        let r_out = (ar * 0.82 + 255.0 * 0.18).round() as u8;
        let g_out = (ag * 0.82 + 255.0 * 0.18).round() as u8;
        let b_out = (ab * 0.82 + 255.0 * 0.18).round() as u8;
        (r_out, g_out, b_out)
    } else {
        (245, 245, 247)
    };

    let mut px = Pixmap::new(size, size)?;
    let raw = rgba.into_raw();
    let dst = px.data_mut();
    for i in (0..raw.len()).step_by(4) {
        let a = raw[i + 3] as u32;
        dst[i] = ((raw[i] as u32 * a) / 255) as u8;
        dst[i + 1] = ((raw[i + 1] as u32 * a) / 255) as u8;
        dst[i + 2] = ((raw[i + 2] as u32 * a) / 255) as u8;
        dst[i + 3] = a as u8;
    }

    Some((px, accent))
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
