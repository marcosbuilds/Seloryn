"""Build the private Seloryn display face used by the native setup UI.

The readable alphabet comes from the OFL-licensed Sora family. The output is
renamed and kept app-only; its uppercase S is replaced by a continuous ribbon
silhouette derived from the Seloryn mark. Body copy remains a system UI font.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont


def ribbon_s(font: TTFont):
    pen = TTGlyphPen(font.getGlyphSet())
    quad = Cu2QuPen(pen, max_err=1.0, reverse_direction=False)
    quad.moveTo((790, 735))
    quad.curveTo((790, 775), (755, 800), (700, 800))
    quad.lineTo((260, 800))
    quad.curveTo((150, 800), (80, 735), (80, 640))
    quad.curveTo((80, 540), (160, 485), (265, 485))
    quad.lineTo((620, 485))
    quad.curveTo((675, 485), (710, 455), (710, 415))
    quad.curveTo((710, 375), (675, 345), (620, 345))
    quad.lineTo((240, 345))
    quad.curveTo((140, 345), (80, 290), (80, 205))
    quad.curveTo((80, 120), (145, 70), (250, 70))
    quad.lineTo((700, 70))
    quad.curveTo((755, 70), (790, 100), (790, 140))
    quad.curveTo((790, 180), (755, 210), (700, 210))
    quad.lineTo((255, 210))
    quad.curveTo((220, 210), (195, 225), (195, 250))
    quad.curveTo((195, 280), (220, 295), (265, 295))
    quad.lineTo((620, 295))
    quad.curveTo((740, 295), (820, 355), (820, 430))
    quad.curveTo((820, 510), (745, 575), (630, 575))
    quad.lineTo((275, 575))
    quad.curveTo((225, 575), (195, 600), (195, 635))
    quad.curveTo((195, 675), (225, 695), (270, 695))
    quad.lineTo((700, 695))
    quad.curveTo((755, 695), (790, 710), (790, 735))
    quad.closePath()
    return pen.glyph()


def set_name(font: TTFont, name_id: int, value: str) -> None:
    font["name"].setName(value, name_id, 3, 1, 0x409)
    font["name"].setName(value, name_id, 1, 0, 0)


def build(source: Path, output: Path) -> None:
    font = TTFont(source)
    if "fvar" in font:
        font = instantiateVariableFont(font, {"wght": 600}, inplace=False)
    cmap = font.getBestCmap()
    s_name = cmap[ord("S")]
    font["glyf"][s_name] = ribbon_s(font)
    advance, _ = font["hmtx"].metrics[s_name]
    font["hmtx"].metrics[s_name] = (max(advance, 900), 20)
    set_name(font, 1, "Seloryn Strand")
    set_name(font, 2, "Display")
    set_name(font, 4, "Seloryn Strand Display")
    set_name(font, 6, "Seloryn-StrandDisplay")
    set_name(font, 16, "Seloryn Strand")
    set_name(font, 17, "Display")
    set_name(font, 3, "Seloryn Strand Display 0.3")
    if "OS/2" in font:
        font["OS/2"].usWeightClass = 600
    output.parent.mkdir(parents=True, exist_ok=True)
    font.save(output)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--source",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "assets" / "Sora-wght.ttf",
    )
    args = parser.parse_args()
    build(args.source, args.output)


if __name__ == "__main__":
    main()
