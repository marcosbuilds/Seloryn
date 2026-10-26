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

    def bounds(character: str):
        pen = BoundsPen(glyph_set)
        glyph_set[cmap[ord(character)]].draw(pen)
        if pen.bounds is None:
            raise SystemExit(f"{character} has no outline")
        return pen.bounds

    for character in REQUIRED.replace(" ", ""):
        bounds(character)

    s_name = cmap[ord("S")]
    left, bottom, right, top = bounds("S")
    width = right - left
    height = top - bottom
    a_bounds = bounds("A")
    o_bounds = bounds("O")
    cap_height = ((a_bounds[3] - a_bounds[1]) + (o_bounds[3] - o_bounds[1])) / 2
    if width < 600 or height < 600:
        raise SystemExit(f"S bounds are too small: {(left, bottom, right, top)}")
    if not 0.96 <= height / cap_height <= 1.04:
        raise SystemExit(f"S cap-height mismatch: S={height:.0f}, caps={cap_height:.0f}")
    if not 0.75 <= width / font["hmtx"].metrics[s_name][0] <= 0.9:
        raise SystemExit("S sidebearings are not optically balanced")
    if font["hmtx"].metrics[s_name][0] < width:
        raise SystemExit("S advance width is narrower than its outline")

    lower_s_bounds = bounds("s")
    lower_s_height = lower_s_bounds[3] - lower_s_bounds[1]
    if not 0.62 <= lower_s_height / cap_height <= 0.76:
        raise SystemExit(f"lowercase s x-height mismatch: s={lower_s_height:.0f}, caps={cap_height:.0f}")

    glyph = font["glyf"][s_name]
    if glyph.numberOfContours != 2:
        raise SystemExit(f"S must retain two SVG-derived strand contours, got {glyph.numberOfContours}")

    print(
        f"Seloryn Strand Display OK: {len(cmap)} mapped codepoints; "
        f"S={width:.0f}x{height:.0f}, contours={glyph.numberOfContours}"
    )


if __name__ == "__main__":
    main()
