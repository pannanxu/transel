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
plutil -lint "${PKG_ROOT}/Contents/Info.plist" >/dev/null
printf 'APPL????' > "${PKG_ROOT}/Contents/PkgInfo"

ICNS="${SCRIPT_DIR}/icon.icns"
if [[ ! -f "${ICNS}" ]]; then
    echo "缺少 ${ICNS}" >&2
    exit 1
fi
echo "==> 复制图标"
cp "${ICNS}" "${PKG_ROOT}/Contents/Resources/AppIcon.icns"
cp "${ICNS}" "${PKG_ROOT}/Contents/Resources/icon.icns"

echo "==> ad-hoc 签名（避免系统判定为损坏）"
xattr -cr "${PKG_ROOT}"
codesign --force --deep --sign - "${PKG_ROOT}"

DMG_STAGE="${ROOT}/dist/dmg-root"
rm -rf "${DMG_STAGE}"
mkdir -p "${DMG_STAGE}"
touch "${ROOT}/dist/.metadata_never_index"
touch "${DMG_STAGE}/.metadata_never_index"
ditto "${PKG_ROOT}" "${DMG_STAGE}/Transel.app"
ln -s /Applications "${DMG_STAGE}/Applications"
cp "${ICNS}" "${DMG_STAGE}/.VolumeIcon.icns"

# 同步文件并清理残留挂载，避免 Resource busy 争用
sync
hdiutil detach "/Volumes/Transel" -force >/dev/null 2>&1 || true

DMG_NAME="Transel-${VERSION}-${TARGET}.dmg"
RW_DMG="${ROOT}/dist/.transel-rw.dmg"
rm -f "${RW_DMG}" "${ROOT}/dist/${DMG_NAME}"

MOUNT_DIR=""
detach_mount() {
    local target="$1"
    for i in {1..5}; do
        if hdiutil detach "${target}" -force >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    diskutil unmount force "${target}" >/dev/null 2>&1 || true
}

cleanup() {
    if [[ -n "${MOUNT_DIR:-}" && -d "${MOUNT_DIR}" ]]; then
        detach_mount "${MOUNT_DIR}"
        rm -rf "${MOUNT_DIR}"
    fi
    rm -f "${RW_DMG:-}"
    rm -rf "${DMG_STAGE:-}"
}
trap cleanup EXIT

echo "==> 创建 ${DMG_NAME}"
CREATE_OK=0
for i in {1..5}; do
    if hdiutil create -volname "Transel" \
        -srcfolder "${DMG_STAGE}" \
        -ov -format UDRW \
        -fs HFS+ \
        "${RW_DMG}" >/dev/null 2>&1; then
        CREATE_OK=1
        break
    fi
    echo "hdiutil create 繁忙，等待重试 ($i/5)..."
    sleep $((i * 2))
done

if [[ "${CREATE_OK}" -ne 1 ]]; then
    echo "hdiutil create 重试失败，执行最后一次并输出日志：" >&2
    hdiutil create -volname "Transel" \
        -srcfolder "${DMG_STAGE}" \
        -ov -format UDRW \
        -fs HFS+ \
        "${RW_DMG}"
fi

MOUNT_DIR="$(mktemp -d /tmp/transel-dmg-mount.XXXXXX)"
if hdiutil attach -readwrite -nobrowse -noverify -noautoopen -mountpoint "${MOUNT_DIR}" "${RW_DMG}" >/dev/null 2>&1; then
    if command -v SetFile >/dev/null 2>&1; then
        SetFile -c icnC "${MOUNT_DIR}/.VolumeIcon.icns" || true
        SetFile -a C "${MOUNT_DIR}" || true
        SetFile -a V "${MOUNT_DIR}/.VolumeIcon.icns" || true
    fi
    sync
    detach_mount "${MOUNT_DIR}"
    rm -rf "${MOUNT_DIR}"
    MOUNT_DIR=""
fi

CONVERT_OK=0
for i in {1..5}; do
    if hdiutil convert "${RW_DMG}" -format UDZO -imagekey zlib-level=9 \
        -ov -o "${ROOT}/dist/${DMG_NAME}" >/dev/null 2>&1; then
        CONVERT_OK=1
        break
    fi
    echo "hdiutil convert 繁忙，等待重试 ($i/5)..."
    sleep $((i * 2))
done

if [[ "${CONVERT_OK}" -ne 1 ]]; then
    echo "hdiutil convert 重试失败，执行最后一次并输出日志：" >&2
    hdiutil convert "${RW_DMG}" -format UDZO -imagekey zlib-level=9 \
        -ov -o "${ROOT}/dist/${DMG_NAME}"
fi

rm -f "${RW_DMG}"
rm -rf "${DMG_STAGE}"

echo "==> 完成：dist/${DMG_NAME}"
