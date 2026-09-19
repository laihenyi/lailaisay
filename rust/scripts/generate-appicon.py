#!/usr/bin/env python3
"""Write rust/macos/AppIcon.icns (PNG-in-icns, no extra deps)."""
from __future__ import annotations

import math
import struct
import sys
import zlib
from pathlib import Path


def png_chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def write_png(width: int, rgba: bytes) -> bytes:
    height = width
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw.extend(rgba[y * stride : (y + 1) * stride])
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + png_chunk(b"IEND", b"")
    )


def paint(size: int) -> bytes:
    out = bytearray(size * size * 4)
    cx = cy = (size - 1) / 2.0
    radius = size * 0.46
    inset = size * 0.16
    heights = (0.28, 0.52, 0.78, 0.52, 0.28)
    bar_w = max(1.0, size * 0.07)
    gap = size * 0.045
    total_w = 5 * bar_w + 4 * gap
    start_x = cx - total_w / 2.0
    for y in range(size):
        for x in range(size):
            dx = x - cx
            dy = y - cy
            dist = math.hypot(dx, dy)
            # Soft rounded-square mask
            ax, ay = abs(dx) / radius, abs(dy) / radius
            n = 4.0
            sd = (ax**n + ay**n) ** (1.0 / n)
            t = max(0.0, min(1.0, (1.02 - sd) / 0.08))
            bg = (28, 30, 34)
            r, g, b, a = bg[0], bg[1], bg[2], int(255 * t)
            # Waveform bars
            if inset < x < size - inset and inset < y < size - inset:
                for i, h in enumerate(heights):
                    bx0 = start_x + i * (bar_w + gap)
                    bx1 = bx0 + bar_w
                    half = size * h / 2.0
                    if bx0 - 0.4 <= x <= bx1 + 0.4 and abs(y - cy) <= half:
                        gold = (212, 168, 48)
                        r, g, b = gold
            idx = (y * size + x) * 4
            out[idx : idx + 4] = bytes((r, g, b, a))
    return bytes(out)


def icns(pngs: list[tuple[bytes, bytes]]) -> bytes:
    body = b"".join(kind + struct.pack(">I", 8 + len(data)) + data for kind, data in pngs)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


# PNG-in-icns types. Include 64 + 1024 and retina pairs so Dock/Finder
# have a real bitmap instead of synthesizing a bundle-id monogram.
ICNS_KINDS: list[tuple[int, bytes]] = [
    (16, b"icp4"),
    (32, b"icp5"),
    (64, b"icp6"),
    (128, b"ic07"),
    (256, b"ic08"),
    (512, b"ic09"),
    (1024, b"ic10"),
    (32, b"ic11"),  # 16@2x
    (64, b"ic12"),  # 32@2x
    (256, b"ic13"),  # 128@2x
]


def build_icns() -> bytes:
    pngs = [(kind, write_png(n, paint(n))) for n, kind in ICNS_KINDS]
    return icns(pngs)


def verify_icns(data: bytes) -> None:
    if data[:4] != b"icns":
        raise SystemExit("AppIcon.icns is not an icns container")
    declared = struct.unpack(">I", data[4:8])[0]
    if declared != len(data):
        raise SystemExit(f"AppIcon.icns size mismatch: declared {declared} bytes {len(data)}")
    found: dict[bytes, int] = {}
    off = 8
    while off < len(data):
        kind = data[off : off + 4]
        size = struct.unpack(">I", data[off + 4 : off + 8])[0]
        payload = data[off + 8 : off + size]
        if not payload.startswith(b"\x89PNG"):
            raise SystemExit(f"icns chunk {kind!r} is not PNG")
        found[kind] = len(payload)
        off += size
    missing = [kind for _, kind in ICNS_KINDS if kind not in found]
    if missing:
        raise SystemExit(f"AppIcon.icns missing chunks {missing}")
    if found[b"ic10"] < 1024:
        raise SystemExit("ic10 (1024px) payload is too small to be a real Dock bitmap")


def main() -> None:
    dest = Path(__file__).resolve().parent.parent / "macos" / "AppIcon.icns"
    dest.parent.mkdir(parents=True, exist_ok=True)
    if "--check" in sys.argv[1:]:
        verify_icns(dest.read_bytes())
        print(f"ok {dest} ({dest.stat().st_size} bytes)")
        return
    dest.write_bytes(build_icns())
    verify_icns(dest.read_bytes())
    print(f"wrote {dest} ({dest.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
