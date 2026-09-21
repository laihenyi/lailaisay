#!/usr/bin/env bash
# Build rust/dist/lailaisay.app (bundle id com.yikai.lailaisay) from the release lailaisay-app binary.
#
# On a Mac (Apple Silicon):
#   ./scripts/package-macos-app.sh
#   CODESIGN_IDENTITY='Developer ID Application: Henyi Lai (S6EDV86VSB)' \
#     ./scripts/package-macos-app.sh --developer-id
#   open dist/lailaisay.app
#
# --developer-id signs locally (Hardened Runtime + entitlements). It does not
# notarize and does not require Apple's notary service. Follow with:
#   ./scripts/notarize-macos.sh
#
# --app-store builds the sandboxed Mac App Store variant (cargo feature
# `appstore`), embeds macos/lailaisay-appstore.provisionprofile, signs with the
# Mac App Distribution identity and writes dist/lailaisay.pkg for upload
# (fastlane mac beta / release). See APP_STORE.md.
#
# On Linux CI this script can validate the layout without linking whisper/mic:
#   ./scripts/package-macos-app.sh --layout-only
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BUNDLE_ID="com.yikai.lailaisay"
INTEL_MAC_TARGET="x86_64-apple-darwin"

LAYOUT_ONLY=0
SKIP_BUILD=0
ZIP=0
DEVELOPER_ID=0
APP_STORE=0
FEATURES="${TOK_FEATURES:-}"
CARGO_TARGET="${TOK_CARGO_TARGET:-}"
DEFAULT_IDENTITY="Developer ID Application: Henyi Lai (S6EDV86VSB)"
APPSTORE_APP_IDENTITY="${APPSTORE_APP_IDENTITY:-3rd Party Mac Developer Application: Henyi Lai (S6EDV86VSB)}"
APPSTORE_INSTALLER_IDENTITY="${APPSTORE_INSTALLER_IDENTITY:-3rd Party Mac Developer Installer: Henyi Lai (S6EDV86VSB)}"

reject_intel_mac_target() {
  local triple="${1:-}"
  if [[ "$triple" == "$INTEL_MAC_TARGET" || "$triple" == i686-apple-darwin ]]; then
    echo "Intel Mac ($triple) is not a supported lailaisay.app product path." >&2
    echo "Package on Apple Silicon (host or --target aarch64-apple-darwin)." >&2
    exit 2
  fi
}

usage() {
  cat <<'EOF'
Usage: package-macos-app.sh [--layout-only] [--skip-build] [--zip]
                            [--target TRIPLE] [--developer-id | --app-store]

  --layout-only   Assemble lailaisay.app with a stub executable (Linux CI / plist check).
  --skip-build    Reuse an existing lailaisay-app (host or --target dir, or TOK_APP_BIN).
  --zip           Also write dist/lailaisay.app.zip (macOS ditto, else zip).
  --target TRIPLE cargo --target (one product: still dist/lailaisay.app).
                  Intel Mac triples (x86_64-apple-darwin) are rejected.
  --developer-id  Sign with CODESIGN_IDENTITY (Developer ID + Hardened Runtime
                  + macos/lailaisay.entitlements). Does not notarize and does
                  not contact notarytool. Follow with ./scripts/notarize-macos.sh.
  --app-store     Mac App Store variant: cargo feature `appstore`, App Sandbox
                  entitlements (macos/lailaisay-appstore.entitlements), embedded
                  macos/lailaisay-appstore.provisionprofile, signed with the
                  "3rd Party Mac Developer Application" identity, then
                  productbuild → dist/lailaisay.pkg signed with the
                  "3rd Party Mac Developer Installer" identity.

macOS packaging is Apple Silicon only. Intel Mac / Rosetta is not supported.

Environment:
  TOK_FEATURES        Cargo features (default: mic,whisper; with --app-store
                      mic,whisper,appstore). Ignored with --layout-only.
  TOK_APP_BIN         Path to an already-built lailaisay-app binary.
  TOK_CARGO_TARGET    Default cargo target if --target is omitted.
  CODESIGN_IDENTITY   Required with --developer-id (Developer ID Application identity).
  APPSTORE_APP_IDENTITY        --app-store app signing identity (default: 3rd Party Mac Developer Application).
  APPSTORE_INSTALLER_IDENTITY  --app-store pkg signing identity (default: 3rd Party Mac Developer Installer).
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --layout-only) LAYOUT_ONLY=1 ;;
    --skip-build) SKIP_BUILD=1 ;;
    --zip) ZIP=1 ;;
    --intel)
      echo "--intel is no longer supported (Intel Mac packaging was removed)." >&2
      usage >&2
      exit 2
      ;;
    --developer-id) DEVELOPER_ID=1 ;;
    --app-store) APP_STORE=1 ;;
    --target)
      if [[ $# -lt 2 ]]; then
        echo "--target needs a rustc triple (e.g. aarch64-apple-darwin)" >&2
        exit 2
      fi
      CARGO_TARGET="$2"
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

reject_intel_mac_target "$CARGO_TARGET"

if [[ "$DEVELOPER_ID" -eq 1 && "$APP_STORE" -eq 1 ]]; then
  echo "--developer-id and --app-store are different products; pick one." >&2
  exit 2
fi
if [[ -z "$FEATURES" ]]; then
  if [[ "$APP_STORE" -eq 1 ]]; then
    FEATURES="mic,whisper,appstore"
  else
    FEATURES="mic,whisper"
  fi
fi

DIST="$ROOT/dist"
APP="$DIST/lailaisay.app"
MACOS="$APP/Contents/MacOS"
RES="$APP/Contents/Resources"
PLIST_SRC="$ROOT/macos/Info.plist"
ENTITLEMENTS="$ROOT/macos/lailaisay.entitlements"
APPSTORE_ENTITLEMENTS="$ROOT/macos/lailaisay-appstore.entitlements"
APPSTORE_PROFILE="$ROOT/macos/lailaisay-appstore.provisionprofile"
PKG="$DIST/lailaisay.pkg"

if [[ "$APP_STORE" -eq 1 ]]; then
  if [[ ! -f "$APPSTORE_ENTITLEMENTS" ]]; then
    echo "missing $APPSTORE_ENTITLEMENTS" >&2
    exit 1
  fi
  if ! grep -A1 '<key>com.apple.security.app-sandbox</key>' "$APPSTORE_ENTITLEMENTS" | grep -q '<true/>'; then
    echo "App Store entitlements must enable com.apple.security.app-sandbox." >&2
    exit 1
  fi
  if grep -q 'com.apple.security.automation.apple-events' "$APPSTORE_ENTITLEMENTS"; then
    echo "App Store entitlements must not request Apple Events automation (sandbox build is clipboard-only)." >&2
    exit 1
  fi
  if [[ "$LAYOUT_ONLY" -eq 0 ]]; then
    if [[ "$(uname -s)" != "Darwin" ]]; then
      echo "--app-store requires macOS (codesign + productbuild)." >&2
      exit 1
    fi
    if [[ ! -f "$APPSTORE_PROFILE" ]]; then
      echo "missing $APPSTORE_PROFILE (Mac App Store provisioning profile for $BUNDLE_ID; see APP_STORE.md)" >&2
      exit 1
    fi
    for identity in "$APPSTORE_APP_IDENTITY" "$APPSTORE_INSTALLER_IDENTITY"; do
      if ! security find-identity -v 2>/dev/null | grep -F "$identity" | grep -q .; then
        echo "signing identity not found in the keychain: $identity" >&2
        echo "Install the certificate (APP_STORE.md §1) or override APPSTORE_APP_IDENTITY / APPSTORE_INSTALLER_IDENTITY." >&2
        exit 1
      fi
    done
    if ! command -v productbuild >/dev/null 2>&1; then
      echo "productbuild not found (install Xcode)" >&2
      exit 1
    fi
  fi
fi

if [[ "$DEVELOPER_ID" -eq 1 ]]; then
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "--developer-id requires macOS (codesign + Developer ID keychain)." >&2
    echo "On Linux use --layout-only, then notarize on the maintainer Mac." >&2
    exit 1
  fi
  if [[ -z "${CODESIGN_IDENTITY:-}" ]]; then
    echo "--developer-id requires CODESIGN_IDENTITY." >&2
    echo "  CODESIGN_IDENTITY='$DEFAULT_IDENTITY' $0 --developer-id" >&2
    exit 1
  fi
  if [[ ! -f "$ENTITLEMENTS" ]]; then
    echo "missing $ENTITLEMENTS" >&2
    exit 1
  fi
  if grep -A1 '<key>com.apple.security.app-sandbox</key>' "$ENTITLEMENTS" | grep -q '<true/>'; then
    echo "App Sandbox must not be enabled for Developer ID outside the Mac App Store." >&2
    exit 1
  fi
  if ! command -v codesign >/dev/null 2>&1; then
    echo "codesign not found (install Xcode or Command Line Tools)" >&2
    exit 1
  fi
  if ! security find-identity -v -p codesigning 2>/dev/null | grep -F "$CODESIGN_IDENTITY" | grep -q .; then
    echo "signing identity not found in the keychain: $CODESIGN_IDENTITY" >&2
    echo "Install the Developer ID Application certificate, or set CODESIGN_IDENTITY." >&2
    exit 1
  fi
fi

if [[ ! -f "$PLIST_SRC" ]]; then
  echo "missing $PLIST_SRC" >&2
  exit 1
fi

if ! grep -q "$BUNDLE_ID" "$PLIST_SRC"; then
  echo "Info.plist must set CFBundleIdentifier $BUNDLE_ID" >&2
  exit 1
fi
if grep -A1 '<key>CFBundleIdentifier</key>' "$PLIST_SRC" | grep -q 'xyz.2qs.Tok'; then
  echo "Info.plist must not use xyz.2qs.Tok as the live bundle id" >&2
  exit 1
fi
if ! grep -q 'LSUIElement' "$PLIST_SRC"; then
  echo "Info.plist must set LSUIElement (menu-bar accessory)" >&2
  exit 1
fi
if ! grep -q 'CFBundleIconFile' "$PLIST_SRC"; then
  echo "Info.plist must set CFBundleIconFile to AppIcon.icns" >&2
  exit 1
fi
# CFBundleIconName is an asset-catalog name. Without Assets.car, Dock/Launch
# Services skip the icns and synthesize a bundle-id monogram (the “e” in ekai).
if grep -q 'CFBundleIconName' "$PLIST_SRC" && [[ ! -f "$ROOT/macos/Assets.car" ]]; then
  echo "Info.plist must not set CFBundleIconName unless macos/Assets.car exists" >&2
  exit 1
fi
if ! grep -q 'NSMicrophoneUsageDescription' "$PLIST_SRC"; then
  echo "Info.plist must include NSMicrophoneUsageDescription" >&2
  exit 1
fi

rm -rf "$APP"
mkdir -p "$MACOS" "$RES"
cp "$PLIST_SRC" "$APP/Contents/Info.plist"
cp "$ROOT/crates/lailaisay-app/assets/fonts/OFL.txt" "$RES/FONT-LICENSE.txt"
printf 'APPL????' > "$APP/Contents/PkgInfo"

ICON_SRC="$ROOT/macos/AppIcon.icns"
if [[ -f "$ICON_SRC" ]]; then
  cp "$ICON_SRC" "$RES/AppIcon.icns"
else
  echo "missing $ICON_SRC" >&2
  exit 1
fi
if grep -q 'CFBundleIconName' "$APP/Contents/Info.plist" && [[ ! -f "$RES/Assets.car" ]]; then
  echo "packaged Info.plist has CFBundleIconName but no Assets.car" >&2
  exit 1
fi

BIN=""
if [[ "$LAYOUT_ONLY" -eq 1 ]]; then
  BIN="$MACOS/lailaisay-app"
  cat > "$BIN" <<'STUB'
#!/bin/sh
echo "lailaisay.app layout stub (not a real lailaisay-app). Build on macOS without --layout-only." >&2
exit 1
STUB
  chmod +x "$BIN"
  echo "Laid out $APP (stub executable; Linux CI / plist check)."
else
  RELEASE_DIR="$ROOT/target/release"
  if [[ -n "$CARGO_TARGET" ]]; then
    RELEASE_DIR="$ROOT/target/$CARGO_TARGET/release"
  fi
  if [[ -n "${TOK_APP_BIN:-}" ]]; then
    BIN="$TOK_APP_BIN"
  elif [[ "$SKIP_BUILD" -eq 1 && -x "$RELEASE_DIR/lailaisay-app" ]]; then
    BIN="$RELEASE_DIR/lailaisay-app"
  else
    if [[ "$(uname -s)" != "Darwin" ]]; then
      echo "Full lailaisay.app needs macOS (whisper.cpp + mic). On Linux use --layout-only." >&2
      echo "  $0 --layout-only" >&2
      exit 1
    fi
    BUILD=(cargo build --release -p lailaisay-app --features "$FEATURES")
    if [[ -n "$CARGO_TARGET" ]]; then
      if command -v rustup >/dev/null 2>&1; then
        rustup target add "$CARGO_TARGET"
      fi
      BUILD+=(--target "$CARGO_TARGET")
    fi
    echo "${BUILD[*]}"
    "${BUILD[@]}"
    BIN="$RELEASE_DIR/lailaisay-app"
  fi
  if [[ ! -x "$BIN" ]]; then
    echo "lailaisay-app binary not executable: $BIN" >&2
    exit 1
  fi
  cp "$BIN" "$MACOS/lailaisay-app"
  chmod +x "$MACOS/lailaisay-app"
  echo "Embedded $(basename "$BIN") → $MACOS/lailaisay-app"
  if command -v file >/dev/null 2>&1; then
    file "$MACOS/lailaisay-app" || true
  fi
  if [[ -n "$CARGO_TARGET" ]]; then
    echo "cargo target: $CARGO_TARGET"
  fi
fi

if [[ "$APP_STORE" -eq 1 ]]; then
  if [[ -f "$APPSTORE_PROFILE" ]]; then
    cp "$APPSTORE_PROFILE" "$APP/Contents/embedded.provisionprofile"
    echo "Embedded provisioning profile → Contents/embedded.provisionprofile"
  fi
  if [[ "$LAYOUT_ONLY" -eq 1 ]]; then
    echo "App Store layout OK (stub executable; not signed, no pkg)."
  else
    codesign --force --deep --timestamp \
      --entitlements "$APPSTORE_ENTITLEMENTS" \
      --sign "$APPSTORE_APP_IDENTITY" \
      "$APP"
    codesign --verify --deep --strict --verbose=2 "$APP"
    if ! codesign -d --entitlements :- "$APP" 2>/dev/null | grep -q 'com.apple.security.app-sandbox'; then
      echo "signed app is missing the App Sandbox entitlement" >&2
      exit 1
    fi
    echo "App Store codesign OK ($APPSTORE_APP_IDENTITY)."
    rm -f "$PKG"
    productbuild --component "$APP" /Applications \
      --sign "$APPSTORE_INSTALLER_IDENTITY" \
      "$PKG"
    echo "Wrote $PKG (upload with: fastlane mac beta | fastlane mac release)"
  fi
elif [[ "$DEVELOPER_ID" -eq 1 ]]; then
  # Preflight already checked Darwin, CODESIGN_IDENTITY, entitlements, and the
  # keychain. No --timestamp / notarytool here so packaging stays offline.
  codesign --force --deep --options runtime \
    --entitlements "$ENTITLEMENTS" \
    --sign "$CODESIGN_IDENTITY" \
    "$APP"
  echo "Developer ID codesign OK ($CODESIGN_IDENTITY). Notarize next: ./scripts/notarize-macos.sh"
elif command -v codesign >/dev/null 2>&1; then
  # Ad-hoc sign so Gatekeeper can at least attach TCC to this bundle.
  # Developer ID + notarization are a follow-up (see MAC_SMOKE.md).
  if codesign --force --deep --sign - "$APP"; then
    echo "Ad-hoc codesign OK (−)."
  else
    echo "warning: ad-hoc codesign failed (continuing)" >&2
  fi
fi

if [[ "$ZIP" -eq 1 ]]; then
  ZIP_PATH="$DIST/lailaisay.app.zip"
  rm -f "$ZIP_PATH"
  if command -v ditto >/dev/null 2>&1; then
    ditto -c -k --keepParent "$APP" "$ZIP_PATH"
  else
    (cd "$DIST" && zip -qr "lailaisay.app.zip" "lailaisay.app")
  fi
  echo "Wrote $ZIP_PATH"
fi

echo
echo "lailaisay.app: $APP"
echo "Bundle id: $BUNDLE_ID"
if [[ -n "$CARGO_TARGET" ]]; then
  echo "Arch / cargo target: $CARGO_TARGET"
else
  echo "Arch: host ($(uname -m))"
fi
echo "Models dir on Mac: ~/Library/Application Support/${BUNDLE_ID}/models/"
echo "Grant TCC to lailaisay.app (not Terminal) after the first launch."
echo "If that models folder is empty, lailaisay copies model weights from the previous bundle cache once."
