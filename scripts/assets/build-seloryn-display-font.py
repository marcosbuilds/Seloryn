"""Build the private Seloryn display face used by the native setup UI.

The source font provides the Unicode scaffolding, while the visible display
alphabet is rebuilt as a Seloryn-specific rounded monoline system. The S is
the exact two-strand construction derived from the official SVG mark; the
remaining glyphs use the same stroke weight, cap treatment, proportions, and
spacing grammar. Body copy remains a system UI font.
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path

from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.transformPen import TransformPen
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
        return (460 + (base_x - 460) * 1.6, 465 + (base_y - 465) * 1.6)

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


def ellipse_points(
    cx: float,
    cy: float,
    rx: float,
    ry: float,
    start: float = 0,
    end: float = math.tau,
    count: int = 28,
) -> list[tuple[float, float]]:
    return [
        (
            cx + math.cos(start + (end - start) * index / count) * rx,
            cy + math.sin(start + (end - start) * index / count) * ry,
        )
        for index in range(count + 1)
    ]


def draw_outline(
    pen: TTGlyphPen,
    centerline: list[tuple[float, float]],
    width: float = 96,
    closed: bool = False,
) -> None:
    """Draw one smooth, rounded monoline stroke as one clean contour."""
    if closed and centerline[-1] == centerline[0]:
        centerline = centerline[:-1]
    radius = width / 2
    left = []
    right = []
    count = len(centerline)
    for index, point in enumerate(centerline):
        if closed:
            previous = centerline[(index - 1) % count]
            following = centerline[(index + 1) % count]
        else:
            previous = centerline[max(0, index - 1)]
            following = centerline[min(count - 1, index + 1)]
        dx = following[0] - previous[0]
        dy = following[1] - previous[1]
        length = max(math.hypot(dx, dy), 1)
        nx = -dy / length * radius
        ny = dx / length * radius
        left.append((point[0] + nx, point[1] + ny))
        right.append((point[0] - nx, point[1] - ny))

    polygon = []
    if closed:
        polygon.extend(left)
        polygon.extend(reversed(right))
    else:
        def cap(center, start_angle, end_angle):
            return [
                (
                    center[0] + math.cos(start_angle + (end_angle - start_angle) * index / 8) * radius,
                    center[1] + math.sin(start_angle + (end_angle - start_angle) * index / 8) * radius,
                )
                for index in range(9)
            ]

        first_angle = math.atan2(
            -(centerline[1][1] - centerline[0][1]),
            -(centerline[1][0] - centerline[0][0]),
        )
        last_angle = math.atan2(
            -(centerline[-1][1] - centerline[-2][1]),
            -(centerline[-1][0] - centerline[-2][0]),
        )
        polygon.extend(cap(centerline[0], first_angle - math.pi / 2, first_angle + math.pi / 2))
        polygon.extend(right[1:])
        polygon.extend(cap(centerline[-1], last_angle + math.pi / 2, last_angle - math.pi / 2))
        polygon.extend(reversed(left[:-1]))

    pen.moveTo(polygon[0])
    for point in polygon[1:]:
        pen.lineTo(point)
    pen.closePath()


def custom_upper(font: TTFont, character: str):
    """Generate the uppercase display alphabet from the same monoline grammar."""
    pen = TTGlyphPen(font.getGlyphSet())
    left, right, top, bottom, mid = 140, 760, 830, 150, 490
    strokes: list[tuple[list[tuple[float, float]], bool]] = []
    if character == "A":
        strokes = [([(left, bottom), (450, top), (right, bottom)], False), ([(270, 430), (630, 430)], False)]
    elif character == "B":
        strokes = [
            ([(left, bottom), (left, top)], False),
            ([(left, top), (560, top), (690, 740), (690, 585), (560, mid), (left, mid)], False),
            ([(left, mid), (560, mid), (690, 395), (690, 240), (560, bottom), (left, bottom)], False),
        ]
    elif character == "C":
        strokes = [(ellipse_points(450, mid, 310, 340, math.radians(50), math.radians(310), 24), False)]
    elif character == "D":
        strokes = [
            ([(left, bottom), (left, top)], False),
            ([(left, top), (530, top), (690, 700), (690, 280), (530, bottom), (left, bottom)], False),
        ]
    elif character == "E":
        strokes = [([(left, bottom), (left, top)], False), ([(left, top), (right, top)], False), ([(left, mid), (610, mid)], False), ([(left, bottom), (right, bottom)], False)]
    elif character == "F":
        strokes = [([(left, bottom), (left, top)], False), ([(left, top), (right, top)], False), ([(left, mid), (610, mid)], False)]
    elif character == "G":
        strokes = [(ellipse_points(450, mid, 310, 340, math.radians(50), math.radians(310), 24), False), ([(450, mid), (right, mid)], False), ([(right, mid), (right, 300)], False)]
    elif character == "H":
        strokes = [([(left, bottom), (left, top)], False), ([(right, bottom), (right, top)], False), ([(left, mid), (right, mid)], False)]
    elif character == "I":
        strokes = [([(left, top), (right, top)], False), ([(450, bottom), (450, top)], False), ([(left, bottom), (right, bottom)], False)]
    elif character == "J":
        strokes = [([(left, top), (right, top)], False), ([(right, top), (right, 260)], False), (ellipse_points(450, 260, 310, 110, math.radians(180), math.radians(360), 16), False)]
    elif character == "K":
        strokes = [([(left, bottom), (left, top)], False), ([(left, mid), (right, top)], False), ([(left, mid), (right, bottom)], False)]
    elif character == "L":
        strokes = [([(left, top), (left, bottom)], False), ([(left, bottom), (right, bottom)], False)]
    elif character == "M":
        strokes = [([(left, bottom), (left, top), (450, 500), (right, top), (right, bottom)], False)]
    elif character == "N":
        strokes = [([(left, bottom), (left, top), (right, bottom), (right, top)], False)]
    elif character == "O":
        strokes = [(ellipse_points(450, mid, 310, 340, count=32), True)]
    elif character == "P":
        strokes = [([(left, bottom), (left, top)], False), ([(left, top), (560, top), (690, 740), (690, 585), (560, mid), (left, mid)], False)]
    elif character == "Q":
        strokes = [(ellipse_points(430, 520, 290, 320, count=32), True), ([(520, 270), (right, bottom)], False)]
    elif character == "R":
        strokes = [([(left, bottom), (left, top)], False), ([(left, top), (560, top), (690, 740), (690, 585), (560, mid), (left, mid)], False), ([(450, mid), (right, bottom)], False)]
    elif character == "T":
        strokes = [([(left, top), (right, top)], False), ([(450, bottom), (450, top)], False)]
    elif character == "U":
        strokes = [([(left, top), (left, 270)], False), (ellipse_points(450, 270, 310, 120, math.radians(180), math.radians(360), 18), False), ([(right, 270), (right, top)], False)]
    elif character == "V":
        strokes = [([(left, top), (450, bottom), (right, top)], False)]
    elif character == "W":
        strokes = [([(left, top), (270, bottom), (450, 500), (630, bottom), (right, top)], False)]
    elif character == "X":
        strokes = [([(left, top), (right, bottom)], False), ([(right, top), (left, bottom)], False)]
    elif character == "Y":
        strokes = [([(left, top), (450, mid)], False), ([(right, top), (450, mid)], False), ([(450, bottom), (450, mid)], False)]
    elif character == "Z":
        strokes = [([(left, top), (right, top), (left, bottom), (right, bottom)], False)]
    elif character in "0123456789":
        digits = {
            "0": (ellipse_points(450, mid, 300, 340, count=32), True),
            "1": ([(300, 680), (450, top), (450, bottom)], False),
            "2": ([(left, 700), (260, top), (620, top), (right, 700), (left, bottom), (right, bottom)], False),
            "3": ([(left, top), (right, top), (600, mid), (right, bottom), (left, bottom)], False),
            "4": ([(left, mid), (610, mid), (610, bottom), (610, top), (left, mid)], False),
            "5": ([(right, top), (left, top), (left, mid), (right, mid), (right, bottom), (left, bottom)], False),
            "6": ([(right, top), (left, mid), (left, bottom), (right, bottom), (right, mid), (left, mid)], False),
            "7": ([(left, top), (right, top), (300, bottom)], False),
            "8": ([(left, mid), (left, top), (right, top), (right, mid), (left, mid), (left, bottom), (right, bottom), (right, mid)], False),
            "9": ([(left, bottom), (right, mid), (right, top), (left, top), (left, mid), (right, mid)], False),
        }
        strokes = [digits[character]]
    for centerline, closed in strokes:
        draw_outline(pen, centerline, closed=closed)
    return pen.glyph()


def custom_lower(font: TTFont, character: str):
    """Generate the lowercase alphabet with a measured Seloryn x-height."""
    pen = TTGlyphPen(font.getGlyphSet())
    left, right, top, bottom, mid = 150, 750, 570, 150, 360
    asc, desc = 830, -80
    strokes: list[tuple[list[tuple[float, float]], bool]] = []
    if character == "a":
        strokes = [
            (ellipse_points(420, mid, 245, 205, count=28), True),
            ([(665, bottom), (665, top)], False),
        ]
    elif character == "b":
        strokes = [
            ([(190, bottom), (190, asc)], False),
            (ellipse_points(430, mid, 250, 205, count=28), True),
        ]
    elif character == "c":
        strokes = [(ellipse_points(440, mid, 285, 205, math.radians(48), math.radians(312), 24), False)]
    elif character == "d":
        strokes = [
            (ellipse_points(430, mid, 250, 205, count=28), True),
            ([(690, bottom), (690, asc)], False),
        ]
    elif character == "e":
        strokes = [
            (ellipse_points(430, mid, 255, 205, count=28), True),
            ([(190, mid), (670, mid)], False),
        ]
    elif character == "f":
        strokes = [
            ([(420, bottom), (420, 700), (470, asc), (560, asc), (620, 760)], False),
            ([(230, top), (610, top)], False),
        ]
    elif character == "g":
        strokes = [
            (ellipse_points(410, mid, 240, 205, count=28), True),
            ([(650, 370), (650, 10), (570, desc), (410, desc), (330, 10)], False),
        ]
    elif character == "h":
        strokes = [
            ([(180, bottom), (180, asc)], False),
            ([(180, 470), (300, top), (470, top), (650, 470), (650, bottom)], False),
        ]
    elif character == "i":
        strokes = [([(430, bottom), (430, top)], False), (ellipse_points(430, 735, 48, 48, count=18), True)]
    elif character == "j":
        strokes = [
            ([(520, top), (520, desc), (440, desc), (350, 10)], False),
            (ellipse_points(520, 735, 48, 48, count=18), True),
        ]
    elif character == "k":
        strokes = [
            ([(190, bottom), (190, asc)], False),
            ([(190, 350), (720, top)], False),
            ([(190, 350), (720, bottom)], False),
        ]
    elif character == "l":
        strokes = [([(410, bottom), (410, asc)], False)]
    elif character == "m":
        strokes = [
            ([(150, bottom), (150, top), (260, top), (350, 470), (350, bottom)], False),
            ([(350, top), (460, top), (550, 470), (550, bottom)], False),
            ([(550, top), (660, top), (750, 470), (750, bottom)], False),
        ]
    elif character == "n":
        strokes = [
            ([(170, bottom), (170, top), (300, top), (470, 470), (650, top), (650, bottom)], False),
        ]
    elif character == "o":
        strokes = [(ellipse_points(450, mid, 285, 205, count=32), True)]
    elif character == "p":
        strokes = [
            ([(190, desc), (190, top)], False),
            (ellipse_points(430, mid, 250, 205, count=28), True),
        ]
    elif character == "q":
        strokes = [
            (ellipse_points(430, mid, 250, 205, count=28), True),
            ([(690, desc), (690, top)], False),
        ]
    elif character == "r":
        strokes = [
            ([(190, bottom), (190, top), (320, top), (470, 470)], False),
        ]
    elif character == "t":
        strokes = [
            ([(470, asc), (470, bottom), (550, bottom)], False),
            ([(250, top), (650, top)], False),
        ]
    elif character == "u":
        strokes = [
            ([(170, top), (170, 250)], False),
            (ellipse_points(460, 250, 290, 110, math.radians(180), math.radians(360), 20), False),
            ([(750, 250), (750, top)], False),
        ]
    elif character == "v":
        strokes = [([(160, top), (450, bottom), (740, top)], False)]
    elif character == "w":
        strokes = [([(130, top), (280, bottom), (450, 390), (620, bottom), (770, top)], False)]
    elif character == "x":
        strokes = [([(170, top), (730, bottom)], False), ([(730, top), (170, bottom)], False)]
    elif character == "y":
        strokes = [
            ([(170, top), (450, 340), (730, top)], False),
            ([(450, 340), (570, desc)], False),
        ]
    elif character == "z":
        strokes = [([(170, top), (730, top), (170, bottom), (730, bottom)], False)]
    for centerline, closed in strokes:
        draw_outline(pen, centerline, width=82, closed=closed)
    return pen.glyph()


def custom_punctuation(font: TTFont, character: str):
    """Keep the small glyph set used by setup labels in the same visual voice."""
    pen = TTGlyphPen(font.getGlyphSet())
    width = 82
    strokes: list[tuple[list[tuple[float, float]], bool]] = []
    if character == ".":
        strokes = [(ellipse_points(450, 150, 42, 42, count=18), True)]
    elif character == ",":
        strokes = [([(450, 170), (390, 50)], False)]
    elif character == ":":
        strokes = [
            (ellipse_points(450, 500, 38, 38, count=18), True),
            (ellipse_points(450, 150, 38, 38, count=18), True),
        ]
    elif character == ";":
        strokes = [
            (ellipse_points(450, 500, 38, 38, count=18), True),
            ([(450, 170), (390, 50)], False),
        ]
    elif character == "!":
        strokes = [([(450, 760), (450, 250)], False), (ellipse_points(450, 150, 38, 38, count=18), True)]
    elif character == "?":
        strokes = [
            ([(220, 650), (270, 760), (550, 790), (680, 650), (610, 520), (450, 430)], False),
            ([(450, 220), (450, 170)], False),
        ]
    elif character in "+=":
        y_values = (360, 200) if character == "=" else (360,)
        strokes = [([(210, y), (690, y)], False) for y in y_values]
        if character == "+":
            strokes.append(([(450, 120), (450, 600)], False))
    elif character in "-_":
        y = 360 if character == "-" else 100
        strokes = [([(160, y), (740, y)], False)]
    elif character in "/\\":
        points = ([(220, 80), (680, 790)] if character == "/" else [(680, 80), (220, 790)])
        strokes = [(points, False)]
    elif character == "|":
        strokes = [([(450, 80), (450, 790)], False)]
    elif character in "()":
        angle_start, angle_end = ((-math.pi / 2, math.pi / 2) if character == ")" else (math.pi / 2, 3 * math.pi / 2))
        strokes = [(ellipse_points(450, 430, 210, 350, angle_start, angle_end, 24), False)]
    elif character in "[]":
        x = 250 if character == "[" else 650
        direction = 1 if character == "[" else -1
        strokes = [([(x + direction * 100, 790), (x, 790), (x, 80), (x + direction * 100, 80)], False)]
    elif character in "{}":
        x = 450
        direction = 1 if character == "}" else -1
        strokes = [([(x + direction * 120, 790), (x + direction * 40, 790), (x, 680), (x, 500), (x + direction * 40, 430), (x, 360), (x, 180), (x + direction * 40, 80), (x + direction * 120, 80)], False)]
    else:
        return None
    for centerline, closed in strokes:
        draw_outline(pen, centerline, width=width, closed=closed)
    return pen.glyph()


def set_name(font: TTFont, name_id: int, value: str) -> None:
    font["name"].setName(value, name_id, 3, 1, 0x409)
    font["name"].setName(value, name_id, 1, 0, 0)


def build(source: Path, output: Path) -> None:
    font = TTFont(source)
    if "fvar" in font:
        font = instantiateVariableFont(font, {"wght": 600}, inplace=False)
    cmap = font.getBestCmap()
    for character in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789":
        glyph_name = cmap[ord(character)]
        font["glyf"][glyph_name] = (
            ribbon_s(font) if character == "S" else custom_upper(font, character)
        )
        advance = 520 if character == "I" else 760 if character == "J" else 900
        font["hmtx"].metrics[glyph_name] = (advance, 20)

    for character in "abcdefghijklmnopqrstuvwxyz":
        glyph_name = cmap[ord(character)]
        if character != "s":
            font["glyf"][glyph_name] = custom_lower(font, character)
        font["hmtx"].metrics[glyph_name] = (760 if character in "mw" else 680, 20)

    s_name = cmap[ord("S")]
    lower_s_name = cmap[ord("s")]
    lower_pen = TTGlyphPen(font.getGlyphSet())
    lower_transform = TransformPen(lower_pen, (0.68, 0, 0, 0.68, 110, 80))
    font.getGlyphSet()[s_name].draw(lower_transform)
    font["glyf"][lower_s_name] = lower_pen.glyph()
    font["hmtx"].metrics[lower_s_name] = (680, 20)

    for character in ".,:;!?+-=_/\\|()[]{}":
        glyph_name = cmap.get(ord(character))
        if glyph_name is None:
            continue
        glyph = custom_punctuation(font, character)
        if glyph is not None:
            font["glyf"][glyph_name] = glyph
            font["hmtx"].metrics[glyph_name] = (420 if character in ".,:;!" else 680, 20)

    # Layout substitutions/kerning from the source family do not describe the
    # rebuilt glyphs. Removing them keeps the private display face deterministic
    # and prevents inherited ligatures from changing setup labels unexpectedly.
    for table in ("GDEF", "GPOS", "GSUB", "STAT"):
        if table in font:
            del font[table]
    set_name(font, 1, "Seloryn Strand")
    set_name(font, 2, "Display")
    set_name(font, 4, "Seloryn Strand Display")
    set_name(font, 6, "Seloryn-StrandDisplay")
    set_name(font, 16, "Seloryn Strand")
    set_name(font, 17, "Display")
    set_name(font, 3, "Seloryn Strand Display 0.3")
    if "OS/2" in font:
        font["OS/2"].usWeightClass = 600
    if "head" in font:
        # OpenType timestamps are seconds since 1904; keep the generated face
        # reproducible without triggering fontTools' low-timestamp fallback.
        font["head"].modified = 2082840000
    font.recalcTimestamp = False
    output.parent.mkdir(parents=True, exist_ok=True)
    font.save(output)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--source",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "assets" / "fonts" / "sora-variable.ttf",
    )
    args = parser.parse_args()
    build(args.source, args.output)


if __name__ == "__main__":
    main()
