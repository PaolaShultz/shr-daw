#!/usr/bin/env python3
"""Render 40x20 README screenshots with the same grid as the TUI."""

from __future__ import annotations

import gzip
import struct
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "images"
FONT_CANDIDATES = (
    Path("/usr/share/consolefonts/Lat15-VGA16.psf.gz"),
    ROOT / "target" / "Lat15-VGA16.psf",
)

CELL_W, CELL_H = 12, 16
COLS, ROWS = 40, 20
W, H = COLS * CELL_W, ROWS * CELL_H
OUTPUT_SCALE = 2

PALETTE = {
    "Reset": (170, 170, 170),
    "Black": (0, 0, 0),
    "Red": (170, 0, 0),
    "Green": (0, 170, 0),
    "Yellow": (170, 85, 0),
    "Blue": (0, 0, 170),
    "Magenta": (170, 0, 170),
    "Cyan": (0, 170, 170),
    "Gray": (170, 170, 170),
    "DarkGray": (85, 85, 85),
    "LightRed": (255, 85, 85),
    "LightGreen": (85, 255, 85),
    "LightYellow": (255, 255, 85),
    "LightBlue": (85, 85, 255),
    "LightMagenta": (255, 85, 255),
    "LightCyan": (85, 255, 255),
    "White": (255, 255, 255),
}
BRIGHT = {
    "Black": "DarkGray",
    "Red": "LightRed",
    "Green": "LightGreen",
    "Yellow": "LightYellow",
    "Blue": "LightBlue",
    "Magenta": "LightMagenta",
    "Cyan": "LightCyan",
    "Gray": "White",
}


def read_font(path: Path) -> bytes:
    raw = path.read_bytes()
    return gzip.decompress(raw) if path.suffix == ".gz" else raw


def load_psf1() -> tuple[list[bytes], dict[int, int]]:
    for path in FONT_CANDIDATES:
        if path.exists():
            raw = read_font(path)
            break
    else:
        candidates = ", ".join(str(path) for path in FONT_CANDIDATES)
        raise FileNotFoundError(f"missing console font; tried {candidates}")

    if raw[:2] != b"\x36\x04":
        raise ValueError("expected a PSF1 console font")
    mode, charsize = raw[2], raw[3]
    glyph_count = 512 if mode & 1 else 256
    glyphs = [
        raw[4 + i * charsize : 4 + (i + 1) * charsize]
        for i in range(glyph_count)
    ]
    mapping: dict[int, int] = {}
    pos = 4 + glyph_count * charsize
    for glyph_index in range(glyph_count):
        while pos + 2 <= len(raw):
            value = struct.unpack_from("<H", raw, pos)[0]
            pos += 2
            if value == 0xFFFF:
                break
            if value != 0xFFFE:
                mapping.setdefault(value, glyph_index)
    return glyphs, mapping


def color(name: str, *, bold: bool = False, background: bool = False) -> tuple[int, int, int]:
    if name == "Reset":
        return PALETTE["Black"] if background else PALETTE["Gray"]
    if bold and not background:
        name = BRIGHT.get(name, name)
    return PALETTE[name]


def fit(line: str) -> str:
    return line[:COLS].ljust(COLS)


def rows(*lines: str) -> list[str]:
    if len(lines) > 3 and lines[-3].startswith("["):
        body = list(lines[:-3])
        footer = list(lines[-3:])
        lines = tuple(body + [""] * max(0, ROWS - len(body) - len(footer)) + footer)
    value = [fit(line) for line in lines[:ROWS]]
    value.extend(" " * COLS for _ in range(ROWS - len(value)))
    return value


SCREENS = {
    "shr-daw-presets.png": rows(
        "synthv1 · Velvet Tines              LCK",
        "> 00  Velvet Tines",
        "  01  Hollow Brass",
        "  02  Soft Fifths",
        "  03  Juniper Lead",
        "  04  Dust Pad",
        "  05  Square Bass",
        "",
        "Sound engine: synthv1",
        "MIDI ready · pickup armed",
        "",
        "",
        "",
        "",
        "",
        "Ready",
        "[ LOAD  ][ ENGINE ][ NAV   ][ SYS   ]",
        "[LOAD]   [PG UP]  [PG DN]  [FIRST] ",
        " PRESETS P1 STOP IDLE        --- BPM",
    ),
    "shr-daw-playback.png": rows(
        "             synthv1 · Velvet Tines",
        "",
        "Held: C4 E4 G4",
        "Chord: C major",
        "",
        "Cut [green]      Res [yellow]",
        "Sus [red]        Rel [green]",
        "Mod [green]      Pan [yellow]",
        "",
        "recorded 48 MIDI events",
        "Playback to review",
        "",
        "",
        "",
        "",
        "",
        "[ OPS   ][ SOUND ][ NAV   ][ SYS   ]",
        "[RECORD][REC END][TAKE]  [SAVE]   ",
        " PLAYBACK P1 RUN PLAY        --- BPM",
    ),
    "shr-daw-ft2-pattern.png": rows(
        "MELODY · dusk-project EDIT",
        "ord 01/04 pat 00 · ONLINE",
        "ROW      L1       L2       L3       L4",
        ">00 C-4 60D E-4 58  G-4 5A  ... ..",
        " 01 ... ..  ... ..  ... ..  ... ..",
        " 02 C-5 70  ... ..  OFF ..  ... ..",
        " 03 ... ..  ... ..  ... ..  ... ..",
        " 04 D-4 62T F-4 50  A-4 55  ... ..",
        " 05 ... ..  ... ..  ... ..  ... ..",
        " 06 ... ..  ... ..  ... ..  ... ..",
        " 07 G-3 6A  ... ..  ... ..  ... ..",
        " 08 C-4 60  E-4 60  G-4 60  B-4 60",
        "",
        "P1/2 MELODY L1 ch1 Configured ON",
        "step edit on",
        "[ OPS   ][ MODE  ][ MOVE  ][ SYS   ]",
        "[PLAY]   [START]  [STEP]   [CELL]  ",
        " FT2 P1 STOP IDLE            120 BPM",
    ),
    "shr-daw-ft2-arrangement.png": rows(
        "FT2 ARRANGEMENT",
        "  4 steps",
        "> 01  pat 00  064 rows 120 BPM 4/4 2p",
        "  02  pat 01  032 rows 92 BPM  4/4 3p",
        "  03  pat 00  064 rows 120 BPM 4/4 2p",
        "  04  pat 02  024 rows 135 BPM 3/4 1p",
        "",
        "",
        "Repeat uses the same pattern ID.",
        "Clone or paste creates a new pattern.",
        "",
        "",
        "",
        "FT2 arrangement · chain pattern steps",
        "",
        "[ OPS   ][ STEP  ][       ][ SYS   ]",
        "[PLAY]   [JUMP]   [APPEND][INSERT]",
        " ARRANGE P1 STOP IDLE        120 BPM",
    ),
    "shr-daw-ft2-pages.png": rows(
        "FT2 PATTERN PAGES · 4 LANES",
        "  3 pages",
        ">01 MELODY   ch01 Configured",
        " 02 DRUMS    ch10 Configured",
        " 03 D-50     ch03 Roland D-50",
        "",
        "",
        "Page setup belongs to this pattern.",
        "Targets can be exact hardware ports.",
        "",
        "",
        "",
        "",
        "page route updated · DONE to keep",
        "",
        "[ OPS   ][ PAGE  ][       ][ SYS   ]",
        "[ADD]    [TARGET][CHANNEL][DONE]  ",
        " TRACKS P1 STOP IDLE        120 BPM",
    ),
    "shr-daw-project-files.png": rows(
        "PROJECT FILES",
        "  saved Projects · 5",
        "> dusk-project",
        "  sunday-sketch",
        "  mt240-drums",
        "  d50-pad-study",
        "  live-set-a",
        "",
        "Files save/load/delete the whole Project.",
        "Pattern tools stay on PATTERNS.",
        "",
        "",
        "",
        "Project files · select an action",
        "",
        "[ OPS   ][PATTERN][ EDIT  ][ SYS   ]",
        "[LOAD]   [SAVE]   [PREVIEW][DELETE]",
        " FILES P1 STOP IDLE         120 BPM",
    ),
    "shr-daw-ft2-loop.png": rows(
        "FT2 WAV LOOP",
        "breakbeat-96.wav",
        "",
        "Source  96.00 BPM  1x",
        "Target 120 BPM     ratio 1.250",
        "Region beat 0 +16",
        "Offset +0 bar(s)",
        "Cut BAR · meter 4/4",
        "",
        "PLAY  00:03 / 00:08",
        "48000 Hz · 2ch",
        "Pitch changes with tempo",
        "",
        "",
        "[ OPS   ][ BPM   ][ CUT   ][ SYS   ]",
        "[IMPORT][HERE]   [START]  [STOP]  ",
        " FT2 LOOP P1 STOP IDLE      120 BPM",
    ),
    "shr-daw-audio-recorder.png": rows(
        "             STEREO RECORDER",
        "",
        "AudioBox USB 96",
        "L system:capture_1",
        "R system:capture_2",
        "",
        "Time 00:02:14",
        "Rate 48000 Hz · 24-bit stereo",
        "Size 36.8 MiB",
        "Dropped 0",
        "",
        "recordings/dusk-project-001.wav",
        "R/REC start · STOP finalize",
        "",
        "[ OPS   ][       ][ NAV   ][ SYS   ]",
        "[RECORD]",
        " AUDIO P1 STOP IDLE          --- BPM",
    ),
}


def style_for(line: str, y: int, x: int, selected: bool) -> tuple[str, str, bool]:
    stripped = line.strip()
    if selected:
        return ("Yellow", "DarkGray", line[x] != " ")
    if y == 0 or stripped.startswith("FT2 ") or stripped.startswith("PROJECT"):
        return ("Green", "Reset", True)
    if y == 1 and ("ord" in stripped or "steps" in stripped or "pages" in stripped or "saved" in stripped):
        return ("Yellow", "Reset", False)
    if stripped.startswith("["):
        return ("Yellow", "Reset", True)
    if y >= ROWS - 2:
        return ("Gray", "Reset", False)
    if "OFFLINE" in stripped or "Dropped" in stripped:
        return ("Red", "Reset", False)
    if stripped.startswith("Project") or stripped.startswith("Pattern") or "belongs" in stripped:
        return ("Cyan", "Reset", False)
    if not stripped:
        return ("Gray", "Reset", False)
    return ("Gray", "Reset", False)


def render(name: str, content: list[str], glyphs: list[bytes], unicode_map: dict[int, int]) -> None:
    image = Image.new("RGB", (W, H), PALETTE["Black"])
    pixels = image.load()
    fallback = unicode_map.get(0x3F, 63)
    for y, raw_line in enumerate(content):
        selected = raw_line.startswith(">")
        line = fit(raw_line[1:] if selected else raw_line)
        for x, character in enumerate(line):
            fg_name, bg_name, bold = style_for(line, y, x, selected)
            glyph = glyphs[unicode_map.get(ord(character), fallback)]
            fg = color(fg_name, bold=bold)
            bg = color(bg_name, background=True)
            cell_x = x * CELL_W
            cell_y = y * CELL_H
            for gy in range(CELL_H):
                bits = glyph[gy] if gy < len(glyph) else 0
                for out_x in range(CELL_W):
                    source_x = out_x * 8 // CELL_W
                    pixels[cell_x + out_x, cell_y + gy] = (
                        fg if bits & (0x80 >> source_x) else bg
                    )
    output = integer_scale(image, OUTPUT_SCALE)
    output.save(OUT / name, optimize=True)


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
    OUT.mkdir(parents=True, exist_ok=True)
    glyphs, unicode_map = load_psf1()
    for name, content in SCREENS.items():
        render(name, content, glyphs, unicode_map)


if __name__ == "__main__":
    main()
