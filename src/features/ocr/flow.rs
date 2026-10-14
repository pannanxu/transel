//! OCR 截图 / 识别 / 独立窗口生命周期。

use std::sync::Arc;

use gpui_kit::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::app::history::HistoryOrigin;
use crate::app::state::AppState;
use crate::features::ocr::view::{OcrActions, OcrView};
use crate::features::popup::{self, Mode};
use crate::services::ocr;
use crate::services::translate::RequestCtx;
use crate::shared::chrome::window_background;
use crate::shared::tokens::OCR_SIZE;
use crate::shell::window::{PopupWindowOpts, open_root_window};
use crate::split;

static OCR_VIEW: Lazy<Mutex<Option<Entity<OcrView>>>> = Lazy::new(|| Mutex::new(None));
static OCR_CONTINUOUS_BUF: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

fn mouse_location() -> Option<(f32, f32)> {
    use enigo::{Enigo, Mouse, Settings};
    let enigo = Enigo::new(&Settings::default()).ok()?;
    let (x, y) = enigo.location().ok()?;
    Some((x as f32, y as f32))
}

/// 截图 → OCR → 翻译弹窗。
pub fn trigger_ocr(cx: &mut App) {
    cx.spawn(async move |cx| {
        let path = cx
            .background_executor()
            .spawn(async { ocr::capture_interactive() })
            .await;
        let path = match path {
            Ok(p) => p,
            Err(e) => {
                log::warn!("截图失败或用户取消: {e:#}");
                return;
            }
        };
        let mouse = cx
            .background_executor()
            .spawn(async { mouse_location() })
            .await;
        cx.update(|cx| popup::show(cx, Mode::recognizing(), None, mouse, HistoryOrigin::Ocr));

        let smart = cx.update(|cx| cx.global::<AppState>().config().ocr_smart_paragraph);
        let result = cx
            .background_executor()
            .spawn(async move {
                let r = ocr::recognize_path(&path, None)?;
                let text = ocr::finalize_text(&r.text, &r.qr_payloads, smart);
                Ok::<_, anyhow::Error>((text, r))
            })
            .await;
        match result {
            Ok((text, _)) if !text.trim().is_empty() => {
                let (prepared, _) = split::prepare(&text);
                let cfg = cx.update(|cx| cx.global::<AppState>().config().clone());
                let ctx = RequestCtx {
                    source: text,
                    prepared,
                    source_lang_hint: if cfg.source_lang == "auto" {
                        None
                    } else {
                        Some(cfg.source_lang.clone())
                    },
                    target_lang: cfg.target_lang.clone(),
                    only: None,
                };
                cx.update(|cx| {
                    popup::show(
                        cx,
                        Mode::translating(),
                        Some(ctx),
                        mouse,
                        HistoryOrigin::Ocr,
                    )
                });
            }
            Ok(_) => {
                log::warn!("OCR 没有识别出文字");
                cx.update(|cx| {
                    popup::show(
                        cx,
                        Mode::Error("没有识别出文字".into()),
                        None,
                        mouse,
                        HistoryOrigin::Ocr,
                    )
                });
            }
            Err(e) => {
                log::warn!("OCR 失败: {e:#}");
                cx.update(|cx| {
                    popup::show(
                        cx,
                        Mode::Error(format!("OCR 失败: {e}")),
                        None,
                        mouse,
                        HistoryOrigin::Ocr,
                    )
                });
            }
        }
    })
    .detach();
}

/// 独立 OCR；`silent` 时只写剪贴板。
pub fn trigger_ocr_only(cx: &mut App, silent: bool) {
    cx.spawn(async move |cx| {
        let path = cx
            .background_executor()
            .spawn(async { ocr::capture_interactive() })
            .await;
        let path = match path {
            Ok(p) => p,
            Err(e) => {
                log::warn!("截图失败或用户取消: {e:#}");
                return;
            }
        };
        let cancel = if silent {
            let mouse = cx
                .background_executor()
                .spawn(async { mouse_location() })
                .await;
            Some(cx.update(|cx| crate::features::silent_hud::begin(cx, "识别中…", mouse)))
        } else {
            cx.update(ensure_loading);
            None
        };
        let (smart, auto_copy) = cx.update(|cx| {
            let c = cx.global::<AppState>().config();
            (c.ocr_smart_paragraph, c.ocr_auto_copy)
        });
        let result = cx
            .background_executor()
            .spawn(async move {
                let r = ocr::recognize_path(&path, None)?;
                let text = ocr::finalize_text(&r.text, &r.qr_payloads, smart);
                Ok::<_, anyhow::Error>(text)
            })
            .await;
        if let Some(flag) = cancel.as_ref() {
            let stopped = crate::features::silent_hud::is_cancelled(flag);
            cx.update(crate::features::silent_hud::finish);
            if stopped {
                return;
            }
        }
        match result {
            Ok(text) if !text.trim().is_empty() => {
                if (silent || auto_copy)
                    && let Ok(mut cb) = arboard::Clipboard::new()
                {
                    let _ = cb.set_text(text.clone());
                }
                if silent {
                    log::info!("静默 OCR 已复制 {} 字", text.chars().count());
                } else {
                    cx.update(|cx| finish_window(cx, text));
                }
            }
            Ok(_) => {
                log::warn!("OCR 没有识别出文字");
                if !silent {
                    cx.update(|cx| set_error(cx, "没有识别出文字"));
                }
            }
            Err(e) => {
                log::warn!("OCR 失败: {e:#}");
                if !silent {
                    cx.update(|cx| set_error(cx, format!("OCR 失败: {e}")));
                }
            }
        }
    })
    .detach();
}

pub fn trigger_finder(cx: &mut App) {
    cx.spawn(async move |cx| {
        let paths = cx
            .background_executor()
            .spawn(async { ocr::finder::selected_image_paths() })
            .await;
        let paths = match paths {
            Ok(p) => p,
            Err(e) => {
                log::warn!("选中文件 OCR 失败: {e:#}");
                return;
            }
        };
        cx.update(ensure_loading);
        let (smart, auto_copy) = cx.update(|cx| {
            let c = cx.global::<AppState>().config();
            (c.ocr_smart_paragraph, c.ocr_auto_copy)
        });
        let result = cx
            .background_executor()
            .spawn(async move {
                let mut parts = Vec::new();
                for path in paths {
                    let r = ocr::recognize_path(&path, None)?;
                    let text = ocr::finalize_text(&r.text, &r.qr_payloads, smart);
                    if !text.trim().is_empty() {
                        parts.push(text);
                    }
                }
                Ok::<_, anyhow::Error>(parts.join("\n\n"))
            })
            .await;
        match result {
            Ok(text) if !text.trim().is_empty() => {
                if auto_copy && let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(text.clone());
                }
                cx.update(|cx| finish_window(cx, text));
            }
            Ok(_) => {
                log::warn!("选中文件 OCR 没有识别出文字");
                cx.update(|cx| set_error(cx, "没有识别出文字"));
            }
            Err(e) => {
                log::warn!("选中文件 OCR 失败: {e:#}");
                cx.update(|cx| set_error(cx, format!("OCR 失败: {e}")));
            }
        }
    })
    .detach();
}

pub fn close_window(cx: &mut App) {
    *OCR_VIEW.lock() = None;
    *OCR_CONTINUOUS_BUF.lock() = None;
    let handle = cx.global_mut::<AppState>().ocr_window.lock().take();
    if let Some(h) = handle {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
}

fn is_continuous(cx: &App) -> bool {
    cx.global::<AppState>()
        .ocr_continuous
        .load(std::sync::atomic::Ordering::Relaxed)
}

fn merge_continuous(prev: Option<&str>, next: &str) -> String {
    let next = next.trim();
    match prev.map(str::trim).filter(|s| !s.is_empty()) {
        Some(prev) if !next.is_empty() => format!("{prev}\n\n{next}"),
        Some(prev) => prev.to_string(),
        None => next.to_string(),
    }
}

pub fn ensure_loading(cx: &mut App) {
    if let Some(view) = OCR_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            let continuous = this
                .actions
                .continuous
                .load(std::sync::atomic::Ordering::Relaxed);
            if !continuous {
                this.text.clear();
            }
            this.set_loading(true);
            cx.notify();
        });
        return;
    }
    let prev = if is_continuous(cx) {
        OCR_CONTINUOUS_BUF.lock().clone().unwrap_or_default()
    } else {
        String::new()
    };
    open_window(cx, prev, true);
}

pub fn finish_window(cx: &mut App, text: String) {
    if let Some(view) = OCR_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            this.apply_result(&text);
            *OCR_CONTINUOUS_BUF.lock() = Some(this.text.clone());
            cx.notify();
        });
        return;
    }
    let text = if is_continuous(cx) {
        merge_continuous(OCR_CONTINUOUS_BUF.lock().as_deref(), &text)
    } else {
        text
    };
    open_window(cx, text, false);
}

pub fn set_error(cx: &mut App, msg: impl Into<String>) {
    let msg = msg.into();
    if let Some(view) = OCR_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            this.set_error(msg);
            cx.notify();
        });
        return;
    }
    open_window(cx, String::new(), false);
    if let Some(view) = OCR_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            this.set_error(msg);
            cx.notify();
        });
    }
}

pub fn open_window(cx: &mut App, text: String, loading: bool) {
    if !loading {
        *OCR_CONTINUOUS_BUF.lock() = Some(text.clone());
    }
    let cont_flag = Arc::new(std::sync::atomic::AtomicBool::new(
        cx.global::<AppState>()
            .ocr_continuous
            .load(std::sync::atomic::Ordering::Relaxed),
    ));
    let translate_q = cx.global::<AppState>().pending.ocr_translate.clone();
    let continuous_q = cx.global::<AppState>().pending.ocr_continuous.clone();

    let opts = PopupWindowOpts {
        bounds: Bounds::new(
            point(px(200.), px(140.)),
            size(px(OCR_SIZE.0), px(OCR_SIZE.1)),
        ),
        display_id: None,
        focus: true,
        movable: true,
        resizable: true,
        background: window_background(),
    };

    let cont = cont_flag.clone();
    match open_root_window(cx, opts, move |window, cx| {
        let actions = OcrActions {
            on_close: Arc::new(|| {
                *OCR_CONTINUOUS_BUF.lock() = None;
                *OCR_VIEW.lock() = None;
            }),
            on_copy: Arc::new(|t| {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(t);
                }
            }),
            on_translate: Arc::new(move |t| {
                translate_q.lock().replace(t);
            }),
            on_toggle_continuous: Arc::new(move |on| {
                continuous_q.lock().replace(on);
            }),
            continuous: cont.clone(),
        };
        let view = if loading {
            cx.new(|cx| OcrView::loading(actions, window, cx, text.clone()))
        } else {
            cx.new(|cx| OcrView::new(text.clone(), actions, window, cx))
        };
        *OCR_VIEW.lock() = Some(view.clone());
        view
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().ocr_window.lock() = Some(handle);
        }
        Err(e) => {
            *OCR_VIEW.lock() = None;
            log::error!("打开 OCR 窗口失败: {e:#}");
        }
    }
}
