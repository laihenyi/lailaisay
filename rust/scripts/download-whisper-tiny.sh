#!/usr/bin/env bash
# Fetch ggml-tiny.bin into the lailaisay model cache. Does not commit the file (~75 MB).
set -euo pipefail

if [[ -n "${TOK_MODELS_DIR:-}" ]]; then
  DEST="$TOK_MODELS_DIR"
elif [[ "$(uname -s)" == "Darwin" ]]; then
  DEST="${HOME}/Library/Application Support/com.yikai.lailaisay/models"
elif [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* || "$(uname -s)" == MINGW64_NT* ]]; then
  DEST="${APPDATA:-$HOME/AppData/Roaming}/lailaisay/models"
else
  DEST="${XDG_DATA_HOME:-$HOME/.local/share}/tok/models"
fi

mkdir -p "$DEST"
OUT="$DEST/ggml-tiny.bin"
URL="${TOK_WHISPER_TINY_URL:-https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin}"

if [[ -f "$OUT" && "${TOK_FORCE_DOWNLOAD:-}" != "1" ]]; then
  echo "already present: $OUT"
else
  echo "downloading $URL"
  echo "         → $OUT"
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --retry-delay 2 -o "$OUT" "$URL"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$OUT" "$URL"
  else
    echo "need curl or wget" >&2
    exit 1
  fi
fi

bytes=$(wc -c < "$OUT" | tr -d ' ')
if [[ "$bytes" -lt 1000000 ]]; then
  echo "download looks too small (${bytes} bytes) — delete $OUT and retry" >&2
  exit 1
fi

echo
echo "export TOK_WHISPER_MODEL=\"$OUT\""
echo
echo "Then, from rust/:"
echo "  cargo run -p lailaisay-app --features whisper -- --once --backend whisper --file fixtures/sample.wav --model \"\$TOK_WHISPER_MODEL\""
echo "  # silence WAV often yields empty / filtered text; dummy sidecar is the guaranteed smoke."
