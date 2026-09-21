# lailaisay Mac App Store 上架準備

> 建立日期：2026-09-21。憑證與流程改寫自 `PilotApp_Flutter_Standalone/docs/ios_release_guide.md`、`docs/ios_certificate_info.md` 與 `ios/fastlane/Fastfile`。密碼、私鑰內容一律不寫進本 repo。

## 0. App Store 建置變體（cargo feature `appstore`）

Mac App Store 審核強制要求 App Sandbox。Developer ID 版（`macos/lailaisay.entitlements`）刻意不開沙盒；App Store 版改用 `--features appstore`，差異如下：

| 功能 | Developer ID 版 | App Store 版（`appstore`） |
| --- | --- | --- |
| 全域按住說話熱鍵 | `CGEventTap`（需輔助使用／輸入監控） | Carbon `RegisterEventHotKey`（`lailaisay-input/src/carbon.rs`），不需任何權限；熱鍵必須是「修飾鍵 + 一個按鍵」，不支援 fn 單鍵或純修飾鍵；Esc 無法取消 |
| 輸出到前景 App | AX insert / CGEvent ⌘V / System Events | 只寫剪貼簿，狀態顯示「已複製」，使用者自行 ⌘V |
| Speak-to-Edit | AX 讀取選取文字 | 無法讀取選取文字，功能實際停用（狀態「no selection」） |
| 前景 App 辨識 | System Events | `NSWorkspace`（沙盒可用） |
| 設定與模型路徑 | `~/Documents/hex_*.json`、`~/Library/Application Support/com.yikai.lailaisay/models` | 同樣的相對路徑，但落在沙盒容器 `~/Library/Containers/com.yikai.lailaisay/Data/…`；不會匯入舊資料 |
| 權限頁 | 輔助使用／麥克風／輸入監控／自動化 | 只顯示麥克風 |
| entitlements | `macos/lailaisay.entitlements` | `macos/lailaisay-appstore.entitlements`（sandbox + audio-input + network.client + files.user-selected.read-only） |

已驗證（2026-09-21，本機 Apple Silicon）：`--app-store` 打包成功、`codesign` 含 sandbox entitlement、`pkgutil --check-signature` 通過；以 `open dist/lailaisay.app` 啟動後程序存活、`hex_settings.json` 寫入容器 `Data/Documents`、stderr 出現「Carbon hotkey registered (⌘⇧SPACE)」。預設字典改為編譯進二進位（原本從 repo 路徑讀取，沙盒下會 EPERM）。

尚待實機驗證：按住 ⌘⇧Space 說話放開後是否出現「已複製」並可 ⌘V、模型下載是否落在容器內、`rfd` 開檔面板匯入字典。注意：從終端機直接執行沙盒版二進位不會重設 `HOME`，會因存取 `~/Documents` 失敗而退出；請用 `open dist/lailaisay.app` 測試。`--once` 需讀 repo 內的 fixture，沙盒版不適用。

## 1. 憑證與 Developer Portal 狀態（2026-09-21）

| 項目 | 狀態 |
| --- | --- |
| `3rd Party Mac Developer Application: Henyi Lai (S6EDV86VSB)`（App Store Connect 憑證 `779932RTZS`，效期至 2027/07/21） | ✅ 鑰匙圈已有私鑰，簽署 `.app` |
| `3rd Party Mac Developer Installer: Henyi Lai (S6EDV86VSB)`（憑證 `93BXZ5BTR6`，效期至 2027/09/21） | ✅ 2026-09-21 以 API 申請並匯入鑰匙圈，簽署 `.pkg` |
| `Apple Distribution`（效期至 2026/12/08） | ✅ 備用 |
| Bundle ID `com.yikai.lailaisay`（Developer Portal 資源 `2F889GYMND`，platform UNIVERSAL） | ✅ 2026-09-21 註冊 |
| Provisioning Profile「lailaisay Mac App Store」（`KBMFVU6T89`，MAC_APP_STORE，效期至 2027/07/21） | ✅ 已下載為 `macos/lailaisay-appstore.provisionprofile`（可提交，非機密） |
| App Store Connect API Key | ✅ Key ID `7AXGRN3B24`、Issuer `69a6de7f-8267-47e3-e053-5b8c7c11a4d1`，私鑰 `AuthKey_7AXGRN3B24.p8` 放在 repo 根目錄（`.gitignore` 與 CI 都會擋下） |
| App Store Connect 的 App 紀錄 | ❌ **尚未建立**（API 不支援建立 App，需在網頁操作，見 §2） |

憑證備份：`~/Desktop/iOS_Certificates_Backup/mac_installer_2026.{key,cer,p12}`，p12 密碼與該資料夾既有備份相同（見 `PilotApp_Flutter_Standalone/docs/ios_certificate_info.md`）。請把該資料夾備份到安全位置。

## 2. App Store Connect 需手動完成

1. https://appstoreconnect.apple.com → 我的 App → ＋ → 新增 App：平台 macOS、名稱 `lailaisay`、主要語言 繁體中文、Bundle ID 選 `lailaisay (com.yikai.lailaisay)`、SKU 自訂（例如 `lailaisay-macos`）。
2. 建好後 `fastlane mac check_version` 才能查到 App。

## 3. 建置設定狀態

| 項目 | 狀態 |
| --- | --- |
| cargo feature `appstore`（app / paste crate） | ✅ |
| `macos/lailaisay-appstore.entitlements` | ✅ |
| `macos/lailaisay-appstore.provisionprofile` | ✅ |
| `Info.plist` `ITSAppUsesNonExemptEncryption = false`、`NSHumanReadableCopyright` | ✅ |
| `Info.plist` `CFBundleVersion` | 每次上傳必須遞增；`fastlane mac check_version` 會比對 |
| `scripts/package-macos-app.sh --app-store` | ✅ 建置 → 嵌入 profile → 沙盒簽署 → `productbuild` → `dist/lailaisay.pkg` |
| `fastlane/Fastfile`（`mac` 平台 lanes） | ✅ |
| 1024×1024 圖示 | ✅ `AppIcon.icns` 內含；App Store Connect 另需上傳同一張 1024×1024 PNG（無 alpha） |
| CI | Linux job 驗證 `--layout-only --app-store` 與 paste 沙盒測試；macOS job 跑 `appstore` feature 的測試與 `cargo check` |

## 4. App Store Connect 上架素材清單（待準備）

| 素材 | 規格 | 狀態 |
| --- | --- | --- |
| App 名稱 | ≤30 字，`lailaisay` | 待填 |
| 副標題 | ≤30 字 | 待撰寫 |
| 說明 | ≤4000 字，zh-Hant（可加 en-US）；需說明 App Store 版是「複製到剪貼簿後 ⌘V」 | 待撰寫 |
| 關鍵字 | ≤100 字，逗號分隔 | 待撰寫 |
| 促銷文字 | ≤170 字，選填 | 選填 |
| 此版本新增功能 | 首版可寫「首次發布」 | 待填 |
| 螢幕截圖 | 至少 1 張；1280×800、1440×900、2560×1600、2880×1800 擇一；建議 3 到 5 張（設定視窗、錄音 HUD、複製結果） | ❌ 無 |
| App 預覽影片 | 選填 | 選填 |
| 支援網址 | 必填，可用 GitHub repo 或 issues 頁 | 待決定 |
| 行銷網址 | 選填 | 選填 |
| 隱私權政策網址 | **必填**；須說明麥克風錄音只在本機處理，選用的 Groq/Gemini 會把文字送到第三方 | ❌ 無 |
| App 隱私（Nutrition Label） | 麥克風音訊不收集、不離開裝置；啟用 Groq/Gemini 時使用者內容會傳給第三方 | 待填 |
| 年齡分級 | 問卷，預期 4+ | 待填 |
| 版權 | `© 2026 Henyi Lai` | 待填 |
| 類別 | 主要：生產力工具；次要：工具程式 | 待填 |
| 價格與供應範圍 | 免費或定價；地區 | 待決定 |
| 審核備註 | 操作步驟（按住 ⌘⇧Space → 說話 → 放開 → ⌘V）；首次啟動需下載 Whisper 模型（約 75MB 起）；不需登入 | 待撰寫 |
| 第三方授權 | whisper.cpp（MIT）、Hex（MIT）、內嵌字型（OFL）已在 `LICENSE`/`NOTICE`/`FONT-LICENSE.txt` | ✅ |

fastlane metadata 目錄結構（`fastlane mac release` 讀取，尚未建立）：

```
rust/fastlane/metadata/zh-Hant/
  name.txt  subtitle.txt  description.txt  keywords.txt
  release_notes.txt  support_url.txt  privacy_url.txt  marketing_url.txt
rust/fastlane/metadata/copyright.txt
rust/fastlane/metadata/primary_category.txt   # PRODUCTIVITY
rust/fastlane/screenshots/zh-Hant/*.png
```

## 5. 上架流程

### 5.1 打包（建置 + 簽署 + pkg）

```sh
cd rust
./scripts/package-macos-app.sh --app-store
# → dist/lailaisay.app（沙盒簽署、嵌入 profile）與 dist/lailaisay.pkg
```

驗證：

```sh
codesign -d --entitlements :- dist/lailaisay.app | grep app-sandbox
codesign --verify --deep --strict --verbose=2 dist/lailaisay.app
pkgutil --check-signature dist/lailaisay.pkg
```

GUI 實測：`open dist/lailaisay.app`，允許麥克風，選模型，按住 ⌘⇧Space 說話放開，狀態應顯示「已複製」，到任一輸入框 ⌘V。

### 5.2 上傳與送審（fastlane，需先完成 §2）

```sh
cd rust
fastlane mac check_version   # 比對 App Store Connect 最新 build 與 Info.plist CFBundleVersion
fastlane mac beta            # 上傳到 TestFlight 內部測試
fastlane mac release         # 上傳 pkg + metadata，不送審
fastlane mac submit_review   # 送審
```

不用 fastlane 時，可用 Transporter.app 或：

```sh
xcrun altool --validate-app -f dist/lailaisay.pkg -t macos \
  --apiKey 7AXGRN3B24 --apiIssuer 69a6de7f-8267-47e3-e053-5b8c7c11a4d1
xcrun altool --upload-app   -f dist/lailaisay.pkg -t macos \
  --apiKey 7AXGRN3B24 --apiIssuer 69a6de7f-8267-47e3-e053-5b8c7c11a4d1
```

（`altool` 會到 `~/.appstoreconnect/private_keys/AuthKey_7AXGRN3B24.p8` 找私鑰。）

### 5.3 送審前自我檢查

- 沙盒版在乾淨帳號實測：熱鍵、麥克風、模型下載、複製結果。
- `spctl` 對 App Store 版不適用（未公證屬正常），改看 App Store Connect 處理結果信件。
- 版本號：`CFBundleShortVersionString` 對應行銷版本，`CFBundleVersion` 每次上傳遞增。
- 審核備註要說明為何是選單列 App（`LSUIElement`）且沒有主視窗。

## 6. 憑證到期提醒

| 憑證 | 到期 |
| --- | --- |
| Apple Distribution | 2026/12/08 |
| 3rd Party Mac Developer Application 與 Provisioning Profile | 2027/07/21 |
| 3rd Party Mac Developer Installer | 2027/09/21 |

到期後重新申請憑證，並在 Developer Portal 重新產生 profile 覆蓋 `macos/lailaisay-appstore.provisionprofile`。
