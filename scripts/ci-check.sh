#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/release-lib.sh"

step() {
  printf '\n==> %s\n' "$1"
}

fail() {
  printf 'ci-check: %s\n' "$1" >&2
  exit 1
}

step "Typechecking workspace"
pnpm typecheck

TARGET_TRIPLE="$(aipanel_target_triple)"
SIDE_CAR="$(aipanel_sidecar_path "apps/desktop/src-tauri/binaries" "$TARGET_TRIPLE")"
step "Checking bundled Codex app-server sidecar"
printf 'Target triple: %s\n' "$TARGET_TRIPLE"
[[ -f "$SIDE_CAR" ]] || fail "missing $SIDE_CAR; run scripts/fetch-codex.sh $TARGET_TRIPLE before pnpm ci:check"
[[ -x "$SIDE_CAR" ]] || fail "sidecar is not executable: $SIDE_CAR"
if expected_sidecar_arch_pattern="$(aipanel_macos_file_arch_pattern "$TARGET_TRIPLE")"; then
  sidecar_file_info="$(file "$SIDE_CAR")"
  printf '%s\n' "$sidecar_file_info"
  printf '%s\n' "$sidecar_file_info" | grep -q "$expected_sidecar_arch_pattern" \
    || fail "sidecar architecture mismatch for $TARGET_TRIPLE"
fi

step "Running Rust test suite"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml

step "Running bundled Codex app-server integration test"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml \
  agent::codex::tests::bundled_sidecar_initializes_with_real_protocol -- --ignored

step "Running Rust Clippy"
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings

step "Building frontend packages"
pnpm build

step "CI check passed"
