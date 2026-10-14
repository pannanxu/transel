//! 翻译弹窗。
//!
//! 设计原则：
//! - 状态由外部 [`PopupController`] 推动（主循环 / 设置面板 / OCR 触发器等）。
//! - 视图负责渲染，以及把关闭/固定/复制/朗读等事件转发到 [`PopupActions`]。
//! - 原文输入框在回车时由视图向翻译服务发请求，避免回调拿不到当前弹窗句柄。

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui_kit::component::Sizable;
use gpui_kit::component::animation::ease_out_cubic;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 翻译窗尺寸变化（内容增高、语言菜单展开等）过渡时长。
const RESIZE_DURATION: Duration = Duration::from_millis(180);

struct ResizeAnim {
    from: Size<Pixels>,
    to: Size<Pixels>,
    started: Instant,
}

use super::text_select::selectable_plain;

use crate::app::history::{HistoryEntry, TranslationResult};
use crate::app::state::AppState;
use crate::services::dictionary::{Dictionary, DictionaryEntry};
use crate::services::translate::{BatchEvent, BatchResult, RequestCtx, SingleResult};
use crate::ui::lang_direction::{LangDirection, LangDirectionEvent, LangSide};
use crate::ui::loading;
use crate::ui::theme::{Palette, lang_label, normalize_lang_code, palette};

use super::icons;

/// 翻译弹窗固定宽度（卡片内容区；不含阴影边距）。
pub const FIXED_W: f32 = 400.;
pub const MAX_H: f32 = 520.;

pub const MIN_W: f32 = FIXED_W;
pub const MAX_W: f32 = FIXED_W;

/// 含阴影边距的窗口外框宽度（始终固定）。
pub fn window_width() -> f32 {
    FIXED_W + SHADOW_MARGIN * 2.
}

#[cfg(target_os = "macos")]
pub const SHADOW_MARGIN: f32 = 0.;
#[cfg(not(target_os = "macos"))]
pub const SHADOW_MARGIN: f32 = 12.;

const PAD: f32 = 14.;
const HEADER_H: f32 = 24.;
const GAP: f32 = 10.;
const SOURCE_SIZE: f32 = 12.;
const SOURCE_LINE: f32 = 20.;
const SOURCE_MAX_ROWS: usize = 8;
const BODY_SIZE: f32 = 15.;
const BODY_LINE: f32 = 22.;

pub fn window_background() -> WindowBackgroundAppearance {
    WindowBackgroundAppearance::Transparent
}

pub const PERMISSION_TEXT: &str = "划词翻译需要“辅助功能”权限才能读取选中的文字。\n\
请打开 系统设置 → 隐私与安全性 → 辅助功能，打开本程序（或你用来启动它的终端）的开关，然后重新启动程序。";

#[derive(Debug, Clone)]
pub enum Mode {
    /// 加载中；message 如「翻译中…」「识别中…」
    Loading(String),
    Input { text: String },
    Batch(BatchResult),
    Dictionary { entry: Box<DictionaryEntry>, source_text: String, translation: Option<String> },
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
    pub copied: bool,
    pub active_tab: usize,
    pub prepared_for_history: Option<String>,
    pub was_split: bool,
    pub origin: crate::app::history::HistoryOrigin,
    pub actions: PopupActions,
    pub focus: FocusHandle,
    pub source_input: Entity<TextareaState>,
    /// 语言方向条（独立组件：触发条 + 展开面板）
    lang_dir: Entity<LangDirection>,
    request_id: u64,
    pending_resize: Option<Size<Pixels>>,
    /// 窗口尺寸过渡；有值时在 render 里按帧插值 `window.resize`
    resize_anim: Option<ResizeAnim>,
    drag: Option<crate::platform::ManualDrag>,
    was_active: bool,
    _subs: Vec<Subscription>,
}

impl TranslatePopup {
    pub fn new(mode: Mode, target_lang: String, source_lang: String, pinned: bool, origin: crate::app::history::HistoryOrigin, actions: PopupActions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial = match &mode {
            Mode::Input { text } => text.clone(),
            Mode::Batch(b) => b.source.clone(),
            Mode::Dictionary { source_text, .. } => source_text.clone(),
            Mode::Loading(_) | Mode::Error(_) | Mode::NoPermission => String::new(),
        };
        let source_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, SOURCE_MAX_ROWS)
                .submit_on_enter(true)
                .placeholder("输入要翻译的文字，回车翻译（Shift+回车换行）")
                .default_value(initial)
        });
        let activation = cx.observe_window_activation(window, |this: &mut Self, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active && !this.pinned {
                // 语言 Popover 打开时可能短暂失焦，不要关掉翻译窗
                if this.lang_dir.read(cx).is_expanded() {
                    return;
                }
                window.remove_window();
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
                InputEvent::Focus => {
                    this.source_input.update(cx, |input, cx| {
                        input.set_auto_grow(1, SOURCE_MAX_ROWS, cx);
                    });
                    this.schedule_resize_from_content(cx);
                    cx.notify();
                }
                InputEvent::Blur => {
                    // 失焦后若仍是单行文本，收回到一行；多行/超长内容保持展开以便阅读。
                    let text = this.source_input.read(cx).value().to_string();
                    let keep_multi = text.contains('\n') || source_needs_wrap(&text);
                    this.source_input.update(cx, |input, cx| {
                        if keep_multi {
                            input.set_auto_grow(1, SOURCE_MAX_ROWS, cx);
                        } else {
                            input.set_auto_grow(1, 1, cx);
                        }
                    });
                    this.schedule_resize_from_content(cx);
                    cx.notify();
                }
                InputEvent::Change => {
                    this.schedule_resize_from_content(cx);
                    cx.notify();
                }
            }
        });
        let source_focused = matches!(mode, Mode::Input { .. });
        if source_focused {
            source_input.update(cx, |input, cx| input.focus(window, cx));
        }
        let lang_dir = cx.new(|_| LangDirection::new(source_lang.clone(), target_lang.clone()));
        let lang_sub = cx.subscribe(&lang_dir, |this, _, ev: &LangDirectionEvent, cx| {
            match ev {
                LangDirectionEvent::Swap => this.swap_languages(cx),
                LangDirectionEvent::Selected { side, code } => {
                    this.set_lang(*side, code.clone(), cx);
                }
            }
        });
        Self {
            mode,
            target_lang,
            source_lang,
            pinned,
            copied: false,
            active_tab: 0,
            prepared_for_history: None,
            was_split: false,
            origin,
            actions,
            focus: cx.focus_handle(),
            source_input,
            lang_dir,
            request_id: 0,
            pending_resize: None,
            resize_anim: None,
            drag: None,
            was_active: false,
            _subs: vec![activation, appearance, enter, lang_sub],
        }
    }

    /// 应用待处理的尺寸目标：所有增高/收矮都走短过渡，避免弹一下。
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
        // 以输入框当前原文为准估算高度，避免 auto-grow 后窗口仍按旧单行高度裁切。
        let source = self.source_input.read(cx).value().to_string();
        let size = match &self.mode {
            Mode::Batch(b) => estimate_batch_size(&source, b),
            Mode::Dictionary { entry, .. } => estimate_size(&source, &entry.word, 0),
            Mode::Input { .. } => estimate_size(&source, "输入要翻译的文字", 0),
            Mode::Loading(msg) => {
                let mut s = estimate_size(&source, msg, 0);
                s.height = px(s.height.to_f64().max(96.) as f32);
                s
            }
            Mode::Error(_) => estimate_size(&source, "错误", 0),
            Mode::NoPermission => content_size(&self.mode, &self.target_lang),
        };
        self.pending_resize = Some(size);
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
            Mode::Dictionary { .. } => "en".into(),
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
                if old_target.eq_ignore_ascii_case("zh-CN") || old_target.eq_ignore_ascii_case("zh-TW") {
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

    /// 用当前输入框内容发起翻译（回车）；单词优先查词典。
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
        self.request_id = self.request_id.wrapping_add(1);
        let id = self.request_id;

        let try_dict = Dictionary::is_word(&text) && !was_split;
        let dict = cx.global::<AppState>().dictionary.clone();
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
                results: names
                    .into_iter()
                    .map(SingleResult::pending_slot)
                    .collect(),
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
            let dict_entry = if try_dict {
                cx.background_executor()
                    .spawn(async move { dict.lookup(&text).ok() })
                    .await
            } else {
                None
            };
            let mut rx = svc.request(ctx.clone());
            while let Some(ev) = rx.next().await {
                let keep_going = this.update(cx, |this, cx| {
                    if this.request_id != id {
                        return false;
                    }
                    match ev {
                        BatchEvent::Started(batch) => {
                            // 先列出全部通道，再并行回写各通道结果
                            this.apply_mode(Mode::Batch(batch), cx);
                            cx.notify();
                            true
                        }
                        BatchEvent::Progress(r) => {
                            if let Mode::Batch(b) = &mut this.mode {
                                b.apply_progress(r);
                                this.pending_resize = Some(estimate_batch_size(
                                    &this.source_input.read(cx).value(),
                                    b,
                                ));
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
                                if let Some(entry) = &dict_entry {
                                    this.apply_mode(
                                        Mode::Dictionary {
                                            entry: Box::new((**entry).clone()),
                                            source_text: ctx.source.clone(),
                                            translation: batch
                                                .first_ok()
                                                .map(|r| r.text.clone()),
                                        },
                                        cx,
                                    );
                                }
                            }
                            cx.notify();
                            false
                        }
                        BatchEvent::Failed(e) => {
                            if let Some(entry) = &dict_entry {
                                this.apply_mode(
                                    Mode::Dictionary {
                                        entry: Box::new((**entry).clone()),
                                        source_text: ctx.source.clone(),
                                        translation: None,
                                    },
                                    cx,
                                );
                            } else {
                                this.apply_mode(Mode::Error(e), cx);
                            }
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
            Mode::Dictionary { source_text, .. } => source_text.clone(),
            Mode::Input { text } => text.clone(),
            _ => return,
        };
        let (translation, backend) = match &self.mode {
            Mode::Batch(b) => match b.first_ok() {
                Some(r) => (r.text.clone(), Some(r.backend.clone())),
                None => return,
            },
            Mode::Dictionary {
                entry,
                translation,
                ..
            } => (
                translation
                    .clone()
                    .unwrap_or_else(|| entry.word.clone()),
                None,
            ),
            _ => return,
        };
        if source.trim().is_empty() || translation.trim().is_empty() {
            return;
        }
        (self.actions.on_favorite)(source, translation, backend);
    }

    /// 更新展示模式，并在下次渲染时按内容重算窗口大小。
    pub fn apply_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.sync_lang_dir(cx);
        self.pending_resize = Some(content_size(&self.mode, &self.target_lang));
    }
}

fn icon_button(id: &'static str, icon: &'static str, color: Hsla, hover: Hsla) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(move |s| s.opacity(0.7))
        .child(svg().path(icon).size(px(14.)).text_color(color))
}

fn text_width(s: &str, font: f32) -> f32 {
    s.chars()
        .map(|c| if c.is_ascii() { 0.56 } else if (c as u32) < 0x2E80 { 0.7 } else { 1.0 })
        .sum::<f32>()
        * font
}

fn line_count(s: &str, font: f32, width: f32) -> f32 {
    s.lines()
        .map(|l| (text_width(l, font) / width).ceil().max(1.))
        .sum::<f32>()
        .max(1.)
}

fn source_input_height(source: &str) -> f32 {
    let rows = source
        .lines()
        .count()
        .max(1)
        .min(SOURCE_MAX_ROWS) as f32;
    // 粗估：每行 SOURCE_LINE，外加内边距。
    rows * SOURCE_LINE + 8.
}

/// 文本是否可能因宽度而换行（用于失焦时决定是否保持多行高度）。
fn source_needs_wrap(s: &str) -> bool {
    text_width(s.lines().next().unwrap_or(""), SOURCE_SIZE) > (MAX_W - PAD * 2. - 40.)
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

pub fn estimate_size(source: &str, body: &str, tabs: usize) -> Size<Pixels> {
    let inner = FIXED_W - PAD * 2.;
    let source_h = source_input_height(source);
    let h = PAD * 2.
        + HEADER_H
        + GAP
        + source_h
        + GAP * 2.
        + 1.
        + 24. * (tabs.max(1) as f32)
        + line_count(body, BODY_SIZE, inner) * BODY_LINE
        + 4.;
    // 宽度恒定；仅高度随内容变化。
    size(px(window_width()), px(h.min(MAX_H) + SHADOW_MARGIN * 2.))
}

/// 多通道全部展开：每个后端标题 + 正文都计入高度。
fn estimate_batch_size(source: &str, b: &BatchResult) -> Size<Pixels> {
    let inner = FIXED_W - PAD * 2.;
    let source_h = source_input_height(source);
    let mut body_h = 0.;
    for (i, r) in b.results.iter().enumerate() {
        let text = if r.ok {
            r.text.as_str()
        } else {
            r.error.as_deref().unwrap_or("错误")
        };
        // 标题行 + 正文 + 通道间距 / 分隔线
        body_h += 18. + line_count(text, BODY_SIZE, inner) * BODY_LINE + 8.;
        if i + 1 < b.results.len() {
            body_h += 8.;
        }
    }
    if b.results.is_empty() {
        body_h = BODY_LINE;
    }
    let h = PAD * 2. + HEADER_H + GAP + source_h + GAP * 2. + 1. + body_h + 4.;
    size(px(window_width()), px(h.min(MAX_H) + SHADOW_MARGIN * 2.))
}

pub fn content_size(mode: &Mode, target_lang: &str) -> Size<Pixels> {
    match mode {
        // Spinner + 文案需要稍高的行高
        Mode::Loading(msg) => {
            let mut s = estimate_size("", msg, 0);
            s.height = px(s.height.to_f64().max(96.) as f32);
            s
        },
        Mode::Error(_) => estimate_size("", "错误", 0),
        Mode::NoPermission => estimate_size("需要辅助功能权限", PERMISSION_TEXT, 0),
        Mode::Input { text } => estimate_size(text, "输入要翻译的文字", 0),
        Mode::Batch(b) => estimate_batch_size(&b.prepared, b),
        Mode::Dictionary { entry, source_text, .. } => estimate_size(source_text, &entry.word, 0),
    }
}

#[allow(dead_code)]
fn _unused_marker() {}

impl Render for TranslatePopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 与全局钉住状态同步（热键 / 托盘切换后刷新）
        let global_pin = cx.global::<AppState>().pinned.load(Ordering::Relaxed);
        if self.pinned != global_pin {
            self.pinned = global_pin;
        }

        self.tick_resize(window, cx);

        let p = palette(window.appearance());

        let (show_pin, copy_text, speak_text) = match &self.mode {
            Mode::Batch(b) => {
                let text = b.first_ok().map(|r| r.text.clone());
                (true, text.clone(), text)
            }
            Mode::Dictionary { entry, .. } => (true, Some(entry.word.clone()), Some(entry.word.clone())),
            _ => (true, None, None),
        };
        let show_copy = copy_text.is_some();
        let show_speak = speak_text.is_some();

        let pin_svg = if self.pinned { icons::PIN_OFF } else { icons::PIN };
        let copy_svg = if self.copied { icons::CHECK } else { icons::COPY };
        let copy_color = if self.copied { p.accent } else { p.muted };

        // 仅标题栏拖动窗口，正文区留给文本选择
        let header = div()
            .id("popup_header")
            .flex()
            .flex_row()
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
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _cx| this.drag = None))
            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(p.title).child("划词翻译"))
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.lang_dir.clone()),
            )
            .child(div().flex_1())
            .when(show_speak, |el| {
                let t = speak_text.clone().unwrap_or_default();
                el.child(icon_button("btn_speak", icons::SPEAKER, p.muted, p.hover).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        (this.actions.on_speak)(t.clone());
                    }),
                ))
            })
            .when(show_pin, |el| {
                el.child(icon_button("btn_pin", pin_svg, p.muted, p.hover).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.pinned = !this.pinned;
                        cx.global::<AppState>().pinned.store(this.pinned, Ordering::Relaxed);
                        (this.actions.on_pin)(this.pinned);
                        cx.notify();
                    }),
                ))
            })
            .when(show_copy, |el| {
                let t = copy_text.clone().unwrap_or_default();
                el.child(icon_button("btn_copy", copy_svg, copy_color, p.hover).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        cx.write_to_clipboard(ClipboardItem::new_string(t.clone()));
                        (this.actions.on_copy)(t.clone());
                        this.copied = true;
                        cx.notify();
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(Duration::from_millis(1200)).await;
                            let _ = this.update(cx, |this, cx| {
                                this.copied = false;
                                cx.notify();
                            });
                        })
                        .detach();
                    }),
                ))
            })
            .child(icon_button("btn_close", icons::CLOSE, p.muted, p.hover).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.lang_dir.update(cx, |ld, cx| ld.close(cx));
                    (this.actions.on_close)();
                    window.remove_window();
                }),
            ));

        // 主体内容
        let main: AnyElement = match &self.mode {
            Mode::Loading(msg) => loading::loading_row(msg.clone(), &p),
            Mode::Error(e) => selectable_plain("popup_error", e, p.danger, 13.5, 20.),
            Mode::NoPermission => selectable_plain("popup_permission", PERMISSION_TEXT, p.text, 13.5, 20.),
            Mode::Input { .. } => selectable_plain(
                "popup_input_hint",
                "在上方输入原文，回车翻译（Shift+回车换行）",
                p.muted,
                13.,
                20.,
            ),
            Mode::Batch(b) => self.render_batch(&p, b, cx),
            Mode::Dictionary { entry, translation, source_text } => self.render_dictionary(&p, entry, source_text, translation.clone()),
        };

        let show_source = !matches!(self.mode, Mode::NoPermission);
        let source_line = div()
            .flex()
            .flex_row()
            .gap(px(6.))
            .items_start()
            .when(self.was_split, |el| {
                el.child(
                    div()
                        .flex_none()
                        .mt(px(6.))
                        .px(px(5.))
                        .rounded(px(4.))
                        .bg(p.pill_bg)
                        .text_size(px(10.))
                        .text_color(p.muted)
                        .child("拆词"),
                )
            })
            .child(
                div()
                    .id("source_input_wrap")
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(28.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .bg(p.pill_bg)
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            window.activate_window();
                            this.source_input.update(cx, |input, cx| input.focus(window, cx));
                        }),
                    )
                    .child(
                        Textarea::new(&self.source_input)
                            .appearance(false)
                            .bordered(false)
                            .small()
                            .w_full()
                            .text_size(px(SOURCE_SIZE))
                            .text_color(p.muted),
                    ),
            );

        let card = div()
            .id("card")
            .size_full()
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
            .when(show_source, |el| el.child(source_line))
            .when(show_source, |el| el.child(div().h(px(1.)).w_full().bg(p.divider)))
            .child(
                div()
                    .id("popup_body")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scrollbar()
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, window, _cx| {
                            // 弹窗默认不抢焦点；开始选择时激活以便 Cmd/Ctrl+C
                            window.activate_window();
                        }),
                    )
                    .child(main),
            );

        div()
            .id("popup")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let key = ev.keystroke.key.as_str();
                let mods = &ev.keystroke.modifiers;
                if key == "escape" {
                    if this.lang_dir.read(cx).is_expanded() {
                        this.lang_dir.update(cx, |ld, cx| ld.close(cx));
                        cx.notify();
                        return;
                    }
                    (this.actions.on_close)();
                    window.remove_window();
                    return;
                }
                // Cmd/Ctrl+W 关闭；Cmd/Ctrl+P 钉住；Cmd/Ctrl+R 重试
                let chord = mods.platform || mods.control;
                if chord && key.eq_ignore_ascii_case("w") {
                    (this.actions.on_close)();
                    window.remove_window();
                } else if chord && key.eq_ignore_ascii_case("p") {
                    this.pinned = !this.pinned;
                    cx.global::<AppState>()
                        .pinned
                        .store(this.pinned, Ordering::Relaxed);
                    (this.actions.on_pin)(this.pinned);
                    cx.notify();
                } else if chord && key.eq_ignore_ascii_case("r") {
                    this.retry(cx);
                } else if chord && key.eq_ignore_ascii_case("s") {
                    this.favorite_current();
                }
            }))
            .w(px(window_width()))
            .h_full()
            .p(px(SHADOW_MARGIN))
            .child(card)
    }
}

impl TranslatePopup {
    fn render_batch(&self, p: &Palette, b: &BatchResult, _cx: &mut Context<Self>) -> AnyElement {
        if b.results.is_empty() {
            return div()
                .text_size(px(13.5))
                .line_height(px(20.))
                .text_color(p.muted)
                .child("没有结果")
                .into_any_element();
        }

        // 多通道：先枚举全部通道，并行翻译；每个后端一个 Collapsible。
        // 文档：https://gpui-kit.com/zh-CN/component/collapsible/
        let mut sections: Vec<AnyElement> = Vec::new();
        for (i, r) in b.results.iter().enumerate() {
            let (title, title_color) = if r.pending {
                (format!("{} · 翻译中…", r.backend), p.muted)
            } else if r.ok {
                (r.backend.clone(), p.muted)
            } else {
                (format!("{} · 失败", r.backend), p.danger)
            };
            let body = if r.pending {
                loading::loading_row("翻译中…", p)
            } else if r.ok {
                selectable_plain(("batch_body", i), &r.text, p.text, BODY_SIZE, BODY_LINE)
            } else {
                selectable_plain(
                    ("batch_err", i),
                    r.error.as_deref().unwrap_or("未知错误"),
                    p.danger,
                    13.,
                    19.,
                )
            };
            sections.push(
                Collapsible::new()
                    .w_full()
                    .gap(px(4.))
                    .open(true)
                    .motion_id(SharedString::from(format!("batch_col_motion_{i}")))
                    .child(
                        div()
                            .w_full()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child(title),
                    )
                    .content(body)
                    .into_any_element(),
            );
            if i + 1 < b.results.len() {
                sections.push(
                    div()
                        .h(px(1.))
                        .w_full()
                        .bg(p.divider)
                        .my(px(2.))
                        .into_any_element(),
                );
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(sections)
            .into_any_element()
    }

    fn render_dictionary(&self, p: &Palette, entry: &DictionaryEntry, source_text: &str, translation: Option<String>) -> AnyElement {
        let mut sections: Vec<AnyElement> = Vec::new();
        if let Some(t) = translation {
            sections.push(selectable_plain("dict_translation", &t, p.text, BODY_SIZE, BODY_LINE));
        }
        let phonetic = crate::services::dictionary::Dictionary::phonetic(entry).unwrap_or_default();
        if !phonetic.is_empty() {
            sections.push(selectable_plain(
                "dict_phonetic",
                &format!("/{}/", phonetic),
                p.accent,
                13.,
                18.,
            ));
        }
        for (mi, m) in entry.meanings.iter().enumerate() {
            let mut defs: Vec<AnyElement> = Vec::new();
            for (idx, d) in m.definitions.iter().take(3).enumerate() {
                let mut de: Vec<AnyElement> = Vec::new();
                let key = (mi * 8 + idx) as u32;
                de.push(selectable_plain(
                    ("dict_def", key),
                    &format!("{}. {}", idx + 1, d.definition),
                    p.text,
                    13.5,
                    20.,
                ));
                if let Some(ex) = &d.example {
                    de.push(selectable_plain(
                        ("dict_ex", key),
                        &format!("例: {ex}"),
                        p.muted,
                        11.5,
                        17.,
                    ));
                }
                defs.push(div().flex().flex_col().gap(px(2.)).children(de).into_any_element());
            }
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.muted)
                            .child(m.part_of_speech.clone()),
                    )
                    .children(defs)
                    .into_any_element(),
            );
        }
        let _ = source_text;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(sections)
            .into_any_element()
    }
}

/// 计算弹窗位置：鼠标右下方偏移，并夹紧在鼠标所在显示器范围内。
pub fn popup_bounds(cx: &App, mouse: Option<(f32, f32)>, size: Size<Pixels>) -> (Bounds<Pixels>, Option<DisplayId>) {
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let displays = cx.displays();
    let primary = cx.primary_display();
    let (mx, my) = match mouse {
        Some(m) => m,
        None => {
            let b = primary.as_ref().map(|d| d.bounds()).unwrap_or_default();
            let c = b.center();
            (f32::from(c.x) - w / 2., f32::from(c.y) - h / 2.)
        }
    };
    let p = point(px(mx), px(my));
    let display = displays.iter().find(|d| d.bounds().contains(&p)).cloned().or(primary);
    let mut x = mx + 8.;
    let mut y = my + 14.;
    let mut id = None;
    if let Some(d) = display {
        id = Some(d.id());
        let b = d.bounds();
        let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (right, bottom) = (left + f32::from(b.size.width), top + f32::from(b.size.height));
        if y + h.max(220.) > bottom {
            y = my - h.max(220.) - 8.;
        }
        x = x.min(right - w - 4.).max(left + 4.);
        y = y.min(bottom - h - 4.).max(top + 4.);
    }
    (Bounds::new(point(px(x), px(y)), size), id)
}
