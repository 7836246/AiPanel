#!/usr/bin/env bash
# 统一更新版本号到所有版本来源,保持一致(tauri.conf.json 为权威来源)。
#
# 用法:
#   scripts/bump-version.sh 0.2.0
#
# 同步以下文件的版本:
#   - apps/desktop/src-tauri/tauri.conf.json   (权威)
#   - package.json                              (workspace 根)
#   - apps/desktop/package.json                 (桌面端)
#   - apps/desktop/src-tauri/Cargo.toml         ([package] version)
#   - apps/desktop/src-tauri/Cargo.lock         (desktop 包条目)
#
# 改完后请提交,再打同名 tag(vX.Y.Z)触发 .github/workflows/release.yml 发布。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "用法: scripts/bump-version.sh <version>   例如 0.2.0" >&2
  exit 1
fi
# 校验 semver(X.Y.Z,可带 -rc.1 之类预发布后缀)。
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  echo "版本号不是合法 semver: $VERSION" >&2
  exit 1
fi

python3 - "$ROOT" "$VERSION" <<'PY'
import json, re, sys
from pathlib import Path

root = Path(sys.argv[1])
version = sys.argv[2]

def bump_json(path: Path):
    data = json.loads(path.read_text(encoding="utf-8"))
    data["version"] = version
    # 保留 2 空格缩进 + 末尾换行,贴合现有风格。
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"  ✓ {path.relative_to(root)} -> {version}")

bump_json(root / "apps/desktop/src-tauri/tauri.conf.json")
bump_json(root / "package.json")
bump_json(root / "apps/desktop/package.json")

# Cargo.toml:只改 [package] 段内的第一个 version(避免误伤依赖的 version)。
cargo_toml = root / "apps/desktop/src-tauri/Cargo.toml"
text = cargo_toml.read_text(encoding="utf-8")
lines = text.splitlines(keepends=True)
in_package = False
done = False
for i, line in enumerate(lines):
    stripped = line.strip()
    if stripped.startswith("["):
        in_package = stripped == "[package]"
        continue
    if in_package and not done and re.match(r'^version\s*=\s*"', line):
        lines[i] = re.sub(r'"[^"]*"', f'"{version}"', line, count=1)
        done = True
cargo_toml.write_text("".join(lines), encoding="utf-8")
print(f"  ✓ {cargo_toml.relative_to(root)} [package] -> {version}" if done
      else "  ! Cargo.toml [package] version 未找到,未改动")

# Cargo.lock:更新 name = "desktop" 的那个 [[package]] 条目的 version。
cargo_lock = root / "apps/desktop/src-tauri/Cargo.lock"
if cargo_lock.exists():
    lock = cargo_lock.read_text(encoding="utf-8")
    # 匹配 name = "desktop" 后紧跟的 version = "..." 行。
    pattern = re.compile(r'(\[\[package\]\]\nname = "desktop"\nversion = )"[^"]*"')
    new_lock, n = pattern.subn(rf'\g<1>"{version}"', lock)
    if n:
        cargo_lock.write_text(new_lock, encoding="utf-8")
        print(f"  ✓ {cargo_lock.relative_to(root)} (desktop) -> {version}")
    else:
        print("  ! Cargo.lock desktop 条目未找到,跳过(下次 cargo build 会修正)")
PY

echo "版本已同步为 ${VERSION}。请检查 git diff,提交后打 tag:git tag v${VERSION} && git push origin v${VERSION}"
