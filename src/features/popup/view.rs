//! 翻译弹窗。
//!
//! 设计原则：
//! - 状态由外部 [`PopupController`] 推动（主循环 / 设置面板 / OCR 触发器等）。
//! - 视图负责渲染，以及把关闭/固定/复制/朗读等事件转发到 [`PopupActions`]。
//! - 原文输入框在回车时由视图向翻译服务发请求，避免回调拿不到当前弹窗句柄。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui_kit::component::Icon;
use gpui_kit::component::animation::ease_out_cubic;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 窗口高度变化过渡时长。
const RESIZE_DURATION: Duration = Duration::from_millis(180);

struct ResizeAnim {
    from: Size<Pixels>,
    to: Size<Pixels>,
    started: Instant,
}

use crate::shared::text_select::{selectable_markdown, selectable_plain};

use super::lang_direction::{LangDirection, LangDirectionEvent, LangSide};
use crate::app::history::{HistoryEntry, HistoryOrigin, TranslationResult};
use crate::app::state::AppState;
use crate::services::translate::{BatchEvent, BatchResult, RequestCtx, SingleResult};
use crate::shared::loading;
use crate::shared::theme::{Palette, lang_label, normalize_lang_code, palette};

use crate::shared::icons;
use crate::shared::tooltip;

/// 翻译弹窗默认 / 最小卡片内容宽度（不含阴影边距）。
pub const MIN_W: f32 = 400.;
/// 大文本时卡片内容宽度上限（再按屏幕宽度夹紧）。
pub const MAX_W: f32 = 720.;

#[cfg(target_os = "macos")]
pub const SHADOW_MARGIN: f32 = 0.;
#[cfg(not(target_os = "macos"))]
pub const SHADOW_MARGIN: f32 = 12.;

const PAD: f32 = 14.;
const HEADER_H: f32 = 24.;
const GAP: f32 = 10.;
const SOURCE_SIZE: f32 = 13.;
const SOURCE_LINE: f32 = 20.;
/// 原文输入固定行数；超出后在框内滚动，不撑高窗口。
const SOURCE_ROWS: usize = 3;
const BODY_SIZE: f32 = 15.;
const BODY_LINE: f32 = 22.;

#[cfg(target_os = "macos")]
pub const PERMISSION_TEXT: &str = "Transel 需要“辅助功能”权限才能读取选中的文字。\n\
请打开 系统设置 → 隐私与安全性 → 辅助功能，打开本程序（或你用来启动它的终端）的开关，然后重新启动程序。";
#[cfg(target_os = "windows")]
pub const PERMISSION_TEXT: &str = "Transel 读不到当前选区。\n\
请先在目标窗口划词，或以相同权限运行 Transel（管理员窗口受 UIPI 限制）。也可划词后 Ctrl+C 再按翻译热键。";
#[cfg(target_os = "linux")]
pub const PERMISSION_TEXT: &str = "Transel 读不到当前选区。\n\
X11 依赖 PRIMARY 剪贴板；Wayland 请安装 wl-clipboard（wl-paste）。全局热键在纯 Wayland 应用上可能无效。";
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub const PERMISSION_TEXT: &str = "读不到当前选区。";

#[derive(Debug, Clone)]
pub enum Mode {
    /// 加载中；message 如「翻译中…」「识别中…」
    Loading(String),
    Input {
        text: String,
    },
    Batch(BatchResult),
    Error(String),
    NoPermission,
}

impl Mode {
    pub fn translating() -> Self {
        Mode::Loading("翻译中…".into())
    }
    pub fn recognizing() -> Self {
        Mode::Loading("识别中…".into())
    }
}

#[derive(Clone)]
pub struct PopupActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_pin: Arc<dyn Fn(bool) + Send + Sync>,
    pub on_speak: Arc<dyn Fn(String) + Send + Sync>,
    pub on_copy: Arc<dyn Fn(String) + Send + Sync>,
    pub on_submit_input: Arc<dyn Fn(String) + Send + Sync>,
    pub on_switch_tab: Arc<dyn Fn(usize) + Send + Sync>,
    /// Cmd+S 收藏：参数为 (source, translation, backend?)
    pub on_favorite: Arc<dyn Fn(String, String, Option<String>) + Send + Sync>,
}

pub struct TranslatePopup {
    pub mode: Mode,
    pub target_lang: String,
    pub source_lang: String,
    pub pinned: bool,
    /// 刚复制过的通道名（用于该通道复制按钮的短暂勾选态）
    pub copied_channel: Option<String>,
    pub active_tab: usize,
    pub prepared_for_history: Option<String>,
    pub was_split: bool,
    pub origin: crate::app::history::HistoryOrigin,
    pub actions: PopupActions,
    pub focus: FocusHandle,
    pub source_input: Entity<TextareaState>,
    /// 语言方向条（独立组件：触发条 + 展开面板）
    lang_dir: Entity<LangDirection>,
    /// 各翻译通道是否展开（按 backend 名）；缺省视为展开
    channel_open: HashMap<String, bool>,
    request_id: u64,
    /// 内容高度已顶到屏上限：窗口不再跟内容收，正文区滚动。
    height_capped: bool,
    pending_resize: Option<Size<Pixels>>,
    resize_anim: Option<ResizeAnim>,
    drag: Option<crate::platform::ManualDrag>,
    was_active: bool,
    /// 拆词后把处理后的文本写回输入框（需在有 Window 时执行）
    pending_input_replace: Option<String>,
    _subs: Vec<Subscription>,
}

impl TranslatePopup {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: Mode,
        target_lang: String,
        source_lang: String,
        pinned: bool,
        origin: crate::app::history::HistoryOrigin,
        actions: PopupActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = match &mode {
            Mode::Input { text } => text.clone(),
            Mode::Batch(b) => b.source.clone(),
            Mode::Loading(_) | Mode::Error(_) | Mode::NoPermission => String::new(),
        };
        let source_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(SOURCE_ROWS, SOURCE_ROWS)
                .submit_on_enter(true)
                .placeholder("输入要翻译的文字，回车翻译（Shift+回车换行）")
                .default_value(initial)
        });
        let activation = cx.observe_window_activation(window, |this: &mut Self, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active && this.pinned {
                crate::platform::apply_popup_pin(window, true);
            } else if this.was_active && !this.pinned {
                // 语言 Popover 打开时可能短暂失焦，不要关掉翻译窗
                if this.lang_dir.read(cx).is_expanded() {
                    return;
                }
                this.cancel_and_close(window, cx);
            }
        });
        let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        let enter = cx.subscribe(&source_input, |this, _, event: &InputEvent, cx| {
            match event {
                // submit_on_enter：回车提交；Shift+回车换行。
                InputEvent::PressEnter { secondary, shift } => {
                    if !secondary && !shift {
                        this.submit_source(cx);
                    }
                }
                InputEvent::Focus | InputEvent::Blur => {
                    this.source_input.update(cx, |input, cx| {
                        input.set_auto_grow(SOURCE_ROWS, SOURCE_ROWS, cx);
                    });
                    cx.notify();
                }
                InputEvent::Change => cx.notify(),
            }
        });
        let source_focused = matches!(mode, Mode::Input { .. });
        if source_focused {
            source_input.update(cx, |input, cx| input.focus(window, cx));
        }
        let lang_dir = cx.new(|_| LangDirection::new(source_lang.clone(), target_lang.clone()));
        let lang_sub = cx.subscribe(&lang_dir, |this, _, ev: &LangDirectionEvent, cx| match ev {
            LangDirectionEvent::Swap => this.swap_languages(cx),
            LangDirectionEvent::Selected { side, code } => {
                this.set_lang(*side, code.clone(), cx);
            }
        });
        let focus = cx.focus_handle();
        if !source_focused {
            focus.focus(window, cx);
        }
        Self {
            mode,
            target_lang,
            source_lang,
            pinned,
            copied_channel: None,
            active_tab: 0,
            prepared_for_history: None,
            was_split: false,
            origin,
            actions,
            focus,
            source_input,
            lang_dir,
            channel_open: HashMap::new(),
            request_id: 0,
            height_capped: false,
            pending_resize: None,
            resize_anim: None,
            pending_input_replace: None,
            drag: None,
            was_active: false,
            _subs: vec![activation, appearance, enter, lang_sub],
        }
    }

    /// 卡片自然高度出来后，窗口跟它走，只封顶到屏高上限。
    fn snap_window_to_card(&mut self, card_h: f32, window: &mut Window, cx: &mut Context<Self>) {
        if card_h < 1. {
            return;
        }
        // 已封顶时量到的是视口高度，不能用来关滚动；折叠会先 uncapped 再量。
        if self.height_capped {
            return;
        }
        let max_h = screen_limits_from_app(cx, None).max_window_h;
        let want = (card_h + SHADOW_MARGIN * 2.).max(96.);
        let h = want.min(max_h);
        let cur = window.bounds().size;
        let target = size(cur.width, px(h));
        let cap = want > max_h + 1.;
        let already = self
            .pending_resize
            .map(|p| size_near(p, target))
            .unwrap_or_else(|| size_near(cur, target));
        if !already {
            self.pending_resize = Some(target);
        }
        if cap != self.height_capped {
            self.height_capped = cap;
            cx.notify();
        } else if !already {
            window.request_animation_frame();
        }
    }

    fn tick_resize(&mut self, window: &mut Window, cx: &App) {
        if let Some(to) = self.pending_resize.take() {
            let from = self
                .resize_anim
                .as_ref()
                .map(|a| {
                    let t = ease_out_cubic(
                        (a.started.elapsed().as_secs_f32() / RESIZE_DURATION.as_secs_f32())
                            .min(1.0),
                    );
                    lerp_size(a.from, a.to, t)
                })
                .unwrap_or_else(|| window.bounds().size);
            if size_near(from, to) || cx.reduce_motion() {
                window.resize(to);
                crate::platform::refresh_shadow(window);
                self.resize_anim = None;
            } else {
                self.resize_anim = Some(ResizeAnim {
                    from,
                    to,
                    started: Instant::now(),
                });
            }
        }
        let Some(anim) = self.resize_anim.as_ref() else {
            return;
        };
        let linear = anim.started.elapsed().as_secs_f32() / RESIZE_DURATION.as_secs_f32();
        let done = linear >= 1.0;
        let t = ease_out_cubic(linear.min(1.0));
        window.resize(lerp_size(anim.from, anim.to, t));
        crate::platform::refresh_shadow(window);
        if done {
            self.resize_anim = None;
        } else {
            window.request_animation_frame();
        }
    }

    pub fn schedule_resize_from_content(&mut self, cx: &mut Context<Self>) {
        self.height_capped = false;
        cx.notify();
    }

    fn is_channel_open(&self, backend: &str) -> bool {
        self.channel_open.get(backend).copied().unwrap_or(true)
    }

    fn toggle_channel(&mut self, backend: &str, cx: &mut Context<Self>) {
        let open = !self.is_channel_open(backend);
        self.channel_open.insert(backend.to_string(), open);
        self.schedule_resize_from_content(cx);
        cx.notify();
    }

    /// 关闭弹窗并取消进行中的翻译请求。
    fn cancel_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_id = self.request_id.wrapping_add(1);
        cx.global::<AppState>().translate.cancel_active();
        self.lang_dir.update(cx, |ld, cx| ld.close(cx));
        (self.actions.on_close)();
        super::forget_view();
        let _ = cx.global_mut::<AppState>().popup.lock().take();
        window.remove_window();
    }

    /// 标题栏展示用的源语言代码（auto 时优先用识别结果）。
    fn effective_source_code(&self) -> String {
        if self.source_lang != "auto" {
            return normalize_lang_code(&self.source_lang);
        }
        match &self.mode {
            Mode::Batch(b) => b
                .first_ok()
                .and_then(|r| r.source_lang.as_deref())
                .map(normalize_lang_code)
                .unwrap_or_else(|| "auto".into()),
            _ => "auto".into(),
        }
    }

    fn source_display_label(&self) -> String {
        // 触发条只用短标签，避免「中文·自动」挤爆标题栏
        lang_label(&self.effective_source_code())
    }

    fn sync_lang_dir(&mut self, cx: &mut Context<Self>) {
        let source = self.source_lang.clone();
        let target = self.target_lang.clone();
        let label = self.source_display_label();
        self.lang_dir.update(cx, |ld, cx| {
            ld.sync(source, target, label, cx);
        });
    }

    fn persist_langs(&self, cx: &App) {
        let state = cx.global::<AppState>();
        let mut cfg = state.config.write();
        cfg.source_lang = self.source_lang.clone();
        cfg.target_lang = self.target_lang.clone();
        if let Err(e) = cfg.save(&crate::app::paths::config_file()) {
            log::warn!("保存语言配置失败: {e:#}");
        }
    }

    fn apply_lang_change(&mut self, cx: &mut Context<Self>) {
        self.lang_dir.update(cx, |ld, cx| ld.close(cx));
        self.persist_langs(cx);
        self.sync_lang_dir(cx);
        let has_text = !self.source_input.read(cx).value().trim().is_empty();
        let can_retry = has_text && !matches!(self.mode, Mode::Loading(_) | Mode::NoPermission);
        if can_retry {
            self.submit_source(cx);
        } else {
            cx.notify();
        }
    }

    fn swap_languages(&mut self, cx: &mut Context<Self>) {
        let old_target = self.target_lang.clone();
        let new_target = if self.source_lang == "auto" {
            let detected = self.effective_source_code();
            if detected == "auto" || detected == old_target {
                if old_target.eq_ignore_ascii_case("zh-CN")
                    || old_target.eq_ignore_ascii_case("zh-TW")
                {
                    "en".into()
                } else {
                    "zh-CN".into()
                }
            } else {
                detected
            }
        } else {
            normalize_lang_code(&self.source_lang)
        };
        self.source_lang = old_target;
        self.target_lang = new_target;
        self.apply_lang_change(cx);
    }

    fn set_lang(&mut self, side: LangSide, code: String, cx: &mut Context<Self>) {
        let code = normalize_lang_code(&code);
        match side {
            LangSide::Source => {
                if self.source_lang == code {
                    self.schedule_resize_from_content(cx);
                    cx.notify();
                    return;
                }
                // 源与目标相同且源不是 auto：自动把目标换到另一侧默认语言
                if code != "auto" && code == normalize_lang_code(&self.target_lang) {
                    self.target_lang = if code == "zh-CN" || code == "zh-TW" {
                        "en".into()
                    } else {
                        "zh-CN".into()
                    };
                }
                self.source_lang = code;
            }
            LangSide::Target => {
                if code == "auto" {
                    return;
                }
                if self.target_lang == code {
                    self.schedule_resize_from_content(cx);
                    cx.notify();
                    return;
                }
                if code == normalize_lang_code(&self.source_lang) {
                    self.source_lang = "auto".into();
                }
                self.target_lang = code;
            }
        }
        self.apply_lang_change(cx);
    }

    /// 用当前输入框内容发起翻译（回车）。词典与其它后端一样作为通道。
    pub fn submit_source(&mut self, cx: &mut Context<Self>) {
        let text = self.source_input.read(cx).value().to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        (self.actions.on_submit_input)(text.clone());
        let (prepared, was_split) = crate::split::prepare(&text);
        self.was_split = was_split;
        self.prepared_for_history = Some(prepared.clone());
        // 拆词后直接把处理后的文本写回输入框，不再单独展示「拆词」标记
        if was_split && prepared != text {
            self.pending_input_replace = Some(prepared.clone());
        }
        self.request_id = self.request_id.wrapping_add(1);
        let id = self.request_id;

        let svc = cx.global::<AppState>().translate.clone();
        let history = cx.global::<AppState>().history.clone();
        let origin = self.origin;
        let hint = if self.source_lang == "auto" {
            None
        } else {
            Some(self.source_lang.clone())
        };
        let target_lang = self.target_lang.clone();
        // 先遍历通道占位，再并行翻译回写
        let names = svc.backend_names();
        if names.is_empty() {
            self.apply_mode(Mode::Error("没有启用的翻译后端".into()), cx);
            return;
        }
        self.apply_mode(
            Mode::Batch(BatchResult {
                source: text.clone(),
                prepared: prepared.clone(),
                target_lang: target_lang.clone(),
                results: names.into_iter().map(SingleResult::pending_slot).collect(),
            }),
            cx,
        );
        cx.notify();

        let ctx = RequestCtx {
            source: text.clone(),
            prepared: prepared.clone(),
            source_lang_hint: hint,
            target_lang: target_lang.clone(),
            only: None,
        };
        cx.spawn(async move |this, cx| {
            use futures::StreamExt;
            let mut rx = svc.request(ctx.clone());
            while let Some(ev) = rx.next().await {
                let keep_going = this.update(cx, |this, cx| {
                    if this.request_id != id {
                        return false;
                    }
                    match ev {
                        BatchEvent::Started(batch) => {
                            this.apply_mode(Mode::Batch(batch), cx);
                            cx.notify();
                            true
                        }
                        BatchEvent::Progress(r) => {
                            if let Mode::Batch(b) = &mut this.mode {
                                b.apply_progress(r);
                                cx.notify();
                            }
                            true
                        }
                        BatchEvent::Done => {
                            if let Mode::Batch(batch) = &this.mode {
                                let was_split = ctx.source != ctx.prepared;
                                let _ = history.append(HistoryEntry {
                                    id: String::new(),
                                    ts: chrono::Utc::now(),
                                    source: ctx.source.clone(),
                                    prepared: ctx.prepared.clone(),
                                    was_split,
                                    source_lang: batch
                                        .first_ok()
                                        .and_then(|r| r.source_lang.clone()),
                                    target_lang: ctx.target_lang.clone(),
                                    origin,
                                    results: batch
                                        .results
                                        .iter()
                                        .map(|r| TranslationResult {
                                            backend: r.backend.clone(),
                                            text: r.text.clone(),
                                            source_lang: r.source_lang.clone(),
                                            success: r.ok,
                                            error: r.error.clone(),
                                            elapsed_ms: r.elapsed_ms,
                                        })
                                        .collect(),
                                    pinned: false,
                                });
                            }
                            cx.notify();
                            false
                        }
                        BatchEvent::Failed(e) => {
                            this.apply_mode(Mode::Error(e), cx);
                            cx.notify();
                            false
                        }
                    }
                });
                if !keep_going.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    /// 重试当前原文翻译。
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        if matches!(self.mode, Mode::Loading(_) | Mode::NoPermission) {
            return;
        }
        self.submit_source(cx);
    }

    /// Cmd+S：收藏当前译文。
    pub fn favorite_current(&self) {
        let source = match &self.mode {
            Mode::Batch(b) => b.source.clone(),
            Mode::Input { text } => text.clone(),
            _ => return,
        };
        let (translation, backend) = match &self.mode {
            Mode::Batch(b) => match b.first_ok() {
                Some(r) => (r.text.clone(), Some(r.backend.clone())),
                None => return,
            },
            _ => return,
        };
        if source.trim().is_empty() || translation.trim().is_empty() {
            return;
        }
        (self.actions.on_favorite)(source, translation, backend);
    }

    /// 更新展示模式，并在下次渲染时按实测内容收窗口。
    pub fn apply_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.height_capped = false;
        self.sync_lang_dir(cx);
        cx.notify();
    }

    /// 固定窗口上发起新翻译：原地换内容，不关窗、不跟鼠标跳走。
    pub fn reuse_for_request(
        &mut self,
        mode: Mode,
        ctx: Option<&RequestCtx>,
        origin: HistoryOrigin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.origin = origin;
        self.copied_channel = None;
        self.channel_open.clear();
        self.active_tab = 0;
        if let Some(ctx_inner) = ctx {
            let display = if ctx_inner.source != ctx_inner.prepared {
                ctx_inner.prepared.clone()
            } else {
                ctx_inner.source.clone()
            };
            self.source_input.update(cx, |input, cx| {
                input.set_value(display, window, cx);
            });
            self.prepared_for_history = Some(ctx_inner.prepared.clone());
            self.was_split = ctx_inner.source != ctx_inner.prepared;
        }
        self.apply_mode(mode, cx);
        crate::platform::apply_popup_pin(window, true);
        cx.notify();
    }
}

fn icon_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    color: Hsla,
    hover: Hsla,
    tip: impl Into<SharedString>,
) -> Stateful<Div> {
    icon_button_sized(id, icon, color, hover, tip, 22., 14., 6.)
}

fn compact_icon_button(
    id: impl Into<ElementId>,
    icon: &'static str,
    color: Hsla,
    hover: Hsla,
    tip: impl Into<SharedString>,
) -> Stateful<Div> {
    icon_button_sized(id, icon, color, hover, tip, 16., 10., 4.)
}

#[allow(clippy::too_many_arguments)]
fn icon_button_sized(
    id: impl Into<ElementId>,
    icon: &'static str,
    color: Hsla,
    hover: Hsla,
    tip: impl Into<SharedString>,
    size: f32,
    icon_size: f32,
    radius: f32,
) -> Stateful<Div> {
    let tip = tip.into();
    div()
        .id(id)
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(move |s| s.opacity(0.7))
        .tooltip(tooltip::build(tip))
        .child(svg().path(icon).size(px(icon_size)).text_color(color))
}

fn text_width(s: &str, font: f32) -> f32 {
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                0.56
            } else if (c as u32) < 0x2E80 {
                0.7
            } else {
                1.0
            }
        })
        .sum::<f32>()
        * font
}

fn source_input_height() -> f32 {
    SOURCE_ROWS as f32 * SOURCE_LINE + 8.
}

fn lerp_size(from: Size<Pixels>, to: Size<Pixels>, t: f32) -> Size<Pixels> {
    let t = t.clamp(0.0, 1.0);
    size(
        px(f32::from(from.width) + (f32::from(to.width) - f32::from(from.width)) * t),
        px(f32::from(from.height) + (f32::from(to.height) - f32::from(from.height)) * t),
    )
}

fn size_near(a: Size<Pixels>, b: Size<Pixels>) -> bool {
    (f32::from(a.width) - f32::from(b.width)).abs() < 0.5
        && (f32::from(a.height) - f32::from(b.height)).abs() < 0.5
}

#[derive(Clone, Copy)]
struct ScreenLimits {
    /// 卡片内容区最大宽度（不含阴影）
    max_content_w: f32,
    /// 窗口外框最大高度（含阴影）
    max_window_h: f32,
}

fn screen_limits_from_app(cx: &App, mouse: Option<(f32, f32)>) -> ScreenLimits {
    let displays = cx.displays();
    let primary = cx.primary_display();
    let display = match mouse {
        Some((mx, my)) => {
            let p = point(px(mx), px(my));
            displays
                .iter()
                .find(|d| d.bounds().contains(&p))
                .cloned()
                .or(primary)
        }
        None => primary,
    };
    let (sw, sh) = display
        .as_ref()
        .map(|d| {
            let b = d.bounds();
            (f32::from(b.size.width), f32::from(b.size.height))
        })
        .unwrap_or((1280., 800.));
    // 宽度：最多占屏宽 70%，且不超过 MAX_W；高度：上下都留空，避免贴底。
    let max_content_w = (sw * 0.7 - SHADOW_MARGIN * 2.).clamp(MIN_W, MAX_W);
    let max_window_h = popup_max_h(sh);
    ScreenLimits {
        max_content_w,
        max_window_h,
    }
}

/// 相对屏顶的偏移：再靠上一些。
fn popup_top_gap(sh: f32) -> f32 {
    (sh * 0.10).max(36.)
}

/// 贴底留白：到最大高度时下面仍露出桌面。
fn popup_bottom_gap(sh: f32) -> f32 {
    (sh * 0.10).max(48.)
}

fn popup_max_h(sh: f32) -> f32 {
    (sh - popup_top_gap(sh) - popup_bottom_gap(sh)).max(280.)
}

/// 按原文 + 译文体量估算卡片内容宽度（仅首帧开窗用）。
fn estimate_content_width(texts: &[&str], limits: ScreenLimits) -> f32 {
    let max_line = texts
        .iter()
        .flat_map(|t| t.lines())
        .map(|l| text_width(l, BODY_SIZE))
        .fold(0f32, f32::max);
    let total_chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    let from_line = (max_line + PAD * 2. + 48.).clamp(MIN_W, limits.max_content_w);
    let from_volume = if total_chars > 900 {
        limits.max_content_w
    } else if total_chars > 280 {
        let t = (total_chars as f32 - 280.) / 620.;
        MIN_W + (limits.max_content_w - MIN_W) * t.clamp(0., 1.)
    } else {
        MIN_W
    };
    from_line
        .max(from_volume)
        .clamp(MIN_W, limits.max_content_w)
}

/// 通道卡片内边距 / 标题栏 / 卡片间距（与 render_batch 保持一致）。
const CHANNEL_CARD_PAD: f32 = 6.;
const CHANNEL_HEADER_H: f32 = 20.;
const CHANNEL_CARD_GAP: f32 = 8.;

pub fn content_size(
    cx: &App,
    mode: &Mode,
    _target_lang: &str,
    mouse: Option<(f32, f32)>,
) -> Size<Pixels> {
    let limits = screen_limits_from_app(cx, mouse);
    let text: &str = match mode {
        Mode::Input { text } => text,
        Mode::Batch(b) => &b.prepared,
        Mode::Loading(msg) | Mode::Error(msg) => msg,
        Mode::NoPermission => PERMISSION_TEXT,
    };
    let w = estimate_content_width(&[text], limits);
    size(
        px(w + SHADOW_MARGIN * 2.),
        px(200f32.min(limits.max_window_h)),
    )
}

impl Render for TranslatePopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 与全局钉住状态同步（热键 / 托盘切换后刷新）
        let global_pin = cx.global::<AppState>().pinned.load(Ordering::Relaxed);
        if self.pinned != global_pin {
            self.pinned = global_pin;
        }

        if let Some(text) = self.pending_input_replace.take() {
            self.source_input.update(cx, |input, cx| {
                input.set_value(text, window, cx);
            });
        }

        self.tick_resize(window, cx);

        let p = palette(window.appearance());

        let pin_svg = if self.pinned {
            icons::PIN_OFF
        } else {
            icons::PIN
        };

        // 仅标题栏拖动窗口，正文区留给文本选择
        let header = div()
            .id("popup_header")
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .h(px(HEADER_H))
            .gap(px(8.))
            .cursor_default()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _cx| this.drag = crate::platform::start_drag(window)),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, _cx| {
                if let Some(d) = &this.drag {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        crate::platform::drag_move(d);
                    } else {
                        this.drag = None;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _cx| this.drag = None),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(img(icons::LOGO).size(px(16.)).rounded(px(3.)))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.title)
                            .child("Transel"),
                    ),
            )
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.lang_dir.clone()),
            )
            .child(div().flex_1())
            .child(
                icon_button(
                    "btn_pin",
                    pin_svg,
                    p.muted,
                    p.hover,
                    if self.pinned {
                        "取消固定"
                    } else {
                        "固定窗口"
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.pinned = !this.pinned;
                        cx.global::<AppState>()
                            .pinned
                            .store(this.pinned, Ordering::Relaxed);
                        crate::platform::apply_popup_pin(window, this.pinned);
                        (this.actions.on_pin)(this.pinned);
                        cx.notify();
                    }),
                ),
            )
            .child(
                icon_button("btn_close", icons::CLOSE, p.muted, p.hover, "关闭").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.cancel_and_close(window, cx);
                    }),
                ),
            );

        // 主体内容
        let main: AnyElement = match &self.mode {
            Mode::Loading(msg) => loading::loading_row(msg.clone(), &p),
            Mode::Error(e) => selectable_plain("popup_error", e, p.danger, 13.5, 20.),
            Mode::NoPermission => {
                selectable_plain("popup_permission", PERMISSION_TEXT, p.text, 13.5, 20.)
            }
            Mode::Input { .. } => selectable_plain(
                "popup_input_hint",
                "在上方输入原文，回车翻译（Shift+回车换行）",
                p.muted,
                13.,
                20.,
            ),
            Mode::Batch(b) => self.render_batch(&p, b, cx),
        };

        let show_source = !matches!(self.mode, Mode::NoPermission);
        let source_line = div()
            .id("source_input_wrap")
            .w_full()
            .flex_shrink_0()
            .h(px(source_input_height()))
            .overflow_hidden()
            .px(px(6.))
            .py(px(4.))
            .rounded(px(8.))
            .bg(p.pill_bg)
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.activate_window();
                    this.source_input
                        .update(cx, |input, cx| input.focus(window, cx));
                }),
            )
            .child(
                Textarea::new(&self.source_input)
                    .appearance(false)
                    .bordered(false)
                    .w_full()
                    .text_size(px(SOURCE_SIZE))
                    .line_height(px(SOURCE_LINE))
                    .text_color(p.text),
            );

        let stack = div()
            .id("popup_stack")
            .w_full()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(GAP))
            .when(show_source, |el| el.child(source_line))
            .when(show_source, |el| {
                el.child(div().h(px(1.)).w_full().flex_shrink_0().bg(p.divider))
            })
            .child(main);

        let on_activate = cx.listener(|_, _, window, _cx| {
            window.activate_window();
        });
        let body = if self.height_capped {
            div()
                .id("popup_body")
                .w_full()
                .min_w(px(0.))
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scrollbar()
                .cursor_text()
                .on_mouse_down(MouseButton::Left, on_activate)
                .child(stack)
                .into_any_element()
        } else {
            div()
                .id("popup_body")
                .w_full()
                .min_w(px(0.))
                .flex_shrink_0()
                .cursor_text()
                .on_mouse_down(MouseButton::Left, on_activate)
                .child(stack)
                .into_any_element()
        };

        let card = div()
            .id("card")
            .w_full()
            .when(self.height_capped, |el| el.h_full().overflow_hidden())
            .flex()
            .flex_col()
            .gap(px(GAP))
            .p(px(PAD))
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .border_1()
            .border_color(p.border)
            .bg(p.card)
            .when(SHADOW_MARGIN > 0., |el| {
                el.shadow(vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(4.)),
                    blur_radius: px(10.),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .font_family(".SystemUIFont")
            .cursor_default()
            .child(header)
            .child(body);

        let view = cx.entity();
        div()
            .id("popup")
            .key_context(crate::shared::actions::POPUP_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_close_window))
            .on_action(cx.listener(Self::on_toggle_pin))
            .on_action(cx.listener(Self::on_retry))
            .on_action(cx.listener(Self::on_favorite))
            .on_action(cx.listener(Self::on_speak))
            .on_action(cx.listener(Self::on_copy_result))
            .size_full()
            .p(px(SHADOW_MARGIN))
            .child(
                div()
                    .w_full()
                    .when(self.height_capped, |el| el.h_full())
                    .on_children_prepainted(move |bounds, window, cx| {
                        let Some(b) = bounds.first() else {
                            return;
                        };
                        view.update(cx, |this, cx| {
                            this.snap_window_to_card(f32::from(b.size.height), window, cx);
                        });
                    })
                    .child(card),
            )
    }
}

impl TranslatePopup {
    fn on_close_window(
        &mut self,
        _: &crate::shared::actions::CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lang_dir.read(cx).is_expanded() {
            self.lang_dir.update(cx, |ld, cx| ld.close(cx));
            cx.notify();
            return;
        }
        self.cancel_and_close(window, cx);
    }

    fn on_toggle_pin(
        &mut self,
        _: &crate::shared::actions::TogglePin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pinned = !self.pinned;
        cx.global::<AppState>()
            .pinned
            .store(self.pinned, Ordering::Relaxed);
        crate::platform::apply_popup_pin(window, self.pinned);
        (self.actions.on_pin)(self.pinned);
        cx.notify();
    }

    fn on_retry(
        &mut self,
        _: &crate::shared::actions::RetryTranslate,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retry(cx);
    }

    fn on_favorite(
        &mut self,
        _: &crate::shared::actions::Favorite,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.favorite_current();
    }

    fn on_speak(
        &mut self,
        _: &crate::shared::actions::Speak,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        // Prefer current translation text when available.
        let text = match &self.mode {
            Mode::Batch(b) => b.first_ok().map(|r| r.text.clone()),
            _ => None,
        };
        if let Some(t) = text {
            (self.actions.on_speak)(t);
        }
    }

    fn on_copy_result(
        &mut self,
        _: &crate::shared::actions::CopyResult,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let text = match &self.mode {
            Mode::Batch(b) => b.first_ok().map(|r| r.text.clone()),
            Mode::Error(e) => Some(e.clone()),
            _ => None,
        };
        if let Some(t) = text {
            (self.actions.on_copy)(t);
        }
    }
}

impl TranslatePopup {
    fn render_batch(&self, p: &Palette, b: &BatchResult, cx: &mut Context<Self>) -> AnyElement {
        if b.results.is_empty() {
            return div()
                .text_size(px(13.5))
                .line_height(px(20.))
                .text_color(p.muted)
                .child("没有结果")
                .into_any_element();
        }

        // 卡片 + Collapsible 展开动画；左侧通道 icon，右侧 chevron。
        let mut cards: Vec<AnyElement> = Vec::new();
        for (i, r) in b.results.iter().enumerate() {
            let open = self.is_channel_open(&r.backend);
            let icon = icons::channel_icon(&r.backend);
            let name_color = if r.ok || r.pending { p.title } else { p.danger };

            let body = if r.pending && r.text.is_empty() {
                loading::loading_row_with_id(
                    "翻译中…",
                    SharedString::from(format!("channel_shimmer_{i}")),
                    p,
                )
            } else if r.pending || r.ok {
                selectable_markdown(
                    ("batch_body", i as u32),
                    &r.text,
                    p.text,
                    BODY_SIZE,
                    BODY_LINE,
                    r.pending,
                )
            } else {
                selectable_plain(
                    ("batch_err", i),
                    r.error.as_deref().unwrap_or("未知错误"),
                    p.danger,
                    13.,
                    19.,
                )
            };

            let body_scroll = div()
                .w_full()
                .min_w(px(0.))
                .overflow_x_hidden()
                .flex_shrink_0()
                .child(body);

            let backend = r.backend.clone();
            let header = div()
                .id(SharedString::from(format!("batch_toggle_{i}")))
                .w_full()
                .h(px(CHANNEL_HEADER_H))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_channel(&backend, cx);
                    }),
                )
                .child(Icon::default().path(icon).size(px(11.)).text_color(p.muted))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_x_hidden()
                        .text_size(px(11.))
                        .line_height(px(CHANNEL_HEADER_H))
                        .font_weight(FontWeight::MEDIUM)
                        .font_family(".SystemUIFont")
                        .text_color(name_color)
                        .map(|el| {
                            if r.pending {
                                el.child(
                                    ShimmerText::new(r.backend.clone())
                                        .id(SharedString::from(format!("channel_name_{i}")))
                                        .text_size(px(11.))
                                        .line_height(px(CHANNEL_HEADER_H))
                                        .font_weight(FontWeight::MEDIUM)
                                        .font_family(".SystemUIFont")
                                        .text_color(name_color)
                                        .highlight_color(p.accent),
                                )
                            } else {
                                el.child(r.backend.clone())
                            }
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(1.))
                        .flex_shrink_0()
                        .child({
                            let copied = self.copied_channel.as_deref() == Some(r.backend.as_str());
                            let text = r.text.clone();
                            let backend = r.backend.clone();
                            let enabled = r.ok && !text.is_empty();
                            let copy_color = if copied {
                                p.accent
                            } else if enabled {
                                p.muted
                            } else {
                                p.muted.opacity(0.4)
                            };
                            compact_icon_button(
                                SharedString::from(format!("ch_copy_{i}")),
                                if copied { icons::CHECK } else { icons::COPY },
                                copy_color,
                                p.hover,
                                if copied { "已复制" } else { "复制" },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if text.trim().is_empty() {
                                        return;
                                    }
                                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                                    (this.actions.on_copy)(text.clone());
                                    this.copied_channel = Some(backend.clone());
                                    cx.notify();
                                    let backend = backend.clone();
                                    cx.spawn(async move |this, cx| {
                                        cx.background_executor()
                                            .timer(Duration::from_millis(1200))
                                            .await;
                                        let _ = this.update(cx, |this, cx| {
                                            if this.copied_channel.as_deref()
                                                == Some(backend.as_str())
                                            {
                                                this.copied_channel = None;
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .detach();
                                }),
                            )
                        })
                        .child({
                            let text = r.text.clone();
                            let speak_color = if r.ok && !text.is_empty() {
                                p.muted
                            } else {
                                p.muted.opacity(0.4)
                            };
                            compact_icon_button(
                                SharedString::from(format!("ch_speak_{i}")),
                                icons::SPEAKER,
                                speak_color,
                                p.hover,
                                "朗读",
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if text.trim().is_empty() {
                                        return;
                                    }
                                    (this.actions.on_speak)(text.clone());
                                }),
                            )
                        })
                        .child(
                            // 右侧箭头：收起朝右，展开旋转 90° 朝下
                            div()
                                .size(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::default()
                                        .path(icons::CHEVRON_RIGHT)
                                        .size(px(10.))
                                        .text_color(p.muted)
                                        .rotate(radians(if open {
                                            std::f32::consts::FRAC_PI_2
                                        } else {
                                            0.
                                        })),
                                ),
                        ),
                );

            cards.push(
                div()
                    .id(SharedString::from(format!("batch_card_{i}")))
                    .w_full()
                    .min_w(px(0.))
                    .flex_shrink_0()
                    .overflow_x_hidden()
                    .px(px(CHANNEL_CARD_PAD))
                    .py(px(CHANNEL_CARD_PAD))
                    .rounded(px(10.))
                    .bg(p.pill_bg)
                    .child(
                        Collapsible::new()
                            .w_full()
                            .min_w(px(0.))
                            .gap(px(6.))
                            .open(open)
                            .motion_id(SharedString::from(format!("batch_col_motion_{i}")))
                            .child(header)
                            .content(
                                div()
                                    .w_full()
                                    .min_w(px(0.))
                                    .overflow_x_hidden()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(div().h(px(1.)).w_full().bg(p.divider))
                                    .child(body_scroll),
                            ),
                    )
                    .into_any_element(),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(CHANNEL_CARD_GAP))
            .children(cards)
            .into_any_element()
    }
}

/// 计算弹窗位置。
///
/// - `follow_mouse == true`：跟在鼠标右下方（无鼠标时退回中上方）
/// - `follow_mouse == false`：固定在目标显示器水平居中、垂直偏上
pub fn popup_bounds(
    cx: &App,
    mouse: Option<(f32, f32)>,
    size: Size<Pixels>,
    follow_mouse: bool,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let displays = cx.displays();
    let primary = cx.primary_display();

    let display = if follow_mouse {
        if let Some((mx, my)) = mouse {
            let p = point(px(mx), px(my));
            displays
                .iter()
                .find(|d| d.bounds().contains(&p))
                .cloned()
                .or(primary)
        } else {
            primary
        }
    } else {
        primary
    };

    let mut id = None;
    let screen = display.as_ref().map(|d| d.bounds()).unwrap_or_default();
    let (left, top) = (f32::from(screen.origin.x), f32::from(screen.origin.y));
    let (sw, sh) = (f32::from(screen.size.width), f32::from(screen.size.height));
    let (right, bottom) = (left + sw, top + sh);

    let top_gap = popup_top_gap(sh);
    let bottom_gap = popup_bottom_gap(sh);
    let (mut x, mut y) = if follow_mouse {
        if let Some((mx, my)) = mouse {
            (mx + 8., my + 14.)
        } else {
            (left + (sw - w) / 2., top + top_gap)
        }
    } else {
        (left + (sw - w) / 2., top + top_gap)
    };

    if let Some(d) = display {
        id = Some(d.id());
        if follow_mouse
            && let Some((_mx, my)) = mouse
            && y + h.max(220.) > bottom - bottom_gap
        {
            y = my - h.max(220.) - 8.;
        }
        x = x.min(right - w - 4.).max(left + 4.);
        y = y.min(bottom - h - bottom_gap).max(top + top_gap);
    }
    (Bounds::new(point(px(x), px(y)), size), id)
}