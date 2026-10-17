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

## Mac App Store

The App Store build is a separate, sandboxed variant (`cargo --features appstore`, Carbon hotkey, clipboard-only output). Packaging, certificates and upload are in [APP_STORE.md](APP_STORE.md). Everything below is the outside-App-Store product.

## Outside App Store: Developer ID + notarization

Distribute `lailaisay.app` outside the Mac App Store with a **Developer ID Application** signature and Apple notarization. Team ID `S6EDV86VSB`. Default identity: the G2 Sub-CA certificate, selected by SHA-1 `59AB8A1C4CFF401054F0D3CE0287208BB0032156` (the name alone is ambiguous while the old Sub-CA certificate is in the keychain).

This path enables the **Hardened Runtime** and does **not** enable App Sandbox. Sandbox would block the global CGEvent tap (Accessibility / Input Monitoring), paste into other apps (AX / CGEvent / System Events), and the existing Documents plus `~/Library/Application Support/com.yikai.lailaisay` paths. Entitlements live in [`macos/lailaisay.entitlements`](macos/lailaisay.entitlements) (audio-input and Apple Events only).

Do not commit `.p8` keys, passwords, or Issuer secrets.

### Package

```sh
cd rust
./scripts/package-macos-app.sh
# optional offline Developer ID sign (no notarytool, no timestamp server):
CODESIGN_IDENTITY="59AB8A1C4CFF401054F0D3CE0287208BB0032156" \
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

## 首次使用引導（手動，macOS 實機）

不要動到正在使用的設定：用環境變數隔離，並只在需要時才重置權限。

    export TOK_CONFIG=/tmp/lailaisay-onboarding/hex_settings.json   # 不存在 = 全新安裝
    export TOK_MODELS_DIR=/tmp/lailaisay-onboarding/models           # 空資料夾 = 沒有模型
    export TOK_MACWHISPER_MODELS_DIR=/tmp/lailaisay-onboarding/none  # 沒設就會掃到 MacWhisper 的模型而跳過模型步驟
    mkdir -p /tmp/lailaisay-onboarding/models /tmp/lailaisay-onboarding/none
    dist/lailaisay.app/Contents/MacOS/lailaisay-app

要連權限步驟一起測，另外執行 `tccutil reset Microphone|Accessibility|ListenEvent com.yikai.lailaisay`（這會撤銷已安裝 App 的授權，測完需重新授權）。

檢查項目（每項記錄實際結果）：
1. 啟動後自動出現「歡迎使用 lailaisay」，按「開始設定」進入模型步驟。
2. 模型步驟預設選中推薦模型；下載顯示狀態；完成後自動載入並進入權限步驟（權限已授權則直接進入試用）。
3. 權限步驟：麥克風出現系統彈窗；輔助使用與輸入監控按「開啟」跳到系統設定；授權後該列自動變綠。
4. 若授權後快捷鍵未啟用，出現「重新啟動 lailaisay」，按下後新實例啟動且精靈回到試用步驟。
5. 試用步驟：按住快捷鍵說一句話，下方出現文字，自動進入「設定完成」。
6. 按「開始使用」後視窗收到選單列；退出再開，不再出現精靈。
7. 中途按「稍後再說」或直接結束，再開：精靈再次出現；選單列提示「需要設定」。
8. 既有使用者（`hex_settings.json` 已存在、模型與權限正常）啟動時不出現精靈。

按鈕不必用手點：egui 透過 accesskit 把精靈的按鈕暴露成 `AXButton`，但 System Events 會依 bundle id 把同名 App 的實例當成同一個，兩個實例同時跑時拿到的是同一棵樹。要用 PID 範圍的 `AXUIElementCreateApplication` ＋ `AXUIElementPerformAction(kAXPressAction)` 才準（元件的文字在 `AXTitle`／`AXValue`，不在 `AXName`）。試用步驟按快捷鍵時文字會貼到前景 App，先把精靈視窗留在前景。

走查結果（2026-10-17，debug 版 `target/debug/lailaisay-app` 隔離環境；模型 Qwen3-ASR 1.7B Q4，Apple Silicon）：

1. 通過 — 空目錄啟動即建立設定檔並寫入 `onboardingPending: true`，視窗自動出現「歡迎使用 lailaisay」，「開始設定」進入模型步驟。
2. 通過 — 預設選中推薦模型（顯示約 1.3 GB、未下載）；按下載後按鈕變「下載中…」並顯示下載訊息，1.32 GB 約 42 秒完成，完成後自動載入模型（16.7 秒，Metal），因權限已授權直接進試用。
3. 未驗證 — debug 與隔離實例都沿用終端機已授權的 TCC，走不到這一屏；要看到它得 `tccutil reset`，會撤銷已安裝版授權。試過 `launchctl submit` 與自建 `.app`（新 bundle id、ad-hoc 簽章）取得乾淨身分，兩者都開不出視窗。
4. 未驗證 — 需要第 3 項的缺權限狀態才會出現。
5. 通過 — 用系統 TTS 經喇叭、麥克風收音代替人聲：音量太小（peak 0.06／rms 0.0117）得到空字串且停在試用（沒誤判通過），放大約 8 dB 後轉錄出「台南吃牛肉湯，我想去台南吃牛肉湯。」，自動進「設定完成」，設定檔變成 `onboardingPending: false`、`hasCompletedOnboarding: true`。
6. 通過 —「開始使用」後視窗收進選單列；重啟（pending=false）不再出現精靈，直接待在選單列。
7. 通過 — pending=true 的設定檔啟動會再出現精靈；「稍後再說」收起視窗，選單列標題變「lailaisay — idle（需要設定，請開啟設定繼續）」，再由選單列「開啟設定」進到一般設定各頁，`onboardingPending` 仍為 true。
8. 通過 — 與第 6 項同一個 `needs_wizard` 判定：設定檔存在、模型與權限正常時不開精靈。

## Data and diagnostics

The bundle identifier is `com.yikai.lailaisay`. macOS permissions must be granted again for this new identity. Existing settings remain at `~/Documents/hex_settings.json`; model files now use `~/Library/Application Support/com.yikai.lailaisay/models/`. When that cache is empty, the app copies existing weights from the legacy `xyz.2qs.Tok` cache and remaps saved model selections when the destination file exists. The settings schema and `TOK_*` overrides are described in [README.md](README.md).

Run `cargo run -p lailaisay-app -- --once` for the fixture pipeline without requesting permissions. For live startup logs, run `dist/lailaisay.app/Contents/MacOS/lailaisay-app` from a terminal; permissions can differ from launching the packaged application.

A successful command-line test does not prove microphone, hotkey or paste permissions for the application bundle. Check these through the packaged GUI before distribution.

## Automated macOS validation

The `macos` CI job runs natively on the Apple Silicon `macos-15` runner. It treats Rust warnings as errors, tests the macOS app/input/paste crates with microphone and Whisper enabled, builds the release executable, packages the real app, verifies its arm64 architecture and ad-hoc signature, and runs the packaged fixture pipeline without microphone/hotkey/paste permissions. A successful job uploads the ZIP for 14 days. This does not replace live GUI/permission testing or Developer ID signing and notarization.
