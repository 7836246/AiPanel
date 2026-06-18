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
