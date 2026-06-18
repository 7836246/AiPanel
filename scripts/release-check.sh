#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT/apps/desktop/src-tauri"
TAURI_CONF="$TAURI_DIR/tauri.conf.json"
SIDE_CAR_DIR="$TAURI_DIR/binaries"

cd "$ROOT"
source "$ROOT/scripts/release-lib.sh"

read -r PRODUCT_NAME VERSION BUNDLE_IDENTIFIER < <(
  node -e '
    const fs = require("node:fs");
    const conf = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    process.stdout.write(`${conf.productName} ${conf.version} ${conf.identifier}\n`);
  ' "$TAURI_CONF"
)
APP_BUNDLE="$TAURI_DIR/target/release/bundle/macos/$PRODUCT_NAME.app"
DMG_DIR="$TAURI_DIR/target/release/bundle/dmg"
TARGET_TRIPLE="$(aipanel_target_triple)"
SIDE_CAR="$(aipanel_sidecar_path "$SIDE_CAR_DIR" "$TARGET_TRIPLE")"

step() {
  printf '\n==> %s\n' "$1"
}

fail() {
  printf 'release-check: %s\n' "$1" >&2
  exit 1
}

signature_info_for() {
  codesign -dv "$1" 2>&1 || true
}

require_developer_id_signature() {
  local artifact="$1"
  local label="$2"
  local verify_args=(--verify --verbose=2)
  if [[ -d "$artifact" ]]; then
    verify_args=(--verify --deep --strict --verbose=2)
  fi

  local signature_info
  signature_info="$(signature_info_for "$artifact")"
  printf '%s\n' "$signature_info" | sed -n '1,24p'

  if ! codesign "${verify_args[@]}" "$artifact"; then
    fail "$label code signature verification failed; create a distributable build with a valid Developer ID Application certificate"
  fi

  if printf '%s\n' "$signature_info" | grep -q 'Authority=Apple Development'; then
    fail "$label is signed with an Apple Development identity; use a Developer ID Application identity for distributable macOS releases"
  fi

  if ! printf '%s\n' "$signature_info" | grep -q 'Authority=Developer ID Application'; then
    fail "$label is not signed with a Developer ID Application identity"
  fi
}

require_stapled_ticket() {
  local artifact="$1"
  local label="$2"
  if ! xcrun stapler validate "$artifact"; then
    fail "missing or invalid notarization ticket on $label"
  fi
}

step "Checking release metadata"
printf 'Product: %s %s\n' "$PRODUCT_NAME" "$VERSION"
printf 'Bundle identifier: %s\n' "$BUNDLE_IDENTIFIER"
if [[ "$BUNDLE_IDENTIFIER" == *.app ]]; then
  fail "bundle identifier must not end with .app; macOS uses .app as the application bundle extension"
fi
if [[ ! "$BUNDLE_IDENTIFIER" =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\.[A-Za-z0-9][A-Za-z0-9-]*)+$ ]]; then
  fail "bundle identifier is invalid: $BUNDLE_IDENTIFIER"
fi

step "Checking version sources are in sync"
# 发布前确保 tauri.conf / package.json(根、桌面)/ Cargo.toml 版本一致;
# 若在 tag 上构建(GITHUB_REF_NAME=v*),还要与 tag 匹配,避免发出版本号错配的更新清单。
RELEASE_TAG="${GITHUB_REF_NAME:-}"
if [[ "$RELEASE_TAG" != v* ]]; then
  RELEASE_TAG="$(git -C "$ROOT" describe --exact-match --tags 2>/dev/null || true)"
fi
aipanel_assert_versions_match "$ROOT" "$RELEASE_TAG" || fail "version sources are not in sync"

step "Checking bundled Codex app-server sidecar"
printf 'Target triple: %s\n' "$TARGET_TRIPLE"
[[ -f "$SIDE_CAR" ]] || fail "missing $SIDE_CAR; run scripts/fetch-codex.sh $TARGET_TRIPLE before release builds"
[[ -x "$SIDE_CAR" ]] || fail "sidecar is not executable: $SIDE_CAR"

step "Running bundled Codex app-server integration test"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml \
  agent::codex::tests::bundled_sidecar_initializes_with_real_protocol -- --ignored

step "Typechecking workspace"
pnpm typecheck

step "Running Rust test suite"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml

step "Running Rust Clippy"
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings

step "Building frontend packages"
pnpm build

step "Building Tauri bundles"
pnpm tauri build

[[ -d "$APP_BUNDLE" ]] || fail "missing app bundle: $APP_BUNDLE"
shopt -s nullglob
dmgs=("$DMG_DIR"/"${PRODUCT_NAME}_${VERSION}"_*.dmg)
shopt -u nullglob
if (( ${#dmgs[@]} == 0 )); then
  fail "missing dmg bundle matching $DMG_DIR/${PRODUCT_NAME}_${VERSION}_*.dmg"
fi
if (( ${#dmgs[@]} > 1 )); then
  printf 'Matched DMGs:\n' >&2
  printf '  %s\n' "${dmgs[@]}" >&2
  fail "multiple dmg bundles matched; clean old release artifacts before shipping"
fi
DMG="${dmgs[0]}"
printf 'Release artifacts:\n  %s\n  %s\n' "$APP_BUNDLE" "$DMG"

step "Checking bundle identity"
actual_identifier="$(plutil -extract CFBundleIdentifier raw "$APP_BUNDLE/Contents/Info.plist")"
printf 'Bundle identifier: %s\n' "$actual_identifier"
if [[ "$actual_identifier" != "$BUNDLE_IDENTIFIER" ]]; then
  fail "bundle identifier mismatch: tauri.conf.json has $BUNDLE_IDENTIFIER but app bundle has $actual_identifier"
fi

expected_arch_pattern="$(aipanel_macos_file_arch_pattern "$TARGET_TRIPLE")" \
  || fail "release-check currently verifies macOS app bundles only; unsupported target triple: $TARGET_TRIPLE"

step "Checking app executable architecture"
app_executable_name="$(plutil -extract CFBundleExecutable raw "$APP_BUNDLE/Contents/Info.plist")"
APP_EXECUTABLE="$APP_BUNDLE/Contents/MacOS/$app_executable_name"
[[ -f "$APP_EXECUTABLE" ]] || fail "missing app executable inside app bundle: $APP_EXECUTABLE"
[[ -x "$APP_EXECUTABLE" ]] || fail "app executable inside app bundle is not executable: $APP_EXECUTABLE"
app_file_info="$(file "$APP_EXECUTABLE")"
printf '%s\n' "$app_file_info"
printf '%s\n' "$app_file_info" | grep -q "$expected_arch_pattern" \
  || fail "app executable architecture mismatch for $TARGET_TRIPLE"

step "Checking bundled sidecar inside app bundle"
APP_SIDE_CAR="$APP_BUNDLE/Contents/MacOS/codex-app-server"
[[ -f "$APP_SIDE_CAR" ]] || fail "missing bundled sidecar inside app bundle: $APP_SIDE_CAR"
[[ -x "$APP_SIDE_CAR" ]] || fail "bundled sidecar inside app bundle is not executable: $APP_SIDE_CAR"
sidecar_file_info="$(file "$APP_SIDE_CAR")"
printf '%s\n' "$sidecar_file_info"
printf '%s\n' "$sidecar_file_info" | grep -q "$expected_arch_pattern" \
  || fail "bundled sidecar architecture mismatch for $TARGET_TRIPLE"

step "Inspecting macOS app signature"
require_developer_id_signature "$APP_BUNDLE" "app bundle"

step "Checking app notarization ticket"
require_stapled_ticket "$APP_BUNDLE" "app bundle"

step "Inspecting macOS DMG signature"
require_developer_id_signature "$DMG" "dmg"

step "Checking DMG notarization ticket"
require_stapled_ticket "$DMG" "dmg"

step "Release check passed"
