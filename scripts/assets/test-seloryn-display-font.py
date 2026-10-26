"""Validate the private Seloryn display face and its SVG-derived S glyph."""

from __future__ import annotations

import argparse
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.ttLib import TTFont


REQUIRED = (
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    "abcdefghijklmnopqrstuvwxyz"
    "0123456789"
    " .,;:!?-_/@#$%+=*()[]{}"
)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("font", type=Path)
    args = parser.parse_args()
    font = TTFont(args.font)
    cmap = font.getBestCmap()
    missing = sorted({character for character in REQUIRED if ord(character) not in cmap})
    if missing:
        raise SystemExit(f"missing glyphs: {''.join(missing)}")

    glyph_set = font.getGlyphSet()
    s_name = cmap[ord("S")]
    s_pen = BoundsPen(glyph_set)
    glyph_set[s_name].draw(s_pen)
    if s_pen.bounds is None:
        raise SystemExit("S has no outline")
    left, bottom, right, top = s_pen.bounds
    width = right - left
    height = top - bottom
    if width < 600 or height < 600:
        raise SystemExit(f"S bounds are too small: {s_pen.bounds}")
    if font["hmtx"].metrics[s_name][0] < width:
        raise SystemExit("S advance width is narrower than its outline")

    glyph = font["glyf"][s_name]
    if glyph.numberOfContours != 2:
        raise SystemExit(f"S must retain two SVG-derived strand contours, got {glyph.numberOfContours}")

    print(
        f"Seloryn Strand Display OK: {len(cmap)} mapped codepoints; "
        f"S={width:.0f}x{height:.0f}, contours={glyph.numberOfContours}"
    )


if __name__ == "__main__":
    main()
