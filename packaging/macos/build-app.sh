#!/usr/bin/env bash
# macOS：生成 Transel.app + UDZO .dmg（含 Applications 快捷方式）。
# 用法：bash packaging/macos/build-app.sh <target-triple>

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "用法: $0 <target>" >&2
    exit 1
fi

TARGET="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${ROOT}"
# shellcheck source=../common.sh
source "${SCRIPT_DIR}/../common.sh"

BIN_DIR="$(resolve_bin_dir "${TARGET}" transel)"
VERSION="$(app_version)"
PKG_ROOT="${ROOT}/dist/Transel.app"

echo "==> 清理旧包"
rm -rf "${PKG_ROOT}"
mkdir -p "${PKG_ROOT}/Contents/MacOS"
mkdir -p "${PKG_ROOT}/Contents/Resources"
mkdir -p "${ROOT}/dist"

echo "==> 复制二进制（${BIN_DIR}）"
cp "${BIN_DIR}/transel" "${PKG_ROOT}/Contents/MacOS/transel"
cp "${BIN_DIR}/transel-cli" "${PKG_ROOT}/Contents/MacOS/transel-cli"
chmod +x "${PKG_ROOT}/Contents/MacOS/transel"
chmod +x "${PKG_ROOT}/Contents/MacOS/transel-cli"

echo "==> 生成 Info.plist"
sed -e "s|\${VERSION}|${VERSION}|g" \
    "${SCRIPT_DIR}/Info.plist" \
    > "${PKG_ROOT}/Contents/Info.plist"

ICNS="${SCRIPT_DIR}/icon.icns"
if [[ ! -f "${ICNS}" ]]; then
    echo "缺少 ${ICNS}" >&2
    exit 1
fi
echo "==> 复制图标"
cp "${ICNS}" "${PKG_ROOT}/Contents/Resources/icon.icns"

DMG_STAGE="${ROOT}/dist/dmg-root"
rm -rf "${DMG_STAGE}"
mkdir -p "${DMG_STAGE}"
cp -R "${PKG_ROOT}" "${DMG_STAGE}/Transel.app"
ln -s /Applications "${DMG_STAGE}/Applications"

DMG_NAME="Transel-${VERSION}-${TARGET}.dmg"
echo "==> 创建 ${DMG_NAME}"
hdiutil create -volname "Transel" \
    -srcfolder "${DMG_STAGE}" \
    -ov -format UDZO \
    "${ROOT}/dist/${DMG_NAME}" >/dev/null
rm -rf "${DMG_STAGE}"

echo "==> 完成：dist/${DMG_NAME}"
