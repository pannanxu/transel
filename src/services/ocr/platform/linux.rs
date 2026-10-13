//! Linux 截图：交互框选优先 grim+slurp / maim / gnome-screenshot 等。

use std::path::PathBuf;
use std::process::Command;

use crate::services::ocr::platform::{ImageData, new_png_path, read_png};

pub(super) fn platform_capture_full_screen() -> anyhow::Result<(ImageData, PathBuf)> {
    let path = new_png_path("full");
    let p = path_str(&path)?;
    if run_first(
        &[
            ("grim", vec![p.to_string()]),
            ("gnome-screenshot", vec!["-f".into(), p.to_string()]),
            ("maim", vec![p.to_string()]),
            ("scrot", vec![p.to_string()]),
            (
                "spectacle",
                vec!["-b".into(), "-n".into(), "-o".into(), p.to_string()],
            ),
        ],
        &path,
    )? {
        return Ok((read_png(&path)?, path));
    }
    anyhow::bail!("未找到可用的截屏工具 (grim / gnome-screenshot / maim / scrot / spectacle)")
}

pub(super) fn platform_capture_region(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> anyhow::Result<(ImageData, PathBuf)> {
    if w > 1 && h > 1 {
        let path = new_png_path("region");
        let p = path_str(&path)?;
        let geom_maim = format!("{w}x{h}+{x}+{y}");
        let geom_grim = format!("{x},{y} {w}x{h}");
        if run_first(
            &[
                ("grim", vec!["-g".into(), geom_grim, p.to_string()]),
                ("maim", vec!["-g".into(), geom_maim.clone(), p.to_string()]),
                (
                    "import",
                    vec![
                        "-window".into(),
                        "root".into(),
                        "-crop".into(),
                        geom_maim,
                        p.to_string(),
                    ],
                ),
            ],
            &path,
        )? {
            return Ok((read_png(&path)?, path));
        }
        log::warn!("指定区域截图失败，回退交互框选");
    }
    let path = capture_interactive_path()?;
    Ok((read_png(&path)?, path))
}

pub(super) fn capture_interactive_path() -> anyhow::Result<PathBuf> {
    let path = new_png_path("region");
    let p = path_str(&path)?;

    if is_wayland()
        && let Some(geom) = slurp_geom()
    {
        let out = Command::new("grim").args(["-g", &geom, p]).output();
        if matches!(out, Ok(ref o) if o.status.success()) && path.exists() {
            return Ok(path);
        }
    }

    if run_first(
        &[
            (
                "gnome-screenshot",
                vec!["-a".into(), "-f".into(), p.to_string()],
            ),
            ("maim", vec!["-s".into(), p.to_string()]),
            ("scrot", vec!["-s".into(), p.to_string()]),
            (
                "spectacle",
                vec![
                    "-r".into(),
                    "-b".into(),
                    "-n".into(),
                    "-o".into(),
                    p.to_string(),
                ],
            ),
            (
                "xfce4-screenshooter",
                vec!["-r".into(), "-s".into(), p.to_string()],
            ),
        ],
        &path,
    )? {
        return Ok(path);
    }
    anyhow::bail!(
        "未找到可用的交互截屏工具（Wayland: grim+slurp；X11: gnome-screenshot / maim / scrot / spectacle）"
    )
}

fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

fn slurp_geom() -> Option<String> {
    let out = Command::new("slurp").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let g = String::from_utf8(out.stdout).ok()?;
    let g = g.trim();
    if g.is_empty() {
        None
    } else {
        Some(g.to_string())
    }
}

fn run_first(cmds: &[(&str, Vec<String>)], path: &std::path::Path) -> anyhow::Result<bool> {
    for (bin, args) in cmds {
        match Command::new(bin).args(args).output() {
            Ok(out) if out.status.success() && path.exists() => return Ok(true),
            Ok(_) | Err(_) => continue,
        }
    }
    Ok(false)
}

fn path_str(path: &std::path::Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("截图路径非 UTF-8"))
}
