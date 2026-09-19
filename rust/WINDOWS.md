# Windows pipeline

Settings / core / enhance / CLI, plus a live **press-and-hold hotkey**
(`WH_KEYBOARD_LL`), **SendInput** paste, and a **notification-area tray**
(開啟設定 / 結束 lailaisay). Complete builds automatically try Vulkan GPU inference,
then a runtime-compatible CPU worker. Minimal builds may omit Whisper.

No API keys.

## What works today

| Piece | Status |
| --- | --- |
| `lailaisay-core` settings, filters, hotkey FSM | **Works** (`cargo test -p lailaisay-core --lib`) |
| Paths | `%APPDATA%\Tok\` (`settings.json`, `custom_words.json`, `models\`) |
| `lailaisay-enhance` | HTTP (Ollama / Groq / Gemini) — same as other OS |
| `lailaisay-stt` dummy + file WAV | **Works**; full app selects isolated Vulkan / AVX2 / compatible CPU workers |
| `lailaisay-cli` `process` / `transcribe --file` / `settings` | **Works** |
| Clipboard `copy_text` | **Works** (`arboard`) |
| Global hotkey | **Works** — [`WindowsEventTap`](crates/lailaisay-input/src/windows.rs) `SetWindowsHookExW(WH_KEYBOARD_LL)` on a message-pump thread. Hold → start, release → stop. Same [`TapMessage`](crates/lailaisay-input/src/tap.rs) channel as macOS. |
| Paste into the focused app | **Works** — clipboard + [`SendInput`](crates/lailaisay-paste/src/windows.rs) Ctrl+V (Shift+Insert fallback). HWND restore is best-effort. Copy-only if injection fails. |
| Tray | **Works** — `tray-icon` notification-area glyph (idle / recording / error) with **開啟設定** / Speak-to-Edit hint / **結束 lailaisay**. Closing Settings **hides to the tray** when the icon is up (same CancelClose idea as macOS). |
| `lailaisay-app --once` | **Works** (no hook / mic) |
| `lailaisay-app` (default / `--settings`) | Tray + live hook. Settings opens on first launch or with `--settings`; later launches stay in the tray when minimize-on-launch is on. `--no-tap` is tray only (plus Settings if shown). |
| Portable folder | [`scripts/package-windows.ps1`](scripts/package-windows.ps1) copies `lailaisay-app.exe`, inference workers + README (optional zip). Standard setup EXE is built from the same payload; auto-start is optional future work. |

```powershell
cd rust
cargo test -p lailaisay-core --lib
cargo test -p lailaisay-input --lib
cargo test -p lailaisay-paste --lib
cargo test -p lailaisay-app --lib
cargo build -p lailaisay-cli
cargo check -p lailaisay-app --features mic
cargo build -p lailaisay-app
cargo run -p lailaisay-cli -- process --text "去台南。"
cargo run -p lailaisay-app -- --once
```

Settings default: `%APPDATA%\Tok\settings.json`  
Models cache: `%APPDATA%\Tok\models\`  
Override with `TOK_CONFIG` / `TOK_MODELS_DIR` as on other OS.

## Run on a Windows machine (tray + hold-to-talk)

1. **Build the host** (mic + local Whisper when you want the full path):

   ```powershell
   cd rust
   # Build machine: install Vulkan SDK first.
   ./scripts/build-windows.ps1
   ./target/release/lailaisay-app.exe
   ```

   Same as the daily product launch: the first launch opens Settings; later
   launches stay in the notification-area tray when **啟動時縮到選單列** is on
   (the default). Tray **開啟設定** and `--settings` open Settings.
   `WH_KEYBOARD_LL` is installed. Default features
   (`cargo run -p lailaisay-app`) still install the hook and tray; STT is dummy
   unless you pick a ggml/gguf model and build the complete worker package.

   Portable folder (optional zip):

   ```powershell
   cd rust
   powershell -File scripts/package-windows.ps1 -Features mic,whisper -Zip
   # → rust\dist\windows\lailaisay\lailaisay-app.exe
   # → rust\dist\windows\lailaisay-windows.zip
   .\dist\windows\lailaisay\lailaisay-app.exe
   ```

2. **Permissions**
   - **Microphone:** Settings → Privacy & security → Microphone → allow
     lailaisay, or the terminal you launched `cargo run` from.
   - **Keyboard hook:** Windows does not show a TCC-style prompt.
     `WH_KEYBOARD_LL` works for a normal user process. If install fails,
     allow lailaisay / the terminal in antivirus or ransomware protection and
     relaunch (tray **結束 lailaisay**, then reopen). Some enterprise policies
     block low-level hooks.
   - No UI Automation / Accessibility grant is required for v1 paste.

3. **Tray**
   - Glyph: grey waveform idle, amber while recording / transcribing /
     enhancing, red on hook / mic / pipeline errors.
   - **開啟設定** — show the Settings window again after you close or minimize it.
     Close and minimize are hide-to-tray (`settings_visible` false); switching
     to another app must not restore Settings.
   - **結束 lailaisay** — quit. Closing the Settings window **does not** quit
     when the tray icon is present.
   - If the tray fails to create, Settings stays visible and closing the
     window quits (stderr prints `MENU EXTRA / TRAY FAILED`).

4. **Test hold-hotkey**
   - Default chord is **Win+Shift+Space** (settings `⌘⇧Space`; ⌘ is the
     Windows key). Speak-to-Edit is **Alt+Shift+Space**.
   - Change the chord in Settings if Win+Shift fights another app; many
     people prefer Ctrl+Shift+Space.
   - Focus Notepad (or any text field) → hold the hotkey → speak →
     release. lailaisay captures that HWND at key-down, records, transcribes,
     enhances, then `SendInput` Ctrl+V.
   - **`--no-tap`** opens Settings + tray without a hook.
   - **`--once`** is the no-hook smoke (`去台南。` with the dummy fixture).

### Focus / paste limits (v1)

- `SetForegroundWindow` + `AttachThreadInput` is **best-effort**. After a
  long Ollama/Groq call Windows may refuse to steal focus. Click the
  field; the transcript is still on the clipboard (Ctrl+V).
- Success means `SendInput` accepted every event. We do **not** use UI
  Automation to prove the field changed.
- Injected keys are tagged `LLKHF_INJECTED` so they do not re-enter the
  hotkey processor.

## CI

`.github/workflows/rust.yml` job `windows` (`windows-latest`):

- `cargo test -p lailaisay-core --lib`
- `cargo test -p lailaisay-input --lib` (VK→Key, processor dispatch; no live hook)
- `cargo test -p lailaisay-paste --lib` (chord builders; SendInput ok-or-fallback)
- `cargo test -p lailaisay-app --lib` (hide-to-tray rules, tray labels)
- `cargo build -p lailaisay-cli` (default features — **no** `whisper` / `mic`)
- `scripts/build-windows.ps1` (host + three isolated Whisper workers)
- Verify CPU instruction flags and run actual small-model inference
- `scripts/package-windows.ps1 -SkipBuild` (portable folder + zip)
- `tok process --text "去台南。"`

CI builds the full Whisper package and checks a real 11-second audio fixture.
Hosted Windows runners verify CPU fallback; physical GPU inference still requires GPU hardware.

## Standard installer

Use `lailaisay-Setup-<version>-x64.exe` for a normal Windows installation. It provides
English and Traditional Chinese setup, per-user installation under
`%LOCALAPPDATA%\Programs\lailaisay`, Start menu and diagnostics shortcuts, an optional
desktop shortcut, and an entry in Windows Installed apps. Administrator rights
are not required. Matching redistributable MSVC runtime DLLs are included next
to the application; the end user does not need a development SDK.

Run a newer installer to upgrade in place. Close lailaisay using the tray's Quit
command first (closing Settings alone hides it). Existing installer versions
share a stable AppId, path and uninstall entry. When migrating from the portable
ZIP, quit that copy before installing and use the new Start menu shortcut.
The application continues to use `%APPDATA%\Tok`; installation, upgrade and
uninstall preserve settings, dictionaries and downloaded models there.

Build after the full portable package:

```powershell
./scripts/package-windows-installer.ps1
# Optional explicit release version (four numeric parts):
./scripts/package-windows-installer.ps1 -Version 0.1.0.130
```

The default installer version uses the workspace version and GitHub run number
(`0.1.0.<run>`; local builds use `0.1.0.0`). The Traditional Chinese translation is pinned in `windows/Languages`.
The build requires Inno Setup 6.5+ and the Visual C++ redistributable directory from the build tools. Outputs are
under `dist/windows/installer`, with a SHA-256 sidecar. The current packaging
does not perform Authenticode signing; code-signing requires a publisher
certificate/service configured separately.

CI tests fresh install, upgrade from an older version, running-app protection,
Start menu/desktop shortcuts, actual small-model inference from a path containing
spaces and Chinese characters, uninstall and user-data retention.
`scripts/test-windows-installer.ps1` is restricted to disposable CI runners.

Auto-start and automated online updates are not configured by this installer.

Apple Silicon macOS paths: [MAC_SMOKE.md](MAC_SMOKE.md) and
[README.md](README.md).

## 自動選擇 Whisper 裝置

Windows 完整套件包含 `lailaisay-app.exe` 和三個常駐辨識 worker：

- `lailaisay-whisper-vulkan.exe`：首次辨識列舉 Vulkan 裝置，優先選獨立 GPU，
  其次整合 GPU；同類型選本機記憶體較大的裝置。忽略軟體 Vulkan CPU 裝置。
- `lailaisay-whisper-avx2.exe`：GPU 無法初始化、原生退出、辨識報錯或逾時，
  由主程式檢查 CPU/OS 的 AVX2、FMA、F16C 支援後啟用。
- `lailaisay-whisper-cpu.exe`：最後的 CPU 相容路徑，沒有 AVX 指令需求。

以實際模型載入和辨識確認 GPU 可用性，不只依顯示卡名稱判斷。
GPU 故障會終止並回收該 worker，將同一段錄音交給 CPU 重試。
模型保持載入，後續錄音沿用成功的 worker；更換／重新載入模型或重啟 lailaisay
會重新偵測。主程式退出時管線關閉，worker 也會退出。
GPU 初始化上限 120 秒、每次辨識上限 120 秒；CPU 每次辨識上限 600 秒。
若所有引擎都失敗，顯示辨識錯誤，不會把失敗當成空白辨識成功。
裝置、回退原因與耗時會寫入診斷紀錄；設定頁顯示目前選擇。

使用者不需安裝 Vulkan SDK 或 Ollama。GPU 需要相容的顯示卡驅動；
若回退 CPU，可更新驅動後重啟重試，或選 base/tiny 減少 CPU 等待。
請完整解壓縮套件，不要只複製主程式。

建置機需要 Vulkan SDK（提供標頭、連結庫及 shader 編譯器），執行
`scripts/build-windows.ps1` 後再以 `scripts/package-windows.ps1 -SkipBuild -Zip`
打包。各 worker 使用獨立 Cargo target 目錄，避免 CMake 的 CPU 指令設定混用。

## Product rename compatibility

The installer and shortcuts now use `lailaisay`. The stable installer AppId and `Local\Tok.Desktop.Running` mutex preserve upgrades and running-app detection. The existing Rust application's data remains in `%APPDATA%\Tok`; upgrades keep the registered install directory and remove obsolete Tok program filenames and shortcuts. A fresh install uses `%LOCALAPPDATA%\Programs\lailaisay`. Existing `TOK_*` environment variables remain supported.
