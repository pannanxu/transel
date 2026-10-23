//! 平台特定的开机自启动管理。
//!
//! 支持：
//! - macOS：通过 `~/Library/LaunchAgents/dev.transel.plist` 注册 LaunchAgent。
//! - Linux：通过 `~/.config/autostart/dev.transel.desktop` 注册 XDG Autostart。
//! - Windows：通过注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 注册。

#[allow(unused_imports)]
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
fn launch_agent_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir()
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or_else(|| anyhow::anyhow!("无法获取用户主目录"))?;
    Ok(home.join("Library/LaunchAgents/dev.transel.plist"))
}

#[cfg(target_os = "macos")]
fn get_app_bundle_path(exe: &Path) -> Option<PathBuf> {
    for ancestor in exe.ancestors() {
        if ancestor.extension().and_then(|s| s.to_str()) == Some("app") {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
pub fn set_auto_start(enabled: bool) -> anyhow::Result<()> {
    let plist_path = launch_agent_path()?;
    if enabled {
        let exe = std::env::current_exe()?;
        let program_args = if let Some(bundle) = get_app_bundle_path(&exe) {
            vec![
                "/usr/bin/open".to_string(),
                "-a".to_string(),
                bundle.to_string_lossy().to_string(),
            ]
        } else {
            vec![exe.to_string_lossy().to_string()]
        };

        let args_xml = program_args
            .iter()
            .map(|arg| format!("        <string>{}</string>", xml_escape(arg)))
            .collect::<Vec<_>>()
            .join("\n");

        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.transel</string>
    <key>ProgramArguments</key>
    <array>
{args_xml}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
        );

        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&plist_path, plist_content)?;
        log::info!("已启用 macOS 开机自启动: {}", plist_path.display());
    } else if plist_path.exists() {
        std::fs::remove_file(&plist_path)?;
        log::info!("已禁用 macOS 开机自启动: {}", plist_path.display());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn is_auto_start_enabled() -> bool {
    launch_agent_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn autostart_desktop_path() -> anyhow::Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .ok_or_else(|| anyhow::anyhow!("无法获取用户配置目录"))?;
    Ok(config_dir.join("autostart/dev.transel.desktop"))
}

#[cfg(target_os = "linux")]
pub fn set_auto_start(enabled: bool) -> anyhow::Result<()> {
    let desktop_path = autostart_desktop_path()?;
    if enabled {
        let exe = std::env::current_exe()?;
        let desktop_content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Version=1.0\n\
             Name=Transel\n\
             Comment=Transel 跨平台划词翻译工具\n\
             Exec={}\n\
             Terminal=false\n\
             StartupNotify=false\n\
             Categories=Utility;\n",
            exe.to_string_lossy()
        );
        if let Some(parent) = desktop_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&desktop_path, desktop_content)?;
        log::info!("已启用 Linux 开机自启动: {}", desktop_path.display());
    } else if desktop_path.exists() {
        std::fs::remove_file(&desktop_path)?;
        log::info!("已禁用 Linux 开机自启动: {}", desktop_path.display());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn is_auto_start_enabled() -> bool {
    autostart_desktop_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(target_os = "windows")]
pub fn set_auto_start(enabled: bool) -> anyhow::Result<()> {
    if enabled {
        let exe = std::env::current_exe()?;
        let exe_str = format!("\"{}\"", exe.to_string_lossy());
        let status = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Transel",
                "/t",
                "REG_SZ",
                "/d",
                &exe_str,
                "/f",
            ])
            .status()?;
        if !status.success() {
            anyhow::bail!("执行 reg add 失败: {status}");
        }
        log::info!("已启用 Windows 开机自启动");
    } else {
        let _ = std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Transel",
                "/f",
            ])
            .status();
        log::info!("已禁用 Windows 开机自启动");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn is_auto_start_enabled() -> bool {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Transel",
        ])
        .output();
    output.map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn set_auto_start(_enabled: bool) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn is_auto_start_enabled() -> bool {
    false
}
