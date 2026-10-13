//! 全局热键管理：把 `HotkeySpec` 转成 [`global_hotkey::hotkey::HotKey`]，
//! 并提供注册/反注册/重绑定。

use std::collections::HashMap;

use anyhow::{Context, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};

use super::config::HotkeySpec;

/// 把字符串形式的按键/修饰键解析为 [`Code`] / [`Modifiers`]。
/// 支持大小写不敏感；接受 `A..Z`、`0..9`、常用功能键。
pub fn parse(spec: &HotkeySpec) -> Result<HotKey> {
    let mut mods = Modifiers::empty();
    for m in &spec.modifiers {
        let m = m.to_ascii_lowercase();
        match m.as_str() {
            "alt" => mods |= Modifiers::ALT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "cmd" | "meta" | "super" | "win" => mods |= Modifiers::META,
            _ => anyhow::bail!("不支持的修饰键: {m}"),
        }
    }
    let key = spec.key.trim();
    let code = parse_key(key).with_context(|| format!("无法解析按键: {key}"))?;
    Ok(HotKey::new(Some(mods), code))
}

fn parse_key(s: &str) -> Result<Code> {
    use Code::*;
    // 功能键
    let upper = s.to_ascii_uppercase();
    if let Some(c) = match upper.as_str() {
        "F1" => Some(F1),
        "F2" => Some(F2),
        "F3" => Some(F3),
        "F4" => Some(F4),
        "F5" => Some(F5),
        "F6" => Some(F6),
        "F7" => Some(F7),
        "F8" => Some(F8),
        "F9" => Some(F9),
        "F10" => Some(F10),
        "F11" => Some(F11),
        "F12" => Some(F12),
        "ESC" | "ESCAPE" => Some(Escape),
        "TAB" => Some(Tab),
        "SPACE" => Some(Space),
        "ENTER" | "RETURN" => Some(Enter),
        "BACKSPACE" => Some(Backspace),
        "DELETE" | "DEL" => Some(Delete),
        "INSERT" | "INS" => Some(Insert),
        "HOME" => Some(Home),
        "END" => Some(End),
        "PAGEUP" | "PGUP" => Some(PageUp),
        "PAGEDOWN" | "PGDN" => Some(PageDown),
        "LEFT" => Some(ArrowLeft),
        "RIGHT" => Some(ArrowRight),
        "UP" => Some(ArrowUp),
        "DOWN" => Some(ArrowDown),
        "COMMA" | "," => Some(Comma),
        "PERIOD" | "." => Some(Period),
        _ => None,
    } {
        return Ok(c);
    }
    if upper.len() == 1 {
        let ch = upper.chars().next().unwrap();
        return match ch {
            'A'..='Z' => Ok(letter_to_code(ch)),
            '0'..='9' => Ok(digit_to_code(ch)),
            _ => anyhow::bail!("不支持的单字符按键: {ch}"),
        };
    }
    anyhow::bail!("无法解析按键: {s}")
}

fn letter_to_code(c: char) -> Code {
    match c {
        'A' => Code::KeyA,
        'B' => Code::KeyB,
        'C' => Code::KeyC,
        'D' => Code::KeyD,
        'E' => Code::KeyE,
        'F' => Code::KeyF,
        'G' => Code::KeyG,
        'H' => Code::KeyH,
        'I' => Code::KeyI,
        'J' => Code::KeyJ,
        'K' => Code::KeyK,
        'L' => Code::KeyL,
        'M' => Code::KeyM,
        'N' => Code::KeyN,
        'O' => Code::KeyO,
        'P' => Code::KeyP,
        'Q' => Code::KeyQ,
        'R' => Code::KeyR,
        'S' => Code::KeyS,
        'T' => Code::KeyT,
        'U' => Code::KeyU,
        'V' => Code::KeyV,
        'W' => Code::KeyW,
        'X' => Code::KeyX,
        'Y' => Code::KeyY,
        'Z' => Code::KeyZ,
        _ => Code::KeyA,
    }
}

fn digit_to_code(c: char) -> Code {
    match c {
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        _ => Code::Digit0,
    }
}

/// 管理若干热键：每个有名字（用于反注册时定位）。
pub struct HotkeySet {
    manager: global_hotkey::GlobalHotKeyManager,
    /// name -> HotKey（保留实例以便正确 unregister）
    by_name: HashMap<String, HotKey>,
    /// HotKey id -> name
    by_id: HashMap<u32, String>,
}

impl HotkeySet {
    pub fn new() -> Result<Self> {
        let manager =
            global_hotkey::GlobalHotKeyManager::new().context("创建全局热键管理器失败")?;
        Ok(Self {
            manager,
            by_name: HashMap::new(),
            by_id: HashMap::new(),
        })
    }

    pub fn register(&mut self, name: &str, spec: &HotkeySpec) -> Result<u32> {
        if spec.key.trim().is_empty() {
            // key 为空：显式不注册（用于可选热键）
            let _ = self.unregister(name);
            return Ok(0);
        }
        if let Some(hk) = self.by_name.get(name) {
            return Ok(hk.id());
        }
        let hk = parse(spec)?;
        let id = hk.id();
        self.manager
            .register(hk)
            .with_context(|| format!("注册热键 {name} 失败"))?;
        self.by_name.insert(name.to_string(), hk);
        self.by_id.insert(id, name.to_string());
        Ok(id)
    }

    pub fn unregister(&mut self, name: &str) -> Result<()> {
        if let Some(hk) = self.by_name.remove(name) {
            let id = hk.id();
            let _ = self.manager.unregister(hk);
            self.by_id.remove(&id);
        }
        Ok(())
    }

    /// 修改一个热键：先反注册再注册。
    pub fn rebind(&mut self, name: &str, spec: &HotkeySpec) -> Result<u32> {
        let _ = self.unregister(name);
        self.register(name, spec)
    }

    /// 按配置重绑定全部已知热键。
    pub fn rebind_from_config(&mut self, cfg: &super::config::Config) {
        let pairs: [(&str, &HotkeySpec); 14] = [
            ("translate", &cfg.translate_hotkey),
            ("ocr", &cfg.ocr_hotkey),
            ("ocr_only", &cfg.ocr_only_hotkey),
            ("ocr_silent", &cfg.ocr_silent_hotkey),
            ("ocr_finder", &cfg.ocr_finder_hotkey),
            ("silent_replace", &cfg.silent_replace_hotkey),
            ("silent_input", &cfg.silent_input_hotkey),
            ("overlay", &cfg.overlay_hotkey),
            ("favorites", &cfg.favorites_hotkey),
            ("input", &cfg.input_hotkey),
            ("speak", &cfg.speak_hotkey),
            ("pin", &cfg.pin_hotkey),
            ("history", &cfg.history_hotkey),
            ("settings", &cfg.settings_hotkey),
        ];
        for (name, spec) in pairs {
            if let Err(e) = self.rebind(name, spec) {
                log::warn!("重绑定热键 {name} ({spec}) 失败: {e:#}");
            }
        }
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn name_of(&self, id: u32) -> Option<&str> {
        self.by_id.get(&id).map(|s| s.as_str())
    }
}

// 在 hotkey 模块里暴露 spec 解析，让 app/hotkey 不再依赖其他模块。
pub use super::config::HotkeySpec as Spec;
