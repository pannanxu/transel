//! ONNX Runtime 动态库：按需下载 + `ort::init_from`。
//!
//! 默认不把 Runtime 静态链进安装包（Cargo `load-dynamic`）。
//! 用户在设置中点击下载后，将 Microsoft 官方预编译库解压到缓存目录，
//! 再在首次 OCR 前加载。

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use parking_lot::Mutex;

use crate::app::paths;

/// 与 `ort` 2.0.0-rc.12 / ONNX Runtime API 24 对齐。
pub const ORT_VERSION: &str = "1.24.2";

static LOADED: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrtStatus {
    /// 本平台无可用预编译包（例如较新的 Intel macOS）。
    Unsupported,
    /// 缓存中尚无 dylib。
    Missing,
    /// 已下载，进程内尚未 `init_from`。
    Downloaded,
    /// 已成功加载进当前进程。
    Ready,
}

struct Dist {
    /// GitHub release 资源文件名。
    asset: &'static str,
    /// 解压后主库相对路径（相对归档根目录）。
    lib_rel: &'static str,
}

fn dist_for_host() -> Option<Dist> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some(Dist {
            asset: "onnxruntime-osx-arm64-1.24.2.tgz",
            lib_rel: "lib/libonnxruntime.dylib",
        }),
        ("linux", "x86_64") => Some(Dist {
            asset: "onnxruntime-linux-x64-1.24.2.tgz",
            lib_rel: "lib/libonnxruntime.so",
        }),
        ("linux", "aarch64") => Some(Dist {
            asset: "onnxruntime-linux-aarch64-1.24.2.tgz",
            lib_rel: "lib/libonnxruntime.so",
        }),
        ("windows", "x86_64") => Some(Dist {
            asset: "onnxruntime-win-x64-1.24.2.zip",
            lib_rel: "lib/onnxruntime.dll",
        }),
        ("windows", "aarch64") => Some(Dist {
            asset: "onnxruntime-win-arm64-1.24.2.zip",
            lib_rel: "lib/onnxruntime.dll",
        }),
        _ => None,
    }
}

fn install_root() -> PathBuf {
    let dir = paths::cache_dir().join("onnxruntime").join(ORT_VERSION);
    let _ = fs::create_dir_all(&dir);
    dir
}

/// 缓存中的主库路径（未必已存在）。
pub fn dylib_path() -> Option<PathBuf> {
    let dist = dist_for_host()?;
    Some(install_root().join(Path::new(dist.lib_rel).file_name()?))
}

pub fn status() -> OrtStatus {
    if dist_for_host().is_none() {
        return OrtStatus::Unsupported;
    }
    if LOADED.load(Ordering::SeqCst) {
        return OrtStatus::Ready;
    }
    match dylib_path() {
        Some(p) if p.is_file() => OrtStatus::Downloaded,
        _ => OrtStatus::Missing,
    }
}

pub fn status_label() -> &'static str {
    match status() {
        OrtStatus::Unsupported => "当前平台暂无预编译 ONNX Runtime",
        OrtStatus::Missing => "未下载（本地 OCR 不可用）",
        OrtStatus::Downloaded => "已下载，待加载",
        OrtStatus::Ready => "已就绪",
    }
}

/// 若已下载则加载；未下载则返回错误（不自动联网）。
pub fn ensure_loaded() -> Result<()> {
    if LOADED.load(Ordering::SeqCst) {
        return Ok(());
    }
    let path = dylib_path().context("当前平台不支持本地 ONNX Runtime")?;
    if !path.is_file() {
        bail!(
            "尚未下载 ONNX Runtime。请到设置 → OCR 点击「下载本地 OCR 引擎」。\n期望路径: {}",
            path.display()
        );
    }
    let _guard = DOWNLOAD_LOCK.lock();
    load_from_locked(&path)
}

fn load_from_locked(path: &Path) -> Result<()> {
    if LOADED.load(Ordering::SeqCst) {
        return Ok(());
    }
    ort::init_from(path)
        .map_err(|e| anyhow::anyhow!("加载 ONNX Runtime 失败 ({}): {e}", path.display()))?
        .with_name("selection-translator")
        .with_telemetry(false)
        .commit();
    LOADED.store(true, Ordering::SeqCst);
    log::info!("ONNX Runtime 已加载: {}", path.display());
    Ok(())
}

/// 下载并解压官方预编译库到缓存；若已存在则直接加载。
pub fn download_and_load() -> Result<PathBuf> {
    let dist = dist_for_host().context("当前平台不支持下载预编译 ONNX Runtime")?;
    let _guard = DOWNLOAD_LOCK.lock();

    let dest = dylib_path().expect("dist_for_host ok");
    if dest.is_file() {
        load_from_locked(&dest)?;
        return Ok(dest);
    }

    let url = format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{ORT_VERSION}/{}",
        dist.asset
    );
    log::info!("下载 ONNX Runtime: {url}");
    let archive = install_root().join(dist.asset);
    download_file(&url, &archive)
        .with_context(|| format!("下载失败: {url}"))?;

    let extract_dir = install_root().join("extract");
    let _ = fs::remove_dir_all(&extract_dir);
    fs::create_dir_all(&extract_dir)?;

    if dist.asset.ends_with(".zip") {
        #[cfg(windows)]
        extract_zip(&archive, &extract_dir)?;
        #[cfg(not(windows))]
        bail!("zip 包仅在 Windows 目标上支持");
    } else {
        extract_tgz(&archive, &extract_dir)?;
    }

    // 归档根目录名通常与 asset stem 一致
    let stem = dist.asset.trim_end_matches(".tgz").trim_end_matches(".zip");
    let lib_src = extract_dir.join(stem).join(dist.lib_rel);
    let lib_src = if lib_src.is_file() {
        lib_src
    } else {
        find_named(&extract_dir, Path::new(dist.lib_rel).file_name().unwrap())?
    };

    // 一并拷贝同目录下的依赖（providers_shared 等）
    if let Some(parent) = lib_src.parent() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if !ty.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.contains("onnxruntime") {
                fs::copy(entry.path(), install_root().join(&name))?;
            }
        }
    } else {
        fs::copy(&lib_src, &dest)?;
    }

    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&extract_dir);

    if !dest.is_file() {
        bail!("解压后未找到 {}", dest.display());
    }

    load_from_locked(&dest)?;
    Ok(dest)
}

/// 删除已下载的 Runtime（不卸载已加载的进程内库）。
pub fn uninstall() -> Result<()> {
    let root = paths::cache_dir().join("onnxruntime").join(ORT_VERSION);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let partial = dest.with_extension("partial");
    let _ = fs::remove_file(&partial);
    let resp = ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("HTTP 请求失败: {e}"))?;
    let code = resp.status().as_u16();
    if !(200..300).contains(&code) {
        bail!("HTTP {code}");
    }
    let mut reader = resp.into_body().into_reader();
    let mut file = File::create(&partial)?;
    io::copy(&mut reader, &mut file)?;
    file.flush()?;
    fs::rename(&partial, dest)?;
    Ok(())
}

fn extract_tgz(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(dec);
    archive.unpack(dest)?;
    Ok(())
}

#[cfg(windows)]
fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let out = dest.join(file.mangled_name());
        if file.name().ends_with('/') {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut outfile = File::create(&out)?;
        io::copy(&mut file, &mut outfile)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_dist_known_or_unsupported() {
        match dist_for_host() {
            Some(d) => {
                assert!(d.asset.contains("onnxruntime"));
                assert!(!d.lib_rel.is_empty());
            }
            None => {
                // Intel macOS etc.
                assert_eq!(status(), OrtStatus::Unsupported);
            }
        }
    }
}

fn find_named(root: &Path, name: &std::ffi::OsStr) -> Result<PathBuf> {
    fn walk(dir: &Path, name: &std::ffi::OsStr) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name() == Some(name) && path.is_file() {
                return Some(path);
            }
            if path.is_dir() {
                if let Some(found) = walk(&path, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(root, name).with_context(|| format!("归档中未找到 {:?}", name))
}
