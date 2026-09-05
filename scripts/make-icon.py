#!/usr/bin/env python3
"""生成桌面图标源图(1024x1024 深底双波占位设计),输出 icons/source.png。仅用 stdlib。"""
import math
import struct
import zlib

W = H = 1024
BG = (15, 23, 42)      # slate-900
WAVE1 = (34, 211, 238)  # cyan-400
WAVE2 = (20, 184, 166)  # teal-500


def pixel(x, y):
    for k, color in ((0, WAVE1), (1, WAVE2)):
        cy = 380 + k * 220 + (60 + k * 30) * math.sin(x / 140 + k * 1.7)
        if abs(y - cy) < 26:
            return color
    return BG


rows = []
for y in range(H):
    row = bytearray([0])
    for x in range(W):
        row += bytes(pixel(x, y))
    rows.append(bytes(row))
raw = b"".join(rows)


def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))


png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(raw, 9))
       + chunk(b"IEND", b""))
with open("icons/source.png", "wb") as f:
    f.write(png)
print("icons/source.png written")
