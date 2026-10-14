//! 原图翻译：截图 → bbox → 逐块翻译 → 覆盖层窗口。

use std::sync::Arc;

use gpui_kit::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::app::history::HistoryOrigin;
use crate::app::state::AppState;
use crate::features::overlay::view::{OverlayActions, OverlayBlock, OverlayView};
use crate::features::popup::{self, Mode};
use crate::services::ocr;
use crate::services::translate::RequestCtx;
use crate::shared::chrome::window_background;
use crate::shared::tokens::{OVERLAY_MAX_H, OVERLAY_MAX_W};
use crate::shell::window::{PopupWindowOpts, open_root_window};
use crate::split;

static OVERLAY_VIEW: Lazy<Mutex<Option<Entity<OverlayView>>>> = Lazy::new(|| Mutex::new(None));

fn mouse_location() -> Option<(f32, f32)> {
    use enigo::{Enigo, Mouse, Settings};
    let enigo = Enigo::new(&Settings::default()).ok()?;
    let (x, y) = enigo.location().ok()?;
    Some((x as f32, y as f32))
}

pub fn trigger_overlay(cx: &mut App) {
    cx.spawn(async move |cx| {
        let path = cx
            .background_executor()
            .spawn(async { ocr::capture_interactive() })
            .await;
        let path = match path {
            Ok(p) => p,
            Err(e) => {
                log::warn!("原图翻译截图失败或取消: {e:#}");
                return;
            }
        };
        cx.update(ensure_loading);
        let boxes = cx
            .background_executor()
            .spawn({
                let path = path.clone();
                async move { ocr::recognize_boxes(&path) }
            })
            .await;
        if boxes.is_empty() {
            log::info!("原图翻译无 bbox，降级为截图翻译");
            cx.update(close_window);
            let smart = cx.update(|cx| cx.global::<AppState>().config().ocr_smart_paragraph);
            let mouse = cx
                .background_executor()
                .spawn(async { mouse_location() })
                .await;
            cx.update(|cx| popup::show(cx, Mode::recognizing(), None, mouse, HistoryOrigin::Ocr));
            let result = cx
                .background_executor()
                .spawn({
                    let path = path.clone();
                    async move {
                        let r = ocr::recognize_path(&path, None)?;
                        Ok::<_, anyhow::Error>(ocr::finalize_text(&r.text, &r.qr_payloads, smart))
                    }
                })
                .await;
            match result {
                Ok(text) if !text.trim().is_empty() => {
                    let (prepared, _) = split::prepare(&text);
                    let cfg = cx.update(|cx| cx.global::<AppState>().config().clone());
                    let ctx = crate::shell::request::build_request_ctx(cfg, &text, &prepared);
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
            return;
        }

        let total = boxes.len();
        cx.update(|cx| set_status(cx, format!("翻译中 0/{total}…")));

        let cfg = cx.update(|cx| cx.global::<AppState>().config().clone());
        let svc = cx.update(|cx| cx.global::<AppState>().translate.clone());
        let target = cfg.target_lang.clone();
        let hint = if cfg.source_lang == "auto" {
            None
        } else {
            Some(cfg.source_lang.clone())
        };

        let mut blocks: Vec<OverlayBlock> = Vec::with_capacity(boxes.len());
        for (i, b) in boxes.into_iter().enumerate() {
            let text = b.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            cx.update(|cx| set_status(cx, format!("翻译中 {}/{total}…", i + 1)));
            let (prepared, _) = split::prepare(&text);
            let ctx = RequestCtx {
                source: text.clone(),
                prepared,
                source_lang_hint: hint.clone(),
                target_lang: target.clone(),
                only: None,
            };
            let translation =
                match crate::services::translate::collect_batch(svc.request(ctx)).await {
                    Ok(batch) => batch.first_ok().map(|r| r.text.clone()).unwrap_or_default(),
                    _ => String::new(),
                };
            blocks.push(OverlayBlock {
                box_: b,
                translation,
            });
        }

        let (img_w, img_h) = ocr::image_size(&path).unwrap_or((800, 600));
        cx.update(|cx| finish_window(cx, blocks, img_w as f32, img_h as f32));
    })
    .detach();
}

pub fn close_window(cx: &mut App) {
    *OVERLAY_VIEW.lock() = None;
    let old = cx.global_mut::<AppState>().overlay_window.lock().take();
    if let Some(h) = old {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
}

pub fn ensure_loading(cx: &mut App) {
    if let Some(view) = OVERLAY_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            this.set_status("识别中…");
            cx.notify();
        });
        return;
    }
    open_window(cx, Vec::new(), 640., 420., true);
}

pub fn set_status(cx: &mut App, msg: String) {
    if let Some(view) = OVERLAY_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            this.set_status(msg);
            cx.notify();
        });
    }
}

pub fn finish_window(cx: &mut App, blocks: Vec<OverlayBlock>, image_w: f32, image_h: f32) {
    if let Some(view) = OVERLAY_VIEW.lock().clone() {
        view.update(cx, |this, cx| {
            this.set_blocks(blocks, image_w, image_h);
            cx.notify();
        });
        let handle = *cx.global::<AppState>().overlay_window.lock();
        if let Some(h) = handle {
            let scale = (OVERLAY_MAX_W / image_w)
                .min(OVERLAY_MAX_H / image_h)
                .clamp(0.35, 1.0);
            let win_w = (image_w * scale).clamp(360., 960.);
            let win_h = (image_h * scale).clamp(280., 760.) + 52.;
            let _ = h.update(cx, |_, window, _| {
                window.resize(size(px(win_w), px(win_h)));
            });
        }
        return;
    }
    open_window(cx, blocks, image_w, image_h, false);
}

pub fn open_window(
    cx: &mut App,
    blocks: Vec<OverlayBlock>,
    image_w: f32,
    image_h: f32,
    loading: bool,
) {
    close_window(cx);

    let scale = (OVERLAY_MAX_W / image_w)
        .min(OVERLAY_MAX_H / image_h)
        .clamp(0.35, 1.0);
    let win_w = (image_w * scale).clamp(360., 960.);
    let win_h = (image_h * scale).clamp(280., 760.) + 52.;

    let opts = PopupWindowOpts {
        bounds: Bounds::new(point(px(160.), px(100.)), size(px(win_w), px(win_h))),
        display_id: None,
        focus: true,
        movable: true,
        resizable: true,
        background: window_background(),
    };

    match open_root_window(cx, opts, move |window, cx| {
        let actions = OverlayActions {
            on_close: Arc::new(|| {
                *OVERLAY_VIEW.lock() = None;
            }),
            on_copy_all: Arc::new(|t| {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(t);
                }
            }),
        };
        let view = if loading {
            cx.new(|cx| OverlayView::loading(actions, window, cx))
        } else {
            cx.new(|cx| OverlayView::new(blocks.clone(), image_w, image_h, actions, window, cx))
        };
        *OVERLAY_VIEW.lock() = Some(view.clone());
        view
    }) {
        Ok(handle) => {
            *cx.global_mut::<AppState>().overlay_window.lock() = Some(handle);
        }
        Err(e) => {
            *OVERLAY_VIEW.lock() = None;
            log::error!("打开原图翻译窗口失败: {e:#}");
        }
    }
}
