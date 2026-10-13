//! 划词 / 输入翻译弹窗。

mod commands;
pub mod lang_direction;
mod model;
mod view;

pub use commands::init_keybindings;
pub use view::{
    MAX_W, MIN_W, Mode, PERMISSION_TEXT, PopupActions, SHADOW_MARGIN, TranslatePopup, content_size,
    popup_bounds,
};

use gpui_kit::*;

use crate::app::history::HistoryOrigin;
use crate::app::state::AppState;
use crate::services::translate::RequestCtx;
use crate::shared::chrome::window_background;
use crate::shell::window::{PopupWindowOpts, open_root_window};

/// 关闭当前翻译弹窗并取消进行中的请求。
pub fn close(cx: &mut App) {
    cx.global::<AppState>().translate.cancel_active();
    let handle = cx.global_mut::<AppState>().popup.lock().take();
    if let Some(h) = handle {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
}

/// 打开或替换翻译弹窗。
pub fn show(
    cx: &mut App,
    mode: Mode,
    ctx: Option<RequestCtx>,
    mouse: Option<(f32, f32)>,
    origin: HistoryOrigin,
) {
    close(cx);
    let state = cx.global::<AppState>();
    let mode = match (&mode, &ctx) {
        (Mode::Loading(_), Some(c)) => {
            let names = state.translate.backend_names();
            if names.is_empty() {
                Mode::Error("没有启用的翻译后端".into())
            } else {
                Mode::Batch(crate::services::translate::BatchResult {
                    source: c.source.clone(),
                    prepared: c.prepared.clone(),
                    target_lang: c.target_lang.clone(),
                    results: names
                        .into_iter()
                        .map(crate::services::translate::SingleResult::pending_slot)
                        .collect(),
                })
            }
        }
        _ => mode,
    };
    let follow_mouse = state.config().popup_follow_mouse;
    let size = content_size(
        cx,
        &mode,
        &state.config().target_lang,
        if follow_mouse { mouse } else { None },
    );
    let (bounds, display_id) = popup_bounds(cx, mouse, size, follow_mouse);
    let target_lang = state.config().target_lang.clone();
    let source_lang = state.config().source_lang.clone();
    let tts_backend = state.config().tts_backend.clone();
    let actions = make_actions(cx, tts_backend, target_lang.clone());
    let pinned = state.config().pin_popup_by_default
        || state.pinned.load(std::sync::atomic::Ordering::Relaxed);
    let mode_clone = mode.clone();

    let opts = PopupWindowOpts {
        bounds,
        display_id,
        focus: true,
        movable: false,
        resizable: false,
        background: window_background(),
    };

    match open_root_window(cx, opts, {
        let ctx = ctx.clone();
        move |window, cx| {
            let view = cx.new(|cx| {
                let mut popup = TranslatePopup::new(
                    mode_clone,
                    target_lang,
                    source_lang,
                    pinned,
                    origin,
                    actions,
                    window,
                    cx,
                );
                if let Some(ctx_inner) = ctx.as_ref() {
                    let display = if ctx_inner.source != ctx_inner.prepared {
                        ctx_inner.prepared.clone()
                    } else {
                        ctx_inner.source.clone()
                    };
                    popup.source_input.update(cx, |input, cx| {
                        input.set_value(display, window, cx);
                    });
                    popup.prepared_for_history = Some(ctx_inner.prepared.clone());
                    popup.was_split = ctx_inner.source != ctx_inner.prepared;
                }
                popup
            });
            if let Some(ctx_inner) = ctx.clone() {
                spawn_translate(view.clone(), ctx_inner, origin, cx);
            }
            view
        }
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().popup.lock() = Some(handle);
        }
        Err(e) => log::error!("打开弹窗失败: {e:#}"),
    }
}

fn spawn_translate(
    view_handle: Entity<TranslatePopup>,
    ctx_inner: RequestCtx,
    origin: HistoryOrigin,
    cx: &mut App,
) {
    use crate::services::translate::BatchEvent;
    use futures::StreamExt;

    let svc = cx.global::<AppState>().translate.clone();
    let history = cx.global::<AppState>().history.clone();

    cx.spawn(async move |async_cx| {
        let mut rx = svc.request(ctx_inner.clone());
        while let Some(ev) = rx.next().await {
            let keep =
                async_cx.update_entity(&view_handle, |this: &mut TranslatePopup, cx| match ev {
                    BatchEvent::Started(batch) => {
                        this.apply_mode(Mode::Batch(batch), cx);
                        cx.notify();
                        true
                    }
                    BatchEvent::Progress(r) => {
                        if let Mode::Batch(b) = &mut this.mode {
                            b.apply_progress(r);
                            this.schedule_resize_from_content(cx);
                            cx.notify();
                        }
                        true
                    }
                    BatchEvent::Done => {
                        if let Mode::Batch(batch) = &this.mode {
                            let was_split = ctx_inner.source != ctx_inner.prepared;
                            let _ = history.append(crate::app::history::HistoryEntry {
                                id: String::new(),
                                ts: chrono::Utc::now(),
                                source: ctx_inner.source.clone(),
                                prepared: ctx_inner.prepared.clone(),
                                was_split,
                                source_lang: batch.first_ok().and_then(|r| r.source_lang.clone()),
                                target_lang: ctx_inner.target_lang.clone(),
                                origin,
                                results: batch
                                    .results
                                    .iter()
                                    .map(|r| crate::app::history::TranslationResult {
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
                });
            if !keep {
                break;
            }
        }
    })
    .detach();
}

fn make_actions(cx: &App, tts_backend: String, target_lang: String) -> PopupActions {
    use std::sync::Arc;
    let fav_q = cx.global::<AppState>().pending.favorite.clone();
    PopupActions {
        on_close: Arc::new(|| {
            log::debug!("翻译弹窗关闭");
        }),
        on_pin: Arc::new(|pinned| {
            log::info!("弹窗固定: {pinned}");
        }),
        on_speak: Arc::new(move |t| {
            crate::services::tts::speak_with_backend_async(
                tts_backend.clone(),
                crate::services::tts::OwnedTtsOptions::new(t).with_language(target_lang.clone()),
            );
        }),
        on_copy: Arc::new(|t| {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(t);
            }
        }),
        on_submit_input: Arc::new(|_t| {}),
        on_switch_tab: Arc::new(|_| {}),
        on_favorite: Arc::new(move |source, translation, backend| {
            fav_q.lock().replace((source, translation, backend));
        }),
    }
}

/// 从选区文本打开翻译。
pub fn open_from_text(
    cx: &mut App,
    text: String,
    mouse: Option<(f32, f32)>,
    origin: HistoryOrigin,
) {
    let (prepared, was_split) = crate::split::prepare(&text);
    if was_split {
        log::info!("标识符拆词: {text} -> {prepared}");
    }
    let cfg = cx.global::<AppState>().config().clone();
    let ctx = crate::shell::request::build_request_ctx(cfg, &text, &prepared);
    show(cx, Mode::translating(), Some(ctx), mouse, origin);
}

/// 手动输入模式。
pub fn open_input(cx: &mut App, mouse: Option<(f32, f32)>) {
    show(
        cx,
        Mode::Input {
            text: String::new(),
        },
        None,
        mouse,
        HistoryOrigin::Input,
    );
}

/// 切换固定。
pub fn toggle_pin(cx: &mut App) {
    let handle = {
        let state = cx.global::<AppState>();
        let new = !state.pinned.load(std::sync::atomic::Ordering::Relaxed);
        state
            .pinned
            .store(new, std::sync::atomic::Ordering::Relaxed);
        log::info!("弹窗固定: {new}");
        *state.popup.lock()
    };
    if let Some(h) = handle {
        let _ = h.update(cx, |_root, window, _cx| {
            window.refresh();
        });
    }
}
