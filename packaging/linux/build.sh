#!/usr/bin/env bash
# Linux：.deb；x86_64 另打 AppImage。
# 用法：bash packaging/linux/build.sh <target-triple>

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
NAME="transel"
DEB_ARCH="$(linux_deb_arch "${TARGET}")"
APPIMAGE_ARCH="$(linux_appimage_arch "${TARGET}")"
DIST_DIR="${ROOT}/dist"
mkdir -p "${DIST_DIR}"

echo "==> 打包 .deb (${DEB_ARCH})"
DEB_ROOT="${DIST_DIR}/${NAME}_${VERSION}_${DEB_ARCH}"
rm -rf "${DEB_ROOT}"
mkdir -p "${DEB_ROOT}/DEBIAN"
mkdir -p "${DEB_ROOT}/usr/bin"
mkdir -p "${DEB_ROOT}/usr/lib/${NAME}"
mkdir -p "${DEB_ROOT}/usr/share/applications"
mkdir -p "${DEB_ROOT}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${DEB_ROOT}/usr/share/doc/${NAME}"

ICON_PNG="${SCRIPT_DIR}/../icon.png"
if [[ ! -f "${ICON_PNG}" ]]; then
    echo "缺少 ${ICON_PNG}（AppImage / 桌面图标需要）" >&2
    exit 1
fi

cp "${BIN_DIR}/transel" "${DEB_ROOT}/usr/lib/${NAME}/transel"
cp "${BIN_DIR}/transel-cli" "${DEB_ROOT}/usr/lib/${NAME}/transel-cli"
chmod 0755 "${DEB_ROOT}/usr/lib/${NAME}/transel"
chmod 0755 "${DEB_ROOT}/usr/lib/${NAME}/transel-cli"

ln -sf "/usr/lib/${NAME}/transel" "${DEB_ROOT}/usr/bin/transel"
ln -sf "/usr/lib/${NAME}/transel-cli" "${DEB_ROOT}/usr/bin/transel-cli"

cat > "${DEB_ROOT}/DEBIAN/control" <<EOF
Package: ${NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: Transel Contributors <noreply@example.com>
Depends: libc6, libgtk-3-0, libayatana-appindicator3-1, espeak-ng
Recommends: grim, slurp, gnome-screenshot, maim, wl-clipboard
Description: Transel — cross-platform selection translator (GPUI)
 Transel is a cross-platform desktop tool for translating
 selected text, taking screenshots for OCR, and configuring multiple
 translation backends in one place.
EOF

cat > "${DEB_ROOT}/usr/share/applications/${NAME}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Transel
GenericName=Translator
Comment=Translate selected text and screenshots
Exec=/usr/bin/transel
Icon=${NAME}
Terminal=false
Categories=Utility;
StartupNotify=true
EOF

cp "${ICON_PNG}" "${DEB_ROOT}/usr/share/icons/hicolor/256x256/apps/${NAME}.png"

cat > "${DEB_ROOT}/usr/share/doc/${NAME}/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: transel
Source: https://github.com/your-org/transel
License: MIT OR Apache-2.0
EOF

dpkg-deb --build "${DEB_ROOT}" "${DIST_DIR}/${NAME}_${VERSION}_${DEB_ARCH}.deb"

if [[ "${APPIMAGE_ARCH}" != "x86_64" ]]; then
    echo "==> 跳过 AppImage（仅打包 x86_64；当前 ${APPIMAGE_ARCH}）"
    echo "==> Linux 打包完成"
    ls -1 "${DIST_DIR}"
    exit 0
fi

echo "==> 打包 .AppImage"
APPDIR="${DIST_DIR}/AppDir"
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/lib/${NAME}"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/256x256/apps"

cp "${BIN_DIR}/transel" "${APPDIR}/usr/lib/${NAME}/transel"
cp "${BIN_DIR}/transel-cli" "${APPDIR}/usr/lib/${NAME}/transel-cli"
chmod 0755 "${APPDIR}/usr/lib/${NAME}/transel"
chmod 0755 "${APPDIR}/usr/lib/${NAME}/transel-cli"

ln -sf "../lib/${NAME}/transel" "${APPDIR}/usr/bin/transel"
ln -sf "../lib/${NAME}/transel-cli" "${APPDIR}/usr/bin/transel-cli"

cp "${DEB_ROOT}/usr/share/applications/${NAME}.desktop" "${APPDIR}/usr/share/applications/${NAME}.desktop"
sed 's|^Exec=/usr/bin/transel|Exec=transel|' \
    "${DEB_ROOT}/usr/share/applications/${NAME}.desktop" \
    > "${APPDIR}/${NAME}.desktop"
cp "${ICON_PNG}" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/${NAME}.png"
cp "${ICON_PNG}" "${APPDIR}/${NAME}.png"

cat > "${APPDIR}/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
exec "${HERE}/usr/bin/transel" "$@"
EOF
chmod +x "${APPDIR}/AppRun"

TOOL="${DIST_DIR}/appimagetool"
if [[ ! -x "${TOOL}" ]]; then
    echo "==> 下载 appimagetool"
    curl -fsSL -o "${TOOL}" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x "${TOOL}"
fi

# CI 无 FUSE 时走 extract-and-run
export APPIMAGE_EXTRACT_AND_RUN=1
ARCH=x86_64 "${TOOL}" "${APPDIR}" \
    "${DIST_DIR}/Transel-${VERSION}-x86_64.AppImage"

echo "==> Linux 打包完成"
ls -1 "${DIST_DIR}"
