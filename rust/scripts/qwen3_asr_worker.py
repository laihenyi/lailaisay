#!/usr/bin/env python3
"""Resident Qwen3-ASR (MLX) worker for lailaisay's `worker_protocol`.

stdin : one JSON line per request  {"samples": [f32...], "language": ..., "prompt": ...}
stdout: one JSON line per reply    {"Ready":{"device":..}} | {"Transcript":{..}} | {"Error":".."}
stderr: diagnostics only.

Setup: python3 -m venv V && V/bin/pip install mlx-audio ; TOK_QWEN_PYTHON=V/bin/python
"""
import json
import os
import struct
import sys
import tempfile
import time
import wave

MODEL = sys.argv[1] if len(sys.argv) > 1 else "mlx-community/Qwen3-ASR-0.6B-8bit"
RATE = 16000
LANGS = {"zh": "Chinese", "en": "English", "ja": "Japanese", "ko": "Korean"}


def reply(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def write_wav(path, samples):
    pcm = struct.pack(
        "<%dh" % len(samples),
        *(max(-32768, min(32767, int(s * 32767))) for s in samples),
    )
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm)


def main():
    from mlx_audio.stt.utils import load_model

    t = time.time()
    model = load_model(MODEL)
    fd, wav = tempfile.mkstemp(suffix=".wav", prefix="lailaisay-qwen-")
    os.close(fd)
    write_wav(wav, [0.0] * RATE)
    model.generate(wav, language="Chinese")  # warm up kernels
    print(f"[qwen3-asr] {MODEL} ready in {time.time() - t:.1f}s", file=sys.stderr, flush=True)
    reply({"Ready": {"device": "MLX " + MODEL.rsplit("/", 1)[-1]}})
    try:
        for line in sys.stdin:
            try:
                req = json.loads(line)
                samples = req["samples"]
                write_wav(wav, samples)
                lang = (req.get("language") or "").split("-")[0].split("_")[0].lower()
                kwargs = {"language": LANGS[lang]} if lang in LANGS else {}
                t = time.time()
                text = model.generate(wav, **kwargs).text.strip()
                print(f"[qwen3-asr] {len(samples)/RATE:.2f}s audio -> {time.time()-t:.3f}s", file=sys.stderr, flush=True)
                segs = [[text, 0.0, len(samples) / RATE]] if text else []
                reply({"Transcript": {"text": text, "language": req.get("language"), "segments": segs}})
            except Exception as e:  # keep the worker alive
                reply({"Error": f"{type(e).__name__}: {e}"})
    finally:
        try:
            os.remove(wav)
        except OSError:
            pass


if __name__ == "__main__":
    main()
