use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::Icon;
use gpui_kit::*;
use uuid::Uuid;

use super::crop::{composite_and_save, copy_image_to_clipboard};
use super::state::{
    DrawElement, ElementHandle, HANDLE_HIT, PALETTE, Tool, estimate_text_height,
    estimate_text_width, palette_color,
};
use crate::shared::actions::CloseWindow;
use crate::shared::icons;

#[derive(Debug, Clone)]
enum Dragging {
    Drawing,
    Moving {
        id: Uuid,
        initial_start: Point<Pixels>,
        initial_end: Point<Pixels>,
        initial_points: Vec<Point<Pixels>>,
        start: Point<Pixels>,
    },
    Resizing {
        id: Uuid,
        handle: ElementHandle,
        initial_start: Point<Pixels>,
        initial_end: Point<Pixels>,
        initial_font_size: f32,
    },
    Panning {
        start_mouse: Point<Pixels>,
        initial_pan: Point<Pixels>,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ImageLayout {
    pub container_origin: Point<Pixels>,
    pub img_win_origin: Point<Pixels>,
    pub disp_size: (f32, f32),
    pub fit_scale: f32,
}

pub struct ScreenshotView {
    pub image_path: PathBuf,
    pub image_size: (f32, f32), // (logical_w, logical_h)
    pub scale_factor: f32,
    pub base_image: Option<Arc<image::RgbaImage>>,
    pub image_layout: Arc<parking_lot::Mutex<ImageLayout>>,

    pub elements: Vec<DrawElement>,
    pub current_element: Option<DrawElement>,
    pub selected_element_id: Option<Uuid>,

    pub active_tool: Tool,
    pub active_color_idx: usize,
    pub line_width: f32,

    pub pan_offset: Point<Pixels>,
    pub zoom: f32,
    pub is_space_down: bool,

    pub focus: FocusHandle,
    dragging: Option<Dragging>,

    // 内联文本编辑状态 (坐标, 当前输入文本, 若是二次编辑则是已有的图元 id)
    pub editing_text: Option<(Point<Pixels>, String, Option<Uuid>)>,

    undo_stack: Vec<Vec<DrawElement>>,
    window_handle: Option<AnyWindowHandle>,
}

impl ScreenshotView {
    pub fn new(
        image_path: PathBuf,
        image_size: (f32, f32),
        scale_factor: f32,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let base_image = image::open(&image_path)
            .ok()
            .map(|img| Arc::new(img.to_rgba8()));

        Self {
            image_path,
            image_size,
            scale_factor,
            base_image,
            image_layout: Arc::new(parking_lot::Mutex::new(ImageLayout::default())),
            elements: Vec::new(),
            current_element: None,
            selected_element_id: None,
            active_tool: Tool::Rect,
            active_color_idx: 0,
            line_width: 3.0,
            pan_offset: point(px(0.0), px(0.0)),
            zoom: 1.0,
            is_space_down: false,
            focus: cx.focus_handle(),
            dragging: None,
            editing_text: None,
            undo_stack: Vec::new(),
            window_handle: None,
        }
    }

    pub fn screen_to_world(&self, p: Point<Pixels>, layout: &ImageLayout) -> Point<Pixels> {
        let scale = layout.fit_scale.max(0.0001);
        let wx = (f32::from(p.x) - f32::from(layout.img_win_origin.x)) / scale;
        let wy = (f32::from(p.y) - f32::from(layout.img_win_origin.y)) / scale;
        point(px(wx), px(wy))
    }

    pub fn world_to_screen(&self, wp: Point<Pixels>, layout: &ImageLayout) -> Point<Pixels> {
        let scale = layout.fit_scale;
        let sx = f32::from(layout.img_win_origin.x) + f32::from(wp.x) * scale;
        let sy = f32::from(layout.img_win_origin.y) + f32::from(wp.y) * scale;
        point(px(sx), px(sy))
    }

    pub fn zoom_at(&mut self, factor: f32, cursor: Point<Pixels>) {
        let old_zoom = self.zoom;
        let new_zoom = (old_zoom * factor).clamp(0.1, 8.0);
        if (new_zoom - old_zoom).abs() < 0.001 {
            return;
        }
        let layout = *self.image_layout.lock();
        let img_origin = layout.img_win_origin;
        let base_scale = (layout.fit_scale / old_zoom).max(0.0001);
        let old_effective_scale = (base_scale * old_zoom).max(0.0001);
        let new_effective_scale = (base_scale * new_zoom).max(0.0001);

        let world_x = (f32::from(cursor.x) - f32::from(img_origin.x)) / old_effective_scale;
        let world_y = (f32::from(cursor.y) - f32::from(img_origin.y)) / old_effective_scale;

        let new_img_x = f32::from(cursor.x) - world_x * new_effective_scale;
        let new_img_y = f32::from(cursor.y) - world_y * new_effective_scale;

        let delta_x = new_img_x - f32::from(img_origin.x);
        let delta_y = new_img_y - f32::from(img_origin.y);

        self.zoom = new_zoom;
        self.pan_offset.x += px(delta_x);
        self.pan_offset.y += px(delta_y);
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        let layout = *self.image_layout.lock();
        let center = point(
            layout.container_origin.x + px(layout.disp_size.0 / 2.0),
            layout.container_origin.y + px(layout.disp_size.1 / 2.0),
        );
        self.zoom_at(1.2, center);
        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        let layout = *self.image_layout.lock();
        let center = point(
            layout.container_origin.x + px(layout.disp_size.0 / 2.0),
            layout.container_origin.y + px(layout.disp_size.1 / 2.0),
        );
        self.zoom_at(0.833, center);
        cx.notify();
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.zoom = 1.0;
        self.pan_offset = point(px(0.0), px(0.0));
        cx.notify();
    }

    pub fn handle_close(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }

    pub fn set_window_handle(&mut self, handle: AnyWindowHandle) {
        self.window_handle = Some(handle);
    }

    pub fn close(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        *super::SCREENSHOT_WINDOW.lock() = None;
        self.base_image = None;
        self.elements.clear();
        self.undo_stack.clear();
        self.current_element = None;
        if self.image_path.exists() {
            let _ = std::fs::remove_file(&self.image_path);
        }
        window.remove_window();
    }

    pub fn cursor_style(&self) -> CursorStyle {
        if self.is_space_down {
            return CursorStyle::PointingHand;
        }
        match self.active_tool {
            Tool::Select => CursorStyle::PointingHand,
            Tool::Text => CursorStyle::IBeam,
            _ => CursorStyle::Crosshair,
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.elements.clone());
        if self.undo_stack.len() > 30 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.commit_inline_text();
        if let Some(prev) = self.undo_stack.pop() {
            self.elements = prev;
            self.selected_element_id = None;
            cx.notify();
        }
    }

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_element_id.take() {
            self.push_undo();
            self.elements.retain(|el| el.id != id);
            cx.notify();
        }
    }

    pub fn commit_and_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_inline_text();

        match composite_and_save(
            &self.image_path,
            &self.elements,
            point(px(0.0), px(0.0)),
            self.image_size,
            None,
        ) {
            Ok(cropped) => {
                if let Err(e) = copy_image_to_clipboard(&cropped) {
                    log::warn!("写入剪贴板失败: {e:#}");
                }
                log::info!("截图已成功复制到剪贴板（纯内存，不写磁盘）");
            }
            Err(e) => {
                log::error!("合成截图失败: {e:#}");
            }
        }

        self.close(window, cx);
    }

    pub fn save_as_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_inline_text();
        let save_dir = {
            let cfg = cx.global::<crate::app::state::AppState>().config();
            cfg.resolve_screenshot_save_dir()
        };
        let _ = std::fs::create_dir_all(&save_dir);

        let filename = format!(
            "Screenshot_{}.png",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        let save_path = save_dir.join(filename);

        match composite_and_save(
            &self.image_path,
            &self.elements,
            point(px(0.0), px(0.0)),
            self.image_size,
            Some(&save_path),
        ) {
            Ok(cropped) => {
                let _ = copy_image_to_clipboard(&cropped);
                log::info!("截图已直接保存至: {}", save_path.display());
            }
            Err(e) => {
                log::error!("保存截图失败: {e:#}");
            }
        }

        self.close(window, cx);
    }

    pub fn commit_inline_text(&mut self) {
        if let Some((pos, text, prev_id)) = self.editing_text.take() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                self.push_undo();
                if let Some(id) = prev_id {
                    if let Some(el) = self.elements.iter_mut().find(|el| el.id == id) {
                        el.text = trimmed.to_string();
                        let w = estimate_text_width(&el.text, el.font_size);
                        let h = estimate_text_height(el.font_size);
                        el.end = point(el.start.x + px(w), el.start.y + px(h));
                    }
                } else {
                    let mut el =
                        DrawElement::new(Tool::Text, self.active_color_idx, self.line_width, pos);
                    el.font_size = 18.0;
                    el.text = trimmed.to_string();
                    let w = estimate_text_width(&el.text, el.font_size);
                    let h = estimate_text_height(el.font_size);
                    el.end = point(pos.x + px(w), pos.y + px(h));
                    self.elements.push(el);
                }
            } else if let Some(id) = prev_id {
                self.push_undo();
                self.elements.retain(|el| el.id != id);
            }
        }
    }

    fn element_handle_at(
        &self,
        el: &DrawElement,
        p: Point<Pixels>,
        layout: &ImageLayout,
    ) -> Option<ElementHandle> {
        let px_x = f32::from(p.x);
        let px_y = f32::from(p.y);

        let check = |hx: f32, hy: f32| -> bool {
            (px_x - hx).abs() <= HANDLE_HIT && (px_y - hy).abs() <= HANDLE_HIT
        };

        let to_s = |wp: Point<Pixels>| self.world_to_screen(wp, layout);

        match el.tool {
            Tool::Arrow => {
                let s = to_s(el.start);
                let e = to_s(el.end);
                if check(f32::from(s.x), f32::from(s.y)) {
                    return Some(ElementHandle::Start);
                }
                if check(f32::from(e.x), f32::from(e.y)) {
                    return Some(ElementHandle::End);
                }
            }
            Tool::Rect | Tool::Circle | Tool::Mosaic | Tool::Text => {
                let b = el.bounds();
                let tl = to_s(point(b.origin.x, b.origin.y));
                let tr = to_s(point(b.origin.x + b.size.width, b.origin.y));
                let br = to_s(point(b.origin.x + b.size.width, b.origin.y + b.size.height));
                let bl = to_s(point(b.origin.x, b.origin.y + b.size.height));

                if check(f32::from(tl.x), f32::from(tl.y)) {
                    return Some(ElementHandle::TopLeft);
                }
                if check(f32::from(tr.x), f32::from(tr.y)) {
                    return Some(ElementHandle::TopRight);
                }
                if check(f32::from(br.x), f32::from(br.y)) {
                    return Some(ElementHandle::BottomRight);
                }
                if check(f32::from(bl.x), f32::from(bl.y)) {
                    return Some(ElementHandle::BottomLeft);
                }
            }
            _ => {}
        }

        let world_p = self.screen_to_world(p, layout);
        let tol = (8.0 / layout.fit_scale).max(4.0);
        if el.hit_test(world_p, tol) {
            Some(ElementHandle::Body)
        } else {
            None
        }
    }

    fn handle_mouse_down(&mut self, p: Point<Pixels>, button: MouseButton, cx: &mut Context<Self>) {
        if self.editing_text.is_some() {
            self.commit_inline_text();
            cx.notify();
            return;
        }

        // 快捷平移画布：中键或按住空格键时，直接触发拖动画布
        if button == MouseButton::Middle || self.is_space_down {
            self.dragging = Some(Dragging::Panning {
                start_mouse: p,
                initial_pan: self.pan_offset,
            });
            cx.notify();
            return;
        }

        let layout = *self.image_layout.lock();
        let world_p = self.screen_to_world(p, &layout);

        // 选择/抓手模式下检查是否命中图元
        if self.active_tool == Tool::Select {
            // 先检查当前选中的图元手柄
            let resizing_target = self.selected_element_id.and_then(|sel_id| {
                let el = self.elements.iter().find(|el| el.id == sel_id)?;
                let handle = self.element_handle_at(el, p, &layout)?;
                if handle != ElementHandle::Body {
                    Some((sel_id, handle, el.start, el.end, el.font_size))
                } else {
                    None
                }
            });
            if let Some((id, handle, initial_start, initial_end, initial_font_size)) =
                resizing_target
            {
                self.dragging = Some(Dragging::Resizing {
                    id,
                    handle,
                    initial_start,
                    initial_end,
                    initial_font_size,
                });
                cx.notify();
                return;
            }

            for el in self.elements.iter().rev() {
                if let Some(handle) = self.element_handle_at(el, p, &layout) {
                    let id = el.id;
                    self.selected_element_id = Some(id);
                    if handle == ElementHandle::Body {
                        self.dragging = Some(Dragging::Moving {
                            id,
                            initial_start: el.start,
                            initial_end: el.end,
                            initial_points: el.points.clone(),
                            start: world_p,
                        });
                    } else {
                        self.dragging = Some(Dragging::Resizing {
                            id,
                            handle,
                            initial_start: el.start,
                            initial_end: el.end,
                            initial_font_size: el.font_size,
                        });
                    }
                    cx.notify();
                    return;
                }
            }

            // 抓手模式在空白区域点击拖拽平移画布
            self.selected_element_id = None;
            self.dragging = Some(Dragging::Panning {
                start_mouse: p,
                initial_pan: self.pan_offset,
            });
            cx.notify();
            return;
        }

        // 文本工具
        if self.active_tool == Tool::Text {
            self.editing_text = Some((world_p, String::new(), None));
            cx.notify();
            return;
        }

        // 绘制图元工具
        let mut el = DrawElement::new(
            self.active_tool,
            self.active_color_idx,
            self.line_width,
            world_p,
        );
        if self.active_tool == Tool::Pen {
            el.points.push(world_p);
        }
        self.current_element = Some(el);
        self.dragging = Some(Dragging::Drawing);
        cx.notify();
    }

    fn handle_mouse_move(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(ref dragging) = self.dragging.clone() {
            let layout = *self.image_layout.lock();
            let world_p = self.screen_to_world(p, &layout);

            match dragging {
                Dragging::Panning {
                    start_mouse,
                    initial_pan,
                } => {
                    let dx = p.x - start_mouse.x;
                    let dy = p.y - start_mouse.y;
                    self.pan_offset = point(initial_pan.x + dx, initial_pan.y + dy);
                    cx.notify();
                }
                Dragging::Drawing => {
                    if let Some(ref mut el) = self.current_element {
                        el.end = world_p;
                        if el.tool == Tool::Pen {
                            el.points.push(world_p);
                        }
                        cx.notify();
                    }
                }
                Dragging::Moving {
                    id,
                    initial_start,
                    initial_end,
                    initial_points,
                    start,
                } => {
                    let dx = world_p.x - start.x;
                    let dy = world_p.y - start.y;
                    if let Some(el) = self.elements.iter_mut().find(|el| el.id == *id) {
                        el.start = point(initial_start.x + dx, initial_start.y + dy);
                        el.end = point(initial_end.x + dx, initial_end.y + dy);
                        if !initial_points.is_empty() {
                            el.points = initial_points
                                .iter()
                                .map(|pt| point(pt.x + dx, pt.y + dy))
                                .collect();
                        }
                        cx.notify();
                    }
                }
                Dragging::Resizing {
                    id,
                    handle,
                    initial_start,
                    initial_end,
                    initial_font_size,
                } => {
                    if let Some(el) = self.elements.iter_mut().find(|el| el.id == *id) {
                        if el.tool == Tool::Text {
                            let initial_w =
                                f32::from(initial_end.x - initial_start.x).abs().max(20.0);
                            let initial_h =
                                f32::from(initial_end.y - initial_start.y).abs().max(16.0);
                            let diag_sq = initial_w * initial_w + initial_h * initial_h;

                            let (pin, dx, dy) = match handle {
                                ElementHandle::BottomRight => {
                                    let pin = *initial_start;
                                    let dx = f32::from(world_p.x - pin.x);
                                    let dy = f32::from(world_p.y - pin.y);
                                    (pin, dx, dy)
                                }
                                ElementHandle::BottomLeft => {
                                    let pin = point(initial_end.x, initial_start.y);
                                    let dx = f32::from(pin.x - world_p.x);
                                    let dy = f32::from(world_p.y - pin.y);
                                    (pin, dx, dy)
                                }
                                ElementHandle::TopRight => {
                                    let pin = point(initial_start.x, initial_end.y);
                                    let dx = f32::from(world_p.x - pin.x);
                                    let dy = f32::from(pin.y - world_p.y);
                                    (pin, dx, dy)
                                }
                                ElementHandle::TopLeft => {
                                    let pin = *initial_end;
                                    let dx = f32::from(pin.x - world_p.x);
                                    let dy = f32::from(pin.y - world_p.y);
                                    (pin, dx, dy)
                                }
                                _ => (*initial_start, initial_w, initial_h),
                            };

                            let factor =
                                ((dx * initial_w + dy * initial_h) / diag_sq).clamp(0.2, 8.0);
                            let new_font_size = (*initial_font_size * factor).clamp(12.0, 120.0);
                            let w = estimate_text_width(&el.text, new_font_size);
                            let h = estimate_text_height(new_font_size);

                            el.font_size = new_font_size;
                            match handle {
                                ElementHandle::BottomRight => {
                                    el.start = pin;
                                    el.end = point(pin.x + px(w), pin.y + px(h));
                                }
                                ElementHandle::BottomLeft => {
                                    el.start = point(pin.x - px(w), pin.y);
                                    el.end = point(pin.x, pin.y + px(h));
                                }
                                ElementHandle::TopRight => {
                                    el.start = point(pin.x, pin.y - px(h));
                                    el.end = point(pin.x + px(w), pin.y);
                                }
                                ElementHandle::TopLeft => {
                                    el.start = point(pin.x - px(w), pin.y - px(h));
                                    el.end = pin;
                                }
                                _ => {}
                            }
                        } else {
                            match handle {
                                ElementHandle::Start => {
                                    el.start = world_p;
                                }
                                ElementHandle::End => {
                                    el.end = world_p;
                                }
                                ElementHandle::TopLeft => {
                                    el.start = world_p;
                                }
                                ElementHandle::TopRight => {
                                    el.start.y = world_p.y;
                                    el.end.x = world_p.x;
                                }
                                ElementHandle::BottomRight => {
                                    el.end = world_p;
                                }
                                ElementHandle::BottomLeft => {
                                    el.start.x = world_p.x;
                                    el.end.y = world_p.y;
                                }
                                ElementHandle::Body => {}
                            }
                        }
                        cx.notify();
                    }
                }
            }
        }
    }

    fn handle_mouse_up(&mut self, cx: &mut Context<Self>) {
        if let Some(dragging) = self.dragging.take() {
            match dragging {
                Dragging::Panning { .. } => {
                    cx.notify();
                }
                Dragging::Drawing => {
                    if let Some(el) = self.current_element.take() {
                        let is_valid = match el.tool {
                            Tool::Pen => el.points.len() > 1,
                            _ => {
                                let dx = f32::from(el.end.x - el.start.x).abs();
                                let dy = f32::from(el.end.y - el.start.y).abs();
                                dx > 3.0 || dy > 3.0
                            }
                        };
                        if is_valid {
                            self.push_undo();
                            self.elements.push(el);
                        }
                        cx.notify();
                    }
                }
                Dragging::Moving { .. } | Dragging::Resizing { .. } => {
                    self.push_undo();
                    cx.notify();
                }
            }
        }
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tools = [
            Tool::Select,
            Tool::Rect,
            Tool::Circle,
            Tool::Arrow,
            Tool::Pen,
            Tool::Mosaic,
            Tool::Text,
        ];

        let has_sel_element = self.selected_element_id.is_some();

        div()
            .id("snip_header_bar")
            .h(px(44.))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px_3()
            .bg(rgba(0xFFFFFF08))
            .border_b_1()
            .border_color(rgba(0xFFFFFF15))
            .flex_shrink_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                }),
            )
            // 1. 图元绘制工具组
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .children(tools.into_iter().enumerate().map(|(idx, t)| {
                        let is_active = self.active_tool == t;
                        div()
                            .id(("tool_btn", idx))
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .bg(if is_active {
                                rgba(0x007AFFFF)
                            } else {
                                rgba(0x00000000)
                            })
                            .hover(|s| {
                                s.bg(if is_active {
                                    rgba(0x007AFFFF)
                                } else {
                                    rgba(0xFFFFFF18)
                                })
                            })
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(t.icon_path())
                                    .size(px(15.))
                                    .text_color(if is_active {
                                        rgba(0xFFFFFFFF)
                                    } else {
                                        rgba(0xCCCCCCFF)
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.commit_inline_text();
                                this.active_tool = t;
                                cx.notify();
                            }))
                    })),
            )
            // 分隔线
            .child(
                div()
                    .w(px(1.))
                    .h(px(16.))
                    .bg(rgba(0xFFFFFF20))
                    .mx_1p5()
                    .flex_shrink_0(),
            )
            // 2. 调色板
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .children((0..PALETTE.len()).map(|idx| {
                        let is_active = self.active_color_idx == idx;
                        let col = palette_color(idx);
                        div()
                            .id(("color_dot", idx))
                            .w(px(16.))
                            .h(px(16.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(col)
                            .border(if is_active { px(2.5) } else { px(1.) })
                            .border_color(if is_active {
                                rgba(0xFFFFFFFF)
                            } else {
                                rgba(0x00000066)
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.active_color_idx = idx;
                                if let Some(el) = this
                                    .selected_element_id
                                    .and_then(|id| this.elements.iter_mut().find(|el| el.id == id))
                                {
                                    el.color_idx = idx;
                                }
                                cx.notify();
                            }))
                    })),
            )
            // 分隔线
            .child(
                div()
                    .w(px(1.))
                    .h(px(16.))
                    .bg(rgba(0xFFFFFF20))
                    .mx_1p5()
                    .flex_shrink_0(),
            )
            // 3. 编辑操作 (撤销 / 删除)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("btn_undo")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(icons::UNDO_2)
                                    .size(px(15.))
                                    .text_color(if self.elements.is_empty() {
                                        rgba(0xFFFFFF44)
                                    } else {
                                        rgba(0xFFFFFFFF)
                                    }),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.undo(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn_delete")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .hover(|s| {
                                s.bg(if has_sel_element {
                                    rgba(0xFF3B3033)
                                } else {
                                    rgba(0x00000000)
                                })
                            })
                            .cursor_pointer()
                            .child(Icon::default().path(icons::TRASH).size(px(15.)).text_color(
                                if has_sel_element {
                                    rgba(0xFF5B5BFF)
                                } else {
                                    rgba(0xFFFFFF33)
                                },
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.delete_selected(cx);
                            })),
                    ),
            )
            // 分隔线
            .child(
                div()
                    .w(px(1.))
                    .h(px(16.))
                    .bg(rgba(0xFFFFFF20))
                    .mx_1p5()
                    .flex_shrink_0(),
            )
            // 4. 缩放控制组 (缩小 / 比例 / 放大 / 适应)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("btn_zoom_out")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.))
                            .h(px(24.))
                            .rounded(px(5.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(icons::MINUS)
                                    .size(px(13.))
                                    .text_color(rgba(0xCCCCCCFF)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.zoom_out(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn_zoom_reset")
                            .px_1p5()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgba(0xCCCCCCFF))
                            .child(format!("{}%", (self.zoom * 100.0).round() as i32))
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.zoom = 1.0;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("btn_zoom_in")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.))
                            .h(px(24.))
                            .rounded(px(5.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(icons::PLUS)
                                    .size(px(13.))
                                    .text_color(rgba(0xCCCCCCFF)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.zoom_in(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn_zoom_fit")
                            .px_1p5()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgba(0xAAAAAAFF))
                            .child("适应")
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.reset_zoom(cx);
                            })),
                    ),
            )
            // 弹性间隔，将右侧动作推向末尾
            .child(div().flex_1().min_w(px(12.)))
            // 4. 结尾动作组 (下载 / 复制 / 关闭)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("btn_download")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(icons::DOWNLOAD)
                                    .size(px(15.))
                                    .text_color(rgba(0xFFFFFFFF)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_as_file(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn_copy")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .bg(rgba(0x007AFFFF))
                            .hover(|s| s.bg(rgba(0x0062CCFF)))
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(icons::COPY)
                                    .size(px(15.))
                                    .text_color(rgba(0xFFFFFFFF)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.commit_and_copy(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn_close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .hover(|s| s.bg(rgba(0xFFFFFF18)))
                            .cursor_pointer()
                            .child(
                                Icon::default()
                                    .path(icons::X)
                                    .size(px(15.))
                                    .text_color(rgba(0xFFFFFFBB)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close(window, cx);
                            })),
                    ),
            )
    }
}

impl Render for ScreenshotView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let win_bounds = window.bounds();
        let win_w = f32::from(win_bounds.size.width);
        let win_h = f32::from(win_bounds.size.height);

        let toolbar_h = 44.0f32;
        let canvas_w = win_w;
        let canvas_h = (win_h - toolbar_h).max(100.0);

        let (actual_log_w, actual_log_h) = self.image_size;
        // 保证四周至少有 40px 的画布边距供用户在图片外自由绘制
        let avail_w = (canvas_w - 80.0).max(100.0);
        let avail_h = (canvas_h - 80.0).max(100.0);

        let fit_scale = (avail_w / actual_log_w)
            .min(avail_h / actual_log_h)
            .min(1.0);

        let effective_scale = fit_scale * self.zoom;
        let disp_w = (actual_log_w * effective_scale).round();
        let disp_h = (actual_log_h * effective_scale).round();

        let base_x = ((canvas_w - disp_w) / 2.0).round();
        let base_y = ((canvas_h - disp_h) / 2.0).round();

        let img_local_x = base_x + f32::from(self.pan_offset.x);
        let img_local_y = base_y + f32::from(self.pan_offset.y);

        let elements = self.elements.clone();
        let curr = self.current_element.clone();
        let sel_id = self.selected_element_id;
        let base_img = self.base_image.clone();
        let layout_lock_prep = self.image_layout.clone();
        let layout_lock = self.image_layout.clone();

        let mut root = div()
            .id("screenshot_editor_view")
            .key_context(crate::shared::actions::SCREENSHOT_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::handle_close))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba(0x18181B66))
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(rgba(0xFFFFFF28))
            .shadow_2xl()
            .cursor(self.cursor_style())
            // 头部工具栏
            .child(self.render_toolbar(cx));

        // 画布区域（支持无限平移与缩放）
        let mut canvas_container = div()
            .id("editor_canvas_container")
            .relative()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .bg(rgba(0x00000028))
            // 1. 底图图片卡片
            .child(
                div()
                    .id("image_box")
                    .absolute()
                    .left(px(img_local_x))
                    .top(px(img_local_y))
                    .w(px(disp_w))
                    .h(px(disp_h))
                    .rounded(px(6.))
                    .overflow_hidden()
                    .shadow_xl()
                    .border_1()
                    .border_color(rgba(0xFFFFFF25))
                    .child(img(self.image_path.clone()).size_full()),
            )
            // 2. 覆盖整个工作区的矢量图元绘制 Canvas
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        *layout_lock_prep.lock() = ImageLayout {
                            container_origin: bounds.origin,
                            img_win_origin: point(
                                bounds.origin.x + px(img_local_x),
                                bounds.origin.y + px(img_local_y),
                            ),
                            disp_size: (disp_w, disp_h),
                            fit_scale: effective_scale,
                        };
                    },
                    move |bounds, _, window, _cx| {
                        *layout_lock.lock() = ImageLayout {
                            container_origin: bounds.origin,
                            img_win_origin: point(
                                bounds.origin.x + px(img_local_x),
                                bounds.origin.y + px(img_local_y),
                            ),
                            disp_size: (disp_w, disp_h),
                            fit_scale: effective_scale,
                        };
                        let img_win_x = f32::from(bounds.origin.x) + img_local_x;
                        let img_win_y = f32::from(bounds.origin.y) + img_local_y;

                        let to_screen = move |wp: Point<Pixels>| -> Point<Pixels> {
                            point(
                                px(img_win_x + f32::from(wp.x) * effective_scale),
                                px(img_win_y + f32::from(wp.y) * effective_scale),
                            )
                        };

                        let mut all = elements;
                        if let Some(c) = curr {
                            all.push(c);
                        }
                        for el in all {
                            let col = palette_color(el.color_idx);
                            let line_w = (el.line_width * effective_scale).max(1.0);

                            match el.tool {
                                Tool::Select => {}
                                Tool::Rect => {
                                    let b = el.normalized_rect();
                                    let tl = to_screen(b.origin);
                                    let br = to_screen(point(
                                        b.origin.x + b.size.width,
                                        b.origin.y + b.size.height,
                                    ));
                                    let mut builder = PathBuilder::stroke(px(line_w));
                                    builder.move_to(tl);
                                    builder.line_to(point(br.x, tl.y));
                                    builder.line_to(br);
                                    builder.line_to(point(tl.x, br.y));
                                    builder.close();
                                    if let Ok(path) = builder.build() {
                                        window.paint_path(path, col);
                                    }
                                }
                                Tool::Mosaic => {
                                    let b = el.normalized_rect();
                                    let min_x = f32::from(b.origin.x);
                                    let min_y = f32::from(b.origin.y);
                                    let w = f32::from(b.size.width);
                                    let h = f32::from(b.size.height);
                                    if w > 1.0 && h > 1.0 {
                                        let block_world = 10.0f32;
                                        let cols = (w / block_world).ceil() as usize;
                                        let rows = (h / block_world).ceil() as usize;

                                        if let Some(ref img) = base_img {
                                            let img_w = img.width();
                                            let img_h = img.height();

                                            for r in 0..rows {
                                                let wy1 = min_y + r as f32 * block_world;
                                                let wy2 = (wy1 + block_world).min(min_y + h);
                                                for c in 0..cols {
                                                    let wx1 = min_x + c as f32 * block_world;
                                                    let wx2 = (wx1 + block_world).min(min_x + w);

                                                    let sx1 = img_win_x + wx1 * effective_scale;
                                                    let sx2 = img_win_x + wx2 * effective_scale;
                                                    let sy1 = img_win_y + wy1 * effective_scale;
                                                    let sy2 = img_win_y + wy2 * effective_scale;

                                                    let is_inside = wx2 > 0.0
                                                        && wx1 < actual_log_w
                                                        && wy2 > 0.0
                                                        && wy1 < actual_log_h;

                                                    let avg_color = if is_inside {
                                                        let px_min_x = ((wx1.max(0.0)
                                                            / actual_log_w
                                                            * img_w as f32)
                                                            .round()
                                                            as u32)
                                                            .min(img_w);
                                                        let px_max_x = ((wx2.max(0.0)
                                                            / actual_log_w
                                                            * img_w as f32)
                                                            .round()
                                                            as u32)
                                                            .min(img_w);
                                                        let px_min_y = ((wy1.max(0.0)
                                                            / actual_log_h
                                                            * img_h as f32)
                                                            .round()
                                                            as u32)
                                                            .min(img_h);
                                                        let px_max_y = ((wy2.max(0.0)
                                                            / actual_log_h
                                                            * img_h as f32)
                                                            .round()
                                                            as u32)
                                                            .min(img_h);

                                                        let mut r_sum = 0u64;
                                                        let mut g_sum = 0u64;
                                                        let mut b_sum = 0u64;
                                                        let mut count = 0u64;

                                                        for py in px_min_y..px_max_y {
                                                            for px in px_min_x..px_max_x {
                                                                let p = img.get_pixel(px, py);
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
                                                            rgba(
                                                                (r as u32) << 24
                                                                    | (g as u32) << 16
                                                                    | (b as u32) << 8
                                                                    | 0xFF,
                                                            )
                                                        } else {
                                                            let sx = px_min_x
                                                                .min(img_w.saturating_sub(1));
                                                            let sy = px_min_y
                                                                .min(img_h.saturating_sub(1));
                                                            let p = img.get_pixel(sx, sy);
                                                            rgba(
                                                                (p[0] as u32) << 24
                                                                    | (p[1] as u32) << 16
                                                                    | (p[2] as u32) << 8
                                                                    | 0xFF,
                                                            )
                                                        }
                                                    } else if (r + c) % 2 == 0 {
                                                        rgba(0x555555FF)
                                                    } else {
                                                        rgba(0x888888FF)
                                                    };

                                                    let mut block_builder = PathBuilder::fill();
                                                    block_builder.move_to(point(px(sx1), px(sy1)));
                                                    block_builder.line_to(point(px(sx2), px(sy1)));
                                                    block_builder.line_to(point(px(sx2), px(sy2)));
                                                    block_builder.line_to(point(px(sx1), px(sy2)));
                                                    block_builder.close();
                                                    if let Ok(path) = block_builder.build() {
                                                        window.paint_path(path, avg_color);
                                                    }
                                                }
                                            }
                                        } else {
                                            let mut path1 = PathBuilder::fill();
                                            let mut path2 = PathBuilder::fill();
                                            for r in 0..rows {
                                                let wy1 = min_y + r as f32 * block_world;
                                                let wy2 = (wy1 + block_world).min(min_y + h);
                                                for c in 0..cols {
                                                    let wx1 = min_x + c as f32 * block_world;
                                                    let wx2 = (wx1 + block_world).min(min_x + w);

                                                    let sx1 = img_win_x + wx1 * effective_scale;
                                                    let sx2 = img_win_x + wx2 * effective_scale;
                                                    let sy1 = img_win_y + wy1 * effective_scale;
                                                    let sy2 = img_win_y + wy2 * effective_scale;

                                                    let builder = if (r + c) % 2 == 0 {
                                                        &mut path1
                                                    } else {
                                                        &mut path2
                                                    };
                                                    builder.move_to(point(px(sx1), px(sy1)));
                                                    builder.line_to(point(px(sx2), px(sy1)));
                                                    builder.line_to(point(px(sx2), px(sy2)));
                                                    builder.line_to(point(px(sx1), px(sy2)));
                                                    builder.close();
                                                }
                                            }
                                            if let Ok(p1) = path1.build() {
                                                window.paint_path(p1, rgba(0x555555FF));
                                            }
                                            if let Ok(p2) = path2.build() {
                                                window.paint_path(p2, rgba(0x999999FF));
                                            }
                                        }

                                        let tl = to_screen(b.origin);
                                        let br = to_screen(point(
                                            b.origin.x + b.size.width,
                                            b.origin.y + b.size.height,
                                        ));
                                        let mut border = PathBuilder::stroke(px(1.0));
                                        border.move_to(tl);
                                        border.line_to(point(br.x, tl.y));
                                        border.line_to(br);
                                        border.line_to(point(tl.x, br.y));
                                        border.close();
                                        if let Ok(bp) = border.build() {
                                            window.paint_path(bp, rgba(0xFFFFFF66));
                                        }
                                    }
                                }
                                Tool::Circle => {
                                    let b = el.normalized_rect();
                                    let tl = to_screen(b.origin);
                                    let br = to_screen(point(
                                        b.origin.x + b.size.width,
                                        b.origin.y + b.size.height,
                                    ));
                                    let cx = f32::from(tl.x + br.x) / 2.0;
                                    let cy = f32::from(tl.y + br.y) / 2.0;
                                    let rx = (f32::from(br.x - tl.x) / 2.0).abs().max(1.0);
                                    let ry = (f32::from(br.y - tl.y) / 2.0).abs().max(1.0);
                                    let steps = 48;
                                    let mut builder = PathBuilder::stroke(px(line_w));
                                    for i in 0..=steps {
                                        let theta =
                                            (i as f32 / steps as f32) * std::f32::consts::TAU;
                                        let pt = point(
                                            px(cx + rx * theta.cos()),
                                            px(cy + ry * theta.sin()),
                                        );
                                        if i == 0 {
                                            builder.move_to(pt);
                                        } else {
                                            builder.line_to(pt);
                                        }
                                    }
                                    builder.close();
                                    if let Ok(path) = builder.build() {
                                        window.paint_path(path, col);
                                    }
                                }
                                Tool::Arrow => {
                                    let s = to_screen(el.start);
                                    let e = to_screen(el.end);
                                    let mut builder = PathBuilder::stroke(px(line_w));
                                    builder.move_to(s);
                                    builder.line_to(e);
                                    if let Ok(path) = builder.build() {
                                        window.paint_path(path, col);
                                    }

                                    let (x1, y1) = (f32::from(s.x), f32::from(s.y));
                                    let (x2, y2) = (f32::from(e.x), f32::from(e.y));
                                    let dx = x2 - x1;
                                    let dy = y2 - y1;
                                    let len = (dx * dx + dy * dy).sqrt();
                                    if len > 5.0 {
                                        let head_len = (line_w * 4.0).min(len * 0.45).max(4.0);
                                        let angle = dy.atan2(dx);
                                        let arrow_angle = std::f32::consts::PI / 6.5;

                                        let p1 = point(
                                            px(x2 - head_len * (angle - arrow_angle).cos()),
                                            px(y2 - head_len * (angle - arrow_angle).sin()),
                                        );
                                        let p2 = point(
                                            px(x2 - head_len * (angle + arrow_angle).cos()),
                                            px(y2 - head_len * (angle + arrow_angle).sin()),
                                        );
                                        let mut head = PathBuilder::fill();
                                        head.move_to(e);
                                        head.line_to(p1);
                                        head.line_to(p2);
                                        head.close();
                                        if let Ok(head_path) = head.build() {
                                            window.paint_path(head_path, col);
                                        }
                                    }
                                }
                                Tool::Pen => {
                                    if el.points.len() > 1 {
                                        let mut builder = PathBuilder::stroke(px(line_w));
                                        builder.move_to(to_screen(el.points[0]));
                                        for pt in el.points.iter().skip(1) {
                                            builder.line_to(to_screen(*pt));
                                        }
                                        if let Ok(path) = builder.build() {
                                            window.paint_path(path, col);
                                        }
                                    }
                                }
                                Tool::Text => {}
                            }

                            // 选中的图元绘制蓝色虚线高亮框及控制手柄
                            if Some(el.id) == sel_id {
                                let b = el.bounds();
                                let tl = to_screen(point(b.origin.x, b.origin.y));
                                let tr = to_screen(point(b.origin.x + b.size.width, b.origin.y));
                                let br = to_screen(point(
                                    b.origin.x + b.size.width,
                                    b.origin.y + b.size.height,
                                ));
                                let bl = to_screen(point(b.origin.x, b.origin.y + b.size.height));

                                let mut select_box = PathBuilder::stroke(px(1.5));
                                select_box.move_to(tl);
                                select_box.line_to(tr);
                                select_box.line_to(br);
                                select_box.line_to(bl);
                                select_box.close();
                                if let Ok(path) = select_box.build() {
                                    window.paint_path(path, rgba(0x007AFFFF));
                                }

                                let handle_pts = match el.tool {
                                    Tool::Arrow => vec![to_screen(el.start), to_screen(el.end)],
                                    Tool::Rect | Tool::Circle | Tool::Mosaic | Tool::Text => {
                                        vec![tl, tr, bl, br]
                                    }
                                    _ => vec![],
                                };
                                for hp in handle_pts {
                                    let hs = 3.5;
                                    let mut h_builder = PathBuilder::fill();
                                    h_builder.move_to(point(hp.x - px(hs), hp.y - px(hs)));
                                    h_builder.line_to(point(hp.x + px(hs), hp.y - px(hs)));
                                    h_builder.line_to(point(hp.x + px(hs), hp.y + px(hs)));
                                    h_builder.line_to(point(hp.x - px(hs), hp.y + px(hs)));
                                    h_builder.close();
                                    if let Ok(hpath) = h_builder.build() {
                                        window.paint_path(hpath, rgba(0x007AFFFF));
                                    }
                                }
                            }
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );

        // 3. 文本图元渲染（直接挂载在画布容器上，支持在图片周围任意位置放置文本，无背景色）
        for el in &self.elements {
            if el.tool == Tool::Text && !el.text.is_empty() {
                let col = palette_color(el.color_idx);
                let text_val = el.text.clone();
                let el_id = el.id;
                let el_start = el.start;
                let b = el.bounds();
                let local_x = img_local_x + f32::from(b.origin.x) * effective_scale;
                let local_y = img_local_y + f32::from(b.origin.y) * effective_scale;
                let screen_font_size = (el.font_size * effective_scale).max(10.0);

                canvas_container = canvas_container.child(
                    div()
                        .id(ElementId::NamedInteger(
                            "txt_elem".into(),
                            el.id.as_u128() as u64,
                        ))
                        .absolute()
                        .left(px(local_x))
                        .top(px(local_y))
                        .w(px(f32::from(b.size.width) * effective_scale))
                        .h(px(f32::from(b.size.height) * effective_scale))
                        .bg(rgba(0x00000000))
                        .border_0()
                        .text_size(px(screen_font_size))
                        .line_height(px(screen_font_size * 1.25))
                        .font_weight(FontWeight::BOLD)
                        .text_color(col)
                        .child(el.text.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.active_tool == Tool::Select {
                                this.selected_element_id = Some(el_id);
                                cx.notify();
                            }
                        }))
                        // 双击文本重新进入编辑
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if event.click_count == 2 {
                                    this.editing_text =
                                        Some((el_start, text_val.clone(), Some(el_id)));
                                    cx.notify();
                                }
                            }),
                        ),
                );
            }
        }

        // 4. 内联文本编辑输入框
        if let Some((world_pos, ref text, _)) = self.editing_text {
            let col = palette_color(self.active_color_idx);
            let local_x = img_local_x + f32::from(world_pos.x) * effective_scale;
            let local_y = img_local_y + f32::from(world_pos.y) * effective_scale;
            canvas_container = canvas_container.child(
                div()
                    .id("inline_text_input_box")
                    .absolute()
                    .left(px(local_x))
                    .top(px(local_y))
                    .min_w(px(100.))
                    .bg(rgba(0x1E1E1EF5))
                    .border_1()
                    .border_color(rgba(0x007AFFFF))
                    .rounded(px(4.))
                    .p_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(col)
                            .child(if text.is_empty() {
                                "输入文字后按回车完成…".to_string()
                            } else {
                                text.clone()
                            }),
                    ),
            );
        }

        // 5. 鼠标绘制、拖拽与无限平移滚动事件
        canvas_container = canvas_container
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.handle_mouse_down(event.position, MouseButton::Left, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.handle_mouse_down(event.position, MouseButton::Middle, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.handle_mouse_move(event.position, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _, cx| {
                    this.handle_mouse_up(cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, _event: &MouseUpEvent, _, cx| {
                    this.handle_mouse_up(cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(16.0));
                if event.modifiers.platform || event.modifiers.control {
                    let dy = f32::from(delta.y);
                    let factor = if dy > 0.0 {
                        1.05
                    } else if dy < 0.0 {
                        0.95
                    } else {
                        1.0
                    };
                    this.zoom_at(factor, event.position);
                    cx.notify();
                } else {
                    this.pan_offset.x += delta.x;
                    this.pan_offset.y += delta.y;
                    cx.notify();
                }
            }));

        root = root.child(canvas_container);

        // 键盘全局热键
        root = root
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // 内联编辑中拦截键盘输入
                if let Some((_, ref mut text, _)) = this.editing_text {
                    if event.keystroke.key == "enter" {
                        this.commit_inline_text();
                        cx.notify();
                        return;
                    }
                    if event.keystroke.key == "escape" {
                        this.editing_text = None;
                        cx.notify();
                        return;
                    }
                    if event.keystroke.key == "backspace" {
                        text.pop();
                        cx.notify();
                        return;
                    }
                    if let Some(ref c) = event.keystroke.key_char {
                        text.push_str(c);
                        cx.notify();
                        return;
                    }
                    if event.keystroke.key.chars().count() == 1 {
                        text.push_str(&event.keystroke.key);
                        cx.notify();
                        return;
                    }
                }

                let key = event.keystroke.key.as_str();
                let mods = &event.keystroke.modifiers;
                let chord = mods.platform || mods.control;

                // 全局快捷键: ESC 或 Cmd+W 关闭窗口
                if key == "escape" || (chord && key.eq_ignore_ascii_case("w")) {
                    this.close(window, cx);
                    return;
                }
                if chord && (key == "=" || key == "+") {
                    this.zoom_in(cx);
                    return;
                }
                if chord && (key == "-" || key == "_") {
                    this.zoom_out(cx);
                    return;
                }
                if chord && key == "0" {
                    this.reset_zoom(cx);
                    return;
                }
                if key == " " || key == "space" {
                    this.is_space_down = true;
                    cx.notify();
                    return;
                }
                if key == "enter" {
                    this.commit_and_copy(window, cx);
                    return;
                }
                if chord && key.eq_ignore_ascii_case("z") {
                    this.undo(cx);
                    return;
                }
                if key == "backspace" || key == "delete" {
                    this.delete_selected(cx);
                }
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                if event.keystroke.key == " " || event.keystroke.key == "space" {
                    this.is_space_down = false;
                    cx.notify();
                }
            }));

        root
    }
}
