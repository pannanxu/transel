//! 跨模块共享的运行时状态。注册为 GPUI 的 `Global` 后可在视图里直接 `cx.global::<AppState>()`。

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use parking_lot::{Mutex, RwLock};

use super::config::Config;
use super::favorites::SharedFavorites;
use super::history::SharedHistory;
use super::hotkey::HotkeySet;
use super::secrets;
use crate::services::translate::TranslateService;

pub type FavoriteIntent = (String, String, Option<String>);

/// 跨 Feature 意图队列（Arc 可克隆进 UI 回调）。
#[derive(Clone, Default)]
pub struct PendingIntents {
    pub favorite: Arc<Mutex<Option<FavoriteIntent>>>,
    pub ocr_translate: Arc<Mutex<Option<String>>>,
    pub ocr_continuous: Arc<Mutex<Option<bool>>>,
}

/// 全局状态。
pub struct AppState {
    pub config: RwLock<Config>,
    pub hotkeys: parking_lot::Mutex<HotkeySet>,
    pub translate: Arc<TranslateService>,
    pub history: SharedHistory,
    pub favorites: SharedFavorites,
    pub popup: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub ocr_window: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub overlay_window: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub favorites_window: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub history_window: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub settings_window: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub silent_hud: parking_lot::Mutex<Option<gpui_kit::AnyWindowHandle>>,
    pub ocr_continuous: AtomicBool,
    pub tray_events: TrayEventQueue,
    pub pinned: AtomicBool,
    pub pending: PendingIntents,
    pub _secret_store: Arc<dyn secrets::SecretStore>,
}

#[derive(Clone)]
pub struct TrayEventQueue {
    pub tx: std::sync::mpsc::Sender<TrayEvent>,
    rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<TrayEvent>>>,
}

#[derive(Debug, Clone)]
pub enum TrayEvent {
    OpenSettings,
    OpenHistory,
    OpenFavorites,
    OpenInput,
    PinToggle,
    Ocr,
    OcrOnly,
    OcrSilent,
    OcrFinder,
    OverlayTranslate,
    SilentReplace,
    SilentInput,
    Quit,
}

impl TrayEventQueue {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            tx,
            rx: std::sync::Arc::new(std::sync::Mutex::new(rx)),
        }
    }
    pub fn send(&self, ev: TrayEvent) {
        let _ = self.tx.send(ev);
    }
    pub fn try_recv(&self) -> Vec<TrayEvent> {
        let g = self.rx.lock().unwrap();
        let mut out = Vec::new();
        while let Ok(ev) = g.try_recv() {
            out.push(ev);
        }
        out
    }
}

impl Default for TrayEventQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new(
        config: Config,
        translate: Arc<TranslateService>,
        history: SharedHistory,
        favorites: SharedFavorites,
        hotkeys: HotkeySet,
        secret_store: Arc<dyn secrets::SecretStore>,
    ) -> Self {
        let ocr_continuous = AtomicBool::new(config.ocr_continuous);
        Self {
            config: RwLock::new(config),
            hotkeys: parking_lot::Mutex::new(hotkeys),
            translate,
            history,
            favorites,
            popup: parking_lot::Mutex::new(None),
            ocr_window: parking_lot::Mutex::new(None),
            overlay_window: parking_lot::Mutex::new(None),
            favorites_window: parking_lot::Mutex::new(None),
            history_window: parking_lot::Mutex::new(None),
            settings_window: parking_lot::Mutex::new(None),
            silent_hud: parking_lot::Mutex::new(None),
            ocr_continuous,
            tray_events: TrayEventQueue::new(),
            pinned: AtomicBool::new(false),
            pending: PendingIntents::default(),
            _secret_store: secret_store,
        }
    }

    /// 重新加载配置到内存。
    pub fn reload_config(&self, cfg: Config) {
        self.translate.rebuild(&cfg);
        self.hotkeys.lock().rebind_from_config(&cfg);
        self.ocr_continuous
            .store(cfg.ocr_continuous, std::sync::atomic::Ordering::Relaxed);
        *self.config.write() = cfg;
    }

    pub fn config(&self) -> parking_lot::RwLockReadGuard<'_, Config> {
        self.config.read()
    }
}

impl gpui_kit::Global for AppState {}
