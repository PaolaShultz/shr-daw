#!/usr/bin/env python3
"""Render README screenshots from real ratatui buffers."""

from __future__ import annotations

import argparse
import gzip
import json
import os
import shlex
import struct
import subprocess
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "images"
PINNED_TOOLCHAIN = Path("/home/patch/.rustup/toolchains/1.85.0-aarch64-unknown-linux-gnu/bin")
PINNED_CARGO = PINNED_TOOLCHAIN / "cargo"
FONT_CANDIDATES = (
    Path("/usr/share/consolefonts/Uni2-TerminusBold24x12.psf.gz"),
    ROOT / "target" / "Uni2-TerminusBold24x12.psf",
)

CELL_W, CELL_H = 12, 24
OUTPUT_SCALE = 2
GLYPH_ALIASES = {
    # TerminusBold has the required double-vertical shape at U+2551 but no
    # U+2016 Unicode-table entry. Keep the TUI's one-cell pause symbol exact
    # while rendering it with that existing console-font glyph.
    0x2016: 0x2551,
}

BRIGHT = {
    (0, 0, 0): (85, 85, 85),
    (170, 0, 0): (255, 85, 85),
    (0, 170, 0): (85, 255, 85),
    (170, 85, 0): (255, 255, 85),
    (0, 0, 170): (85, 85, 255),
    (170, 0, 170): (255, 85, 255),
    (0, 170, 170): (85, 255, 255),
    (170, 170, 170): (255, 255, 255),
}


def read_font(path: Path) -> bytes:
    raw = path.read_bytes()
    return gzip.decompress(raw) if path.suffix == ".gz" else raw


def load_psf2() -> tuple[list[bytes], dict[int, int]]:
    for path in FONT_CANDIDATES:
        if path.exists():
            raw = read_font(path)
            break
    else:
        candidates = ", ".join(str(path) for path in FONT_CANDIDATES)
        raise FileNotFoundError(f"missing console font; tried {candidates}")

    if raw[:4] != b"\x72\xb5\x4a\x86":
        raise ValueError(f"{path}: expected a PSF2 console font")
    (
        _magic,
        version,
        header_size,
        flags,
        glyph_count,
        charsize,
        height,
        width,
    ) = struct.unpack_from("<8I", raw)
    if version != 0 or header_size < 32:
        raise ValueError(f"{path}: unsupported PSF2 header")
    if (width, height) != (CELL_W, CELL_H):
        raise ValueError(
            f"{path}: expected the tty1 {CELL_W}x{CELL_H} font, got {width}x{height}"
        )
    row_bytes = (width + 7) // 8
    if charsize != row_bytes * height:
        raise ValueError(f"{path}: inconsistent PSF2 glyph size")
    glyph_end = header_size + glyph_count * charsize
    if glyph_end > len(raw):
        raise ValueError(f"{path}: truncated PSF2 glyph table")
    glyphs = [
        raw[header_size + i * charsize : header_size + (i + 1) * charsize]
        for i in range(glyph_count)
    ]
    mapping: dict[int, int] = {}
    if not flags & 1:
        raise ValueError(f"{path}: PSF2 font has no Unicode table")
    pos = glyph_end
    for glyph_index in range(glyph_count):
        end = raw.find(b"\xff", pos)
        if end < 0:
            raise ValueError(f"{path}: truncated PSF2 Unicode table")
        entries = raw[pos:end].split(b"\xfe")
        for entry_index, sequence in enumerate(entries):
            if not sequence:
                continue
            try:
                text = sequence.decode("utf-8")
            except UnicodeDecodeError as error:
                raise ValueError(f"{path}: invalid PSF2 Unicode table") from error
            if entry_index == 0:
                for character in text:
                    mapping.setdefault(ord(character), glyph_index)
            elif len(text) == 1:
                mapping.setdefault(ord(text), glyph_index)
        pos = end + 1
    return glyphs, mapping


def screenshot_data() -> dict:
    command = os.environ.get("SHR_SCREENSHOT_COMMAND")
    args = (
        shlex.split(command)
        if command
        else [
            os.environ.get(
                "CARGO", str(PINNED_CARGO if PINNED_CARGO.exists() else "cargo")
            ),
            "run",
            "--quiet",
            "--locked",
            "--",
            "screenshots",
        ]
    )
    env = os.environ.copy()
    if PINNED_TOOLCHAIN.is_dir():
        env["PATH"] = f"{PINNED_TOOLCHAIN}:{env.get('PATH', '')}"
    result = subprocess.run(
        args,
        cwd=ROOT,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return json.loads(result.stdout)


def render(
    name: str,
    cols: int,
    rows: int,
    cells: list[dict],
    glyphs: list[bytes],
    unicode_map: dict[int, int],
) -> None:
    if len(cells) != cols * rows:
        raise ValueError(f"{name}: expected {cols * rows} cells, got {len(cells)}")
    image = Image.new("RGB", (cols * CELL_W, rows * CELL_H), (0, 0, 0))
    pixels = image.load()
    fallback = unicode_map.get(0x3F, 63)
    for index, cell in enumerate(cells):
        x = index % cols
        y = index // cols
        symbol = cell.get("symbol") or " "
        character = symbol[0]
        codepoint = GLYPH_ALIASES.get(ord(character), ord(character))
        glyph = glyphs[unicode_map.get(codepoint, fallback)]
        fg = tuple(cell["fg"])
        bg = tuple(cell["bg"])
        if cell.get("bold"):
            fg = BRIGHT.get(fg, fg)
        cell_x = x * CELL_W
        cell_y = y * CELL_H
        for gy in range(CELL_H):
            for gx in range(CELL_W):
                byte = glyph[gy * 2 + gx // 8]
                pixels[cell_x + gx, cell_y + gy] = (
                    fg if byte & (0x80 >> (gx % 8)) else bg
                )
    destination = OUT / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    integer_scale(image, OUTPUT_SCALE).save(destination, optimize=True)


def integer_scale(image: Image.Image, scale: int) -> Image.Image:
    output = Image.new("RGB", (image.width * scale, image.height * scale))
    source = image.load()
    dest = output.load()
    for y in range(image.height):
        for x in range(image.width):
            value = source[x, y]
            for dy in range(scale):
                for dx in range(scale):
                    dest[x * scale + dx, y * scale + dy] = value
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--only",
        metavar="NAME",
        help="render one exact output name from the screenshot manifest",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify that every manifest image is present and exactly 2x scaled",
    )
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    glyphs, unicode_map = load_psf2()
    data = screenshot_data()
    cols = int(data["cols"])
    rows = int(data["rows"])
    if args.check:
        check_rendered(data, cols, rows)
        return
    for screen in data["screens"]:
        if args.only is not None and screen["name"] != args.only:
            continue
        render(screen["name"], cols, rows, screen["cells"], glyphs, unicode_map)


def check_rendered(data: dict, cols: int, rows: int) -> None:
    expected_size = (cols * CELL_W * OUTPUT_SCALE, rows * CELL_H * OUTPUT_SCALE)
    expected = {screen["name"] for screen in data["screens"]}
    rendered = {
        str(path.relative_to(OUT))
        for path in OUT.rglob("*.png")
        if path.name.startswith("shr-daw-") or path.parent == OUT / "menu"
    }
    missing = sorted(expected - rendered)
    if missing:
        raise ValueError(f"missing screenshot outputs: {', '.join(missing)}")
    for name in sorted(expected):
        with Image.open(OUT / name) as image:
            image = image.convert("RGB")
            if image.size != expected_size:
                raise ValueError(
                    f"{name}: expected {expected_size[0]}x{expected_size[1]}, "
                    f"got {image.width}x{image.height}"
                )
            pixels = image.load()
            for y in range(0, image.height, OUTPUT_SCALE):
                for x in range(0, image.width, OUTPUT_SCALE):
                    value = pixels[x, y]
                    if any(
                        pixels[x + dx, y + dy] != value
                        for dy in range(OUTPUT_SCALE)
                        for dx in range(OUTPUT_SCALE)
                    ):
                        raise ValueError(f"{name}: non-integer scaling at {x},{y}")


if __name__ == "__main__":
    main()
