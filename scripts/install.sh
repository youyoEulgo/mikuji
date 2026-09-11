#!/bin/sh
# ─────────────────────────────────────────────────────────────────────────────
# mikuji installer — Linux / macOS
#
#   curl -fsSL https://raw.githubusercontent.com/youyoEulgo/mikuji/master/scripts/install.sh | sh
#   sh install.sh --no-images
#
# 本地模式：脚本同目录（或仓库根目录的 assets/）若存在 mikuji 二进制和/或
# data.json + images/，则优先使用本地文件，缺失的部分才联网下载。
# 发布包内自带本脚本，解压后可直接执行 —— 此时不需要网络。
#
# 数据目录规则必须与 src/paths.rs 保持一致：
#   MIKUJI_DATA_DIR > $XDG_DATA_HOME/mikuji > ~/.local/share/mikuji
# 安装/升级只覆盖 data.json 与 images/，永不触碰 user_seed.txt / config.toml。
# ─────────────────────────────────────────────────────────────────────────────
set -eu
# 避免调用方的 CDPATH 影响下面用 cd 解析脚本目录的结果。
unset CDPATH || true

REPO="youyoEulgo/mikuji"
REPO_URL="https://github.com/${REPO}.git"
BIN_NAME="mikuji"
MARKER_NAME=".mikuji_data_version"
# 签池数据使用独立 tag，与二进制版本解耦；签池更新时改这里并发布新的 data tag。
DEFAULT_DATA_VERSION="data-v1"
DEFAULT_BASE_URL="https://github.com/${REPO}/releases/download"

# ── 参数（环境变量为默认值，命令行可覆盖）────────────────────────────────────
VERSION="${MIKUJI_VERSION:-}"
DATA_VERSION="${MIKUJI_DATA_VERSION:-}"
BASE_URL="${MIKUJI_BASE_URL:-}"
INSTALL_DIR="${MIKUJI_INSTALL_DIR:-}"
DATA_DIR="${MIKUJI_DATA_DIR:-}"

QUIET=0
NO_IMAGES=0
SKIP_DATA=0
FORCE=0
NO_PATH=0
NO_VERIFY=0
UNINSTALL=0
PURGE=0
RESET_SEED=0
ASSUME_YES=0
FROM_SOURCE=0
CLI_DATA_DIR=0
TMP_DIR=""

truthy() {
  case "${1:-}" in
    1 | true | yes | on | TRUE | YES | ON | True | Yes) return 0 ;;
    *) return 1 ;;
  esac
}

if truthy "${MIKUJI_NO_IMAGES:-}"; then NO_IMAGES=1; fi
if truthy "${MIKUJI_SKIP_DATA:-}"; then SKIP_DATA=1; fi
if truthy "${MIKUJI_FORCE:-}"; then FORCE=1; fi
if truthy "${MIKUJI_NO_PATH:-}" || truthy "${MIKUJI_NO_MODIFY_PATH:-}"; then NO_PATH=1; fi
if truthy "${MIKUJI_NO_VERIFY:-}"; then NO_VERIFY=1; fi
if truthy "${MIKUJI_FROM_SOURCE:-}"; then FROM_SOURCE=1; fi
if truthy "${MIKUJI_YES:-}"; then ASSUME_YES=1; fi
if truthy "${MIKUJI_QUIET:-}"; then QUIET=1; fi

# ── 输出 ─────────────────────────────────────────────────────────────────────
say() { printf '%s\n' "$*"; }
log() { if [ "$QUIET" != 1 ]; then printf '%s\n' "$*" >&2; fi; }
warn() { printf 'mikuji-install: 警告: %s\n' "$*" >&2; }
die() {
  printf 'mikuji-install: 错误: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
mikuji 一键安装脚本（Linux / macOS）

用法:
  sh install.sh [选项]

常用:
  --no-images            不安装立绘（省约 224 MB，只显示文字）
  --version <tag>        指定二进制版本，如 v0.5.1（默认取最新 release）
  --data-version <tag>   指定签池数据 tag（默认内置的 data-v1）
  --force                强制重新下载并安装（忽略"已是最新"）
  --no-path              不修改 shell 配置，只打印需要手动执行的命令
  --quiet                仅输出最终结果
  --uninstall            卸载二进制（保留签池数据与用户种子）
  --purge                配合 --uninstall，连同数据目录一起删除（需 --yes 或交互确认）
  --reset-seed           删除 user_seed.txt（签运会改变，需 --yes 或交互确认）
  --from-source          用 cargo install --git 从源码编译安装（需要 Rust 工具链）
  --base-url <url>       Release 下载前缀，用于镜像/代理
  --install-dir <dir>    二进制安装目录（默认 $XDG_BIN_HOME 或 ~/.local/bin）
  --data-dir <dir>       签池数据目录（默认 $XDG_DATA_HOME/mikuji 或 ~/.local/share/mikuji）
  --yes                  对确认提示一律回答 yes
  -h, --help             显示本帮助

环境变量（与同名选项等价）:
  MIKUJI_VERSION MIKUJI_DATA_VERSION MIKUJI_BASE_URL
  MIKUJI_INSTALL_DIR MIKUJI_DATA_DIR
  MIKUJI_NO_IMAGES MIKUJI_SKIP_DATA MIKUJI_FORCE MIKUJI_NO_PATH
  MIKUJI_NO_MODIFY_PATH MIKUJI_NO_VERIFY MIKUJI_FROM_SOURCE
  MIKUJI_YES MIKUJI_QUIET

安装行为:
  * 二进制  -> $INSTALL_DIR/mikuji
  * 签池数据 -> $DATA_DIR/{data.json,images/}
  * 升级不会覆盖 user_seed.txt 与 config.toml
  * 数据版本未变时跳过 224 MB 下载，只更新二进制
EOF
}

need_value() {
  [ "$#" -ge 2 ] || die "$1 需要一个参数"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    --version)
      need_value "$@"
      VERSION="$2"
      shift 2
      ;;
    --version=*)
      VERSION="${1#*=}"
      shift
      ;;
    --data-version)
      need_value "$@"
      DATA_VERSION="$2"
      shift 2
      ;;
    --data-version=*)
      DATA_VERSION="${1#*=}"
      shift
      ;;
    --base-url)
      need_value "$@"
      BASE_URL="$2"
      shift 2
      ;;
    --base-url=*)
      BASE_URL="${1#*=}"
      shift
      ;;
    --install-dir)
      need_value "$@"
      INSTALL_DIR="$2"
      shift 2
      ;;
    --install-dir=*)
      INSTALL_DIR="${1#*=}"
      shift
      ;;
    --data-dir)
      need_value "$@"
      DATA_DIR="$2"
      CLI_DATA_DIR=1
      shift 2
      ;;
    --data-dir=*)
      DATA_DIR="${1#*=}"
      CLI_DATA_DIR=1
      shift
      ;;
    --no-images) NO_IMAGES=1; shift ;;
    --skip-data) SKIP_DATA=1; shift ;;
    --force) FORCE=1; shift ;;
    --no-path) NO_PATH=1; shift ;;
    --no-verify) NO_VERIFY=1; shift ;;
    --quiet) QUIET=1; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    --purge) PURGE=1; UNINSTALL=1; shift ;;
    --reset-seed) RESET_SEED=1; shift ;;
    --yes) ASSUME_YES=1; shift ;;
    --from-source) FROM_SOURCE=1; shift ;;
    --)
      shift
      break
      ;;
    -*)
      die "未知选项: $1（用 --help 查看用法）"
      ;;
    *)
      die "未知参数: $1（用 --help 查看用法）"
      ;;
  esac
done
[ "$#" -eq 0 ] || die "未知参数: $1（用 --help 查看用法）"

# ── 目录解析（必须与 src/paths.rs 一致）──────────────────────────────────────
if [ -z "$INSTALL_DIR" ]; then
  if [ -n "${XDG_BIN_HOME:-}" ]; then
    INSTALL_DIR="$XDG_BIN_HOME"
  elif [ -n "${HOME:-}" ]; then
    INSTALL_DIR="$HOME/.local/bin"
  else
    die "HOME 未设置，请用 --install-dir 指定安装目录"
  fi
fi

if [ -z "$DATA_DIR" ]; then
  if [ -n "${XDG_DATA_HOME:-}" ]; then
    DATA_DIR="$XDG_DATA_HOME/mikuji"
  elif [ -n "${HOME:-}" ]; then
    DATA_DIR="$HOME/.local/share/mikuji"
  else
    die "HOME 未设置，请用 --data-dir 指定数据目录"
  fi
fi

cleanup() {
  if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

confirm() {
  if [ "$ASSUME_YES" = 1 ]; then
    return 0
  fi
  if [ -t 0 ]; then
    printf '%s [y/N] ' "$1" >&2
    read -r ans || ans=""
    case "$ans" in
      y | Y | yes | YES | Yes) return 0 ;;
      *) return 1 ;;
    esac
  fi
  warn "非交互模式，需要 --yes 才能确认：$1"
  return 1
}

checksum_good() {
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1
}

fetch_stdout() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --retry-delay 2 --connect-timeout 15 "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$1"
  else
    return 1
  fi
}

download() {
  if command -v curl >/dev/null 2>&1; then
    if [ "$QUIET" = 1 ] || [ ! -t 2 ]; then
      curl -fsSL --retry 3 --retry-delay 2 --connect-timeout 15 "$1" -o "$2"
    else
      curl -fL --retry 3 --retry-delay 2 --connect-timeout 15 --progress-bar "$1" -o "$2"
    fi
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    die "需要 curl 或 wget 才能下载"
  fi
}

# ── 卸载 ─────────────────────────────────────────────────────────────────────
do_uninstall() {
  target="${INSTALL_DIR}/${BIN_NAME}"
  if [ -f "$target" ]; then
    rm -f "$target"
    log "已删除 $target"
  else
    log "未找到 $target（可能已经卸载）"
  fi

  if [ "$PURGE" != 1 ]; then
    log "签池数据与用户种子保留在 $DATA_DIR（加 --purge 一并删除）"
    return 0
  fi

  case "$DATA_DIR" in
    "" | "/" | "$HOME")
      die "拒绝删除 $DATA_DIR，请手动清理"
      ;;
  esac
  if [ ! -d "$DATA_DIR" ]; then
    log "数据目录不存在: $DATA_DIR"
    return 0
  fi
  # 只删"看起来确实是 mikuji 数据目录"的目录，避免误删。
  if [ ! -f "${DATA_DIR}/data.json" ] && [ ! -f "${DATA_DIR}/${MARKER_NAME}" ]; then
    die "目录 $DATA_DIR 内没有 mikuji 数据，为安全起见拒绝 --purge，请手动清理"
  fi
  if confirm "将删除整个数据目录 $DATA_DIR（包含 data.json、images/ 与 user_seed.txt，签运会改变）"; then
    rm -rf "$DATA_DIR"
    log "已删除 $DATA_DIR"
  else
    die "已取消"
  fi
}

if [ "$UNINSTALL" = 1 ]; then
  do_uninstall
  exit 0
fi

# ── 平台探测 ─────────────────────────────────────────────────────────────────
detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Linux)
      case "$arch" in
        x86_64 | amd64) printf 'x86_64-unknown-linux-musl\n' ;;
        aarch64 | arm64) printf 'aarch64-unknown-linux-musl\n' ;;
        *) die "没有适用于 Linux/${arch} 的预编译包，可用 --from-source 从源码编译" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        arm64 | aarch64) printf 'aarch64-apple-darwin\n' ;;
        x86_64 | amd64) printf 'x86_64-apple-darwin\n' ;;
        *) die "没有适用于 macOS/${arch} 的预编译包，可用 --from-source 从源码编译" ;;
      esac
      ;;
    *)
      die "不支持的操作系统: ${os}（mikuji 仅支持 Linux 与 macOS）"
      ;;
  esac
}

# 旧版发布资产的命名（v0.5.0 及更早的整包），仅作回退。
legacy_target() {
  case "$1" in
    x86_64-unknown-linux-musl) printf 'x86_64-linux\n' ;;
    aarch64-apple-darwin) printf 'arm64-macos\n' ;;
    *) printf '\n' ;;
  esac
}

resolve_version() {
  if [ -n "$VERSION" ]; then
    printf '%s\n' "$VERSION"
    return 0
  fi
  # 优先用 release 页跳转解析，避开 GitHub API 限流。
  if command -v curl >/dev/null 2>&1; then
    eff=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest" 2>/dev/null || true)
    tag=${eff##*/}
    case "$tag" in
      v[0-9]*) printf '%s\n' "$tag"; return 0 ;;
    esac
  fi
  # 回退：列出 releases，跳过数据包等非版本 tag。
  json=$(fetch_stdout "https://api.github.com/repos/${REPO}/releases?per_page=30" 2>/dev/null || true)
  tag=$(printf '%s\n' "$json" | sed -n 's/.*"tag_name" *: *"\(v[0-9][^"]*\)".*/\1/p' | head -n 1)
  if [ -z "$tag" ]; then
    die "无法确定最新版本，请用 --version vX.Y.Z 指定（或设置 MIKUJI_VERSION）"
  fi
  printf '%s\n' "$tag"
}

# ── 校验 ─────────────────────────────────────────────────────────────────────
# 每个 release 的 checksums.txt 只下载一次，结果缓存在临时目录。
checksums_for() {
  key=$(printf '%s' "$1" | tr '/: ' '___')
  cache="${TMP_DIR}/checksums-${key}.txt"
  tried="${TMP_DIR}/checksums-${key}.tried"
  if [ -f "$tried" ]; then
    if [ -f "$cache" ]; then printf '%s\n' "$cache"; fi
    return 0
  fi
  : >"$tried"
  if download "${BASE_URL}/$1/checksums.txt" "$cache" 2>/dev/null; then
    printf '%s\n' "$cache"
  else
    rm -f "$cache"
  fi
  return 0
}

verify_asset() {
  version="$1"
  file="$2"
  asset="$3"

  if [ "$NO_VERIFY" = 1 ]; then
    log "已跳过校验（--no-verify）: ${asset}"
    return 0
  fi

  cs=$(checksums_for "$version")
  if [ -z "$cs" ] || [ ! -f "$cs" ]; then
    warn "${version} 未提供 checksums.txt，跳过校验"
    return 0
  fi

  want=$(awk -v n="$asset" '{ k = $2; sub(/^\*/, "", k); if (k == n) { print $1; exit } }' "$cs")
  if [ -z "$want" ]; then
    warn "checksums.txt 中没有 ${asset}，跳过校验"
    return 0
  fi

  if ! checksum_good; then
    warn "未找到 sha256sum/shasum，跳过校验"
    return 0
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    got=$(sha256sum "$file" | cut -d' ' -f1)
  else
    got=$(shasum -a 256 "$file" | cut -d' ' -f1)
  fi

  if [ "$got" != "$want" ]; then
    printf 'mikuji-install: 错误: %s 校验失败\n  期望: %s\n  实际: %s\n' "$asset" "$want" "$got" >&2
    exit 1
  fi
  log "校验通过: ${asset}"
}

# ── 下载并解包 ───────────────────────────────────────────────────────────────
fetch_asset() {
  version="$1"
  asset="$2"
  dest="$3"
  if download "${BASE_URL}/${version}/${asset}" "$dest"; then
    verify_asset "$version" "$dest" "$asset"
    return 0
  fi
  rm -f "$dest"
  return 1
}

extract_tarball() {
  tar -xzf "$1" -C "$2" || die "解压失败: $1"
}

find_bin() {
  find "$1" -type f -name "$BIN_NAME" 2>/dev/null | head -n 1
}

find_data_dir() {
  found=$(find "$1" -type f -name data.json 2>/dev/null | head -n 1)
  if [ -z "$found" ]; then
    return 1
  fi
  dirname -- "$found"
}

# ── 安装 ─────────────────────────────────────────────────────────────────────
install_images() {
  src="$1"
  dest="$2"
  mkdir -p "$dest"
  n=0
  skipped=0
  for f in "$src"/*; do
    if [ ! -f "$f" ]; then
      continue
    fi
    base=$(basename -- "$f")
    case "$base" in
      ._* | .DS_Store | .*)
        skipped=$((skipped + 1))
        continue
        ;;
    esac
    cp -f "$f" "$dest/$base" || return 1
    n=$((n + 1))
  done
  log "立绘: ${n} 张 -> $dest"
  if [ "$skipped" -gt 0 ]; then
    log "已跳过 ${skipped} 个 macOS 元数据文件"
  fi
  return 0
}

install_binary() {
  src="$1"
  mkdir -p "$INSTALL_DIR"
  tmp="${INSTALL_DIR}/.${BIN_NAME}.new.$$"
  if command -v install >/dev/null 2>&1; then
    install -m 0755 "$src" "$tmp" || die "无法写入 $INSTALL_DIR"
  else
    cp -f "$src" "$tmp" || die "无法写入 $INSTALL_DIR"
    chmod 0755 "$tmp"
  fi
  mv -f "$tmp" "${INSTALL_DIR}/${BIN_NAME}"
  if [ "$(uname -s)" = "Darwin" ] && command -v xattr >/dev/null 2>&1; then
    xattr -dr com.apple.quarantine "${INSTALL_DIR}/${BIN_NAME}" 2>/dev/null || true
  fi
}

install_data() {
  src="$1"    # 含 data.json（及 images/）的目录，可为空
  json="$2"   # 单独的 data.json 文件（--no-images 走这条），可为空
  mkdir -p "$DATA_DIR"

  # data.json 原子替换；其余文件（user_seed.txt / config.toml）一律不动。
  json_src=""
  if [ -n "$json" ] && [ -f "$json" ]; then
    json_src="$json"
  elif [ -f "${src}/data.json" ]; then
    json_src="${src}/data.json"
  fi
  if [ -n "$json_src" ]; then
    cp -f "$json_src" "${DATA_DIR}/.data.json.new.$$" || die "无法写入 $DATA_DIR"
    mv -f "${DATA_DIR}/.data.json.new.$$" "${DATA_DIR}/data.json"
  fi

  marker="$DATA_VERSION"
  if [ "$NO_IMAGES" = 1 ]; then
    marker="${DATA_VERSION}+text"
    log "已跳过立绘（--no-images），仅安装 data.json"
  elif [ -d "${src}/images" ]; then
    install_images "${src}/images" "${DATA_DIR}/images" || die "复制立绘失败"
  else
    warn "数据包内没有 images/ 目录，按纯文字模式安装"
    marker="${DATA_VERSION}+text"
  fi

  printf '%s\n' "$marker" >"${DATA_DIR}/${MARKER_NAME}.$$"
  mv -f "${DATA_DIR}/${MARKER_NAME}.$$" "${DATA_DIR}/${MARKER_NAME}"
}

data_up_to_date() {
  if [ "$FORCE" = 1 ]; then
    return 1
  fi
  if [ ! -f "${DATA_DIR}/data.json" ] || [ ! -f "${DATA_DIR}/${MARKER_NAME}" ]; then
    return 1
  fi
  cur=$(cat "${DATA_DIR}/${MARKER_NAME}" 2>/dev/null || true)
  if [ "$cur" = "$DATA_VERSION" ]; then
    return 0
  fi
  # 纯文字模式装过同一版本数据，也视为已就绪。
  if [ "$cur" = "${DATA_VERSION}+text" ] && [ "$NO_IMAGES" = 1 ]; then
    return 0
  fi
  return 1
}

add_path_line() {
  target_file="$1"
  line="$2"
  mkdir -p "$(dirname -- "$target_file")"
  touch "$target_file"
  if ! grep -Fqs "$line" "$target_file"; then
    printf '\n# Added by the mikuji installer\n%s\n' "$line" >>"$target_file"
    log "已写入 $target_file"
  fi
}

update_path() {
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*)
      PATH_READY=1
      return 0
      ;;
  esac
  PATH_READY=0
  if [ "$NO_PATH" = 1 ]; then
    return 0
  fi
  line="export PATH=\"${INSTALL_DIR}:\$PATH\""
  case "$(basename -- "${SHELL:-sh}")" in
    zsh) add_path_line "${HOME}/.zshrc" "$line" ;;
    bash) add_path_line "${HOME}/.bashrc" "$line" ;;
    fish)
      fish_config="${HOME}/.config/fish/config.fish"
      mkdir -p "$(dirname -- "$fish_config")"
      touch "$fish_config"
      if ! grep -Fqs "$INSTALL_DIR" "$fish_config"; then
        printf '\n# Added by the mikuji installer\nfish_add_path "%s"\n' "$INSTALL_DIR" >>"$fish_config"
        log "已写入 $fish_config"
      fi
      ;;
    *) add_path_line "${HOME}/.profile" "$line" ;;
  esac
}

# ── 主流程 ───────────────────────────────────────────────────────────────────
TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/mikuji-install.XXXXXX") || die "无法创建临时目录"
TARGET=$(detect_target)
VERSION=$(resolve_version)
if [ -z "$DATA_VERSION" ]; then
  DATA_VERSION="$DEFAULT_DATA_VERSION"
fi
if [ -z "$BASE_URL" ]; then
  BASE_URL="$DEFAULT_BASE_URL"
fi

log "平台: ${TARGET}"
log "版本: ${VERSION}"
log "二进制 -> ${INSTALL_DIR}/${BIN_NAME}"
log "签池数据 -> ${DATA_DIR} (${DATA_VERSION})"

# 本地包探测：解压后的发布包内直接执行时使用本地文件。
SCRIPT_DIR=""
case "$0" in
  */*) SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" 2>/dev/null && pwd) || SCRIPT_DIR="" ;;
  *)
    # curl | sh 时 $0 是 "sh"，此时当前目录里的同名文件不应被误认为本地包。
    if [ -f "$0" ]; then
      SCRIPT_DIR=$(pwd)
    fi
    ;;
esac

LOCAL_BIN=""
if [ -n "$SCRIPT_DIR" ] && [ -f "${SCRIPT_DIR}/${BIN_NAME}" ]; then
  LOCAL_BIN="${SCRIPT_DIR}/${BIN_NAME}"
fi

LOCAL_DATA=""
if [ -n "$SCRIPT_DIR" ]; then
  if [ -f "${SCRIPT_DIR}/data.json" ]; then
    LOCAL_DATA="$SCRIPT_DIR"
  elif [ -f "${SCRIPT_DIR}/../assets/data.json" ]; then
    LOCAL_DATA=$(cd -- "${SCRIPT_DIR}/../assets" && pwd)
  fi
fi

# 已是最新则直接退出（本地包模式除外：用户显式执行安装脚本时应完成安装）。
if [ "$FORCE" != 1 ] && [ -z "$LOCAL_BIN" ] && [ -z "$LOCAL_DATA" ] && [ -x "${INSTALL_DIR}/${BIN_NAME}" ]; then
  cur_bin=$("${INSTALL_DIR}/${BIN_NAME}" --version 2>/dev/null | awk '{ print $NF }' || true)
  if [ "$cur_bin" = "${VERSION#v}" ]; then
    if [ "$SKIP_DATA" = 1 ] || data_up_to_date; then
      say "${BIN_NAME} ${cur_bin} 已是最新（签池 ${DATA_VERSION}）"
      say "如需强制重装请加 --force"
      exit 0
    fi
  fi
fi

# ── 取二进制 ─────────────────────────────────────────────────────────────────
BIN_SRC=""
DATA_SRC=""
DATA_JSON=""
if [ -n "$LOCAL_BIN" ]; then
  BIN_SRC="$LOCAL_BIN"
  log "使用本地二进制: ${LOCAL_BIN}"
elif [ "$FROM_SOURCE" = 1 ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    die "--from-source 需要 Rust 工具链，见 https://rustup.rs"
  fi
  log "从源码编译（会 clone 整个仓库，含约 224 MB 立绘，耗时较长）"
  cargo install --git "$REPO_URL" --locked --force --root "${TMP_DIR}/cargo-root" ||
    die "cargo install 失败"
  BIN_SRC="${TMP_DIR}/cargo-root/bin/${BIN_NAME}"
  [ -f "$BIN_SRC" ] || die "编译产物不存在: $BIN_SRC"
else
  asset="${BIN_NAME}-${VERSION}-${TARGET}.tar.gz"
  mkdir -p "${TMP_DIR}/bin-pkg"
  if fetch_asset "$VERSION" "$asset" "${TMP_DIR}/${asset}"; then
    extract_tarball "${TMP_DIR}/${asset}" "${TMP_DIR}/bin-pkg"
    BIN_SRC=$(find_bin "${TMP_DIR}/bin-pkg")
    [ -n "$BIN_SRC" ] || die "压缩包内找不到 ${BIN_NAME} 可执行文件"
  else
    legacy=$(legacy_target "$TARGET")
    if [ -z "$legacy" ]; then
      die "下载失败: ${BASE_URL}/${VERSION}/${asset}"
    fi
    legacy_asset="${BIN_NAME}-${VERSION}-${legacy}.tar.gz"
    warn "没有新命名的 ${TARGET} 包，回退旧版整包 ${legacy_asset}"
    if ! fetch_asset "$VERSION" "$legacy_asset" "${TMP_DIR}/${legacy_asset}"; then
      die "下载失败: ${BASE_URL}/${VERSION}/${legacy_asset}"
    fi
    extract_tarball "${TMP_DIR}/${legacy_asset}" "${TMP_DIR}/bin-pkg"
    BIN_SRC=$(find_bin "${TMP_DIR}/bin-pkg")
    [ -n "$BIN_SRC" ] || die "旧版整包内找不到 ${BIN_NAME}"
    if [ -z "$LOCAL_DATA" ] && [ "$SKIP_DATA" != 1 ]; then
      # 旧版整包同时含签池数据，直接复用。
      if DATA_SRC=$(find_data_dir "${TMP_DIR}/bin-pkg"); then
        log "旧版整包内含签池数据，复用"
      else
        DATA_SRC=""
      fi
    fi
  fi
fi

# ── 取签池数据 ───────────────────────────────────────────────────────────────
if [ -n "$DATA_SRC" ]; then
  :
elif [ "$SKIP_DATA" = 1 ]; then
  log "已跳过签池数据（--skip-data）"
elif [ -n "$LOCAL_DATA" ]; then
  DATA_SRC="$LOCAL_DATA"
  log "使用本地签池数据: ${LOCAL_DATA}"
elif data_up_to_date; then
  log "签池数据已是 ${DATA_VERSION}，跳过下载"
elif [ "$NO_IMAGES" = 1 ]; then
  # 纯文字安装只需要 data.json：数据 release 额外提供了这个约 292 KB 的资产，
  # 避免为了不看图也下载 224 MB 整包。
  json_asset="${BIN_NAME}-${DATA_VERSION}.json"
  if ! fetch_asset "$DATA_VERSION" "$json_asset" "${TMP_DIR}/${json_asset}"; then
    printf 'mikuji-install: 错误: 无法下载签池数据 %s\n' "${BASE_URL}/${DATA_VERSION}/${json_asset}" >&2
    printf '  %s\n' "该资产只含 data.json，体积约 292 KB。可选：" >&2
    printf '  %s\n' "  去掉 --no-images    下载完整数据包（含立绘，约 224 MB）" >&2
    printf '  %s\n' "  --data-version TAG 指定其它数据版本" >&2
    printf '  %s\n' "  --skip-data        只装二进制，稍后手动部署签池数据" >&2
    printf '  %s\n' "  --base-url URL     使用镜像/代理前缀" >&2
    exit 1
  fi
  DATA_JSON="${TMP_DIR}/${json_asset}"
else
  data_asset="${BIN_NAME}-${DATA_VERSION}.tar.gz"
  mkdir -p "${TMP_DIR}/data-pkg"
  if ! fetch_asset "$DATA_VERSION" "$data_asset" "${TMP_DIR}/${data_asset}"; then
    printf 'mikuji-install: 错误: 无法下载签池数据 %s\n' "${BASE_URL}/${DATA_VERSION}/${data_asset}" >&2
    printf '  %s\n' "data.json 是运行所必需的。可选：" >&2
    printf '  %s\n' "  --no-images        只下载 data.json（约 292 KB）" >&2
    printf '  %s\n' "  --data-version TAG 指定其它数据版本" >&2
    printf '  %s\n' "  --skip-data        只装二进制，稍后手动部署签池数据" >&2
    printf '  %s\n' "  --base-url URL     使用镜像/代理前缀" >&2
    exit 1
  fi
  extract_tarball "${TMP_DIR}/${data_asset}" "${TMP_DIR}/data-pkg"
  if ! DATA_SRC=$(find_data_dir "${TMP_DIR}/data-pkg"); then
    die "数据包内找不到 data.json"
  fi
fi

# ── 落盘 ─────────────────────────────────────────────────────────────────────
install_binary "$BIN_SRC"
log "已安装可执行文件: ${INSTALL_DIR}/${BIN_NAME}"

if [ -n "$DATA_SRC" ] || [ -n "$DATA_JSON" ]; then
  install_data "$DATA_SRC" "$DATA_JSON"
  log "已安装签池数据: ${DATA_DIR}"
elif [ "$SKIP_DATA" = 1 ]; then
  if [ ! -f "${DATA_DIR}/data.json" ]; then
    warn "未安装签池数据，mikuji 需要 ${DATA_DIR}/data.json 才能运行"
  fi
fi

if [ "$RESET_SEED" = 1 ]; then
  seed="${DATA_DIR}/user_seed.txt"
  if [ ! -f "$seed" ]; then
    log "用户种子不存在，无需重置"
  elif confirm "将删除 $seed，你的签运会改变"; then
    rm -f "$seed"
    log "已删除用户种子（下次运行会重新生成）"
  else
    warn "已跳过用户种子重置"
  fi
fi

update_path

# ── 自检 ─────────────────────────────────────────────────────────────────────
installed="${INSTALL_DIR}/${BIN_NAME}"
if [ -x "$installed" ]; then
  got_version=$("$installed" --version 2>/dev/null || true)
  log "自检: ${got_version:-无法执行 --version}"
  if [ -f "${DATA_DIR}/data.json" ]; then
    count=$(MIKUJI_DATA_DIR="$DATA_DIR" "$installed" --list 2>/dev/null | wc -l | tr -d ' ')
    case "$count" in
      '' | *[!0-9]*) count=0 ;;
    esac
    if [ "$count" -gt 0 ]; then
      log "自检: 签池可用，共 ${count} 签"
    else
      warn "mikuji --list 执行失败，请检查数据目录 ${DATA_DIR}"
    fi
  fi
fi

# ── 结果 ─────────────────────────────────────────────────────────────────────
say ""
say "mikuji 已安装到 ${installed}"
if [ -f "${DATA_DIR}/data.json" ]; then
  if [ "$NO_IMAGES" = 1 ]; then
    say "签池数据: ${DATA_DIR}（纯文字模式，未安装立绘）"
  else
    say "签池数据: ${DATA_DIR}"
  fi
else
  say "签池数据: 未安装（mikuji 需要 ${DATA_DIR}/data.json）"
fi

if [ "$CLI_DATA_DIR" = 1 ] && [ "${MIKUJI_DATA_DIR:-}" != "$DATA_DIR" ]; then
  say ""
  say "注意：数据目录是自定义的，运行前请让 mikuji 知道它："
  say "  export MIKUJI_DATA_DIR=\"${DATA_DIR}\""
fi

say ""
if [ "${PATH_READY:-0}" = 1 ]; then
  say "运行: ${BIN_NAME}"
elif [ "$NO_PATH" = 1 ]; then
  say "把下面这行加入 shell 配置后运行 ${BIN_NAME}:"
  say "  export PATH=\"${INSTALL_DIR}:\$PATH\""
else
  say "已把 ${INSTALL_DIR} 写入 shell 配置。重开终端，或先执行："
  say "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  say "然后运行: ${BIN_NAME}"
fi
