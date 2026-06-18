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

# Windows 资产是 .exe(.tar.gz 包装);Unix 是裸二进制的 .tar.gz。
BINNAME="$(aipanel_sidecar_name "$TRIPLE")"
ASSET="${BINNAME}.tar.gz"

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
mkdir -p "$DEST"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

echo "↓ $URL"
curl -fSL --retry 3 -o "$tmp/$ASSET" "$URL"
tar xzf "$tmp/$ASSET" -C "$tmp"

src="$(find "$tmp" -type f -name "codex-app-server-${TRIPLE}*" ! -name '*.tar*' | head -1)"
[ -n "$src" ] || { echo "归档里没找到 codex-app-server 二进制" >&2; exit 1; }

out="$DEST/$BINNAME"
cp "$src" "$out"
chmod +x "$out"
echo "✓ 安装到 $out ($(du -h "$out" | cut -f1)) [$VERSION]"
