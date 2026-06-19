#!/usr/bin/env bash
# 取得用于打包的 codex-app-server 二进制(Codex 桌面 app 的同款引擎),放到 Tauri
# sidecar 目录。**二进制不进仓库**(见 .gitignore);dev 首次构建前 + 发布 CI 跑一次。
#
# 用法:
#   scripts/fetch-codex.sh                 # 取当前平台
#   scripts/fetch-codex.sh <target-triple> # 取指定平台(交叉打包时)
#
# 来源:GitHub openai/codex 的 rust 版 release,资产 codex-app-server-<triple>。
# 我们只用 app-server(直接 stdio 运行,无需 app-server 子命令),比完整 codex CLI 小很多。
set -euo pipefail

VERSION="rust-v0.141.0"
REPO="openai/codex"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/apps/desktop/src-tauri/binaries"
source "$ROOT/scripts/release-lib.sh"

TRIPLE="${1:-}"
if [ -z "$TRIPLE" ]; then
  TRIPLE="$(aipanel_target_triple)"
fi

# 安装名必须与 Tauri 构建目标三元组一致(externalBin 以三元组结尾)。
BINNAME="$(aipanel_sidecar_name "$TRIPLE")"

# 下载三元组:codex 的 Linux 资产只发 musl(静态二进制,gnu 系统亦可直接运行),
# 故把 *-linux-gnu 映射到 *-linux-musl 下载;其它平台原样。Windows 资产是 .exe(.tar.gz 包装)。
DL_TRIPLE="$TRIPLE"
case "$TRIPLE" in
  *-linux-gnu) DL_TRIPLE="${TRIPLE%-gnu}-musl" ;;
esac
DL_NAME="$(aipanel_sidecar_name "$DL_TRIPLE")"
ASSET="${DL_NAME}.tar.gz"

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
mkdir -p "$DEST"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

echo "↓ $URL"
curl -fSL --retry 3 -o "$tmp/$ASSET" "$URL"
tar xzf "$tmp/$ASSET" -C "$tmp"

src="$(find "$tmp" -type f -name "codex-app-server-${DL_TRIPLE}*" ! -name '*.tar*' | head -1)"
[ -n "$src" ] || { echo "归档里没找到 codex-app-server 二进制" >&2; exit 1; }

out="$DEST/$BINNAME"
cp "$src" "$out"
chmod +x "$out"
echo "✓ 安装到 $out ($(du -h "$out" | cut -f1)) [$VERSION]"
