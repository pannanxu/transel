use std::borrow::Cow;
use std::path::Path;

use ab_glyph::{Font, FontArc, FontVec, Glyph, PxScale, ScaleFont, point};
use gpui_kit::*;
use image::{Rgba, RgbaImage};

use super::state::{DrawElement, PALETTE, Tool};

static SYSTEM_FONTS: once_cell::sync::Lazy<Vec<FontArc>> = once_cell::sync::Lazy::new(|| {
    let font_candidates = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Helvetica.ttc",
        "/Library/Fonts/Arial.ttf",
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\arial.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];

    let mut list = Vec::new();
    for p in &font_candidates {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(font) = FontArc::try_from_vec(bytes.clone()) {
                list.push(font);
            } else if let Ok(fv) = FontVec::try_from_vec_and_index(bytes, 0) {
                list.push(FontArc::new(fv));
            }
        }
    }
    list
});

pub fn composite_and_save(
    base_image_path: &Path,
    elements: &[DrawElement],
    img_win_origin: Point<Pixels>,
    disp_size: (f32, f32),
    out_path: Option<&Path>,
) -> anyhow::Result<RgbaImage> {
    let orig = image::open(base_image_path)?.to_rgba8();
    let (orig_w, orig_h) = (orig.width(), orig.height());
    let (disp_w, disp_h) = disp_size;

    let ratio_x = if disp_w > 0.0 {
        orig_w as f32 / disp_w
    } else {
        1.0
    };
    let ratio_y = if disp_h > 0.0 {
        orig_h as f32 / disp_h
    } else {
        1.0
    };
    let line_scale = ((ratio_x + ratio_y) / 2.0).max(1.0);

    let mut min_x = 0.0f32;
    let mut min_y = 0.0f32;
    let mut max_x = disp_w;
    let mut max_y = disp_h;

    for el in elements {
        let b = el.bounds();
        let x1 = f32::from(b.origin.x) - f32::from(img_win_origin.x);
        let y1 = f32::from(b.origin.y) - f32::from(img_win_origin.y);
        let x2 = x1 + f32::from(b.size.width);
        let y2 = y1 + f32::from(b.size.height);

        min_x = min_x.min(x1);
        min_y = min_y.min(y1);
        max_x = max_x.max(x2);
        max_y = max_y.max(y2);
    }

    let has_outer = min_x < 0.0 || min_y < 0.0 || max_x > disp_w || max_y > disp_h;
    if has_outer {
        if min_x < 0.0 {
            min_x -= 16.0;
        }
        if min_y < 0.0 {
            min_y -= 16.0;
        }
        if max_x > disp_w {
            max_x += 16.0;
        }
        if max_y > disp_h {
            max_y += 16.0;
        }
    } else {
        min_x = 0.0;
        min_y = 0.0;
        max_x = disp_w;
        max_y = disp_h;
    }

    let total_w = ((max_x - min_x) * ratio_x).round().max(1.0) as u32;
    let total_h = ((max_y - min_y) * ratio_y).round().max(1.0) as u32;

    let mut cropped = if has_outer {
        let mut canvas_img = RgbaImage::from_pixel(total_w, total_h, Rgba([0, 0, 0, 0]));
        let dst_x = ((-min_x) * ratio_x).round() as i64;
        let dst_y = ((-min_y) * ratio_y).round() as i64;
        image::imageops::overlay(&mut canvas_img, &orig, dst_x, dst_y);
        canvas_img
    } else {
        orig
    };

    let crop_w = cropped.width();
    let crop_h = cropped.height();

    let to_local = |p: Point<Pixels>| -> (f32, f32) {
        let lx = (f32::from(p.x) - f32::from(img_win_origin.x) - min_x) * ratio_x;
        let ly = (f32::from(p.y) - f32::from(img_win_origin.y) - min_y) * ratio_y;
        (lx, ly)
    };

    for el in elements {
        let (r, g, b, _) = PALETTE.get(el.color_idx).copied().unwrap_or(PALETTE[0]);
        let color = Rgba([r, g, b, 255]);
        let line_w = (el.line_width * line_scale).max(1.0);

        let (s_x, s_y) = to_local(el.start);
        let (e_x, e_y) = to_local(el.end);

        match el.tool {
            Tool::Select => {}
            Tool::Mosaic => {
                let min_px_x = s_x.min(e_x).max(0.0) as u32;
                let max_px_x = s_x.max(e_x).min(crop_w as f32) as u32;
                let min_px_y = s_y.min(e_y).max(0.0) as u32;
                let max_px_y = s_y.max(e_y).min(crop_h as f32) as u32;
                let block_px = (10.0f32 * line_scale).max(8.0f32).round() as u32;
                apply_mosaic(
                    &mut cropped,
                    min_px_x,
                    min_px_y,
                    max_px_x,
                    max_px_y,
                    block_px,
                );
            }
            Tool::Rect => {
                let min_x = s_x.min(e_x);
                let max_x = s_x.max(e_x);
                let min_y = s_y.min(e_y);
                let max_y = s_y.max(e_y);
                draw_rect(&mut cropped, min_x, min_y, max_x, max_y, line_w, color);
            }
            Tool::Circle => {
                let min_x = s_x.min(e_x);
                let max_x = s_x.max(e_x);
                let min_y = s_y.min(e_y);
                let max_y = s_y.max(e_y);
                draw_oval(&mut cropped, min_x, min_y, max_x, max_y, line_w, color);
            }
            Tool::Arrow => {
                draw_arrow(&mut cropped, s_x, s_y, e_x, e_y, line_w, color);
            }
            Tool::Pen => {
                if el.points.len() > 1 {
                    for pair in el.points.windows(2) {
                        let (p1x, p1y) = to_local(pair[0]);
                        let (p2x, p2y) = to_local(pair[1]);
                        draw_thick_line(&mut cropped, p1x, p1y, p2x, p2y, line_w, color);
                    }
                }
            }
            Tool::Text => {
                draw_text_on_image(
                    &mut cropped,
                    s_x,
                    s_y,
                    &el.text,
                    color,
                    el.font_size,
                    line_scale,
                );
            }
        }
    }

    if let Some(out) = out_path {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        cropped.save(out)?;
    }

    Ok(cropped)
}

pub fn copy_image_to_clipboard(image: &RgbaImage) -> anyhow::Result<()> {
    let mut cb = arboard::Clipboard::new()?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    cb.set_image(arboard::ImageData {
        width: w,
        height: h,
        bytes: Cow::Borrowed(image.as_raw()),
    })?;
    Ok(())
}

fn apply_mosaic(img: &mut RgbaImage, min_x: u32, min_y: u32, max_x: u32, max_y: u32, block: u32) {
    let (w, h) = (img.width(), img.height());
    let mut by = min_y;
    while by < max_y && by < h {
        let mut bx = min_x;
        while bx < max_x && bx < w {
            let bw = (block).min(max_x - bx).min(w - bx);
            let bh = (block).min(max_y - by).min(h - by);

            let mut r_sum = 0u64;
            let mut g_sum = 0u64;
            let mut b_sum = 0u64;
            let mut count = 0u64;

            for dy in 0..bh {
                for dx in 0..bw {
                    let p = img.get_pixel(bx + dx, by + dy);
                    r_sum += p[0] as u64;
                    g_sum += p[1] as u64;
                    b_sum += p[2] as u64;
                    count += 1;
                }
            }

            if let (Some(r), Some(g), Some(b)) = (
                r_sum.checked_div(count),
                g_sum.checked_div(count),
                b_sum.checked_div(count),
            ) {
                let avg = Rgba([r as u8, g as u8, b as u8, 255]);
                for dy in 0..bh {
                    for dx in 0..bw {
                        img.put_pixel(bx + dx, by + dy, avg);
                    }
                }
            }

            bx += block;
        }
        by += block;
    }
}

fn draw_rect(
    img: &mut RgbaImage,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    thickness: f32,
    color: Rgba<u8>,
) {
    draw_thick_line(img, min_x, min_y, max_x, min_y, thickness, color);
    draw_thick_line(img, max_x, min_y, max_x, max_y, thickness, color);
    draw_thick_line(img, max_x, max_y, min_x, max_y, thickness, color);
    draw_thick_line(img, min_x, max_y, min_x, min_y, thickness, color);
}

fn draw_oval(
    img: &mut RgbaImage,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    thickness: f32,
    color: Rgba<u8>,
) {
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    let rx = ((max_x - min_x) / 2.0).max(1.0);
    let ry = ((max_y - min_y) / 2.0).max(1.0);

    let steps = ((rx + ry) * 1.5).clamp(36.0, 720.0) as usize;
    let mut prev_x = cx + rx;
    let mut prev_y = cy;

    for i in 1..=steps {
        let theta = (i as f32 / steps as f32) * std::f32::consts::TAU;
        let curr_x = cx + rx * theta.cos();
        let curr_y = cy + ry * theta.sin();
        draw_thick_line(img, prev_x, prev_y, curr_x, curr_y, thickness, color);
        prev_x = curr_x;
        prev_y = curr_y;
    }
}

fn draw_arrow(
    img: &mut RgbaImage,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    color: Rgba<u8>,
) {
    draw_thick_line(img, x1, y1, x2, y2, thickness, color);

    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len > 5.0 {
        let head_len = (thickness * 4.5).min(len * 0.45).max(4.0);
        let angle = dy.atan2(dx);
        let arrow_angle = std::f32::consts::PI / 6.0;

        let p1x = x2 - head_len * (angle - arrow_angle).cos();
        let p1y = y2 - head_len * (angle - arrow_angle).sin();
        let p2x = x2 - head_len * (angle + arrow_angle).cos();
        let p2y = y2 - head_len * (angle + arrow_angle).sin();

        draw_thick_line(img, x2, y2, p1x, p1y, thickness, color);
        draw_thick_line(img, x2, y2, p2x, p2y, thickness, color);
        draw_thick_line(img, p1x, p1y, p2x, p2y, thickness, color);
    }
}

fn draw_thick_line(
    img: &mut RgbaImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: Rgba<u8>,
) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let dx = x1 - x0;
    let dy = y1 - y0;
    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
    let steps = (dist * 1.5).ceil() as usize;

    let rad = (thickness / 2.0).max(0.5);
    let rad_sq = rad * rad;
    let r_ceil = rad.ceil() as i32;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let cx = (x0 + t * dx).round() as i32;
        let cy = (y0 + t * dy).round() as i32;

        for ry in -r_ceil..=r_ceil {
            for rx in -r_ceil..=r_ceil {
                if (rx * rx + ry * ry) as f32 <= rad_sq {
                    let px = cx + rx;
                    let py = cy + ry;
                    if px >= 0 && px < w && py >= 0 && py < h {
                        img.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }
}

fn find_glyph_in_fonts(fonts: &[FontArc], ch: char) -> Option<(&FontArc, ab_glyph::GlyphId)> {
    for font in fonts {
        let id = font.glyph_id(ch);
        if id.0 != 0 {
            return Some((font, id));
        }
    }
    fonts.first().map(|f| (f, f.glyph_id(ch)))
}

fn draw_text_on_image(
    img: &mut RgbaImage,
    x: f32,
    y: f32,
    text: &str,
    color: Rgba<u8>,
    font_size: f32,
    scale: f32,
) {
    if text.is_empty() {
        return;
    }
    let effective_font_size = (font_size * scale).max(10.0);
    let px_scale = PxScale::from(effective_font_size);

    let fonts = &*SYSTEM_FONTS;
    if fonts.is_empty() {
        // Fallback: draw clean line if no system font could be loaded
        let line_y = (y + effective_font_size).min(img.height() as f32 - 1.0);
        let w = (text.chars().count() as f32 * effective_font_size * 0.6).max(20.0);
        draw_thick_line(
            img,
            x,
            line_y,
            (x + w).min(img.width() as f32 - 1.0),
            line_y,
            2.0,
            color,
        );
        return;
    }

    let primary_font = &fonts[0];
    let v_metrics = primary_font.as_scaled(px_scale).ascent();

    let mut cursor_x = x;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        if let Some((font, glyph_id)) = find_glyph_in_fonts(fonts, ch) {
            let scaled_font = font.as_scaled(px_scale);
            let advance = scaled_font.h_advance(glyph_id);
            let glyph = Glyph {
                id: glyph_id,
                scale: px_scale,
                position: point(cursor_x, y + v_metrics),
            };
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    if coverage > 0.02 {
                        let px = bounds.min.x as i32 + gx as i32;
                        let py = bounds.min.y as i32 + gy as i32;
                        if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32
                        {
                            let cur = img.get_pixel(px as u32, py as u32);
                            let alpha = (color[3] as f32 * coverage) / 255.0;
                            let inv_alpha = 1.0 - alpha;
                            let r =
                                (color[0] as f32 * alpha + cur[0] as f32 * inv_alpha).round() as u8;
                            let g =
                                (color[1] as f32 * alpha + cur[1] as f32 * inv_alpha).round() as u8;
                            let b =
                                (color[2] as f32 * alpha + cur[2] as f32 * inv_alpha).round() as u8;
                            let a = (255.0 * (alpha + (cur[3] as f32 / 255.0) * inv_alpha)).round()
                                as u8;
                            img.put_pixel(px as u32, py as u32, Rgba([r, g, b, a]));
                        }
                    }
                });
            }
            cursor_x += advance;
        } else {
            cursor_x += effective_font_size * 0.6;
        }
    }
}
