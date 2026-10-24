//! 热键 + 托盘 + pending 意图轮询。

use std::time::Duration;

use global_hotkey::GlobalHotKeyEvent;
use gpui_kit::*;

use crate::app::favorites::FavoriteEntry;
use crate::app::history::HistoryOrigin;
use crate::app::state::{AppState, TrayEvent};
use crate::features;
use crate::platform::mouse_location;
use crate::services::input;
use crate::services::tts;
use crate::shell::request;
use crate::split;

pub fn spawn(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            drain_pending(cx).await;

            let tray = cx.update(|cx| cx.global::<AppState>().tray_events.try_recv());
            for ev in tray {
                match ev {
                    TrayEvent::OpenSettings => cx.update(features::settings::open),
                    TrayEvent::OpenHistory => cx.update(features::history::open),
                    TrayEvent::OpenFavorites => cx.update(features::favorites::open),
                    TrayEvent::OpenInput => {
                        let mouse = cx
                            .background_executor()
                            .spawn(async { mouse_location() })
                            .await;
                        cx.update(|cx| features::popup::open_input(cx, mouse));
                    }
                    TrayEvent::Screenshot => cx.update(features::screenshot::trigger_screenshot),
                    TrayEvent::Ocr => cx.update(features::ocr::trigger_ocr),
                    TrayEvent::OcrOnly => {
                        cx.update(|cx| features::ocr::trigger_ocr_only(cx, false))
                    }
                    TrayEvent::OcrSilent => {
                        cx.update(|cx| features::ocr::trigger_ocr_only(cx, true))
                    }
                    TrayEvent::OcrFinder => cx.update(features::ocr::trigger_finder),
                    TrayEvent::OverlayTranslate => cx.update(features::overlay::trigger_overlay),
                    TrayEvent::SilentReplace => cx.update(|cx| trigger_silent_replace(cx, false)),
                    TrayEvent::SilentInput => cx.update(|cx| trigger_silent_replace(cx, true)),
                    TrayEvent::PinToggle => cx.update(features::popup::toggle_pin),
                    TrayEvent::Quit => {
                        cx.update(|cx| cx.quit());
                        return;
                    }
                }
            }

            while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                if ev.state != global_hotkey::HotKeyState::Pressed {
                    continue;
                }
                let name = cx.update(|cx| {
                    cx.global::<AppState>()
                        .hotkeys
                        .lock()
                        .name_of(ev.id)
                        .map(|s| s.to_string())
                });
                if let Some(name) = name {
                    handle_hotkey(&name, cx).await;
                }
            }

            cx.background_executor()
                .timer(Duration::from_millis(30))
                .await;
        }
    })
    .detach();
}

async fn drain_pending(cx: &mut AsyncApp) {
    let pending_tr = cx.update(|cx| cx.global::<AppState>().pending.ocr_translate.lock().take());
    if let Some(text) = pending_tr {
        let (prepared, _) = split::prepare(&text);
        let cfg = cx.update(|cx| cx.global::<AppState>().config().clone());
        let mouse = cx
            .background_executor()
            .spawn(async { mouse_location() })
            .await;
        let ctx = request::build_request_ctx(cfg, &text, &prepared);
        cx.update(|cx| {
            features::popup::show(
                cx,
                features::popup::Mode::translating(),
                Some(ctx),
                mouse,
                HistoryOrigin::Ocr,
            )
        });
    }

    if let Some(on) = cx.update(|cx| cx.global::<AppState>().pending.ocr_continuous.lock().take()) {
        cx.update(|cx| {
            cx.global::<AppState>()
                .ocr_continuous
                .store(on, std::sync::atomic::Ordering::Relaxed);
            cx.global_mut::<AppState>().config.write().ocr_continuous = on;
        });
    }

    if let Some((source, translation, backend)) =
        cx.update(|cx| cx.global::<AppState>().pending.favorite.lock().take())
    {
        cx.update(|cx| {
            let target = cx.global::<AppState>().config().target_lang.clone();
            let mut fav = FavoriteEntry {
                id: String::new(),
                ts: chrono::Utc::now(),
                source,
                prepared: String::new(),
                target_lang: target,
                translation,
                backend,
                origin: HistoryOrigin::Selection,
            };
            fav.prepared = fav.source.clone();
            fav.id = uuid::Uuid::new_v4().to_string();
            match cx.global::<AppState>().favorites.append(fav) {
                Ok(()) => log::info!("已收藏"),
                Err(e) => log::warn!("收藏失败: {e:#}"),
            }
        });
    }
}

async fn handle_hotkey(name: &str, cx: &mut AsyncApp) {
    match name {
        "translate" => {
            let (source, mouse) = cx
                .background_executor()
                .spawn(async { (input::grab_translate_source(), mouse_location()) })
                .await;
            match source {
                Ok(input::TranslateSource::Text(t)) => {
                    cx.update(|cx| {
                        features::popup::open_from_text(cx, t, mouse, HistoryOrigin::Selection)
                    });
                }
                Ok(input::TranslateSource::Images(paths)) => {
                    cx.update(|cx| open_translate_from_images(cx, paths, mouse));
                }
                Err(input::GrabError::NoPermission) => {
                    cx.update(|cx| {
                        features::popup::show(
                            cx,
                            features::popup::Mode::NoPermission,
                            None,
                            mouse,
                            HistoryOrigin::Selection,
                        )
                    });
                }
                Err(input::GrabError::Empty) => {
                    log::info!("没有可翻译的选中内容（文字 / 文本文件 / 图片）")
                }
            }
        }
        "screenshot" => cx.update(features::screenshot::trigger_screenshot),
        "ocr" => cx.update(features::ocr::trigger_ocr),
        "ocr_only" => cx.update(|cx| features::ocr::trigger_ocr_only(cx, false)),
        "ocr_silent" => cx.update(|cx| features::ocr::trigger_ocr_only(cx, true)),
        "ocr_finder" => cx.update(features::ocr::trigger_finder),
        "silent_replace" => cx.update(|cx| trigger_silent_replace(cx, false)),
        "silent_input" => cx.update(|cx| trigger_silent_replace(cx, true)),
        "overlay" => cx.update(features::overlay::trigger_overlay),
        "favorites" => cx.update(features::favorites::open),
        "input" => {
            let mouse = cx
                .background_executor()
                .spawn(async { mouse_location() })
                .await;
            cx.update(|cx| features::popup::open_input(cx, mouse));
        }
        "speak" => {
            let t = cx
                .background_executor()
                .spawn(async { input::grab_selection().ok() })
                .await;
            if let Some(t) = t {
                let (lang, backend) = cx.update(|cx| {
                    let c = cx.global::<AppState>().config();
                    (c.target_lang.clone(), c.tts_backend.clone())
                });
                tts::speak_with_backend_async(
                    backend,
                    tts::OwnedTtsOptions::new(t).with_language(lang),
                );
            }
        }
        "history" => cx.update(features::history::open),
        "settings" => cx.update(features::settings::open),
        "pin" => cx.update(features::popup::toggle_pin),
        _ => {}
    }
}

fn open_translate_from_images(
    cx: &mut App,
    paths: Vec<std::path::PathBuf>,
    mouse: Option<(f32, f32)>,
) {
    use crate::services::ocr;

    if paths.is_empty() {
        return;
    }
    features::popup::show(
        cx,
        features::popup::Mode::recognizing(),
        None,
        mouse,
        HistoryOrigin::Ocr,
    );
    cx.spawn(async move |cx| {
        let smart = cx.update(|cx| cx.global::<AppState>().config().ocr_smart_paragraph);
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
                cx.update(|cx| {
                    features::popup::open_from_text(cx, text, mouse, HistoryOrigin::Ocr);
                });
            }
            Ok(_) => {
                log::warn!("选中图片 OCR 没有识别出文字");
                cx.update(|cx| {
                    features::popup::show(
                        cx,
                        features::popup::Mode::Error("没有识别出文字".into()),
                        None,
                        mouse,
                        HistoryOrigin::Ocr,
                    )
                });
            }
            Err(e) => {
                log::warn!("选中图片 OCR 失败: {e:#}");
                cx.update(|cx| {
                    features::popup::show(
                        cx,
                        features::popup::Mode::Error(format!("OCR 失败: {e}")),
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

fn trigger_silent_replace(cx: &mut App, full_field: bool) {
    cx.spawn(async move |cx| {
        let grab = cx
            .background_executor()
            .spawn(async move {
                if full_field {
                    input::grab_focused_field()
                } else {
                    input::grab_selection()
                }
            })
            .await;
        let text = match grab {
            Ok(t) => t,
            Err(input::GrabError::NoPermission) => {
                log::warn!("静默翻译缺少读取选区所需的系统权限");
                return;
            }
            Err(input::GrabError::Empty) => {
                if full_field {
                    log::info!("静默输入框替换：读不到焦点框全文（请点进输入框再按热键）");
                } else {
                    log::info!("静默划词替换：没有选中文本");
                }
                return;
            }
        };
        let mouse = cx
            .background_executor()
            .spawn(async { mouse_location() })
            .await;
        let label = if full_field {
            "替换中…"
        } else {
            "翻译中…"
        };
        let cancel = cx.update(|cx| features::silent_hud::begin(cx, label, mouse));
        let (prepared, _) = split::prepare(&text);
        let cfg = cx.update(|cx| cx.global::<AppState>().config().clone());
        let svc = cx.update(|cx| cx.global::<AppState>().translate.clone());
        let ctx = request::build_request_ctx(cfg, &text, &prepared);
        let translated = match crate::services::translate::collect_batch(svc.request(ctx)).await {
            Ok(batch) => batch.first_ok().map(|r| r.text.clone()),
            Err(e) => {
                log::warn!("静默翻译失败: {e}");
                None
            }
        };
        if features::silent_hud::is_cancelled(&cancel) {
            cx.update(features::silent_hud::finish);
            return;
        }
        cx.update(features::silent_hud::finish);
        let Some(out) = translated.filter(|s| !s.trim().is_empty()) else {
            log::warn!("静默翻译无可用译文");
            return;
        };
        let nchars = out.chars().count();
        let replace_result = cx
            .background_executor()
            .spawn(async move {
                if full_field {
                    input::replace_focused_field(&out)
                } else {
                    input::replace_selection(&out)
                }
            })
            .await;
        match replace_result {
            Ok(()) => log::info!("静默翻译已替换 {nchars} 字"),
            Err(e) => log::warn!("静默替换失败: {e}"),
        }
    })
    .detach();
}
