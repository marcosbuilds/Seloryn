"""Build the first private Seloryn display face used by the native setup UI.

The face is intentionally small and scoped to the app. It is a geometric,
grid-derived display face rather than a replacement for the language's future
text typography. The generated TTF is committed so the installer build does
not need Python or font tooling.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen


PATTERNS: dict[str, tuple[str, ...]] = {
    "A": ("01110", "10001", "10001", "11111", "10001", "10001", "10001"),
    "B": ("11110", "10001", "10001", "11110", "10001", "10001", "11110"),
    "C": ("01111", "10000", "10000", "10000", "10000", "10000", "01111"),
    "D": ("11110", "10001", "10001", "10001", "10001", "10001", "11110"),
    "E": ("11111", "10000", "10000", "11110", "10000", "10000", "11111"),
    "F": ("11111", "10000", "10000", "11110", "10000", "10000", "10000"),
    "G": ("01111", "10000", "10000", "10111", "10001", "10001", "01111"),
    "H": ("10001", "10001", "10001", "11111", "10001", "10001", "10001"),
    "I": ("11111", "00100", "00100", "00100", "00100", "00100", "11111"),
    "J": ("00111", "00010", "00010", "00010", "10010", "10010", "01100"),
    "K": ("10001", "10010", "10100", "11000", "10100", "10010", "10001"),
    "L": ("10000", "10000", "10000", "10000", "10000", "10000", "11111"),
    "M": ("10001", "11011", "10101", "10101", "10001", "10001", "10001"),
    "N": ("10001", "11001", "11001", "10101", "10011", "10011", "10001"),
    "O": ("01110", "10001", "10001", "10001", "10001", "10001", "01110"),
    "P": ("11110", "10001", "10001", "11110", "10000", "10000", "10000"),
    "Q": ("01110", "10001", "10001", "10001", "10101", "10010", "01101"),
    "R": ("11110", "10001", "10001", "11110", "10100", "10010", "10001"),
    "S": ("01111", "10000", "10000", "01110", "00001", "00001", "11110"),
    "T": ("11111", "00100", "00100", "00100", "00100", "00100", "00100"),
    "U": ("10001", "10001", "10001", "10001", "10001", "10001", "01110"),
    "V": ("10001", "10001", "10001", "10001", "10001", "01010", "00100"),
    "W": ("10001", "10001", "10001", "10101", "10101", "11011", "10001"),
    "X": ("10001", "10001", "01010", "00100", "01010", "10001", "10001"),
    "Y": ("10001", "10001", "01010", "00100", "00100", "00100", "00100"),
    "Z": ("11111", "00001", "00010", "00100", "01000", "10000", "11111"),
    "0": ("01110", "10011", "10101", "10101", "10101", "11001", "01110"),
    "1": ("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
    "2": ("01110", "10001", "00001", "00010", "00100", "01000", "11111"),
    "3": ("11110", "00001", "00001", "01110", "00001", "00001", "11110"),
    "4": ("00010", "00110", "01010", "10010", "11111", "00010", "00010"),
    "5": ("11111", "10000", "10000", "11110", "00001", "00001", "11110"),
    "6": ("01110", "10000", "10000", "11110", "10001", "10001", "01110"),
    "7": ("11111", "00001", "00010", "00100", "01000", "01000", "01000"),
    "8": ("01110", "10001", "10001", "01110", "10001", "10001", "01110"),
    "9": ("01110", "10001", "10001", "01111", "00001", "00001", "01110"),
    "?": ("01110", "10001", "00001", "00010", "00100", "00100", "00100"),
}

CELL = 110
ADVANCE = 690
SIDE = 42


def rectangle(pen: TTGlyphPen, x: int, y: int, width: int, height: int) -> None:
    pen.moveTo((x, y))
    pen.lineTo((x + width, y))
    pen.lineTo((x + width, y + height))
    pen.lineTo((x, y + height))
    pen.closePath()


def glyph_for(character: str):
    pen = TTGlyphPen(None)
    if character == " ":
        return pen.glyph(), 360
    pattern = PATTERNS.get(character.upper(), PATTERNS["?"])
    for row, line in enumerate(pattern):
        y = (len(pattern) - row - 1) * CELL + 80
        for column, active in enumerate(line):
            if active == "1":
                rectangle(pen, SIDE + column * CELL, y, CELL - 14, CELL - 14)
    if character in ".,:;":
        pen = TTGlyphPen(None)
        rectangle(pen, 260, 80, 130, 130)
        if character in ":;":
            rectangle(pen, 260, 560, 130, 130)
    return pen.glyph(), ADVANCE


def build(output: Path) -> None:
    glyph_order = [".notdef", "space"]
    glyph_order.extend(f"uni{codepoint:04X}" for codepoint in range(33, 127))
    glyphs = {}
    metrics = {}
    glyphs[".notdef"], width = glyph_for("?")
    metrics[".notdef"] = (width, 0)
    glyphs["space"], width = glyph_for(" ")
    metrics["space"] = (width, 0)
    cmap = {}
    for codepoint in range(33, 127):
        glyph_name = f"uni{codepoint:04X}"
        glyphs[glyph_name], width = glyph_for(chr(codepoint))
        metrics[glyph_name] = (width, 0)
        cmap[codepoint] = glyph_name

    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(glyph_order)
    builder.setupCharacterMap(cmap)
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics(metrics)
    builder.setupHorizontalHeader(ascent=900, descent=-180)
    builder.setupOS2(
        usWeightClass=600,
        usWidthClass=5,
        sTypoAscender=900,
        sTypoDescender=-180,
        usWinAscent=900,
        usWinDescent=180,
    )
    builder.setupNameTable(
        {
            "familyName": "Seloryn Strand",
            "styleName": "Display",
            "uniqueFontIdentifier": "Seloryn Strand Display 0.1",
            "fullName": "Seloryn Strand Display",
            "psName": "Seloryn-StrandDisplay",
            "version": "Version 0.1",
        }
    )
    builder.setupPost(keepGlyphNames=False)
    output.parent.mkdir(parents=True, exist_ok=True)
    builder.save(output)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    build(args.output)


if __name__ == "__main__":
    main()
