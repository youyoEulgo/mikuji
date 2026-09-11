#!/bin/sh
# mikuji installer 离线自测。
#
# 构造一个假的 release 目录并用本地 HTTP 服务提供下载，验证 install.sh 的
# 各条分支：默认目录解析、纯文字/全量安装、幂等、--force、校验失败、数据缺失、
# 旧版整包回退、本地包模式、PATH 写入、卸载。全程不访问 GitHub。
#
# 用法:
#   cargo build --release --bin mikuji
#   sh scripts/install_test.sh [path/to/install.sh]
#
# 需要: python3（起本地 HTTP 服务）、tar、sha256sum 或 shasum。
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
INSTALLER=${1:-"$ROOT/scripts/install.sh"}
BIN_SRC=""
for candidate in "$ROOT/target/release/mikuji" "$ROOT/target/debug/mikuji"; do
  if [ -x "$candidate" ]; then
    BIN_SRC="$candidate"
    break
  fi
done
if [ -z "$BIN_SRC" ]; then
  printf 'install_test: 请先 cargo build --release --bin mikuji\n' >&2
  exit 1
fi
if [ ! -f "$INSTALLER" ]; then
  printf 'install_test: 找不到安装脚本 %s\n' "$INSTALLER" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  printf 'install_test: 需要 python3 起本地 HTTP 服务\n' >&2
  exit 1
fi

# 假 release 用真实版本号，install.sh 的"已是最新"判断依赖二进制自报版本。
REAL_VERSION=$(sed -n 's/^version *= *"\(.*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
if [ -z "$REAL_VERSION" ]; then
  printf 'install_test: 无法从 Cargo.toml 读取版本\n' >&2
  exit 1
fi
V_GOOD="v${REAL_VERSION}"

# 与 install.sh 相同的平台映射。
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64 | Linux/amd64) TARGET=x86_64-unknown-linux-musl; LEGACY=x86_64-linux ;;
  Linux/aarch64 | Linux/arm64) TARGET=aarch64-unknown-linux-musl; LEGACY="" ;;
  Darwin/arm64 | Darwin/aarch64) TARGET=aarch64-apple-darwin; LEGACY=arm64-macos ;;
  Darwin/x86_64 | Darwin/amd64) TARGET=x86_64-apple-darwin; LEGACY="" ;;
  *)
    printf 'install_test: 跳过：不支持的平台 %s/%s\n' "$(uname -s)" "$(uname -m)"
    exit 0
    ;;
esac

WORK=$(mktemp -d "${TMPDIR:-/tmp}/mikuji-install-test.XXXXXX")
SERVER_PID=""
cleanup() {
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  if [ "${FAIL:-0}" -ne 0 ] || [ "${MIKUJI_TEST_KEEP:-0}" = 1 ]; then
    printf 'install_test: 保留工作目录（清理后重跑即可删除）\nWORK_DIR=%s\n' "$WORK" >&2
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

SERVE="$WORK/serve"
TMP_T="$WORK/tmp"
HOME_T="$WORK/home"
SHELL_T=/bin/bash
NO_PATH_T=1
OUT="$WORK/out.txt"
TRANSCRIPT="$WORK/transcript.txt"
SERVER_LOG="$WORK/server.log"
BASE=""
PASS=0
FAIL=0

mkdir -p "$TMP_T" "$HOME_T" "$SERVE/$V_GOOD" "$SERVE/v9.9.8" "$SERVE/v9.9.7" "$SERVE/data-v9"

# 安装器要放到工作目录：install.sh 会把"脚本同目录的 ../assets"当作本地签池，
# 直接跑仓库里的脚本会让下载分支永远走不到，这里刻意隔离。
TOOL_DIR="$WORK/tool"
mkdir -p "$TOOL_DIR"
cp "$INSTALLER" "$TOOL_DIR/install.sh"
INSTALLER="$TOOL_DIR/install.sh"

# ── 断言 ─────────────────────────────────────────────────────────────────────
ok() { PASS=$((PASS + 1)); printf '  ok   %s\n' "$1"; }
bad() {
  FAIL=$((FAIL + 1))
  printf '  FAIL %s\n' "$1" >&2
}
assert_status() { # 说明 实际 期望
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1（退出码 期望 $3 实际 $2）"; fi
}
assert_file() { # 说明 路径
  if [ -f "$2" ]; then ok "$1"; else bad "$1（缺少文件 $2）"; fi
}
assert_no_file() { # 说明 路径
  if [ ! -e "$2" ]; then ok "$1"; else bad "$1（不应存在 $2）"; fi
}
assert_eq() { # 说明 实际 期望
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1（期望 [$3] 实际 [$2]）"; fi
}
assert_contains() { # 说明 文件 子串
  if grep -Fq "$3" "$2" 2>/dev/null; then ok "$1"; else bad "$1（[$3] 未出现在 $2）"; fi
}
assert_lacks() { # 说明 文件 子串
  if grep -Fq "$3" "$2" 2>/dev/null; then bad "$1（不应出现 [$3]）"; else ok "$1"; fi
}

file_count() { # 目录
  if [ ! -d "$1" ]; then
    printf '0'
    return 0
  fi
  n=$(find "$1" -mindepth 1 -maxdepth 1 -type f 2>/dev/null | wc -l | tr -d ' ')
  case "$n" in '' | *[!0-9]*) n=0 ;; esac
  printf '%s' "$n"
}

request_count() { # 子串
  c=$(grep -c "$1" "$SERVER_LOG" 2>/dev/null || true)
  case "$c" in '' | *[!0-9]*) c=0 ;; esac
  printf '%s' "$c"
}

run() { # 在隔离环境里执行给定命令
  # SH_BIN 可指定 shell，用于在本机复现 CI 的 /bin/sh（dash）：
  #   SH_BIN=/path/to/sh sh scripts/install_test.sh
  set +e
  (
    export TMPDIR="$TMP_T" HOME="$HOME_T" SHELL="$SHELL_T" MIKUJI_BASE_URL="$BASE"
    if [ "$NO_PATH_T" = 1 ]; then export MIKUJI_NO_PATH=1; fi
    if [ -n "${SH_BIN:-}" ]; then
      "$SH_BIN" "$@"
    else
      "$@"
    fi
  ) >"$OUT" 2>&1
  status=$?
  set -e
  {
    printf '\n===== exit %s: %s\n' "$status" "$*"
    cat "$OUT"
  } >>"$TRANSCRIPT"
}

# ── 造包 ─────────────────────────────────────────────────────────────────────
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# 最小 PNG（1x1），仅用于验证复制逻辑，不参与解码。
python3 - "$WORK/tiny.png" <<'PY'
import base64, sys
data = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg=="
)
open(sys.argv[1], "wb").write(data)
PY

make_images_dir() { # 目标目录
  mkdir -p "$1"
  cp "$WORK/tiny.png" "$1/博丽灵梦.png"
  cp "$WORK/tiny.png" "$1/雾雨魔理沙.png"
  # 混入 macOS 元数据，验证安装时被过滤。
  printf 'junk' >"$1/._博丽灵梦.png"
  printf 'junk' >"$1/.DS_Store"
}

make_bin_package() { # 版本 目标文件
  stage="$WORK/stage-bin-$1"
  mkdir -p "$stage/mikuji-$1-$TARGET"
  cp "$BIN_SRC" "$stage/mikuji-$1-$TARGET/mikuji"
  chmod 0755 "$stage/mikuji-$1-$TARGET/mikuji"
  printf '# mikuji %s\n' "$1" >"$stage/mikuji-$1-$TARGET/LICENSE"
  tar -czf "$2" -C "$stage" "mikuji-$1-$TARGET"
}

make_data_package() { # tag 目标文件
  stage="$WORK/stage-data-$1"
  mkdir -p "$stage/mikuji-data-$1"
  cp "$ROOT/assets/data.json" "$stage/mikuji-data-$1/data.json"
  make_images_dir "$stage/mikuji-data-$1/images"
  tar -czf "$2" -C "$stage" "mikuji-data-$1"
}

make_legacy_package() { # 版本 目标文件（模仿 v0.5.0 的整包布局）
  stage="$WORK/stage-legacy-$1"
  mkdir -p "$stage/mikuji"
  cp "$BIN_SRC" "$stage/mikuji/mikuji"
  chmod 0700 "$stage/mikuji/mikuji"
  printf 'junk' >"$stage/mikuji/._mikuji" # AppleDouble 垃圾，必须不干扰二进制查找
  cp "$ROOT/assets/data.json" "$stage/mikuji/data.json"
  make_images_dir "$stage/mikuji/images"
  printf '# 旧版安装说明\n' >"$stage/mikuji/INSTALL.md"
  tar -czf "$2" -C "$stage" mikuji
}

write_checksums() { # 版本
  dir="$SERVE/$1"
  (
    cd "$dir"
    : >checksums.txt
    for f in *.tar.gz *.json; do
      [ -e "$f" ] || continue
      printf '%s  %s\n' "$(sha256_of "$f")" "$f" >>checksums.txt
    done
  )
}

echo "==> 构造假 release（target=${TARGET}）"
make_bin_package "$REAL_VERSION" "$SERVE/$V_GOOD/mikuji-$V_GOOD-${TARGET}.tar.gz"
write_checksums $V_GOOD
make_data_package v9 "$SERVE/data-v9/mikuji-data-v9.tar.gz"
cp "$ROOT/assets/data.json" "$SERVE/data-v9/mikuji-data-v9.json"
write_checksums data-v9
make_bin_package 9.9.8 "$SERVE/v9.9.8/mikuji-v9.9.8-${TARGET}.tar.gz"
printf '%s  %s\n' "0000000000000000000000000000000000000000000000000000000000000000" \
  "mikuji-v9.9.8-${TARGET}.tar.gz" >"$SERVE/v9.9.8/checksums.txt"
rmdir "$SERVE/v9.9.7" 2>/dev/null || true
if [ -n "$LEGACY" ]; then
  mkdir -p "$SERVE/v9.9.7"
  make_legacy_package 9.9.7 "$SERVE/v9.9.7/mikuji-v9.9.7-${LEGACY}.tar.gz"
  write_checksums v9.9.7
fi

echo "==> 启动本地 HTTP 服务"
python3 -u -m http.server 0 --bind 127.0.0.1 --directory "$SERVE" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
i=0
while [ -z "$BASE" ] && [ "$i" -lt 50 ]; do
  port=$(sed -n 's/.*port \([0-9][0-9]*\).*/\1/p' "$SERVER_LOG" 2>/dev/null | head -n 1)
  if [ -n "$port" ]; then
    BASE="http://127.0.0.1:${port}"
  else
    sleep 0.2
    i=$((i + 1))
  fi
done
if [ -z "$BASE" ]; then
  printf 'install_test: 无法启动本地 HTTP 服务\n' >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
fi
echo "    ${BASE}"

# ── 场景 1：默认目录 + 纯文字安装 ────────────────────────────────────────────
echo "==> 场景 1: 默认目录 + --no-images"
run "$INSTALLER" --version $V_GOOD --data-version data-v9 --no-images
assert_status "退出码为 0" "$status" 0
assert_file "二进制落在 ~/.local/bin" "$HOME_T/.local/bin/mikuji"
assert_file "data.json 落在 ~/.local/share/mikuji" "$HOME_T/.local/share/mikuji/data.json"
assert_eq "数据版本标记为纯文字模式" \
  "$(cat "$HOME_T/.local/share/mikuji/.mikuji_data_version" 2>/dev/null || true)" "data-v9+text"
assert_no_file "未安装立绘" "$HOME_T/.local/share/mikuji/images/博丽灵梦.png"
assert_contains "输出提示纯文字模式" "$OUT" "纯文字模式"
assert_eq "纯文字安装未下载 224 MB 整包" "$(request_count "mikuji-data-v9.tar.gz")" 0
assert_eq "纯文字安装只取 data.json 资产" "$(request_count "mikuji-data-v9.json")" 1

# ── 场景 2：幂等（不应重新下载）──────────────────────────────────────────────
echo "==> 场景 2: 重复执行为幂等"
before=$(request_count "mikuji-$V_GOOD-")
run "$INSTALLER" --version $V_GOOD --data-version data-v9 --no-images
assert_status "退出码为 0" "$status" 0
assert_contains "提示已是最新" "$OUT" "已是最新"
assert_eq "没有重新下载二进制" "$(request_count "mikuji-$V_GOOD-")" "$before"

# ── 场景 3：--force 强制重装 ─────────────────────────────────────────────────
echo "==> 场景 3: --force"
run "$INSTALLER" --version $V_GOOD --data-version data-v9 --no-images --force
assert_status "退出码为 0" "$status" 0
assert_contains "重新下载并校验" "$OUT" "校验通过: mikuji-$V_GOOD-${TARGET}.tar.gz"

# ── 场景 4：全量安装（含立绘、过滤 macOS 元数据）────────────────────────────
echo "==> 场景 4: 全量安装 + 立绘过滤"
DATA_T4="$WORK/data4"
run "$INSTALLER" --version $V_GOOD --data-version data-v9 \
  --install-dir "$WORK/bin4" --data-dir "$DATA_T4"
assert_status "退出码为 0" "$status" 0
assert_eq "标记为完整数据" "$(cat "$DATA_T4/.mikuji_data_version" 2>/dev/null || true)" "data-v9"
assert_eq "只复制真实立绘" "$(file_count "$DATA_T4/images")" 2
assert_no_file "过滤 AppleDouble 文件" "$DATA_T4/images/._博丽灵梦.png"
assert_no_file "过滤 .DS_Store" "$DATA_T4/images/.DS_Store"
assert_contains "自检报告签池条数" "$OUT" "签池可用，共 128 签"
assert_contains "自定义数据目录给出提示" "$OUT" "MIKUJI_DATA_DIR"

# ── 场景 4b：纯文字 → 完整安装（由数据版本标记驱动）─────────────────────────
echo "==> 场景 4b: 纯文字安装后补装立绘"
before_tar=$(request_count "mikuji-data-v9.tar.gz")
run "$INSTALLER" --version $V_GOOD --data-version data-v9
assert_status "退出码为 0" "$status" 0
assert_eq "标记升级为完整数据" \
  "$(cat "$HOME_T/.local/share/mikuji/.mikuji_data_version" 2>/dev/null || true)" "data-v9"
assert_eq "补装了立绘" "$(file_count "$HOME_T/.local/share/mikuji/images")" 2
assert_eq "这次才下载整包" "$(request_count "mikuji-data-v9.tar.gz")" "$((before_tar + 1))"

# ── 场景 5：校验失败必须中止 ─────────────────────────────────────────────────
echo "==> 场景 5: checksum 不匹配"
run "$INSTALLER" --version v9.9.8 --data-version data-v9 \
  --install-dir "$WORK/bin5" --data-dir "$WORK/data5"
if [ "$status" -ne 0 ]; then ok "退出码非 0"; else bad "退出码应为非 0"; fi
assert_contains "报告校验失败" "$OUT" "校验失败"
assert_no_file "校验失败时不落盘二进制" "$WORK/bin5/mikuji"

# ── 场景 6：数据包缺失 ───────────────────────────────────────────────────────
echo "==> 场景 6: 数据包缺失 / --skip-data"
run "$INSTALLER" --version $V_GOOD --data-version data-nope \
  --install-dir "$WORK/bin6" --data-dir "$WORK/data6"
if [ "$status" -ne 0 ]; then ok "整包缺失时退出码非 0"; else bad "退出码应为非 0"; fi
assert_contains "给出补救提示" "$OUT" "无法下载签池数据"
assert_no_file "失败时不安装数据" "$WORK/data6/data.json"
run "$INSTALLER" --version $V_GOOD --data-version data-nope --no-images \
  --install-dir "$WORK/bin6" --data-dir "$WORK/data6"
if [ "$status" -ne 0 ]; then ok "纯文字数据缺失时退出码非 0"; else bad "退出码应为非 0"; fi
assert_contains "纯文字路径也给出提示" "$OUT" "约 292 KB"
assert_no_file "纯文字失败时也不落盘数据" "$WORK/data6/data.json"
run "$INSTALLER" --version $V_GOOD --data-version data-nope --skip-data \
  --install-dir "$WORK/bin6" --data-dir "$WORK/data6"
assert_status "--skip-data 可以只装二进制" "$status" 0
assert_file "二进制已安装" "$WORK/bin6/mikuji"
assert_contains "提示缺少 data.json" "$OUT" "未安装签池数据"

# ── 场景 7：旧版整包回退 ─────────────────────────────────────────────────────
if [ -n "$LEGACY" ]; then
  echo "==> 场景 7: 旧版整包回退（${LEGACY}）"
  run "$INSTALLER" --version v9.9.7 --data-version data-v9 \
    --install-dir "$WORK/bin7" --data-dir "$WORK/data7"
  assert_status "退出码为 0" "$status" 0
  assert_file "从旧版整包安装二进制" "$WORK/bin7/mikuji"
  assert_file "复用旧版整包内的 data.json" "$WORK/data7/data.json"
  assert_eq "旧版包内立绘可用" "$(file_count "$WORK/data7/images")" 2
  assert_contains "提示已回退" "$OUT" "回退旧版整包"
else
  echo "==> 场景 7: 跳过（本平台无旧版包）"
fi

# ── 场景 8：本地包模式（完全离线）────────────────────────────────────────────
echo "==> 场景 8: 本地包模式（离线）"
LOCAL="$WORK/local"
mkdir -p "$LOCAL/images"
cp "$INSTALLER" "$LOCAL/install.sh"
cp "$BIN_SRC" "$LOCAL/mikuji"
chmod 0755 "$LOCAL/mikuji"
cp "$ROOT/assets/data.json" "$LOCAL/data.json"
cp "$WORK/tiny.png" "$LOCAL/images/博丽灵梦.png"
GOOD_BASE="$BASE"
BASE="http://127.0.0.1:9" # 死端口：本地模式不应发起任何下载
run "$LOCAL/install.sh" --version v0.0.0-local --data-version data-v9 \
  --install-dir "$WORK/bin8" --data-dir "$WORK/data8"
assert_status "退出码为 0" "$status" 0
assert_file "使用本地二进制" "$WORK/bin8/mikuji"
assert_file "使用本地数据" "$WORK/data8/data.json"
assert_lacks "未发起下载" "$OUT" "下载"
BASE="$GOOD_BASE"

# ── 场景 9：环境变量等价 + PATH 写入 ────────────────────────────────────────
echo "==> 场景 9: 环境变量与 PATH 写入"
NO_PATH_T=0
# 通过导出而非 env 前缀传参，这样 SH_BIN（dash/ash）也能照常工作。
export MIKUJI_VERSION=$V_GOOD MIKUJI_DATA_VERSION=data-v9 MIKUJI_NO_IMAGES=1
export MIKUJI_INSTALL_DIR="$WORK/bin9" MIKUJI_DATA_DIR="$WORK/data9"
run "$INSTALLER"
assert_status "用环境变量安装成功" "$status" 0
assert_file "二进制已安装" "$WORK/bin9/mikuji"
assert_file "数据已安装" "$WORK/data9/data.json"
assert_contains "写入了 shell 配置" "$HOME_T/.bashrc" "Added by the mikuji installer"
assert_contains "写入了 PATH 行" "$HOME_T/.bashrc" "$WORK/bin9"
rc1=$(grep -c "mikuji installer" "$HOME_T/.bashrc" || true)
run "$INSTALLER" --force
assert_status "强制重装成功" "$status" 0
rc2=$(grep -c "mikuji installer" "$HOME_T/.bashrc" || true)
assert_eq "PATH 写入幂等" "$rc2" "$rc1"
unset MIKUJI_VERSION MIKUJI_DATA_VERSION MIKUJI_NO_IMAGES MIKUJI_INSTALL_DIR MIKUJI_DATA_DIR
NO_PATH_T=1

# ── 场景 10：卸载 ────────────────────────────────────────────────────────────
echo "==> 场景 10: 卸载"
run "$INSTALLER" --uninstall --install-dir "$WORK/bin9" --data-dir "$WORK/data9"
assert_status "退出码为 0" "$status" 0
assert_no_file "二进制已删除" "$WORK/bin9/mikuji"
assert_file "卸载保留数据" "$WORK/data9/data.json"
run "$INSTALLER" --uninstall --purge --yes --install-dir "$WORK/bin9" --data-dir "$WORK/data9"
assert_status "purge 退出码为 0" "$status" 0
assert_no_file "数据目录已删除" "$WORK/data9"

# 安全护栏：目录里没有 mikuji 数据时拒绝 --purge。
mkdir -p "$WORK/notmikuji"
printf 'important' >"$WORK/notmikuji/keep.txt"
run "$INSTALLER" --uninstall --purge --yes --data-dir "$WORK/notmikuji"
if [ "$status" -ne 0 ]; then ok "拒绝 purge 非数据目录"; else bad "purge 应被拒绝"; fi
assert_contains "说明拒绝原因" "$OUT" "拒绝 --purge"
assert_file "非数据目录内容未被删除" "$WORK/notmikuji/keep.txt"

# ── 场景 11：--help 与非法参数 ───────────────────────────────────────────────
echo "==> 场景 11: 帮助与参数校验"
run "$INSTALLER" --help
assert_status "--help 退出码为 0" "$status" 0
assert_contains "--help 输出用法" "$OUT" "用法:"
run "$INSTALLER" --bogus
if [ "$status" -ne 0 ]; then ok "未知选项报错退出"; else bad "未知选项应报错"; fi
assert_contains "未知选项给出提示" "$OUT" "未知选项"

# ── 汇总 ─────────────────────────────────────────────────────────────────────
echo ""
echo "通过 ${PASS} 项，失败 ${FAIL} 项"
if [ "$FAIL" -ne 0 ]; then
  echo "完整安装输出见 $TRANSCRIPT" >&2
  exit 1
fi
