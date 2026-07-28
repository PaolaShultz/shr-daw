#!/usr/bin/env python3
"""Render README screenshots from real ratatui buffers."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import shlex
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageChops


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "images"
PINNED_TOOLCHAIN = Path("/home/patch/.rustup/toolchains/1.85.0-aarch64-unknown-linux-gnu/bin")
PINNED_CARGO = PINNED_TOOLCHAIN / "cargo"
APPROVED_FONT = Path("/usr/share/consolefonts/Uni2-TerminusBold24x12.psf.gz")
FALLBACK_FONT = ROOT / "target" / "Uni2-TerminusBold24x12.psf"
APPROVED_FONT_SHA256 = "76cbb7a30085000dab63323650d2296486f8af5528f51eead1519dbfce96b1f9"

CELL_W, CELL_H = 12, 24
TERMINAL_COLS, TERMINAL_ROWS = 40, 13
OUTPUT_SCALE = 2
NATIVE_SIZE = (TERMINAL_COLS * CELL_W, TERMINAL_ROWS * CELL_H)
OUTPUT_SIZE = tuple(value * OUTPUT_SCALE for value in NATIVE_SIZE)
PAUSE = "‖"
NON_TUI_ROOT_PNGS = {"shr-daw-social-card.png"}

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

FIXED_PALETTE = frozenset(BRIGHT) | frozenset(BRIGHT.values()) | {(28, 28, 28)}
REQUIRED_GLYPHS = (
    "".join(chr(codepoint) for codepoint in range(0x20, 0x7F))
    + "·×–—…─│┌┐└┘═║╔╗╚╝■▶●°"
    + "ČĆĐŠŽčćđšž"
)

# Independent fixtures from the approved decompressed PSF. These do not use
# the parser below, so a stride, bit-order, row-index, or Unicode-table
# regression cannot validate itself.
KNOWN_GLYPH_ROWS = {
    "A": (
        0x000, 0x000, 0x000, 0x000, 0x1F8, 0x30C, 0x606, 0x606,
        0x606, 0x606, 0x606, 0x606, 0x7FE, 0x606, 0x606, 0x606,
        0x606, 0x606, 0x606, 0x000, 0x000, 0x000, 0x000, 0x000,
    ),
    "g": (
        0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
        0x1FE, 0x306, 0x606, 0x606, 0x606, 0x606, 0x606, 0x606,
        0x606, 0x30E, 0x1FE, 0x006, 0x006, 0x00C, 0x3F8, 0x000,
    ),
    "●": (
        0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
        0x000, 0x0F0, 0x1F8, 0x1F8, 0x1F8, 0x1F8, 0x0F0, 0x000,
        0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
    ),
    "■": (
        0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x3FC,
        0x3FC, 0x3FC, 0x3FC, 0x3FC, 0x3FC, 0x3FC, 0x3FC, 0x3FC,
        0x3FC, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
    ),
    PAUSE: (
        0x000, 0x000, 0x000, 0x000, 0x198, 0x198, 0x198, 0x198,
        0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198,
        0x198, 0x198, 0x198, 0x000, 0x000, 0x000, 0x000, 0x000,
    ),
    "║": (
        0x060, 0x060, 0x060, 0x060, 0x060, 0x060, 0x060, 0x060,
        0x060, 0x060, 0x060, 0x060, 0x060, 0x060, 0x060, 0x060,
        0x060, 0x060, 0x060, 0x060, 0x060, 0x060, 0x060, 0x060,
    ),
    "°": (
        0x000, 0x000, 0x1F0, 0x318, 0x318, 0x318, 0x318, 0x1F0,
        0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
        0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000, 0x000,
    ),
    "č": (
        0x000, 0x000, 0x000, 0x000, 0x198, 0x0F0, 0x060, 0x000,
        0x1F8, 0x30C, 0x606, 0x600, 0x600, 0x600, 0x600, 0x600,
        0x606, 0x30C, 0x1F8, 0x000, 0x000, 0x000, 0x000, 0x000,
    ),
}


@dataclass(frozen=True)
class PsfFont:
    path: Path
    sha256: str
    glyphs: tuple[bytes, ...]
    unicode_map: dict[int, int]
    width: int
    height: int
    row_bytes: int


def read_font(path: Path) -> bytes:
    raw = path.read_bytes()
    return gzip.decompress(raw) if raw.startswith(b"\x1f\x8b") else raw


def load_approved_font() -> PsfFont:
    errors = []
    for path in (APPROVED_FONT, FALLBACK_FONT):
        if not path.exists():
            errors.append(f"{path}: missing")
            continue
        raw = read_font(path)
        digest = hashlib.sha256(raw).hexdigest()
        if digest != APPROVED_FONT_SHA256:
            errors.append(
                f"{path}: decompressed SHA-256 {digest}, "
                f"expected {APPROVED_FONT_SHA256}"
            )
            continue
        return parse_psf2(path, raw, digest)
    raise FileNotFoundError(
        "missing approved console font or byte-identical fallback:\n  "
        + "\n  ".join(errors)
    )


def parse_psf2(path: Path, raw: bytes, digest: str) -> PsfFont:
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
        direct, *sequences = raw[pos:end].split(b"\xfe")
        try:
            text = direct.decode("utf-8")
            for sequence in sequences:
                sequence.decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValueError(f"{path}: invalid PSF2 Unicode table") from error
        for character in text:
            mapping.setdefault(ord(character), glyph_index)
        pos = end + 1
    if pos != len(raw):
        raise ValueError(f"{path}: unexpected data after PSF2 Unicode table")
    return PsfFont(
        path=path,
        sha256=digest,
        glyphs=tuple(glyphs),
        unicode_map=mapping,
        width=width,
        height=height,
        row_bytes=row_bytes,
    )


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
    font: PsfFont,
) -> None:
    destination = OUT / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    integer_scale(
        render_native(name, cols, rows, cells, font), OUTPUT_SCALE
    ).save(destination, optimize=True)


def render_native(
    name: str,
    cols: int,
    rows: int,
    cells: list[dict],
    font: PsfFont,
) -> Image.Image:
    if len(cells) != cols * rows:
        raise ValueError(f"{name}: expected {cols * rows} cells, got {len(cells)}")
    image = Image.new("RGB", (cols * CELL_W, rows * CELL_H), (0, 0, 0))
    pixels = image.load()
    for index, cell in enumerate(cells):
        x = index % cols
        y = index // cols
        symbol = cell.get("symbol") or " "
        if len(symbol) != 1:
            raise ValueError(f"{name}: cell {x},{y} has non-scalar symbol {symbol!r}")
        codepoint = ord(symbol)
        if codepoint not in font.unicode_map:
            raise ValueError(
                f"{name}: cell {x},{y} uses unsupported U+{codepoint:04X} {symbol!r}"
            )
        glyph = font.glyphs[font.unicode_map[codepoint]]
        fg = tuple(cell["fg"])
        bg = tuple(cell["bg"])
        if fg not in FIXED_PALETTE or bg not in FIXED_PALETTE:
            raise ValueError(
                f"{name}: cell {x},{y} uses non-terminal palette colours {fg}/{bg}"
            )
        if cell.get("bold"):
            fg = BRIGHT.get(fg, fg)
        cell_x = x * CELL_W
        cell_y = y * CELL_H
        draw_glyph(pixels, cell_x, cell_y, glyph, font.row_bytes, fg, bg)
    return image


def draw_glyph(
    pixels,
    cell_x: int,
    cell_y: int,
    glyph: bytes,
    row_bytes: int,
    fg: tuple[int, int, int],
    bg: tuple[int, int, int],
) -> None:
    for glyph_y in range(CELL_H):
        row_start = glyph_y * row_bytes
        for glyph_x in range(CELL_W):
            byte = glyph[row_start + glyph_x // 8]
            pixels[cell_x + glyph_x, cell_y + glyph_y] = (
                fg if byte & (0x80 >> (glyph_x % 8)) else bg
            )


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
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument(
        "--only",
        metavar="NAME",
        help="render one exact output name from the screenshot manifest",
    )
    modes.add_argument(
        "--check",
        action="store_true",
        help="verify the complete manifest, font raster, palette, and exact scaling",
    )
    modes.add_argument(
        "--self-test",
        action="store_true",
        help="validate the approved PSF identity, glyphs, and row/cell placement",
    )
    args = parser.parse_args()
    font = load_approved_font()
    validate_font(font)
    if args.self_test:
        print(
            f"approved PSF self-test passed: {font.path} "
            f"(decompressed SHA-256 {font.sha256})"
        )
        return

    data = screenshot_data()
    cols, rows, screens = validate_manifest(data)
    if args.check:
        check_rendered(screens, cols, rows, font)
        return
    OUT.mkdir(parents=True, exist_ok=True)
    rendered = 0
    for screen in screens:
        if args.only is not None and screen["name"] != args.only:
            continue
        render(screen["name"], cols, rows, screen["cells"], font)
        rendered += 1
    if args.only is not None and rendered == 0:
        raise ValueError(f"{args.only}: no exact screenshot name in manifest")
    if args.only is None:
        remove_stale_outputs({screen["name"] for screen in screens})
    print(
        f"rendered {rendered} screenshot{'s' if rendered != 1 else ''} "
        f"with {font.path}"
    )


def validate_font(font: PsfFont) -> None:
    missing = [glyph for glyph in REQUIRED_GLYPHS if ord(glyph) not in font.unicode_map]
    if missing:
        labels = ", ".join(f"U+{ord(glyph):04X} {glyph!r}" for glyph in missing)
        raise ValueError(f"{font.path}: missing required glyphs: {labels}")
    if font.unicode_map[ord(PAUSE)] == font.unicode_map[ord("║")]:
        raise ValueError(f"{font.path}: pause glyph incorrectly aliases the box border")
    for character, expected_rows in KNOWN_GLYPH_ROWS.items():
        glyph = font.glyphs[font.unicode_map[ord(character)]]
        actual_rows = glyph_rows(glyph, font.row_bytes)
        if actual_rows != expected_rows:
            raise ValueError(
                f"{font.path}: U+{ord(character):04X} {character!r} bitmap mismatch"
            )
    validate_row_and_cell_placement(font.row_bytes)


def glyph_rows(glyph: bytes, row_bytes: int) -> tuple[int, ...]:
    rows = []
    for glyph_y in range(CELL_H):
        row = 0
        row_start = glyph_y * row_bytes
        for glyph_x in range(CELL_W):
            if glyph[row_start + glyph_x // 8] & (0x80 >> (glyph_x % 8)):
                row |= 1 << (CELL_W - glyph_x - 1)
        rows.append(row)
    return tuple(rows)


def validate_row_and_cell_placement(row_bytes: int) -> None:
    glyph = bytearray(row_bytes * CELL_H)
    expected_rows = []
    for glyph_y in range(CELL_H):
        row = glyph_y + 1
        expected_rows.append(row)
        encoded = row << (row_bytes * 8 - CELL_W)
        glyph[glyph_y * row_bytes : (glyph_y + 1) * row_bytes] = encoded.to_bytes(
            row_bytes, "big"
        )
    image = Image.new("RGB", (CELL_W * 3, CELL_H * 3), (3, 4, 5))
    draw_glyph(
        image.load(),
        CELL_W,
        CELL_H,
        bytes(glyph),
        row_bytes,
        (255, 255, 255),
        (0, 0, 0),
    )
    pixels = image.load()
    for image_y in range(image.height):
        for image_x in range(image.width):
            in_cell = (
                CELL_W <= image_x < CELL_W * 2
                and CELL_H <= image_y < CELL_H * 2
            )
            if not in_cell:
                if pixels[image_x, image_y] != (3, 4, 5):
                    raise ValueError("glyph drawing overwrote an adjacent cell or row")
                continue
            glyph_y = image_y - CELL_H
            glyph_x = image_x - CELL_W
            expected = bool(expected_rows[glyph_y] & (1 << (CELL_W - glyph_x - 1)))
            actual = pixels[image_x, image_y] == (255, 255, 255)
            if actual != expected:
                raise ValueError(
                    f"glyph row mapping mismatch at source row {glyph_y}, x {glyph_x}"
                )


def validate_manifest(data: dict) -> tuple[int, int, list[dict]]:
    cols = int(data["cols"])
    rows = int(data["rows"])
    if (cols, rows) != (TERMINAL_COLS, TERMINAL_ROWS):
        raise ValueError(
            f"manifest geometry must be {TERMINAL_COLS}x{TERMINAL_ROWS}, "
            f"got {cols}x{rows}"
        )
    screens = data["screens"]
    names = [screen["name"] for screen in screens]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    if duplicates:
        raise ValueError(f"duplicate screenshot names: {', '.join(duplicates)}")
    for screen in screens:
        name = screen["name"]
        path = Path(name)
        if (
            path.is_absolute()
            or ".." in path.parts
            or path.suffix != ".png"
            or (path.parent != Path(".") and path.parent != Path("menu"))
        ):
            raise ValueError(f"unsafe screenshot output path: {name}")
        if len(screen["cells"]) != cols * rows:
            raise ValueError(
                f"{name}: expected {cols * rows} cells, got {len(screen['cells'])}"
            )
    return cols, rows, screens


def rendered_outputs() -> set[str]:
    outputs = {
        str(path.relative_to(OUT))
        for path in (OUT / "menu").glob("*.png")
        if path.is_file()
    }
    outputs.update(
        path.name
        for path in OUT.glob("shr-daw-*.png")
        if path.is_file() and path.name not in NON_TUI_ROOT_PNGS
    )
    return outputs


def remove_stale_outputs(expected: set[str]) -> None:
    for name in sorted(rendered_outputs() - expected):
        (OUT / name).unlink()
        print(f"removed stale screenshot {name}")


def check_rendered(
    screens: list[dict],
    cols: int,
    rows: int,
    font: PsfFont,
) -> None:
    expected = {screen["name"] for screen in screens}
    rendered = rendered_outputs()
    missing = sorted(expected - rendered)
    if missing:
        raise ValueError(f"missing screenshot outputs: {', '.join(missing)}")
    extra = sorted(rendered - expected)
    if extra:
        raise ValueError(f"stale or extra screenshot outputs: {', '.join(extra)}")
    for screen in sorted(screens, key=lambda item: item["name"]):
        name = screen["name"]
        expected_native = render_native(name, cols, rows, screen["cells"], font)
        expected_output = integer_scale(expected_native, OUTPUT_SCALE)
        with Image.open(OUT / name) as image:
            image = image.convert("RGB")
            if image.size != OUTPUT_SIZE:
                raise ValueError(
                    f"{name}: expected {OUTPUT_SIZE[0]}x{OUTPUT_SIZE[1]}, "
                    f"got {image.width}x{image.height}"
                )
            pixels = image.load()
            colors = image.getcolors(maxcolors=OUTPUT_SIZE[0] * OUTPUT_SIZE[1])
            if colors is None:
                raise ValueError(f"{name}: too many colours for a terminal raster")
            actual_palette = {color for _count, color in colors}
            unexpected = sorted(actual_palette - FIXED_PALETTE)
            if unexpected:
                raise ValueError(f"{name}: non-terminal palette colours: {unexpected}")
            for y in range(0, OUTPUT_SIZE[1], OUTPUT_SCALE):
                for x in range(0, OUTPUT_SIZE[0], OUTPUT_SCALE):
                    value = pixels[x, y]
                    if any(
                        pixels[x + dx, y + dy] != value
                        for dy in range(OUTPUT_SCALE)
                        for dx in range(OUTPUT_SCALE)
                    ):
                        raise ValueError(f"{name}: non-integer scaling at {x},{y}")
            difference = ImageChops.difference(image, expected_output)
            if difference.getbbox() is not None:
                raise ValueError(
                    f"{name}: pixels do not exactly match the approved PSF/manifest raster"
                )
    print(
        f"checked {len(screens)} screenshots: approved PSF, exact glyph rows, "
        f"fixed palette, {OUTPUT_SIZE[0]}x{OUTPUT_SIZE[1]}, and 2x replication"
    )


if __name__ == "__main__":
    main()
