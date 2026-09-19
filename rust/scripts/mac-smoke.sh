#!/usr/bin/env bash
# TCC-free first smoke: build lailaisay-app and run --once on the dummy fixture.
# Safe on Linux CI and on a Mac before granting Accessibility.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
RUST_ROOT="$(cd "$HERE/.." && pwd)"
cd "$RUST_ROOT"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install rustup: https://rustup.rs" >&2
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
  exit 1
fi

echo "== rustc =="
rustc --version
echo "toolchain pin: $RUST_ROOT/rust-toolchain.toml (channel = stable, rust-version 1.85+)"
echo

echo "== cargo test --workspace (quick check) =="
if [[ "${TOK_SMOKE_SKIP_TEST:-}" == "1" ]]; then
  echo "(skipped: TOK_SMOKE_SKIP_TEST=1)"
else
  cargo test --workspace
fi
echo

echo "== lailaisay-app --once (dummy, no paste, no TCC) =="
set +e
OUT="$(cargo run -q -p lailaisay-app -- --once --file fixtures/sample.wav --backend dummy --no-paste 2>/tmp/lailaisay-app-once.err)"
STATUS=$?
set -e
if [[ -s /tmp/lailaisay-app-once.err ]]; then
  cat /tmp/lailaisay-app-once.err >&2
fi
echo "$OUT"
if [[ "$STATUS" -ne 0 ]]; then
  echo "FAIL: lailaisay-app --once exited $STATUS" >&2
  exit "$STATUS"
fi
if ! printf '%s\n' "$OUT" | grep -q '去台南'; then
  echo "FAIL: expected 去台南 in stdout (dummy sidecar → local filters)" >&2
  exit 1
fi

echo
echo "OK: pipeline printed 去台南。 No Accessibility / mic / paste was required."
echo
echo "Next (on a Mac — these need human TCC clicks):"
echo "  1. Read  $RUST_ROOT/MAC_SMOKE.md"
echo "  2. $HERE/package-macos-app.sh          # → dist/lailaisay.app (com.yikai.lailaisay)"
echo "  3. open dist/lailaisay.app"
echo "  4. System Settings → Privacy & Security → Accessibility"
echo "     + Input Monitoring + Microphone  (enable lailaisay.app, not Terminal)"
echo "  5. Settings opens immediately (sections + status strip). Pick a ggml"
echo "     model or Download selected → 儲存 Save. Quit from the tray or 結束 lailaisay."
echo "  6. Hold the hotkey (⌘⇧Space or your configured fn-only chord), speak, release."
echo
echo "  Dev loop still: cargo run -p lailaisay-app --release --features mic,whisper"
echo
echo "Optional real STT (downloads ~75 MB, not needed for the dummy smoke):"
echo "  $HERE/download-whisper-tiny.sh"
