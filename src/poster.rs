use crate::app::StatsResponse;
use gpui::RenderImage;
use image::{Frame, ImageBuffer, ImageEncoder, Rgba};
use smallvec::SmallVec;
use std::sync::Arc;
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point,
    RadialGradient, SpreadMode, Stroke, Transform,
};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GetDC,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CLEARTYPE_NATURAL_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
    FF_DONTCARE, HFONT, OUT_TT_PRECIS, TRANSPARENT,
};

pub const POSTER_W: u32 = 750;
pub const POSTER_H: u32 = 1000;

pub struct WrappedPoster {
    pub png_bytes: Vec<u8>,
    pub render_image: Arc<RenderImage>,
}

fn build_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w * 0.5).min(h * 0.5);
    let k = 0.55228475 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(
        x + w,
        y + h - r + k,
        x + w - r + k,
        y + h,
        x + w - r,
        y + h,
    );
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

unsafe fn make_font(height_px: i32, weight: i32, italic: u32) -> HFONT {
    CreateFontW(
        -height_px,
        0,
        0,
        0,
        weight,
        italic,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_TT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_NATURAL_QUALITY as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Microsoft YaHei UI"),
    )
}

const fn rgb_to_colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

pub fn render_wrapped_poster(d: &StatsResponse, days: i64) -> WrappedPoster {
    let w = POSTER_W;
    let h = POSTER_H;
    let mut pixmap = Pixmap::new(w, h).expect("Failed to allocate poster pixmap");

    // 1. Deep dark ambient background gradient
    let mut bg_paint = Paint::default();
    bg_paint.shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(w as f32, h as f32),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(9, 13, 19, 255)),
            GradientStop::new(0.5, Color::from_rgba8(18, 23, 32, 255)),
            GradientStop::new(1.0, Color::from_rgba8(22, 27, 36, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32) {
        pixmap.fill_rect(rect, &bg_paint, Transform::identity(), None);
    }

    // 2. Red & Blue Ambient Radial Glows
    if let Some(red_shader) = RadialGradient::new(
        Point::from_xy(620.0, 160.0),
        Point::from_xy(620.0, 160.0),
        320.0,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(232, 65, 66, 72)),
            GradientStop::new(0.6, Color::from_rgba8(232, 65, 66, 20)),
            GradientStop::new(1.0, Color::from_rgba8(232, 65, 66, 0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let mut p = Paint::default();
        p.shader = red_shader;
        if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32) {
            pixmap.fill_rect(rect, &p, Transform::identity(), None);
        }
    }

    if let Some(blue_shader) = RadialGradient::new(
        Point::from_xy(120.0, 840.0),
        Point::from_xy(120.0, 840.0),
        340.0,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(56, 139, 253, 56)),
            GradientStop::new(0.7, Color::from_rgba8(56, 139, 253, 14)),
            GradientStop::new(1.0, Color::from_rgba8(56, 139, 253, 0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let mut p = Paint::default();
        p.shader = blue_shader;
        if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32) {
            pixmap.fill_rect(rect, &p, Transform::identity(), None);
        }
    }

    // 3. Outer border frame
    if let Some(frame_path) = build_rounded_rect(24.0, 24.0, (w - 48) as f32, (h - 48) as f32, 28.0)
    {
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(255, 255, 255, 26);
        border_paint.anti_alias = true;
        let mut stroke = Stroke::default();
        stroke.width = 2.0;
        pixmap.stroke_path(
            &frame_path,
            &border_paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }

    // Divider lines
    let mut div_paint = Paint::default();
    div_paint.set_color_rgba8(255, 255, 255, 22);
    if let Some(r) = tiny_skia::Rect::from_xywh(54.0, 98.0, (w - 108) as f32, 1.0) {
        pixmap.fill_rect(r, &div_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(54.0, 930.0, (w - 108) as f32, 1.0) {
        pixmap.fill_rect(r, &div_paint, Transform::identity(), None);
    }

    // 6. Core 4 Metric Boxes (2x2 Grid)
    let box_w = 305.0;
    let box_h = 96.0;
    let gap_x = 32.0;
    let gap_y = 16.0;
    let start_x = 54.0;
    let start_y = 226.0;
    for i in 0..4 {
        let col = (i % 2) as f32;
        let row = (i / 2) as f32;
        let bx = start_x + col * (box_w + gap_x);
        let by = start_y + row * (box_h + gap_y);
        if let Some(p) = build_rounded_rect(bx, by, box_w, box_h, 14.0) {
            let mut fill = Paint::default();
            fill.set_color_rgba8(22, 27, 34, 185);
            fill.anti_alias = true;
            pixmap.fill_path(&p, &fill, FillRule::Winding, Transform::identity(), None);

            let mut stroke_p = Paint::default();
            stroke_p.set_color_rgba8(255, 255, 255, 22);
            stroke_p.anti_alias = true;
            let mut s = Stroke::default();
            s.width = 1.0;
            pixmap.stroke_path(&p, &stroke_p, &s, Transform::identity(), None);
        }
    }

    // 7. Top 3 Artist rows background
    let art_y = 470.0;
    let arts_len = d.top_artists.len().min(3);
    for i in 0..arts_len {
        let ay = art_y + 22.0 + (i as f32) * 58.0;
        if let Some(p) = build_rounded_rect(54.0, ay, (w - 108) as f32, 48.0, 10.0) {
            let mut fill = Paint::default();
            fill.set_color_rgba8(22, 27, 34, 160);
            fill.anti_alias = true;
            pixmap.fill_path(&p, &fill, FillRule::Winding, Transform::identity(), None);

            let mut stroke_p = Paint::default();
            stroke_p.set_color_rgba8(255, 255, 255, 18);
            stroke_p.anti_alias = true;
            let mut s = Stroke::default();
            s.width = 1.0;
            pixmap.stroke_path(&p, &stroke_p, &s, Transform::identity(), None);
        }
    }

    // 8. Loved track box
    let loved_box_y = 710.0;
    if let Some(p) = build_rounded_rect(54.0, loved_box_y, (w - 108) as f32, 64.0, 12.0) {
        let mut fill = Paint::default();
        fill.set_color_rgba8(46, 160, 67, 28);
        fill.anti_alias = true;
        pixmap.fill_path(&p, &fill, FillRule::Winding, Transform::identity(), None);

        let mut stroke_p = Paint::default();
        stroke_p.set_color_rgba8(46, 160, 67, 80);
        stroke_p.anti_alias = true;
        let mut s = Stroke::default();
        s.width = 1.2;
        pixmap.stroke_path(&p, &stroke_p, &s, Transform::identity(), None);
    }

    // 9. Recs box
    let rec_box_y = 828.0;
    if let Some(p) = build_rounded_rect(54.0, rec_box_y, (w - 108) as f32, 56.0, 12.0) {
        let mut fill = Paint::default();
        fill.set_color_rgba8(56, 139, 253, 24);
        fill.anti_alias = true;
        pixmap.fill_path(&p, &fill, FillRule::Winding, Transform::identity(), None);

        let mut stroke_p = Paint::default();
        stroke_p.set_color_rgba8(56, 139, 253, 68);
        stroke_p.anti_alias = true;
        let mut s = Stroke::default();
        s.width = 1.2;
        pixmap.stroke_path(&p, &stroke_p, &s, Transform::identity(), None);
    }

    // Determine persona & metrics
    let mut peak_h: i64 = 15;
    let mut peak_cnt: i64 = 0;
    for hr in 0..24 {
        let cnt = d.hours.get(&hr).copied().unwrap_or(0);
        if cnt > peak_cnt {
            peak_cnt = cnt;
            peak_h = hr;
        }
    }
    let (persona, quote) = if (0..6).contains(&peak_h) {
        (
            "🌙 深夜造梦家",
            "“当世界进入沉睡，你的灵魂在旋律中肆意漫游”",
        )
    } else if (6..12).contains(&peak_h) {
        (
            "🌅 晨光独奏者",
            "“清晨的第一缕微光，伴随跃动的音符一同苏醒”",
        )
    } else if (12..18).contains(&peak_h) {
        ("☀️ 午后心流体", "“沉浸在专注的节奏中，把灵感酿成现实”")
    } else {
        ("🌆 暮光旅人", "“卸下白昼的喧嚣，在晚风里与好歌同频共振”")
    };

    let mins = ((d.totals.ms as f64) / 60000.0).round() as i64;
    let dur_text = if mins >= 60 {
        format!("{:.1} 小时", (mins as f64) / 60.0)
    } else {
        format!("{} 分钟", mins)
    };
    let comp_rate = if d.totals.n > 0 {
        ((d.totals.c as f64 / d.totals.n as f64) * 100.0).round() as i64
    } else {
        0
    };

    let boxes = [
        (
            "累计播放曲目",
            format!("{} 首", d.totals.n),
            "SMTC 全程无感监听".to_string(),
        ),
        (
            "专注听歌时长",
            dur_text,
            format!("沉浸式陪伴 {} 分钟", mins),
        ),
        (
            "完整听完率",
            format!("{}%", comp_rate),
            format!("完播 ≥85% 达成 {} 次", d.totals.c),
        ),
        (
            "生物钟活跃峰值",
            format!("{:02}:00", peak_h),
            format!("单小时累计播放 {} 首", peak_cnt),
        ),
    ];

    // Copy pixmap into GDI DIBSection to render crisp ClearType typography
    let rgba_out: Vec<u8> = unsafe {
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(screen_dc);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // Top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbitmap =
            CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0).unwrap();
        let old_bmp = SelectObject(mem_dc, hbitmap);

        let src = pixmap.data();
        let dst = std::slice::from_raw_parts_mut(bits_ptr as *mut u8, (w * h * 4) as usize);
        for i in 0..(w * h) as usize {
            let idx = i * 4;
            dst[idx] = src[idx + 2]; // B
            dst[idx + 1] = src[idx + 1]; // G
            dst[idx + 2] = src[idx]; // R
            dst[idx + 3] = 255; // Opaque poster
        }

        SetBkMode(mem_dc, TRANSPARENT);

        let f_brand = make_font(18, 700, 0);
        let f_date = make_font(14, 500, 0);
        let f_persona = make_font(36, 700, 0);
        let f_quote = make_font(16, 400, 1);
        let f_sec = make_font(18, 700, 0);
        let f_val = make_font(26, 700, 0);
        let f_body_b = make_font(16, 700, 0);
        let f_body = make_font(14, 500, 0);
        let f_sub = make_font(12, 400, 0);

        let draw_str =
            |font: HFONT, col: COLORREF, left: i32, top: i32, right: i32, bottom: i32, text: &str, align_right: bool| {
                SelectObject(mem_dc, font);
                SetTextColor(mem_dc, col);
                let mut utf16: Vec<u16> = text.encode_utf16().collect();
                let mut rc = RECT {
                    left,
                    top,
                    right,
                    bottom,
                };
                let flags = DT_VCENTER
                    | DT_SINGLELINE
                    | DT_END_ELLIPSIS
                    | DT_NOPREFIX
                    | if align_right { DT_RIGHT } else { DT_LEFT };
                DrawTextW(mem_dc, &mut utf16, &mut rc, flags);
            };

        let c_red = rgb_to_colorref(232, 65, 66);
        let c_white = rgb_to_colorref(255, 255, 255);
        let c_text = rgb_to_colorref(230, 237, 243);
        let c_muted = rgb_to_colorref(139, 148, 158);
        let c_dim = rgb_to_colorref(110, 118, 129);
        let c_green = rgb_to_colorref(86, 211, 100);
        let c_blue = rgb_to_colorref(56, 139, 253);

        // Header
        draw_str(
            f_brand,
            c_red,
            54,
            52,
            500,
            88,
            "🎵 MUSIC HUB · 听歌画像报告",
            false,
        );
        let period_str = if days >= 365 {
            "全部历史".to_string()
        } else {
            format!("近 {} 天", days)
        };
        let date_str = chrono::Local::now().format("%Y.%m.%d").to_string();
        draw_str(
            f_date,
            c_muted,
            400,
            52,
            (w as i32) - 54,
            88,
            &format!("{} · {}", period_str, date_str),
            true,
        );

        // Persona & quote
        draw_str(f_persona, c_white, 54, 118, (w as i32) - 54, 170, persona, false);
        draw_str(f_quote, c_muted, 54, 174, (w as i32) - 54, 206, quote, false);

        // 4 Metric boxes
        for (i, (label, val, sub)) in boxes.iter().enumerate() {
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            let bx = 54 + col * (305 + 32);
            let by = 226 + row * (96 + 16);
            draw_str(f_body, c_muted, bx + 16, by + 8, bx + 290, by + 34, label, false);
            draw_str(f_val, c_text, bx + 16, by + 34, bx + 290, by + 70, val, false);
            draw_str(f_sub, c_dim, bx + 16, by + 68, bx + 290, by + 92, sub, false);
        }

        // Top 3 Artists
        draw_str(
            f_sec,
            c_white,
            54,
            446,
            500,
            480,
            "🎤 灵魂常听音乐人 TOP 3",
            false,
        );
        let medals = ["🥇", "🥈", "🥉"];
        if d.top_artists.is_empty() {
            draw_str(
                f_body,
                c_dim,
                54,
                492,
                500,
                526,
                "暂无足够播放记录",
                false,
            );
        } else {
            for (i, a) in d.top_artists.iter().take(3).enumerate() {
                let ay = 492 + (i as i32) * 58;
                draw_str(
                    f_body_b,
                    c_white,
                    70,
                    ay,
                    480,
                    ay + 48,
                    &format!("{}  {}", medals[i], a.artist),
                    false,
                );
                let mins_a = ((a.ms as f64) / 60000.0).round() as i64;
                draw_str(
                    f_body,
                    c_muted,
                    420,
                    ay,
                    (w as i32) - 74,
                    ay + 48,
                    &format!("收听 {} 首 · {} 分钟", a.n, mins_a),
                    true,
                );
            }
        }

        // Loved single
        draw_str(
            f_sec,
            c_green,
            54,
            668,
            500,
            702,
            "💚 本期真爱单曲（完播率最高）",
            false,
        );
        if let Some(loved) = d.loved.first() {
            draw_str(
                f_body_b,
                c_white,
                74,
                716,
                520,
                744,
                &loved.title,
                false,
            );
            draw_str(
                f_body,
                c_muted,
                74,
                744,
                520,
                768,
                &format!("{} · 完整听完 {} 次", loved.artist, loved.c),
                false,
            );
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
        } else {
            draw_str(
                f_body,
                c_dim,
                74,
                710,
                500,
                774,
                "暂无真爱单曲记录",
                false,
            );
        }

        // Recommendation stats
        draw_str(
            f_sec,
            c_blue,
            54,
            788,
            500,
            820,
            "🎯 智能推荐闭环满意度",
            false,
        );
        let rec_text = if d.recs.total > 0 {
            format!(
                "系统共推荐 {} 首 · 试听转化 {:.1}% · 完播满意度 {:.1}%",
                d.recs.total, d.recs.conversion_rate, d.recs.hit_rate
            )
        } else {
            "正在积累推荐命中池 · 探索更多单曲雷达与全盘漫游即可解锁".to_string()
        };
        draw_str(
            f_body,
            c_text,
            74,
            828,
            (w as i32) - 74,
            884,
            &rec_text,
            false,
        );

        // Footer watermark
        draw_str(
            f_sub,
            c_dim,
            54,
            940,
            400,
            972,
            "私人音乐中枢 · Music Hub 原生引擎出品",
            false,
        );
        draw_str(
            f_sub,
            c_dim,
            400,
            940,
            (w as i32) - 54,
            972,
            "本地私密存储 · 零云端依赖",
            true,
        );

        // Convert BGRA DIB back to RGBA for PNG & GPUI RenderImage
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for i in 0..(w * h) as usize {
            let idx = i * 4;
            rgba[idx] = dst[idx + 2]; // R
            rgba[idx + 1] = dst[idx + 1]; // G
            rgba[idx + 2] = dst[idx]; // B
            rgba[idx + 3] = 255; // A
        }

        SelectObject(mem_dc, old_bmp);
        for f in [
            f_brand, f_date, f_persona, f_quote, f_sec, f_val, f_body_b, f_body, f_sub,
        ] {
            let _ = DeleteObject(f);
        }
        let _ = DeleteObject(hbitmap);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        rgba
    };

    // Encode PNG bytes
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    let _ = encoder.write_image(&rgba_out, w, h, image::ExtendedColorType::Rgba8);

    // Build GPUI RenderImage (note: GPUI expects BGRA in Frame!)
    let mut bgra_buf = rgba_out;
    for chunk in bgra_buf.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    let img_buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w, h, bgra_buf).expect("Valid poster buffer");
    let render_image = Arc::new(RenderImage::new(SmallVec::from_elem(
        Frame::new(img_buf),
        1,
    )));

    WrappedPoster {
        png_bytes,
        render_image,
    }
}
