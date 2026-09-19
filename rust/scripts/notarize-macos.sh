#!/usr/bin/env bash
# Developer ID sign (Hardened Runtime) + notarize + staple an existing lailaisay.app.
#
# On the maintainer Mac, after packaging:
#   ./scripts/package-macos-app.sh
#   ./scripts/notarize-macos.sh
#   ./scripts/notarize-macos.sh dist/lailaisay.app
#
# Does not change bundle id, settings paths, or app features. Requires network
# for the Apple timestamp server and notarytool submit. No secrets belong in
# the repo — pass credentials through the environment.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DEFAULT_IDENTITY="Developer ID Application: Henyi Lai (S6EDV86VSB)"
TEAM_ID="S6EDV86VSB"
ENTITLEMENTS="$ROOT/macos/lailaisay.entitlements"
DIST="$ROOT/dist"
DEFAULT_APP="$DIST/lailaisay.app"
NOTARIZED_ZIP="$DIST/lailaisay-notarized.zip"

APP=""
WRITE_ZIP=1

usage() {
  cat <<EOF
Usage: notarize-macos.sh [--no-zip-out] [path-to-lailaisay.app]

  Deep-sign an existing lailaisay.app with Developer ID Application, submit
  it to Apple notarization, staple the ticket, and optionally write
  dist/lailaisay-notarized.zip.

  Default app: dist/lailaisay.app
  --no-zip-out   Staple the .app only (skip dist/lailaisay-notarized.zip).

Signing (overridable):
  CODESIGN_IDENTITY   Default: ${DEFAULT_IDENTITY}
                      Team ID ${TEAM_ID}. Must be in the signing keychain.

Notarization — App Store Connect API key (do not commit the .p8):
  APP_STORE_CONNECT_API_KEY_PATH   Path to AuthKey_<KEY_ID>.p8
  APP_STORE_CONNECT_KEY_ID         Key ID
  APP_STORE_CONNECT_ISSUER_ID      Issuer UUID

  or a notarytool keychain profile created with
  \`xcrun notarytool store-credentials\`:
  NOTARYTOOL_PROFILE               Profile name (e.g. lailaisay-notary)

This script does not accept Apple ID passwords on the command line.

After a successful run, verify on the Mac:
  codesign --verify --deep --strict --verbose=2 dist/lailaisay.app
  xcrun stapler validate dist/lailaisay.app
  spctl --assess --type execute -vv dist/lailaisay.app
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-zip-out) WRITE_ZIP=0 ;;
    -h|--help) usage; exit 0 ;;
    --*)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ -n "$APP" ]]; then
        echo "unexpected extra argument: $1" >&2
        usage >&2
        exit 2
      fi
      APP="$1"
      ;;
  esac
  shift
done

if [[ -z "$APP" ]]; then
  APP="$DEFAULT_APP"
fi

# Allow a relative path from rust/ or from the caller cwd (already rust/).
if [[ "$APP" != /* ]]; then
  APP="$ROOT/$APP"
fi

fail() {
  echo "error: $*" >&2
  exit 1
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  fail "notarize-macos.sh must run on macOS (codesign, ditto, notarytool, stapler).
  On Linux, package with ./scripts/package-macos-app.sh --layout-only and
  notarize on the maintainer Mac."
fi

if [[ ! -d "$APP" || ! -f "$APP/Contents/Info.plist" ]]; then
  fail "expected an existing lailaisay.app at:
  $APP
  Build one first (no network required):
  ./scripts/package-macos-app.sh"
fi

if [[ ! -x "$APP/Contents/MacOS/lailaisay-app" ]]; then
  fail "missing executable: $APP/Contents/MacOS/lailaisay-app"
fi

if ! grep -q 'com.yikai.lailaisay' "$APP/Contents/Info.plist"; then
  fail "Info.plist must keep bundle id com.yikai.lailaisay (do not resign a different id)"
fi

if [[ ! -f "$ENTITLEMENTS" ]]; then
  fail "missing entitlements: $ENTITLEMENTS"
fi

if grep -A1 '<key>com.apple.security.app-sandbox</key>' "$ENTITLEMENTS" | grep -q '<true/>'; then
  fail "App Sandbox must not be enabled for Developer ID outside the Mac App Store.
  See comments in $ENTITLEMENTS"
fi

if grep -A1 '<key>com.apple.security.get-task-allow</key>' "$ENTITLEMENTS" | grep -q '<true/>'; then
  fail "get-task-allow is a debug entitlement and will fail notarization.
  Remove it from $ENTITLEMENTS"
fi

IDENTITY="${CODESIGN_IDENTITY:-$DEFAULT_IDENTITY}"
if [[ -z "$IDENTITY" ]]; then
  fail "CODESIGN_IDENTITY is empty.
  Set it to a Developer ID Application identity, e.g.
  CODESIGN_IDENTITY='${DEFAULT_IDENTITY}'"
fi

if ! command -v codesign >/dev/null 2>&1; then
  fail "codesign not found (install Xcode or Command Line Tools)"
fi
if ! command -v xcrun >/dev/null 2>&1; then
  fail "xcrun not found (install Xcode or Command Line Tools)"
fi
if ! command -v ditto >/dev/null 2>&1; then
  fail "ditto not found"
fi

if ! security find-identity -v -p codesigning 2>/dev/null | grep -F "$IDENTITY" | grep -q .; then
  fail "signing identity not found in the keychain:
  $IDENTITY
  Install the Developer ID Application certificate (Team ID ${TEAM_ID}),
  or set CODESIGN_IDENTITY to a listed identity from:
  security find-identity -v -p codesigning"
fi

NOTARY_ARGS=()
if [[ -n "${NOTARYTOOL_PROFILE:-}" ]]; then
  echo "Notarization credentials: notarytool keychain profile (NOTARYTOOL_PROFILE)."
  NOTARY_ARGS+=(--keychain-profile "$NOTARYTOOL_PROFILE")
elif [[ -n "${APP_STORE_CONNECT_API_KEY_PATH:-}" || -n "${APP_STORE_CONNECT_KEY_ID:-}" || -n "${APP_STORE_CONNECT_ISSUER_ID:-}" ]]; then
  missing=()
  [[ -z "${APP_STORE_CONNECT_API_KEY_PATH:-}" ]] && missing+=("APP_STORE_CONNECT_API_KEY_PATH")
  [[ -z "${APP_STORE_CONNECT_KEY_ID:-}" ]] && missing+=("APP_STORE_CONNECT_KEY_ID")
  [[ -z "${APP_STORE_CONNECT_ISSUER_ID:-}" ]] && missing+=("APP_STORE_CONNECT_ISSUER_ID")
  if [[ ${#missing[@]} -gt 0 ]]; then
    fail "incomplete App Store Connect API key env: missing ${missing[*]}.
  Set all three, or use NOTARYTOOL_PROFILE instead.
  Do not commit the .p8 or issuer secrets."
  fi
  if [[ ! -f "$APP_STORE_CONNECT_API_KEY_PATH" ]]; then
    fail "APP_STORE_CONNECT_API_KEY_PATH is not a file:
  $APP_STORE_CONNECT_API_KEY_PATH
  Do not commit .p8 keys; keep them outside the tree."
  fi
  echo "Notarization credentials: App Store Connect API key (path + key id + issuer id)."
  NOTARY_ARGS+=(
    --key "$APP_STORE_CONNECT_API_KEY_PATH"
    --key-id "$APP_STORE_CONNECT_KEY_ID"
    --issuer "$APP_STORE_CONNECT_ISSUER_ID"
  )
else
  fail "missing notarization credentials.
  Set either:
    NOTARYTOOL_PROFILE
  or all of:
    APP_STORE_CONNECT_API_KEY_PATH
    APP_STORE_CONNECT_KEY_ID
    APP_STORE_CONNECT_ISSUER_ID
  Do not commit .p8 files, passwords, or issuer secrets."
fi

mkdir -p "$DIST"

echo "== codesign (Developer ID, hardened runtime, timestamp) =="
echo "App: $APP"
echo "Identity: $IDENTITY"
echo "Entitlements: $ENTITLEMENTS"
codesign --force --deep --options runtime --timestamp \
  --entitlements "$ENTITLEMENTS" \
  --sign "$IDENTITY" \
  "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
echo "Developer ID codesign OK."

SUBMIT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lailaisay-notary.XXXXXX")"
cleanup() {
  rm -rf "$SUBMIT_DIR"
}
trap cleanup EXIT

SUBMIT_ZIP="$SUBMIT_DIR/lailaisay.app.zip"
echo "== zip for notarytool (ditto --keepParent) =="
ditto -c -k --keepParent "$APP" "$SUBMIT_ZIP"
echo "Wrote $SUBMIT_ZIP"

echo "== xcrun notarytool submit --wait =="
if ! xcrun notarytool submit "$SUBMIT_ZIP" --wait "${NOTARY_ARGS[@]}"; then
  fail "notarytool submit failed.
  Fetch the log with the submission id printed above, for example:
    xcrun notarytool log <submission-id> --keychain-profile \"\$NOTARYTOOL_PROFILE\"
  or the matching --key / --key-id / --issuer flags.
  Common causes: unsigned nested code, get-task-allow, or a stale signed ticket."
fi

echo "== staple =="
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
echo "Staple OK."

echo "== Gatekeeper assess =="
spctl --assess --type execute -vv "$APP"
echo "spctl assess OK."

if [[ "$WRITE_ZIP" -eq 1 ]]; then
  rm -f "$NOTARIZED_ZIP"
  ditto -c -k --keepParent "$APP" "$NOTARIZED_ZIP"
  echo "Wrote $NOTARIZED_ZIP"
fi

echo
echo "Stapled outside-App-Store build ready:"
echo "  $APP"
if [[ "$WRITE_ZIP" -eq 1 ]]; then
  echo "  $NOTARIZED_ZIP"
fi
echo "Bundle id is still com.yikai.lailaisay."
echo "Distribute the stapled .app (or the notarized zip). Grant TCC to"
echo "lailaisay.app on first launch, not to Terminal."
