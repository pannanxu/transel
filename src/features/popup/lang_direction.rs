//! 语言方向组件：源 ⇄ 目标。
//!
//! 语言列表用**独立 OS 弹窗**（`WindowKind::PopUp`）展示，这样不会被翻译窗裁切。
//! 同窗内的 GPUI Popover / deferred 浮层画不出父窗口边界。

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::features::popup::SHADOW_MARGIN;
use crate::platform;
use crate::shared::chrome::window_background;
use crate::shared::theme::{LANGUAGES, Palette, lang_label, normalize_lang_code, palette};

/// 正在编辑源语言或目标语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangSide {
    Source,
    Target,
}

/// 组件对外事件。
#[derive(Debug, Clone)]
pub enum LangDirectionEvent {
    /// 点击中间对调
    Swap,
    /// 在菜单中选中语言
    Selected { side: LangSide, code: String },
}

const MENU_W: f32 = 188.;
const ROW_H: f32 = 28.;
const TITLE_H: f32 = 30.;

/// 语言方向条状态。
pub struct LangDirection {
    source_lang: String,
    target_lang: String,
    /// 源侧短标签（如「自动」「中文」）
    source_label: String,
    source_open: bool,
    target_open: bool,
    /// 独立语言菜单窗口（与翻译窗分离，避免被裁切）
    menu_window: Option<AnyWindowHandle>,
    /// 芯片在父窗局部坐标中的 bounds（canvas 每帧更新）
    source_chip: Bounds<Pixels>,
    target_chip: Bounds<Pixels>,
}

impl EventEmitter<LangDirectionEvent> for LangDirection {}

impl LangDirection {
    pub fn new(source_lang: impl Into<String>, target_lang: impl Into<String>) -> Self {
        let source_lang = source_lang.into();
        let target_lang = target_lang.into();
        let source_label = lang_label(&source_lang);
        Self {
            source_lang,
            target_lang,
            source_label,
            source_open: false,
            target_open: false,
            menu_window: None,
            source_chip: Bounds::default(),
            target_chip: Bounds::default(),
        }
    }

    pub fn is_expanded(&self) -> bool {
        self.source_open || self.target_open
    }

    /// 同步父级语言；`source_label` 为短标签展示。
    pub fn sync(
        &mut self,
        source_lang: impl Into<String>,
        target_lang: impl Into<String>,
        source_label: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let source_lang = source_lang.into();
        let target_lang = target_lang.into();
        let source_label = source_label.into();
        if self.source_lang == source_lang
            && self.target_lang == target_lang
            && self.source_label == source_label
        {
            return;
        }
        self.source_lang = source_lang;
        self.target_lang = target_lang;
        self.source_label = source_label;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.dismiss_menu(cx);
    }

    fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.menu_window.take() {
            let _ = handle.update(cx, |_, window, _| {
                window.remove_window();
            });
        }
        if self.source_open || self.target_open {
            self.source_open = false;
            self.target_open = false;
            cx.notify();
        }
    }

    fn toggle_menu(&mut self, side: LangSide, window: &Window, cx: &mut Context<Self>) {
        let already = match side {
            LangSide::Source => self.source_open,
            LangSide::Target => self.target_open,
        };
        if already {
            self.dismiss_menu(cx);
            return;
        }

        // 先关掉另一侧菜单窗，但不要清掉即将打开的 expanded 标志
        if let Some(handle) = self.menu_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }

        // 必须在 open_window(focus) 之前标成 expanded，否则翻译窗失焦会把自己关掉
        match side {
            LangSide::Source => {
                self.source_open = true;
                self.target_open = false;
            }
            LangSide::Target => {
                self.target_open = true;
                self.source_open = false;
            }
        }

        let chip = match side {
            LangSide::Source => self.source_chip,
            LangSide::Target => self.target_chip,
        };
        let win = window.bounds();
        // 锚在芯片左下角（屏幕坐标）
        let anchor = point(win.origin.x + chip.origin.x, win.origin.y + chip.bottom());

        let current = match side {
            LangSide::Source => normalize_lang_code(&self.source_lang),
            LangSide::Target => normalize_lang_code(&self.target_lang),
        };
        let rows = LANGUAGES
            .iter()
            .filter(|(code, _, _)| !(side == LangSide::Target && *code == "auto"))
            .count();
        let inner_h = 6. + TITLE_H + 8. + rows as f32 * ROW_H + 6.;
        let size = size(
            px(MENU_W + SHADOW_MARGIN * 2.),
            px(inner_h + SHADOW_MARGIN * 2.),
        );
        let (bounds, display_id) = place_menu(cx, window, anchor, size);

        let parent = cx.weak_entity();
        let options = WindowOptions {
            titlebar: None,
            kind: WindowKind::Floating,
            focus: true,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_background: window_background(),
            app_id: Some("dev.transel".into()),
            ..Default::default()
        };

        match gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| LangMenuView::new(side, current, parent, window, cx))
        }) {
            Ok((handle, _)) => {
                let _ = handle.update(cx, |_, window, _| {
                    platform::setup_popup_window(window, true);
                });
                self.menu_window = Some(handle);
                cx.notify();
            }
            Err(e) => {
                self.source_open = false;
                self.target_open = false;
                log::error!("打开语言菜单失败: {e:#}");
                cx.notify();
            }
        }
    }

    fn pick(&mut self, side: LangSide, code: String, cx: &mut Context<Self>) {
        // 菜单窗由点击方自己 remove；这里只清状态，避免二次关窗。
        self.menu_window = None;
        self.source_open = false;
        self.target_open = false;
        cx.emit(LangDirectionEvent::Selected { side, code });
        cx.notify();
    }

    /// 菜单窗失焦关闭时回调（窗口已在拆除）。
    fn on_menu_closed(&mut self, cx: &mut Context<Self>) {
        self.menu_window = None;
        if self.source_open || self.target_open {
            self.source_open = false;
            self.target_open = false;
            cx.notify();
        }
    }
}

/// 把菜单放到触发点下方；不够则翻到上方，并夹在当前显示器内。
fn place_menu(
    cx: &App,
    parent: &Window,
    anchor: Point<Pixels>,
    size: Size<Pixels>,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let ax = f32::from(anchor.x);
    let ay = f32::from(anchor.y);
    let mut x = ax;
    let mut y = ay + 4.;

    let displays = cx.displays();
    let primary = cx.primary_display();
    let parent_display = parent.display(cx);
    let p = point(px(ax), px(ay));
    let display = parent_display
        .or_else(|| displays.iter().find(|d| d.bounds().contains(&p)).cloned())
        .or(primary);

    let mut id = None;
    if let Some(d) = display {
        id = Some(d.id());
        let b = d.bounds();
        let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (right, bottom) = (
            left + f32::from(b.size.width),
            top + f32::from(b.size.height),
        );
        if y + h > bottom - 4. {
            y = ay - h - 4.;
        }
        x = x.min(right - w - 4.).max(left + 4.);
        y = y.min(bottom - h - 4.).max(top + 4.);
    }
    (Bounds::new(point(px(x), px(y)), size), id)
}

impl Render for LangDirection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let source_label = self.source_label.clone();
        let target_label = lang_label(&self.target_lang);
        let source_open = self.source_open;
        let target_open = self.target_open;

        div()
            .id("lang_direction")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(1.))
            .h(px(22.))
            .px(px(2.))
            .rounded(px(6.))
            .bg(p.pill_bg)
            .child(lang_chip(
                "lang_src_chip",
                LangSide::Source,
                &source_label,
                source_open,
                &p,
                cx,
            ))
            .child(
                div()
                    .id("lang_swap")
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .text_size(px(11.))
                    .text_color(p.accent)
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.close(cx);
                            cx.emit(LangDirectionEvent::Swap);
                        }),
                    )
                    .child("⇄"),
            )
            .child(lang_chip(
                "lang_tgt_chip",
                LangSide::Target,
                &target_label,
                target_open,
                &p,
                cx,
            ))
    }
}

fn lang_chip(
    id: &'static str,
    side: LangSide,
    label: &str,
    open: bool,
    p: &Palette,
    cx: &mut Context<LangDirection>,
) -> Stateful<Div> {
    let accent = p.accent;
    let muted = p.muted;
    let text = p.text;
    let hover = p.hover;
    let active_bg = p.tab_active;
    let weak = cx.weak_entity();
    div()
        .id(id)
        .relative()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.))
        .rounded(px(4.))
        .when(open, |el| el.bg(active_bg))
        .text_size(px(10.5))
        .text_color(if open { text } else { muted })
        .cursor_pointer()
        .hover(move |s| s.bg(hover).text_color(text))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_menu(side, window, cx);
            }),
        )
        .child(label.to_string())
        .child(
            div()
                .text_size(px(7.))
                .line_height(px(10.))
                .text_color(if open { accent } else { muted })
                .child(if open { "▴" } else { "▾" }),
        )
        // 测量芯片 bounds，供独立菜单窗锚定（不抢点击）
        .child(
            canvas(
                move |bounds, _, cx| {
                    let _ = weak.update(cx, |this, _| match side {
                        LangSide::Source => this.source_chip = bounds,
                        LangSide::Target => this.target_chip = bounds,
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
            .top_0()
            .left_0(),
        )
}

/// 独立窗口里的语言列表。
struct LangMenuView {
    side: LangSide,
    current: String,
    parent: WeakEntity<LangDirection>,
    focus: FocusHandle,
    _activation: Subscription,
}

impl LangMenuView {
    fn new(
        side: LangSide,
        current: String,
        parent: WeakEntity<LangDirection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let parent_for_close = parent.clone();
        let activation = cx.observe_window_activation(window, move |_, window, cx| {
            if window.is_window_active() {
                return;
            }
            let _ = parent_for_close.update(cx, |ld, cx| ld.on_menu_closed(cx));
            window.remove_window();
        });
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            side,
            current,
            parent,
            focus,
            _activation: activation,
        }
    }

    fn close_menu(
        &mut self,
        _: &crate::shared::actions::CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent = self.parent.clone();
        let _ = parent.update(cx, |ld, cx| ld.on_menu_closed(cx));
        window.remove_window();
    }
}

impl Render for LangMenuView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window.appearance());
        let surface = match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => hsla(0., 0., 0.14, 0.96),
            _ => hsla(0., 0., 0.99, 0.98),
        };
        let title = match self.side {
            LangSide::Source => "源语言",
            LangSide::Target => "目标语言",
        };
        let side = self.side;
        let current = self.current.clone();
        let parent = self.parent.clone();

        let mut rows: Vec<AnyElement> = Vec::new();
        for (code, _short, label) in LANGUAGES.iter() {
            if side == LangSide::Target && *code == "auto" {
                continue;
            }
            let is_sel = current == *code;
            let code_owned = (*code).to_string();
            let parent = parent.clone();
            rows.push(
                div()
                    .id(format!(
                        "lang_opt_{}_{code}",
                        match side {
                            LangSide::Source => "s",
                            LangSide::Target => "t",
                        }
                    ))
                    .w_full()
                    .h(px(ROW_H))
                    .px(px(10.))
                    .rounded(px(6.))
                    .bg(if is_sel {
                        p.tab_active
                    } else {
                        transparent_black()
                    })
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .hover(|s| if is_sel { s } else { s.bg(p.hover) })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, window, cx| {
                            cx.stop_propagation();
                            let code = code_owned.clone();
                            let _ = parent.update(cx, |ld, cx| {
                                ld.pick(side, code, cx);
                            });
                            window.remove_window();
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(if is_sel { p.text } else { p.muted })
                            .font_weight(if is_sel {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child((*label).to_string()),
                    )
                    .when(is_sel, |el| {
                        el.child(div().text_size(px(11.)).text_color(p.accent).child("✓"))
                    })
                    .into_any_element(),
            );
        }

        let card = div()
            .id("lang_menu_card")
            .size_full()
            .flex()
            .flex_col()
            .p(px(6.))
            .gap(px(1.))
            .rounded(px(platform::CORNER_RADIUS as f32))
            .border_1()
            .border_color(p.border)
            .bg(surface)
            .when(SHADOW_MARGIN > 0., |el| {
                el.shadow(vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(4.)),
                    blur_radius: px(12.),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .child(
                div()
                    .px(px(10.))
                    .pt(px(2.))
                    .pb(px(6.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.title)
                    .child(title),
            )
            .child(div().h(px(1.)).w_full().bg(p.divider).mb(px(2.)))
            .child(
                div()
                    .id("lang_menu_list")
                    .w_full()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows),
            );

        div()
            .id("lang_menu")
            .key_context(crate::shared::actions::UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close_menu))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .font_family(".SystemUIFont")
            .child(card)
    }
}
