# lailaisay on macOS

The application is built from the Rust workspace. Install Rust and the Apple command-line SDK tools for native linking.

## Build and package

Run from `rust/`:

```sh
./scripts/mac-smoke.sh
./scripts/package-macos-app.sh
open dist/lailaisay.app
```

The bundle contains `Contents/MacOS/lailaisay-app`, the application icon and font license. The package script uses ad-hoc signing for local tests; this is not Developer ID signing or notarization. For an outside-App-Store build, see [Developer ID + notarization](#outside-app-store-developer-id--notarization).

macOS packaging is Apple Silicon (arm64) only. Intel Mac / Rosetta / `x86_64-apple-darwin` is not a supported product path.

`--layout-only` creates a non-runnable stub for packaging validation. `--skip-build` reuses an existing release binary. `TOK_APP_BIN` selects a specific prebuilt binary and `TOK_CARGO_TARGET` selects a target triple (Intel Mac triples are rejected). `--zip` creates `dist/lailaisay.app.zip`. `--developer-id` signs with `CODESIGN_IDENTITY` (Hardened Runtime + `macos/lailaisay.entitlements`) and does not notarize.

## Outside App Store: Developer ID + notarization

Distribute `lailaisay.app` outside the Mac App Store with a **Developer ID Application** signature and Apple notarization. Team ID `S6EDV86VSB`. Default identity: `Developer ID Application: Henyi Lai (S6EDV86VSB)`.

This path enables the **Hardened Runtime** and does **not** enable App Sandbox. Sandbox would block the global CGEvent tap (Accessibility / Input Monitoring), paste into other apps (AX / CGEvent / System Events), and the existing Documents plus `~/Library/Application Support/com.yikai.lailaisay` paths. Entitlements live in [`macos/lailaisay.entitlements`](macos/lailaisay.entitlements) (audio-input and Apple Events only).

Do not commit `.p8` keys, passwords, or Issuer secrets.

### Package

```sh
cd rust
./scripts/package-macos-app.sh
# optional offline Developer ID sign (no notarytool, no timestamp server):
CODESIGN_IDENTITY="Developer ID Application: Henyi Lai (S6EDV86VSB)" \
  ./scripts/package-macos-app.sh --developer-id
```

### Notarize

On the maintainer Mac, pass an App Store Connect API key **or** a `notarytool` keychain profile:

```sh
export APP_STORE_CONNECT_API_KEY_PATH=/path/to/AuthKey_XXXXXXXXXX.p8
export APP_STORE_CONNECT_KEY_ID=XXXXXXXXXX
export APP_STORE_CONNECT_ISSUER_ID=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
# or: export NOTARYTOOL_PROFILE=lailaisay-notary

./scripts/notarize-macos.sh
# or: ./scripts/notarize-macos.sh dist/lailaisay.app
```

`notarize-macos.sh` deep-signs with `--options runtime --timestamp` and the entitlements file, zips with `ditto -c -k --keepParent`, runs `xcrun notarytool submit … --wait`, staples the ticket, and writes `dist/lailaisay-notarized.zip`. Override the identity with `CODESIGN_IDENTITY`. The script fails clearly if the identity or credentials are missing.

### Verify

```sh
codesign --verify --deep --strict --verbose=2 dist/lailaisay.app
xcrun stapler validate dist/lailaisay.app
spctl --assess --type execute -vv dist/lailaisay.app
```

Gatekeeper should accept the stapled app as notarized. Then smoke-test that bundle as in [First launch](#first-launch) — grant Microphone and Accessibility to `lailaisay.app`, not Terminal.

## First launch

1. Quit any running earlier copy from its menu-bar menu.
2. Open the packaged application. The first launch opens Settings so you can grant permissions and pick a model; the menu-bar icon stays available. Later launches stay in the menu bar when **啟動時縮到選單列** is on (the default). Tray **開啟設定** opens Settings; `--settings` forces it open.
3. Enable Microphone and Accessibility for the application when required. If the hook cannot start, also check Input Monitoring and relaunch after changing permissions.
4. In the model settings, download or select a usable ggml model and save. Wait for the loaded-model status.
5. Hold the configured hotkey, speak, and release. The default macOS chord is Command+Shift+Space; saved settings may use a different chord.
6. Confirm text is pasted into the application that was active when recording began. Enable optional AI polish separately if desired.

## Release smoke checklist

- Settings title and tray menus say `lailaisay`; Chinese glyphs render correctly.
- Settings controls save and reload; the selected model and inference status are correct.
- Microphone input produces Chinese text, spoken numbers, punctuation and expected dictionary replacements.
- Double-tap lock and release work without duplicate starts or empty-text paste.
- Clicking another application lets that application come forward; settings do not remain above it.
- Closing **or minimizing** Settings hides to the tray (`settings_visible` false). Switching to LINE / Safari / any other app must not restore Settings. Tray **開啟設定** (or Dock / explicit restore) brings it back once.
- Hold the hotkey while another app (Safari) is frontmost: a floating 「錄音中」 pill appears above other windows and does not steal focus. Release: 「處理中」, then the lamp hides after paste/idle.
- Restart preserves settings and models; run these checks on the Apple Silicon release bundle.
- Enable **顯示 Dock 圖示**, quit from the tray, and relaunch. Dock must show the dark rounded square with five gold waveform bars (the AppIcon motif), not a generic letter monogram “e”. Disable the switch and relaunch: menu-bar / accessory only, no Dock tile.

## Data and diagnostics

The bundle identifier is `com.yikai.lailaisay`. macOS permissions must be granted again for this new identity. Existing settings remain at `~/Documents/hex_settings.json`; model files now use `~/Library/Application Support/com.yikai.lailaisay/models/`. When that cache is empty, the app copies existing weights from the legacy `xyz.2qs.Tok` cache and remaps saved model selections when the destination file exists. The settings schema and `TOK_*` overrides are described in [README.md](README.md).

Run `cargo run -p lailaisay-app -- --once` for the fixture pipeline without requesting permissions. For live startup logs, run `dist/lailaisay.app/Contents/MacOS/lailaisay-app` from a terminal; permissions can differ from launching the packaged application.

A successful command-line test does not prove microphone, hotkey or paste permissions for the application bundle. Check these through the packaged GUI before distribution.

## Automated macOS validation

The `macos` CI job runs natively on the Apple Silicon `macos-15` runner. It treats Rust warnings as errors, tests the macOS app/input/paste crates with microphone and Whisper enabled, builds the release executable, packages the real app, verifies its arm64 architecture and ad-hoc signature, and runs the packaged fixture pipeline without microphone/hotkey/paste permissions. A successful job uploads the ZIP for 14 days. This does not replace live GUI/permission testing or Developer ID signing and notarization.
