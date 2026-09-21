# lailaisay Rust workspace

lailaisay is a Rust desktop dictation application for macOS and Windows. Application logic, UI, audio coordination, hotkeys, paste and settings are maintained in this workspace. There is no separate application implementation or model picker from the predecessor.

Speech recognition uses `whisper-rs` and its native whisper.cpp dependency; macOS uses Metal and Windows packages Vulkan plus CPU workers. Shell, PowerShell, Inno Setup, CMake, plist and YAML files are build/configuration assets, not another application implementation.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `lailaisay-app` | egui settings, tray, recording and pipeline coordination |
| `lailaisay-core` | Settings, text processing, punctuation, dictionary and hotkey state |
| `lailaisay-stt` | Audio, Whisper adapters and isolated Windows workers |
| `lailaisay-enhance` | Optional Ollama, Groq and Gemini text polish |
| `lailaisay-input` | Native global keyboard hooks and permission status |
| `lailaisay-paste` | Paste to the previously active application |
| `lailaisay-cli` | Command-line processing and transcription |

## Validation

```sh
cargo test --workspace --features lailaisay-stt/process-whisper
cargo run -p lailaisay-cli -- process --text '去台南。'
cargo run -p lailaisay-app -- --once
```

The no-argument `--once` path uses the fixture backend without microphone, paste, or permission prompts. A real model can be tested with:

```sh
cargo run -p lailaisay-app --release --features whisper -- --once \
  --backend whisper --model /path/ggml-small.bin --file /path/speech.wav
```

## Desktop builds

- [macOS build and smoke checks](MAC_SMOKE.md)
- [macOS Developer ID + notarization](MAC_SMOKE.md#outside-app-store-developer-id--notarization)
- [Mac App Store (sandbox) variant, certificates and upload](APP_STORE.md) — `--features appstore`, `./scripts/package-macos-app.sh --app-store`, `fastlane mac …`
- [Windows build, installer and diagnostics](WINDOWS.md)
- [Release checklist](../RELEASE_CHECKLIST.md)

The model menu lists usable local ggml/gguf files from the application cache, `TOK_WHISPER_MODEL`, and MacWhisper. The catalog downloads multilingual tiny, base, small, medium, large-v3 and large-v3-turbo. Optional AI polish is separate from speech recognition; Ollama is not needed for local Whisper.

## Existing user data

The product name and binaries are now lailaisay. The macOS bundle identifier is now `com.yikai.lailaisay`; grant macOS permissions again after upgrading. Existing settings are retained, and an empty model cache imports weights from the legacy `xyz.2qs.Tok` cache:

- macOS bundle identifier and model cache: `com.yikai.lailaisay`, `~/Library/Application Support/com.yikai.lailaisay/models`.
- macOS settings and dictionaries: `~/Documents/hex_settings.json`, `hex_custom_words.json`, `hex_phonetic_glossary.json`; correction history uses `correction_history.json` in the same directory.
- Windows data: `%APPDATA%\Tok`, including `settings.json` and `models`.
- Linux data/config paths retain the existing `tok` directory.
- Existing `TOK_*` environment overrides and camelCase JSON fields remain supported. Examples: `TOK_CONFIG`, `TOK_MODELS_DIR`, `TOK_WHISPER_MODEL`, `TOK_CUSTOM_WORDS`, `TOK_GROQ_API_KEY`, `TOK_GEMINI_API_KEY`.
- The Windows installer retains its stable AppId and running-app mutex to upgrade the earlier Rust release. A fresh installation uses `%LOCALAPPDATA%\Programs\lailaisay`; upgrades reuse the registered installation directory.

These are persisted data/API compatibility contracts, not dependencies on an older language runtime. Third-party attribution remains in [LICENSE](../LICENSE), [NOTICE](../NOTICE) and the bundled font license.
