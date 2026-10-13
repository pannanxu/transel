#!/usr/bin/env bash
# 供各平台打包脚本 source。假定调用方已 cd 到仓库根。

packaging_root() {
    (cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
}

repo_root() {
    (cd "$(packaging_root)/.." && pwd)
}

host_triple() {
    rustc -vV | awk '/^host:/{print $2}'
}

# APP_VERSION > GitHub tag v* > Cargo.toml version
app_version() {
    if [[ -n "${APP_VERSION:-}" ]]; then
        echo "${APP_VERSION#v}"
        return
    fi
    if [[ -n "${GITHUB_REF_NAME:-}" && "${GITHUB_REF_NAME}" == v* ]]; then
        echo "${GITHUB_REF_NAME#v}"
        return
    fi
    local root
    root="$(pwd)"
    sed -n 's/^version = "\([^"]*\)"/\1/p' "${root}/Cargo.toml" | head -n 1
}

# 解析 cargo 产物目录：优先 target/<triple>/release，本机 triple 可回退 target/release。
# 用法：resolve_bin_dir <target> <exe-name>
# 打印目录路径；找不到则退出 1。
resolve_bin_dir() {
    local target="$1"
    local exe="$2"
    local dir="target/${target}/release"
    if [[ -f "${dir}/${exe}" || -x "${dir}/${exe}" ]]; then
        echo "${dir}"
        return
    fi
    local host
    host="$(host_triple)"
    if [[ "${target}" == "${host}" && ( -f "target/release/${exe}" || -x "target/release/${exe}" ) ]]; then
        echo "target/release"
        return
    fi
    echo "找不到 ${dir}/${exe}" >&2
    echo "请先执行: cargo build --release --target ${target}" >&2
    echo "或对本机: cargo build --release && 使用 $(host_triple)" >&2
    return 1
}

linux_deb_arch() {
    case "$1" in
        *aarch64* | *arm64*) echo "arm64" ;;
        *) echo "amd64" ;;
    esac
}

linux_appimage_arch() {
    case "$1" in
        *aarch64* | *arm64*) echo "aarch64" ;;
        *) echo "x86_64" ;;
    esac
}

windows_zip_arch() {
    case "$1" in
        *aarch64* | *arm64*) echo "arm64" ;;
        *) echo "x86_64" ;;
    esac
}
