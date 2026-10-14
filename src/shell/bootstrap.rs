//! 应用初始化：theme、服务、热键、托盘、主循环。

use std::sync::Arc;

use gpui_kit::*;

use crate::app::config::Config;
use crate::app::favorites::FavoritesStore;
use crate::app::history::HistoryStore;
use crate::app::hotkey::HotkeySet;
use crate::app::paths;
use crate::app::secrets;
use crate::app::state::AppState;
use crate::features;
use crate::platform;
#[cfg(target_os = "macos")]
use crate::services::input;
use crate::services::ocr;
use crate::services::translate::TranslateService;
use crate::shared::icons::AppAssets;
use crate::shared::theme;
use crate::shell::event_loop;

pub fn run() {
    let cfg = Config::load_or_default(&paths::config_file());
    if let Err(e) = cfg.save(&paths::config_file()) {
        log::warn!("保存配置失败: {e:#}");
    }
    log::info!(
        "配置: target={} source={} backends={}",
        cfg.target_lang,
        cfg.source_lang,
        cfg.backends.iter().filter(|b| b.enabled).count()
    );

    gpui_kit::application()
        .with_assets(AppAssets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            theme::sync_kit_theme(None, cx);
            cx.set_quit_mode(QuitMode::Explicit);
            // Info.plist 不设 LSUIElement，以便 Spotlight / 启动台能搜到；
            // 运行后再切 accessory，避免常驻 Dock。
            #[cfg(target_os = "macos")]
            cx.set_activation_policy(ActivationPolicy::Accessory);

            features::popup::init_keybindings(cx);
            features::settings::init_keybindings(cx);
            features::history::init_keybindings(cx);
            features::favorites::init_keybindings(cx);
            features::ocr::init_keybindings(cx);
            features::overlay::init_keybindings(cx);
            // 输入框聚焦时 context 可能不是窗口根；无 context 绑定作兜底。
            crate::shared::actions::bind_window_close_keys(cx, None);
            #[cfg(target_os = "macos")]
            cx.bind_keys([KeyBinding::new(
                "cmd-q",
                crate::shared::actions::QuitApp,
                None,
            )]);
            #[cfg(not(target_os = "macos"))]
            cx.bind_keys([
                KeyBinding::new("alt-f4", crate::shared::actions::QuitApp, None),
                KeyBinding::new("ctrl-q", crate::shared::actions::QuitApp, None),
            ]);
            crate::shared::actions::install_app_menus(cx);
            cx.on_action(|_: &crate::shared::actions::CloseWindow, cx| {
                crate::shared::actions::close_front_window(cx);
            });
            cx.on_action(|_: &crate::shared::actions::QuitApp, cx| {
                cx.quit();
            });

            #[cfg(target_os = "macos")]
            if !input::ensure_accessibility_permission() {
                log::warn!("请在 系统设置 → 隐私与安全性 → 辅助功能 中授权本程序，然后重启它");
            }

            let service = TranslateService::start(&cfg);
            let history = Arc::new(
                HistoryStore::open(&paths::history_file(), cfg.history_limit).expect("history"),
            );
            let favorites =
                Arc::new(FavoritesStore::open(&paths::favorites_file()).expect("favorites"));

            let mut hotkeys = HotkeySet::new().expect("hotkey manager");
            for (name, spec) in [
                ("translate", &cfg.translate_hotkey),
                ("ocr", &cfg.ocr_hotkey),
                ("input", &cfg.input_hotkey),
                ("speak", &cfg.speak_hotkey),
                ("history", &cfg.history_hotkey),
                ("settings", &cfg.settings_hotkey),
                ("pin", &cfg.pin_hotkey),
                ("ocr_only", &cfg.ocr_only_hotkey),
                ("ocr_silent", &cfg.ocr_silent_hotkey),
                ("ocr_finder", &cfg.ocr_finder_hotkey),
                ("silent_replace", &cfg.silent_replace_hotkey),
                ("silent_input", &cfg.silent_input_hotkey),
                ("overlay", &cfg.overlay_hotkey),
                ("favorites", &cfg.favorites_hotkey),
            ] {
                if let Err(e) = hotkeys.register(name, spec) {
                    log::warn!("注册 {name} 热键失败: {e:#}");
                }
            }

            let state = AppState::new(
                cfg.clone(),
                service.clone(),
                history.clone(),
                favorites.clone(),
                hotkeys,
                Arc::new(secrets::KeyringStore),
            );
            let state_for_tray = state.tray_events.clone();
            platform::tray::spawn_tray(state_for_tray.tx.clone());
            cx.set_global(state);

            let history_for_cb = history.clone();
            service.set_history_callback(Arc::new(move |entry| {
                let _ = history_for_cb.append(entry);
            }));

            let ocr_backend = cfg.ocr_backend.clone();
            cx.background_executor()
                .spawn(async move { ocr::warmup_with_config(&ocr_backend) })
                .detach();

            event_loop::spawn(cx);
        });
}
