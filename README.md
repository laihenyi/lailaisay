# lailaisay

Press-and-hold a hotkey to transcribe your voice and paste the result wherever you're typing.

**lailaisay** (`com.yikai.lailaisay`) is a macOS menu-bar and Windows system-tray voice input app written in **Rust** (egui + whisper.cpp). It is a separate product from [Hex](https://github.com/kitlangton/Hex). Hex is MIT-licensed (Copyright (c) 2025 Kit Langton); this repository keeps that license and adds copyright for lailaisay modifications. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## Release preparation

`main` is the single maintained branch. See [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) for the validated baseline and remaining release checks. Windows users can use the standard per-user installer described in [rust/WINDOWS.md](rust/WINDOWS.md).

## Build (macOS)

```bash
cd rust
./scripts/mac-smoke.sh
# expected: 去台南。

cargo run -p lailaisay-cli -- process --text '去台南。'
cargo run -p lailaisay-app --release --features mic,whisper
```

Grant **Microphone** and **Accessibility** (or Input Monitoring) when prompted. Tray → Open Settings → pick a ggml model → Save.

- Smoke path: [`rust/MAC_SMOKE.md`](rust/MAC_SMOKE.md)
- Outside App Store (Developer ID + notarization): [`rust/MAC_SMOKE.md`](rust/MAC_SMOKE.md#outside-app-store-developer-id--notarization) and `rust/scripts/notarize-macos.sh`
- Windows (tray + hotkey): [`rust/WINDOWS.md`](rust/WINDOWS.md)

`rust/rust-toolchain.toml` pins current stable (1.85+). From the repo root:

```bash
cargo +stable test --manifest-path rust/Cargo.toml --workspace
```

## Features

- Local STT via whisper.cpp (optional Metal)
- Post-filters, custom dictionary, zh-TW conversion
- Optional AI polish: Ollama, Groq, or Gemini
- Global hotkey + paste into the frontmost app (macOS CGEvent; Windows `WH_KEYBOARD_LL` + SendInput)

## License

MIT. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
