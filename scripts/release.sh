#!/usr/bin/env bash
# Transel 一键版本发布脚本
#
# 功能：
#   1. 检查工作区状态与当前分支
#   2. 自动递增版本号（默认 patch，支持 minor / major 或直接指定版本）
#   3. 更新 Cargo.toml 并通过 cargo check 刷新 Cargo.lock
#   4. 运行质量检查（cargo clippy）保证零 warning 发布
#   5. 提交版本变更并创建 Git Tag
#   6. 推送到 GitHub 自动触发 CI 多平台打包发布流水线
#
# 用法：
#   ./scripts/release.sh               # 默认自动递增 patch（例如 0.3.1 -> 0.3.2）
#   ./scripts/release.sh patch         # 递增 patch
#   ./scripts/release.sh minor         # 递增 minor（例如 0.3.1 -> 0.4.0）
#   ./scripts/release.sh major         # 递增 major（例如 0.3.1 -> 1.0.0）
#   ./scripts/release.sh 0.4.0         # 发布指定版本号
#   ./scripts/release.sh --dry-run     # 演练模式（仅打印操作，不修改）
#   ./scripts/release.sh -y            # 跳过交互式确认

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT}"

# 终端颜色
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info() { echo -e "${BLUE}==>${NC} ${BOLD}$*${NC}"; }
success() { echo -e "${GREEN}==>${NC} ${BOLD}$*${NC}"; }
warn() { echo -e "${YELLOW}警告:${NC} $*"; }
error() { echo -e "${RED}错误:${NC} $*" >&2; exit 1; }

DRY_RUN=0
ASSUME_YES=0
TARGET_BUMP="patch"

# 解析参数
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -y|--yes)
            ASSUME_YES=1
            shift
            ;;
        -h|--help)
            cat <<'EOF'
Transel 一键版本发布脚本

用法:
  ./scripts/release.sh [patch|minor|major|<version>] [选项]

参数:
  patch                  递增修订版本号 (例如 0.3.1 -> 0.3.2，默认)
  minor                  递增次版本号 (例如 0.3.1 -> 0.4.0)
  major                  递增主版本号 (例如 0.3.1 -> 1.0.0)
  <version>              指定具体的版本号 (例如 0.4.0 或 v0.4.0)

选项:
  --dry-run              仅显示将要执行的操作，不实际修改和推送
  -y, --yes              跳过确认提示，直接执行发布
  -h, --help             显示此帮助信息

示例:
  ./scripts/release.sh
  ./scripts/release.sh minor
  ./scripts/release.sh 0.4.0
  ./scripts/release.sh --dry-run
EOF
            exit 0
            ;;
        patch|minor|major)
            TARGET_BUMP="$1"
            shift
            ;;
        v[0-9]*|[0-9]*)
            TARGET_BUMP="${1#v}"
            shift
            ;;
        *)
            error "未知参数: $1（运行 ./scripts/release.sh --help 查看用法）"
            ;;
    esac
done

# 1. 检查 git 工作区
if ! git diff-index --quiet HEAD --; then
    error "当前 Git 工作区有未提交的代码，请先提交或 stash 后再执行发布。"
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [[ "${CURRENT_BRANCH}" != "main" && "${CURRENT_BRANCH}" != "master" ]]; then
    warn "当前分支为 '${CURRENT_BRANCH}'，通常应在 'main' 分支上发布版本。"
    if [[ "${ASSUME_YES}" -ne 1 && "${DRY_RUN}" -ne 1 ]]; then
        read -r -p "是否继续在 ${CURRENT_BRANCH} 上发布? (y/N) " confirm
        if [[ ! "${confirm}" =~ ^[yY]$ ]]; then
            echo "已取消发布。"
            exit 0
        fi
    fi
fi

# 2. 读取当前版本号
CURRENT_VERSION="$(awk -F'"' '/^version = "/ {print $2; exit}' Cargo.toml)"
if [[ -z "${CURRENT_VERSION}" ]]; then
    error "无法从 Cargo.toml 中解析出当前版本号。"
fi

# 3. 计算新版本号
bump_version() {
    local cur="$1"
    local type="$2"
    IFS="." read -r major minor patch <<< "${cur%%-*}"
    case "$type" in
        patch)
            patch=$((patch + 1))
            echo "${major}.${minor}.${patch}"
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            echo "${major}.${minor}.${patch}"
            ;;
        major)
            major=$((major + 1))
            minor=0
            patch=0
            echo "${major}.${minor}.${patch}"
            ;;
        *)
            local custom="${type#v}"
            if [[ "$custom" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
                echo "$custom"
            else
                error "无效的版本号: $type"
            fi
            ;;
    esac
}

NEW_VERSION="$(bump_version "${CURRENT_VERSION}" "${TARGET_BUMP}")"
TAG_NAME="v${NEW_VERSION}"

# 检查 Tag 是否已存在
if git rev-parse "${TAG_NAME}" >/dev/null 2>&1; then
    error "Git 标签 '${TAG_NAME}' 已存在，无法重复发布此版本！"
fi

echo -e "──────────────────────────────────────────────"
echo -e "  当前版本:  ${YELLOW}${CURRENT_VERSION}${NC}"
echo -e "  目标版本:  ${GREEN}${NEW_VERSION}${NC} (${TAG_NAME})"
echo -e "  发布分支:  ${BLUE}${CURRENT_BRANCH}${NC}"
if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo -e "  运行模式:  ${YELLOW}DRY-RUN (仅演练，不提交/推送)${NC}"
fi
echo -e "──────────────────────────────────────────────"

if [[ "${ASSUME_YES}" -ne 1 && "${DRY_RUN}" -ne 1 ]]; then
    read -r -p "确认发布版本 ${TAG_NAME}? (y/N) " confirm
    if [[ ! "${confirm}" =~ ^[yY]$ ]]; then
        echo "已取消发布。"
        exit 0
    fi
fi

if [[ "${DRY_RUN}" -eq 1 ]]; then
    info "[Dry Run] 将依次执行以下操作："
    echo "  1. 更新 Cargo.toml: version = \"${NEW_VERSION}\""
    echo "  2. 运行 cargo check 刷新 Cargo.lock"
    echo "  3. 运行 cargo clippy 验证代码质量"
    echo "  4. git commit -am \"chore(release): bump version to ${NEW_VERSION}\""
    echo "  5. git tag -a \"${TAG_NAME}\" -m \"Release ${TAG_NAME}\""
    echo "  6. git push origin ${CURRENT_BRANCH} && git push origin ${TAG_NAME}"
    success "演练完成，未产生任何实际修改。"
    exit 0
fi

# 4. 更新 Cargo.toml 中的版本号
info "更新 Cargo.toml 中的版本号到 ${NEW_VERSION}..."
awk -v new_ver="${NEW_VERSION}" '
!done && /^version = "/ {
    print "version = \"" new_ver "\""
    done=1
    next
}
{ print }
' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

# 5. 更新 Cargo.lock
info "运行 cargo check 刷新 Cargo.lock..."
cargo check

# 6. 代码质量检查
info "运行 cargo clippy 检查代码质量..."
if ! cargo clippy --all-targets -- -D warnings; then
    error "代码检查未通过，已中止发布。请修复 warning 后重新运行。"
fi

# 7. Git 提交与打标签
info "提交版本变更..."
git commit -am "chore(release): bump version to ${NEW_VERSION}"

info "创建 Git 附注标签 ${TAG_NAME}..."
git tag -a "${TAG_NAME}" -m "Release ${TAG_NAME}"

# 8. 推送至远程仓库
info "推送提交与标签至远程 origin..."
git push origin "${CURRENT_BRANCH}"
git push origin "${TAG_NAME}"

echo ""
success "版本 ${TAG_NAME} 已成功发布并推送！"
echo -e "GitHub Actions 正在自动化打包多平台产物："
echo -e "  流水线状态: ${BLUE}https://github.com/pannanxu/transel/actions/workflows/release.yml${NC}"
echo -e "  发布页面:   ${BLUE}https://github.com/pannanxu/transel/releases/tag/${TAG_NAME}${NC}"

if command -v gh >/dev/null 2>&1; then
    echo ""
    info "正在查询 GitHub Actions 流水线..."
    sleep 3
    gh run list --workflow=release.yml --limit=1 || true
fi
