use gpui_kit::*;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotMode {
    /// 独立截图（默认 F1）：标注工具栏，回车复制到剪贴板，支持保存
    Annotate,
    /// 截图 OCR：框选后立即识别，打开翻译弹窗
    Ocr,
    /// 仅 OCR 文本（可选静默复制）
    OcrOnly { silent: bool },
    /// 原图覆盖翻译
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Rect,
    Circle,
    Arrow,
    Pen,
    Mosaic,
    Text,
}

impl Tool {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Select => "选择",
            Self::Rect => "矩形",
            Self::Circle => "椭圆",
            Self::Arrow => "箭头",
            Self::Pen => "画笔",
            Self::Mosaic => "马赛克",
            Self::Text => "文字",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Select => crate::shared::icons::HAND,
            Self::Rect => crate::shared::icons::SQUARE,
            Self::Circle => crate::shared::icons::CIRCLE,
            Self::Arrow => crate::shared::icons::ARROW_UP_RIGHT,
            Self::Pen => crate::shared::icons::PEN_LINE,
            Self::Mosaic => crate::shared::icons::GRID_2X2,
            Self::Text => crate::shared::icons::TYPE,
        }
    }
}

pub const HANDLE_HIT: f32 = 8.0;

pub const PALETTE: &[(u8, u8, u8, &str)] = &[
    (250, 59, 59, "红"),   // Red #FA3B3B
    (255, 149, 0, "橙"),   // Orange #FF9500
    (255, 204, 0, "黄"),   // Yellow #FFCC00
    (52, 199, 89, "绿"),   // Green #34C759
    (0, 122, 255, "蓝"),   // Blue #007AFF
    (255, 255, 255, "白"), // White #FFFFFF
];

pub fn palette_color(idx: usize) -> Hsla {
    let (r, g, b, _) = PALETTE.get(idx).copied().unwrap_or(PALETTE[0]);
    rgba((r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | 255).into()
}

#[derive(Debug, Clone)]
pub struct DrawElement {
    pub id: Uuid,
    pub tool: Tool,
    pub color_idx: usize,
    pub line_width: f32,
    pub font_size: f32,
    pub start: Point<Pixels>,
    pub end: Point<Pixels>,
    pub points: Vec<Point<Pixels>>,
    pub text: String,
}

pub fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    let mut w = 0.0f32;
    for ch in text.chars() {
        if ch.is_ascii() {
            w += font_size * 0.6;
        } else {
            w += font_size * 1.05;
        }
    }
    w.max(font_size * 1.5)
}

pub fn estimate_text_height(font_size: f32) -> f32 {
    (font_size * 1.25).max(18.0)
}

impl DrawElement {
    pub fn new(tool: Tool, color_idx: usize, line_width: f32, start: Point<Pixels>) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool,
            color_idx,
            line_width,
            font_size: 18.0,
            start,
            end: start,
            points: vec![start],
            text: String::new(),
        }
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        match self.tool {
            Tool::Pen => {
                if self.points.is_empty() {
                    return Bounds::new(self.start, size(px(10.), px(10.)));
                }
                let mut min_x = f32::from(self.points[0].x);
                let mut max_x = min_x;
                let mut min_y = f32::from(self.points[0].y);
                let mut max_y = min_y;
                for p in &self.points {
                    let (x, y) = (f32::from(p.x), f32::from(p.y));
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
                let pad = self.line_width.max(6.);
                Bounds::from_corners(
                    point(px(min_x - pad), px(min_y - pad)),
                    point(px(max_x + pad), px(max_y + pad)),
                )
            }
            Tool::Text => {
                let min_x = f32::from(self.start.x).min(f32::from(self.end.x));
                let max_x = f32::from(self.start.x).max(f32::from(self.end.x));
                let min_y = f32::from(self.start.y).min(f32::from(self.end.y));
                let max_y = f32::from(self.start.y).max(f32::from(self.end.y));
                let calc_w = estimate_text_width(&self.text, self.font_size);
                let calc_h = estimate_text_height(self.font_size);
                let w = (max_x - min_x).max(calc_w);
                let h = (max_y - min_y).max(calc_h);
                Bounds::new(point(px(min_x), px(min_y)), size(px(w), px(h)))
            }
            _ => {
                let min_x = f32::from(self.start.x).min(f32::from(self.end.x));
                let max_x = f32::from(self.start.x).max(f32::from(self.end.x));
                let min_y = f32::from(self.start.y).min(f32::from(self.end.y));
                let max_y = f32::from(self.start.y).max(f32::from(self.end.y));
                let pad = self.line_width.max(4.);
                Bounds::from_corners(
                    point(px(min_x - pad), px(min_y - pad)),
                    point(px(max_x + pad), px(max_y + pad)),
                )
            }
        }
    }

    pub fn hit_test(&self, p: Point<Pixels>, tolerance: f32) -> bool {
        let (px_x, px_y) = (f32::from(p.x), f32::from(p.y));
        match self.tool {
            Tool::Select => false,
            Tool::Rect | Tool::Mosaic => {
                let b = self.normalized_rect();
                let (min_x, max_x) = (f32::from(b.origin.x), f32::from(b.origin.x + b.size.width));
                let (min_y, max_y) = (f32::from(b.origin.y), f32::from(b.origin.y + b.size.height));
                // 边框附近或内部命中
                px_x >= min_x - tolerance
                    && px_x <= max_x + tolerance
                    && px_y >= min_y - tolerance
                    && px_y <= max_y + tolerance
            }
            Tool::Circle => {
                let b = self.normalized_rect();
                let cx = f32::from(b.origin.x + b.size.width / 2.0);
                let cy = f32::from(b.origin.y + b.size.height / 2.0);
                let rx = f32::from(b.size.width / 2.0).max(1.0);
                let ry = f32::from(b.size.height / 2.0).max(1.0);
                let dx = (px_x - cx) / (rx + tolerance);
                let dy = (px_y - cy) / (ry + tolerance);
                dx * dx + dy * dy <= 1.0
            }
            Tool::Arrow => {
                dist_to_segment(
                    px_x,
                    px_y,
                    f32::from(self.start.x),
                    f32::from(self.start.y),
                    f32::from(self.end.x),
                    f32::from(self.end.y),
                ) <= tolerance.max(self.line_width + 4.)
            }
            Tool::Pen => {
                if self.points.len() < 2 {
                    return false;
                }
                for window in self.points.windows(2) {
                    let d = dist_to_segment(
                        px_x,
                        px_y,
                        f32::from(window[0].x),
                        f32::from(window[0].y),
                        f32::from(window[1].x),
                        f32::from(window[1].y),
                    );
                    if d <= tolerance.max(self.line_width + 4.) {
                        return true;
                    }
                }
                false
            }
            Tool::Text => {
                let b = self.bounds();
                let min_x = f32::from(b.origin.x);
                let max_x = f32::from(b.origin.x + b.size.width);
                let min_y = f32::from(b.origin.y);
                let max_y = f32::from(b.origin.y + b.size.height);
                px_x >= min_x - tolerance
                    && px_x <= max_x + tolerance
                    && px_y >= min_y - tolerance
                    && px_y <= max_y + tolerance
            }
        }
    }

    pub fn normalized_rect(&self) -> Bounds<Pixels> {
        let min_x = f32::from(self.start.x).min(f32::from(self.end.x));
        let max_x = f32::from(self.start.x).max(f32::from(self.end.x));
        let min_y = f32::from(self.start.y).min(f32::from(self.end.y));
        let max_y = f32::from(self.start.y).max(f32::from(self.end.y));
        Bounds::new(
            point(px(min_x), px(min_y)),
            size(px(max_x - min_x), px(max_y - min_y)),
        )
    }

    pub fn translate(&mut self, dx: Pixels, dy: Pixels) {
        self.start.x += dx;
        self.start.y += dy;
        self.end.x += dx;
        self.end.y += dy;
        for p in &mut self.points {
            p.x += dx;
            p.y += dy;
        }
    }
}

fn dist_to_segment(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let l2 = (x2 - x1).powi(2) + (y2 - y1).powi(2);
    if l2 == 0.0 {
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }
    let t = (((px - x1) * (x2 - x1) + (py - y1) * (y2 - y1)) / l2).clamp(0.0, 1.0);
    let proj_x = x1 + t * (x2 - x1);
    let proj_y = y1 + t * (y2 - y1);
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Inside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementHandle {
    Start,
    End,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Body,
}
