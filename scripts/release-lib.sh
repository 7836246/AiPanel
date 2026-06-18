#!/usr/bin/env bash

aipanel_target_triple() {
  if [[ -n "${RELEASE_TARGET_TRIPLE:-}" ]]; then
    printf '%s\n' "$RELEASE_TARGET_TRIPLE"
    return
  fi
  rustc -vV | awk '/^host: / { print $2 }'
}

aipanel_sidecar_name() {
  local target_triple="$1"
  case "$target_triple" in
    *windows*) printf 'codex-app-server-%s.exe\n' "$target_triple" ;;
    *)         printf 'codex-app-server-%s\n' "$target_triple" ;;
  esac
}

aipanel_sidecar_path() {
  local sidecar_dir="$1"
  local target_triple="$2"
  printf '%s/%s\n' "$sidecar_dir" "$(aipanel_sidecar_name "$target_triple")"
}

aipanel_macos_file_arch() {
  local target_triple="$1"
  case "$target_triple" in
    aarch64-apple-darwin) printf 'arm64\n' ;;
    x86_64-apple-darwin)  printf 'x86_64\n' ;;
    *)                    return 1 ;;
  esac
}

aipanel_macos_file_arch_pattern() {
  local target_triple="$1"
  local arch
  arch="$(aipanel_macos_file_arch "$target_triple")" || return 1
  printf 'Mach-O 64-bit executable %s\n' "$arch"
}

# 读取 tauri.conf.json 的版本(权威来源)。参数:仓库根目录。
aipanel_conf_version() {
  local root="$1"
  node -e 'console.log(require(process.argv[1]).version)' "$root/apps/desktop/src-tauri/tauri.conf.json"
}

# 读取某 package.json 的版本。
aipanel_pkg_version() {
  node -e 'console.log(require(process.argv[1]).version)' "$1"
}

# 读取 Cargo.toml [package] 段的 version。
aipanel_cargo_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[package]") }
    in_pkg && /^version[[:space:]]*=/ {
      match($0, /"[^"]*"/); print substr($0, RSTART+1, RLENGTH-2); exit
    }
  ' "$1"
}

# 断言所有版本来源一致;若给了第二个参数(tag,如 v0.2.0)还要与之匹配。
# 参数:<repo_root> [tag]。不一致则非零退出并打印差异。
aipanel_assert_versions_match() {
  local root="$1"
  local tag="${2:-}"
  local conf root_pkg desk_pkg cargo
  conf="$(aipanel_conf_version "$root")"
  root_pkg="$(aipanel_pkg_version "$root/package.json")"
  desk_pkg="$(aipanel_pkg_version "$root/apps/desktop/package.json")"
  cargo="$(aipanel_cargo_version "$root/apps/desktop/src-tauri/Cargo.toml")"

  local ok=1
  printf 'tauri.conf.json   %s\n' "$conf"
  printf 'package.json      %s\n' "$root_pkg"
  printf 'desktop/pkg.json  %s\n' "$desk_pkg"
  printf 'Cargo.toml        %s\n' "$cargo"
  [[ "$conf" == "$root_pkg" && "$conf" == "$desk_pkg" && "$conf" == "$cargo" ]] || ok=0

  if [[ -n "$tag" ]]; then
    local tagver="${tag#v}"
    printf 'git tag           %s (=> %s)\n' "$tag" "$tagver"
    [[ "$conf" == "$tagver" ]] || ok=0
  fi

  if [[ "$ok" -ne 1 ]]; then
    echo "版本不一致:用 scripts/bump-version.sh <version> 同步后再发布。" >&2
    return 1
  fi
  echo "版本一致:$conf"
}
