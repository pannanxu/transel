use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn should_recompile(src: &Path, out: &Path) -> bool {
    if !out.exists() {
        return true;
    }
    let src_meta = match fs::metadata(src) {
        Ok(m) => m,
        Err(_) => return true,
    };
    let out_meta = match fs::metadata(out) {
        Ok(m) => m,
        Err(_) => return true,
    };
    let src_time = match src_meta.modified() {
        Ok(t) => t,
        Err(_) => return true,
    };
    let out_time = match out_meta.modified() {
        Ok(t) => t,
        Err(_) => return true,
    };
    out_time < src_time
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=macos/apple-ocr.swift");
    println!("cargo:rerun-if-changed=macos/apple-translate.swift");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let ocr_bin = out_dir.join("apple-ocr");
    let translate_bin = out_dir.join("apple-translate");

    if target_os == "macos" {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let ocr_src = manifest_dir.join("macos/apple-ocr.swift");
        let translate_src = manifest_dir.join("macos/apple-translate.swift");

        let has_swiftc = Command::new("/usr/bin/swiftc")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_swiftc {
            // Compile apple-ocr
            if should_recompile(&ocr_src, &ocr_bin) {
                let mut cmd = Command::new("/usr/bin/swiftc");
                cmd.args([
                    "-O",
                    "-o",
                    ocr_bin.to_str().unwrap(),
                    "-framework",
                    "Vision",
                    "-framework",
                    "AppKit",
                    ocr_src.to_str().unwrap(),
                ]);
                if target_arch != env::consts::ARCH {
                    let triple = match target_arch.as_str() {
                        "x86_64" => "x86_64-apple-macos11.0",
                        "aarch64" => "arm64-apple-macos11.0",
                        _ => "",
                    };
                    if !triple.is_empty() {
                        cmd.args(["-target", triple]);
                    }
                }
                let status = cmd
                    .status()
                    .expect("Failed to execute swiftc for apple-ocr");
                if !status.success() {
                    panic!("swiftc failed to compile macos/apple-ocr.swift");
                }
                let _ = Command::new("/usr/bin/codesign")
                    .args([
                        "--force",
                        "--sign",
                        "-",
                        "--timestamp=none",
                        ocr_bin.to_str().unwrap(),
                    ])
                    .status();
            }

            // Compile apple-translate
            if should_recompile(&translate_src, &translate_bin) {
                let mut cmd = Command::new("/usr/bin/swiftc");
                cmd.args([
                    "-O",
                    "-o",
                    translate_bin.to_str().unwrap(),
                    "-framework",
                    "AppKit",
                    "-framework",
                    "SwiftUI",
                    "-framework",
                    "Translation",
                    translate_src.to_str().unwrap(),
                ]);
                if target_arch != env::consts::ARCH {
                    let triple = match target_arch.as_str() {
                        "x86_64" => "x86_64-apple-macos26.0",
                        "aarch64" => "arm64-apple-macos26.0",
                        _ => "",
                    };
                    if !triple.is_empty() {
                        cmd.args(["-target", triple]);
                    }
                }
                let status = cmd
                    .status()
                    .expect("Failed to execute swiftc for apple-translate");
                if !status.success() {
                    panic!("swiftc failed to compile macos/apple-translate.swift");
                }
                let _ = Command::new("/usr/bin/codesign")
                    .args([
                        "--force",
                        "--sign",
                        "-",
                        "--timestamp=none",
                        translate_bin.to_str().unwrap(),
                    ])
                    .status();
            }
        } else {
            if !ocr_bin.exists() {
                fs::write(&ocr_bin, b"").unwrap();
            }
            if !translate_bin.exists() {
                fs::write(&translate_bin, b"").unwrap();
            }
        }
    } else {
        if !ocr_bin.exists() {
            fs::write(&ocr_bin, b"").unwrap();
        }
        if !translate_bin.exists() {
            fs::write(&translate_bin, b"").unwrap();
        }
    }
}
