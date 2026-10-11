#!/usr/bin/env bash
# Create the Python (MLX) environment used by the Qwen3-ASR models in lailaisay
# settings, and pre-download a model so the first dictation does not wait.
#
#   ./scripts/setup-qwen3-asr.sh [hf-repo]     # default mlx-community/Qwen3-ASR-0.6B-8bit
#
# Apple Silicon macOS only. Not part of the Mac App Store build (sandbox).
set -euo pipefail

REPO="${1:-mlx-community/Qwen3-ASR-0.6B-8bit}"
VENV="${TOK_QWEN_VENV:-$HOME/Library/Application Support/com.yikai.lailaisay/qwen-venv}"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "Qwen3-ASR (MLX) needs Apple Silicon macOS." >&2
  exit 1
fi

if [[ ! -x "$VENV/bin/python" ]]; then
  mkdir -p "$(dirname "$VENV")"
  if command -v uv >/dev/null 2>&1; then
    uv venv -q --python 3.11 "$VENV"
  else
    python3 -m venv "$VENV"
  fi
fi

if command -v uv >/dev/null 2>&1; then
  uv pip install -q --python "$VENV/bin/python" -U mlx-audio
else
  "$VENV/bin/python" -m pip install -q -U mlx-audio
fi

echo "Downloading $REPO …"
"$VENV/bin/python" -c "from mlx_audio.stt.utils import load_model; load_model('$REPO'); print('ready')"
echo "Done. In lailaisay: Settings → 語音模型 → pick a Qwen3-ASR entry → Save."
echo "Python: $VENV/bin/python"
