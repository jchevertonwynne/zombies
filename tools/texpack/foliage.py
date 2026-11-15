#!/usr/bin/env python3
"""Make the procedural leaf-cluster card the Trees' crowns are drawn with (CC0, ours), as the
colour and opacity PNGs that `texpack --color --opacity` packs into `assets/decals/foliage/`.

    python3 tools/texpack/foliage.py <out dir>
"""
import math
import pathlib
import random
import struct
import sys
import zlib

N = 512


def png(path, rows, channels):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\0" + bytes(row) for row in rows)
    colour_type = {1: 0, 3: 2}[channels]
    ihdr = struct.pack(">IIBBBBB", N, N, 8, colour_type, 0, 0, 0)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(7)
    colour = [[(0, 0, 0)] * N for _ in range(N)]
    alpha = [[0] * N for _ in range(N)]
    # Leaves scattered over a soft blob, drawn back to front, lighter toward the top.
    leaves = []
    for _ in range(420):
        r = math.sqrt(rng.random()) * 0.46
        a = rng.random() * math.tau
        leaves.append((0.5 + r * math.cos(a), 0.5 + r * math.sin(a) * 0.85, rng.uniform(0.025, 0.05), rng.random() * math.pi))
    leaves.sort(key=lambda leaf: -leaf[1])
    for cx, cy, size, turn in leaves:
        shade = rng.uniform(0.7, 1.15) * (1.0 - 0.35 * cy)
        base = (int(34 * shade), int(66 * shade), int(24 * shade))
        c, s = math.cos(turn), math.sin(turn)
        reach = int(size * N * 2.2)
        px, py = int(cx * N), int(cy * N)
        for y in range(max(0, py - reach), min(N, py + reach)):
            for x in range(max(0, px - reach), min(N, px + reach)):
                dx, dy = (x - px) / N, (y - py) / N
                u, v = dx * c + dy * s, -dx * s + dy * c
                if (u / (size * 2.0)) ** 2 + (v / size) ** 2 <= 1.0:
                    colour[y][x] = base
                    alpha[y][x] = 255
    png(out / "color.png", [[v for px in row for v in px] for row in colour], 3)
    png(out / "opacity.png", alpha, 1)


main()
