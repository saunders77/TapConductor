#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-validate}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PRODUCT="TapConductor"
VERSION="$(node -p "require('${ROOT}/package.json').version")"

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command is unavailable: $1" >&2
    exit 1
  }
}

require_env() {
  [[ -n "${!1:-}" ]] || {
    echo "Required release input is unset: $1" >&2
    exit 1
  }
}

validate_apple_files() {
  require_command plutil
  plutil -lint \
    "${ROOT}/src-tauri/apple/Info.macos.plist" \
    "${ROOT}/src-tauri/apple/Info.ios.plist" \
    "${ROOT}/src-tauri/apple/PrivacyInfo.xcprivacy" \
    "${ROOT}/src-tauri/apple/MacAppStore.entitlements.template"

  node -e "
    for (const path of [
      'src-tauri/tauri.conf.json',
      'src-tauri/tauri.macos.conf.json',
      'src-tauri/tauri.ios.conf.json',
      'src-tauri/tauri.appstore.conf.json'
    ]) JSON.parse(require('fs').readFileSync(path, 'utf8'));
  "
}

quality_checks() {
  if [[ "${SKIP_QUALITY_CHECKS:-0}" == "1" ]]; then
    echo "Skipping duplicate quality checks for this packaging pass."
    return
  fi
  npm ci
  npm run build
  npm run test:auto-follow
  npm run test:beat
  cargo test --locked --workspace
  cargo fmt --all -- --check
  cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
}

initialize_ios() {
  if [[ ! -d "${ROOT}/src-tauri/gen/apple" ]]; then
    npm run tauri -- ios init --ci
  fi
}

stage_mac_artifacts() {
  local source_root="${ROOT}/target/universal-apple-darwin/release/bundle"
  local output="${ROOT}/target/apple-artifacts/${1}"
  mkdir -p "${output}"
  while IFS= read -r -d '' artifact; do
    ditto "${artifact}" "${output}/$(basename "${artifact}")"
  done < <(find "${source_root}" -maxdepth 2 \( -name "*.app" -o -name "*.dmg" -o -name "*.pkg" \) -print0)
  find "${output}" -type f -exec shasum -a 256 {} \; > "${output}/SHA256SUMS.txt"
  echo "Apple artifacts staged in ${output}"
}

cd "${ROOT}"
require_command node
require_command npm
require_command cargo

case "${MODE}" in
  validate)
    validate_apple_files
    cargo check --locked --workspace
    ;;

  mac-test)
    validate_apple_files
    quality_checks
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    APPLE_SIGNING_IDENTITY="-" npm run tauri -- build \
      --target universal-apple-darwin \
      --bundles app dmg \
      --ci
    stage_mac_artifacts "macos-ad-hoc"
    ;;

  mac-developer-id)
    require_env APPLE_SIGNING_IDENTITY
    if [[ -z "${APPLE_API_KEY:-}" ]]; then
      require_env APPLE_ID
      require_env APPLE_PASSWORD
      require_env APPLE_TEAM_ID
    else
      require_env APPLE_API_ISSUER
      require_env APPLE_API_KEY_PATH
    fi
    validate_apple_files
    quality_checks
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    npm run tauri -- build \
      --target universal-apple-darwin \
      --bundles app dmg \
      --ci
    stage_mac_artifacts "macos-developer-id-notarized"
    ;;

  mac-app-store)
    require_env APPLE_TEAM_ID
    require_env APPLE_MAS_APP_IDENTITY
    require_env APPLE_MAS_INSTALLER_IDENTITY
    require_env APPLE_MAS_PROVISIONING_PROFILE
    validate_apple_files
    quality_checks
    rustup target add aarch64-apple-darwin x86_64-apple-darwin

    config_dir="${ROOT}/target/apple-config"
    mkdir -p "${config_dir}"
    entitlements="${config_dir}/MacAppStore.entitlements"
    profile="${config_dir}/embedded.provisionprofile"
    sed "s/__APPLE_TEAM_ID__/${APPLE_TEAM_ID}/g" \
      "${ROOT}/src-tauri/apple/MacAppStore.entitlements.template" > "${entitlements}"
    cp "${APPLE_MAS_PROVISIONING_PROFILE}" "${profile}"
    runtime_config="{\"bundle\":{\"macOS\":{\"entitlements\":\"${entitlements}\",\"files\":{\"embedded.provisionprofile\":\"${profile}\"}}}}"

    APPLE_SIGNING_IDENTITY="${APPLE_MAS_APP_IDENTITY}" npm run tauri -- build \
      --no-bundle \
      --target universal-apple-darwin \
      --ci
    APPLE_SIGNING_IDENTITY="${APPLE_MAS_APP_IDENTITY}" npm run tauri -- bundle \
      --bundles app \
      --target universal-apple-darwin \
      --config src-tauri/tauri.appstore.conf.json \
      --config "${runtime_config}" \
      --ci

    app_path="${ROOT}/target/universal-apple-darwin/release/bundle/macos/${PRODUCT}.app"
    output="${ROOT}/target/apple-artifacts/macos-app-store"
    mkdir -p "${output}"
    xcrun productbuild \
      --sign "${APPLE_MAS_INSTALLER_IDENTITY}" \
      --component "${app_path}" /Applications \
      "${output}/${PRODUCT}_${VERSION}_universal.pkg"
    ditto "${app_path}" "${output}/${PRODUCT}.app"
    find "${output}" -type f -exec shasum -a 256 {} \; > "${output}/SHA256SUMS.txt"
    ;;

  ios-simulator)
    validate_apple_files
    quality_checks
    initialize_ios
    npm run tauri -- ios build \
      --target aarch64-sim \
      --debug \
      --ci
    echo "Simulator output: ${ROOT}/src-tauri/gen/apple/build"
    ;;

  ios-development)
    require_env APPLE_DEVELOPMENT_TEAM
    validate_apple_files
    quality_checks
    initialize_ios
    npm run tauri -- ios build \
      --target aarch64 \
      --export-method debugging \
      --ci
    echo "Development IPA output: ${ROOT}/src-tauri/gen/apple/build/arm64"
    ;;

  ios-app-store)
    require_env APPLE_DEVELOPMENT_TEAM
    validate_apple_files
    quality_checks
    initialize_ios
    npm run tauri -- ios build \
      --target aarch64 \
      --export-method app-store-connect \
      --ci
    echo "App Store IPA output: ${ROOT}/src-tauri/gen/apple/build/arm64/${PRODUCT}.ipa"
    ;;

  *)
    echo "Unknown mode '${MODE}'." >&2
    echo "Use: validate, mac-test, mac-developer-id, mac-app-store, ios-simulator, ios-development, or ios-app-store." >&2
    exit 2
    ;;
esac
