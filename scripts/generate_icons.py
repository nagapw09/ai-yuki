#!/usr/bin/env python3
"""Генератор иконок приложения — тот же Orb, что и на главном экране (ТЗ §13).

Пишет PNG/ICO/ICNS без сторонних зависимостей: нужен только стандартный zlib.
Запуск: python scripts/generate_icons.py
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

ICONS_DIR = Path(__file__).resolve().parent.parent / "apps" / "desktop" / "src-tauri" / "icons"

# Палитра совпадает с токенами дизайн-системы: графитовый фон и холодное ядро.
BACKDROP = (0x0B, 0x0D, 0x10)
CORE = (0xE8, 0xF7, 0xFF)
INNER = (0x7F, 0xD1, 0xFF)
OUTER = (0x6C, 0x5C, 0xE7)


def _mix(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    t = max(0.0, min(1.0, t))
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))  # type: ignore[return-value]


def _smoothstep(edge0: float, edge1: float, x: float) -> float:
    if edge1 == edge0:
        return 0.0
    t = max(0.0, min(1.0, (x - edge0) / (edge1 - edge0)))
    return t * t * (3.0 - 2.0 * t)


def render(size: int) -> bytes:
    """Рисует Orb размера size×size и возвращает сырые RGBA-строки."""
    rows: list[bytes] = []
    center = (size - 1) / 2.0
    # Радиус скругления квадрата — как у macOS-иконок, примерно 22% стороны.
    corner = size * 0.22
    orb_r = size * 0.30
    glow_r = size * 0.46

    for y in range(size):
        row = bytearray()
        for x in range(size):
            dx, dy = x - center, y - center
            dist = math.hypot(dx, dy)

            # Фон: скруглённый квадрат со сглаженным краем.
            qx = max(abs(dx) - (center - corner), 0.0)
            qy = max(abs(dy) - (center - corner), 0.0)
            corner_dist = math.hypot(qx, qy)
            bg_alpha = 1.0 - _smoothstep(corner - 1.0, corner + 0.5, corner_dist)

            # Свечение вокруг ядра — то самое «мягкое свечение» из ТЗ §13.
            glow = 1.0 - _smoothstep(orb_r * 0.55, glow_r, dist)
            colour = _mix(BACKDROP, OUTER, glow * 0.55)

            # Само ядро с градиентом от холодного центра к фиолетовому краю.
            if dist <= orb_r + 1.0:
                t = min(1.0, dist / orb_r)
                orb_colour = _mix(_mix(CORE, INNER, _smoothstep(0.0, 0.55, t)), OUTER, _smoothstep(0.45, 1.0, t))
                edge = 1.0 - _smoothstep(orb_r - 1.0, orb_r + 0.5, dist)
                colour = _mix(colour, orb_colour, edge)

            alpha = round(255 * bg_alpha)
            row += bytes((colour[0], colour[1], colour[2], alpha))
        rows.append(bytes(row))

    return b"".join(b"\x00" + r for r in rows)


def _chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def png_bytes(size: int) -> bytes:
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _chunk(b"IHDR", header)
        + _chunk(b"IDAT", zlib.compress(render(size), 9))
        + _chunk(b"IEND", b"")
    )


def write_ico(path: Path, pngs: dict[int, bytes]) -> None:
    """ICO с PNG-полезной нагрузкой — поддерживается начиная с Windows Vista."""
    sizes = sorted(pngs)
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = len(header) + 16 * len(sizes)
    entries, blobs = b"", b""
    for size in sizes:
        data = pngs[size]
        # 256 записывается нулём: в поле отведён один байт.
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    path.write_bytes(header + entries + blobs)


def write_icns(path: Path, pngs: dict[int, bytes]) -> None:
    """ICNS из PNG-чанков: ic07/ic08/ic09/ic10 покрывают все нужные macOS размеры."""
    tags = {128: b"ic07", 256: b"ic08", 512: b"ic09", 1024: b"ic10"}
    body = b""
    for size, tag in tags.items():
        if size in pngs:
            data = pngs[size]
            body += tag + struct.pack(">I", len(data) + 8) + data
    path.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main() -> None:
    ICONS_DIR.mkdir(parents=True, exist_ok=True)
    needed = [16, 32, 48, 64, 128, 256, 512, 1024]
    pngs = {size: png_bytes(size) for size in needed}

    for name, size in {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }.items():
        (ICONS_DIR / name).write_bytes(pngs[size])

    write_ico(ICONS_DIR / "icon.ico", {s: pngs[s] for s in (16, 32, 48, 64, 128, 256)})
    write_icns(ICONS_DIR / "icon.icns", {s: pngs[s] for s in (128, 256, 512, 1024)})

    for file in sorted(ICONS_DIR.iterdir()):
        print(f"{file.name:20} {file.stat().st_size:>8} B")


if __name__ == "__main__":
    main()
