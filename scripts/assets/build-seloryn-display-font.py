"""Build the private Seloryn display face used by the native setup UI.

The readable alphabet comes from the OFL-licensed Sora family. The output is
renamed and kept app-only; its uppercase S is replaced by a continuous ribbon
silhouette derived from the Seloryn mark. Body copy remains a system UI font.
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path

from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont


def ribbon_s(font: TTFont):
    pen = TTGlyphPen(font.getGlyphSet())

    def transform(point: tuple[float, float]) -> tuple[float, float]:
        x, y = point
        base_x = 80 + x / 160 * 760
        base_y = 850 - y / 160 * 760
        # The mark occupies only the middle 55% of its square SVG viewBox.
        # Scale that exact two-strand geometry into a normal cap-height while
        # preserving its aspect ratio and generous rounded side bearings.
        return (460 + (base_x - 460) * 1.3, 465 + (base_y - 465) * 1.3)

    def line(start: tuple[float, float], end: tuple[float, float], count: int = 8):
        return [
            (
                start[0] + (end[0] - start[0]) * index / count,
                start[1] + (end[1] - start[1]) * index / count,
            )
            for index in range(1, count + 1)
        ]

    def cubic(
        start: tuple[float, float],
        control_one: tuple[float, float],
        control_two: tuple[float, float],
        end: tuple[float, float],
        count: int = 20,
    ):
        result = []
        for index in range(1, count + 1):
            t = index / count
            u = 1 - t
            result.append(
                (
                    u**3 * start[0]
                    + 3 * u**2 * t * control_one[0]
                    + 3 * u * t**2 * control_two[0]
                    + t**3 * end[0],
                    u**3 * start[1]
                    + 3 * u**2 * t * control_one[1]
                    + 3 * u * t**2 * control_two[1]
                    + t**3 * end[1],
                )
            )
        return result

    def outline(centerline: list[tuple[float, float]], width: float = 100) -> None:
        radius = width / 2
        left = []
        right = []
        for index, point in enumerate(centerline):
            previous = centerline[max(0, index - 1)]
            following = centerline[min(len(centerline) - 1, index + 1)]
            dx = following[0] - previous[0]
            dy = following[1] - previous[1]
            length = max(math.hypot(dx, dy), 1)
            nx = -dy / length * radius
            ny = dx / length * radius
            left.append((point[0] + nx, point[1] + ny))
            right.append((point[0] - nx, point[1] - ny))

        def cap(center, start_angle, end_angle):
            return [
                (
                    center[0] + math.cos(start_angle + (end_angle - start_angle) * index / 8) * radius,
                    center[1] + math.sin(start_angle + (end_angle - start_angle) * index / 8) * radius,
                )
                for index in range(9)
            ]

        first_dx = centerline[1][0] - centerline[0][0]
        first_dy = centerline[1][1] - centerline[0][1]
        first_angle = math.atan2(-first_dy, -first_dx)
        last_dx = centerline[-1][0] - centerline[-2][0]
        last_dy = centerline[-1][1] - centerline[-2][1]
        last_angle = math.atan2(-last_dy, -last_dx)
        polygon = cap(centerline[0], first_angle - math.pi / 2, first_angle + math.pi / 2)
        polygon.extend(right[1:])
        polygon.extend(cap(centerline[-1], last_angle + math.pi / 2, last_angle - math.pi / 2))
        polygon.extend(reversed(left[:-1]))
        quad = Cu2QuPen(pen, max_err=1.0, reverse_direction=False)
        quad.moveTo(polygon[0])
        for point in polygon[1:]:
            quad.lineTo(point)
        quad.closePath()

    # These are the two actual SVG centerlines, normalized into the font's em.
    top = [(120, 37), (69, 37)]
    top += cubic((69, 37), (46, 37), (35, 49), (35, 66))
    top += cubic((35, 66), (35, 83), (47, 91), (69, 91))
    top += line((69, 91), (97, 91))
    bottom = [(40, 125), (91, 125)]
    bottom += cubic((91, 125), (115, 125), (126, 113), (126, 95))
    bottom += cubic((126, 95), (126, 78), (114, 69), (92, 69))
    bottom += line((92, 69), (63, 69))
    outline([transform(point) for point in top])
    outline([transform(point) for point in bottom])
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
