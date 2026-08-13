#!/usr/bin/env bash
# Copyright (c) 2026 Michael Saunders
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="all"
FAST=0

usage() {
  cat <<'EOF'
Usage: bash tools/rebuild-apple.sh [all|mac|ios|iphone|ipad] [--fast]

  all      Run quality checks once, then build macOS and the iPhone/iPad Simulator app.
  mac      Build the universal macOS app and DMG.
  ios      Build the arm64 iPhone/iPad Simulator app.
  iphone   Alias for ios.
  ipad     Backward-compatible alias for ios.
  --fast   Skip tests, formatting, and Clippy for this packaging pass.

With no arguments, the script builds both platforms and runs all checks.
EOF
}

for argument in "$@"; do
  case "${argument}" in
    all|mac|ios)
      TARGET="${argument}"
      ;;
    iphone|ipad)
      TARGET="ios"
      ;;
    --fast)
      FAST=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: ${argument}" >&2
      usage >&2
      exit 2
      ;;
  esac
done

# rustup normally installs here even when a shell profile has not yet been
# reloaded.
if [[ -d "${HOME}/.cargo/bin" ]]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi

require_command() {
  local command_name="$1"
  local installation_hint="$2"
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "Required command is unavailable: ${command_name}" >&2
    echo "${installation_hint}" >&2
    exit 1
  fi
}

require_command node "Install Node.js 22 LTS, then open a new Terminal window."
require_command npm "Install Node.js 22 LTS, which includes npm."
require_command cargo "Install Rust from https://rustup.rs, then open a new Terminal window."
require_command rustup "Install Rust from https://rustup.rs, then open a new Terminal window."
require_command xcodebuild "Install Xcode and its command-line tools."
require_command xcrun "Install Xcode and its command-line tools."
require_command plutil "plutil is supplied by macOS."
VERSION="$(node -p "require('${ROOT}/package.json').version")"

if [[ ("${TARGET}" == "all" || "${TARGET}" == "ios") &&
      ! -d "${ROOT}/src-tauri/gen/apple" ]]; then
  require_command xcodegen "Install the first-run iOS helper with: brew install xcodegen"
  require_command pod "Install the first-run iOS helper with: brew install cocoapods"
  require_command idevicesyslog \
    "Install the first-run iOS helper with: brew install libimobiledevice"
fi

cd "${ROOT}"

build_mac() {
  if [[ "${FAST}" -eq 1 ]]; then
    SKIP_QUALITY_CHECKS=1 bash tools/apple-release.sh mac-test
  else
    bash tools/apple-release.sh mac-test
  fi
}

build_ios() {
  if [[ "${FAST}" -eq 1 ]]; then
    SKIP_QUALITY_CHECKS=1 bash tools/apple-release.sh ios-simulator
  else
    bash tools/apple-release.sh ios-simulator
  fi
}

case "${TARGET}" in
  mac)
    build_mac
    ;;
  ios)
    build_ios
    ;;
  all)
    build_mac
    # The macOS pass already ran the shared quality checks.
    SKIP_QUALITY_CHECKS=1 bash tools/apple-release.sh ios-simulator
    ;;
esac

echo
echo "Apple rebuild complete."
if [[ "${TARGET}" == "all" || "${TARGET}" == "mac" ]]; then
  echo "macOS DMG: ${ROOT}/target/apple-artifacts/macos-ad-hoc/TapConductor_${VERSION}_universal.dmg"
fi
if [[ "${TARGET}" == "all" || "${TARGET}" == "ios" ]]; then
  echo "iPhone/iPad Simulator app: ${ROOT}/src-tauri/gen/apple/build/arm64-sim/TapConductor.app"
fi
