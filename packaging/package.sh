#!/usr/bin/env bash
# 跨端打包入口：在当前系统编译（可选）并调用对应平台脚本。
#
# 用法：
#   bash packaging/package.sh                 # 本机 triple，只打包（需已有 release 产物）
#   bash packaging/package.sh --build         # 先 cargo build --release 再打包
#   bash packaging/package.sh --build --target aarch64-apple-darwin
#   bash packaging/package.sh --help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT}"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

BUILD=0
TARGET=""
HELP=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build) BUILD=1; shift ;;
        --target)
            TARGET="${2:-}"
            if [[ -z "${TARGET}" ]]; then
                echo "--target 需要 triple" >&2
                exit 1
            fi
            shift 2
            ;;
        -h|--help) HELP=1; shift ;;
        *)
            echo "未知参数: $1" >&2
            HELP=1
            shift
            ;;
    esac
done

if [[ "${HELP}" -eq 1 ]]; then
    cat <<'EOF'
Transel 跨端打包

  bash packaging/package.sh [--build] [--target <rust-triple>]

  --build    先 cargo build --release（带 --target 时传给 cargo）
  --target   默认本机 rustc host triple

产物写入 dist/：
  macOS    Transel-<ver>-<triple>.dmg  与  dist/Transel.app
  Linux    transel_<ver>_<arch>.deb  与  Transel-<ver>-<arch>.AppImage（x86_64）
  Windows  请用 pwsh packaging/package.ps1（或 Git Bash 下仍可 --build，打包需 PowerShell）

Windows 示例：
  pwsh packaging/package.ps1 -Build
  pwsh packaging/package.ps1 -Build -Target x86_64-pc-windows-msvc
EOF
    exit 0
fi

if [[ -z "${TARGET}" ]]; then
    TARGET="$(host_triple)"
fi

echo "==> 仓库: ${ROOT}"
echo "==> 版本: $(app_version)"
echo "==> 目标: ${TARGET}"

case "$(uname -s)" in
    Darwin)
        if [[ "${TARGET}" != *apple-darwin ]]; then
            echo "当前是 macOS，不能为 ${TARGET} 打 .app/.dmg" >&2
            exit 1
        fi
        if [[ "${BUILD}" -eq 1 ]]; then
            echo "==> cargo build --release --target ${TARGET}"
            cargo build --release --target "${TARGET}"
        fi
        bash "${SCRIPT_DIR}/macos/build-app.sh" "${TARGET}"
        ;;
    Linux)
        if [[ "${TARGET}" != *linux* ]]; then
            echo "当前是 Linux，不能为 ${TARGET} 打 .deb/.AppImage" >&2
            exit 1
        fi
        if [[ "${BUILD}" -eq 1 ]]; then
            echo "==> cargo build --release --target ${TARGET}"
            cargo build --release --target "${TARGET}"
        fi
        bash "${SCRIPT_DIR}/linux/build.sh" "${TARGET}"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        if ! command -v pwsh >/dev/null 2>&1; then
            echo "请安装 PowerShell 7+，或直接运行: pwsh packaging/package.ps1 -Target ${TARGET}" >&2
            exit 1
        fi
        extra=()
        if [[ "${BUILD}" -eq 1 ]]; then
            extra+=(-Build)
        fi
        pwsh "${SCRIPT_DIR}/package.ps1" -Target "${TARGET}" "${extra[@]}"
        ;;
    *)
        echo "不支持的打包主机: $(uname -s)" >&2
        echo "Windows 请用: pwsh packaging/package.ps1 -Build" >&2
        exit 1
        ;;
esac

echo "==> dist/"
ls -1 dist 2>/dev/null || true
