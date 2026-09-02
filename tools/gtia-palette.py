#!/usr/bin/env python3
"""Generate the GTIA NTSC and PAL palettes from atari800's colour model.

GTIA's colour byte is a hue in the high nibble and one of sixteen
luminances in the low nibble. The OS and most software use only the even
luminances, but GTIA mode 9 shows all sixteen, so the emulator needs a
256-entry table rather than the 128-entry TIA-style one.

The values come from atari800's generators (`src/colours_ntsc.c` and
`src/colours_pal.c`, vendored in the umbrella tree at
`emulators/atari/atari800/`) at its "Standard" preset: hue 0, saturation 0,
contrast 0, brightness 0, CRT gamma 2.35, black level 16, white level 235,
GTIA colour delay 26.8 degrees (NTSC, matched to the colour names in the GTIA
datasheet) and 23.2 degrees (PAL). Both models take the luminance ladder
from the CGIA datasheet, place hues around the colour subcarrier, and
convert YIQ (NTSC) or averaged even/odd-line YUV (PAL) to sRGB through the
CRT's gamma. The tables are reproduced here to the byte; `--check` against
a dump of atari800's own output is how that was established.

Run from the repo root:

    python3 tools/gtia-palette.py --out crates/atari-gtia/src/palette.rs
    python3 tools/gtia-palette.py --check   # CI: the committed file matches
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

OUT = Path("crates/atari-gtia/src/palette.rs")

# NTSC luma multipliers from the CGIA datasheet, as atari800 carries them.
LUMA_MULT = [
    0.6941, 0.7091, 0.7241, 0.7401,
    0.7560, 0.7741, 0.7931, 0.8121,
    0.8260, 0.8470, 0.8700, 0.8930,
    0.9160, 0.9420, 0.9690, 1.0000,
]

# atari800's "Standard" preset.
HUE = 0.0
SATURATION = 0.0
CONTRAST = 0.0
BRIGHTNESS = 0.0
GAMMA = 2.35
BLACK_LEVEL = 16
WHITE_LEVEL = 235

NTSC_COLOR_DELAY = 26.8  # degrees between consecutive hues
PAL_COLOR_DELAY = 23.2

NTSC_COLORBURST_ANGLE = math.radians(303.0)

# YUV to RGB, as atari800's `YUV2RGB_matrix`.
YUV2RGB = (
    (1.0, 0.0, 1.13983),
    (1.0, -0.39465, -0.58060),
    (1.0, 2.03211, 0.0),
)


def luminance(lm: int) -> float:
    """Y for luminance nibble `lm`, scaled between the black and white levels."""
    y = (LUMA_MULT[lm] - LUMA_MULT[0]) / (LUMA_MULT[15] - LUMA_MULT[0])
    y *= CONTRAST * 0.5 + 1
    y += BRIGHTNESS * 0.5
    black = BLACK_LEVEL / 255.0
    white = WHITE_LEVEL / 255.0
    return y * (white - black) + black


def gamma_to_linear(c: float) -> float:
    return c**GAMMA if c >= 0.0 else c / 12.92


def linear_to_srgb(c: float) -> float:
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1.0 / 2.4) - 0.055


def to_rgb24(r: float, g: float, b: float) -> int:
    def channel(c: float) -> int:
        c = linear_to_srgb(gamma_to_linear(c))
        return min(255, max(0, int(c * 255)))

    return (channel(r) << 16) | (channel(g) << 8) | channel(b)


def ntsc() -> list[int]:
    """`COLOURS_NTSC_Update` at the standard preset."""
    start_angle = NTSC_COLORBURST_ANGLE + HUE * math.pi
    color_diff = math.radians(NTSC_COLOR_DELAY)
    table = []
    for cr in range(16):
        angle = start_angle + (cr - 1) * color_diff
        saturation = (SATURATION + 1) * 0.175 if cr else 0.0
        i = math.cos(angle) * saturation
        q = math.sin(angle) * saturation
        for lm in range(16):
            y = luminance(lm)
            r = y + 0.9563 * i + 0.6210 * q
            g = y - 0.2721 * i - 0.6474 * q
            b = y - 1.1070 * i + 1.7046 * q
            table.append(to_rgb24(r, g, b))
    return table


# Delay coefficients (add, mult) for hues $1-$F on even and odd lines.
PAL_EVEN = [
    (1, 5), (1, 6), (1, 7), (0, 0), (0, 1), (0, 2), (0, 4), (0, 5),
    (0, 6), (0, 7), (1, 1), (1, 2), (1, 3), (1, 4), (1, 5),
]
PAL_ODD = [
    (1, 1), (1, 0), (0, 7), (0, 6), (0, 5), (0, 4), (0, 2), (0, 1),
    (0, 0), (1, 7), (1, 5), (1, 4), (1, 3), (1, 2), (1, 1),
]
PAL_BASE_DEL = 0.421894970414201  # 1/4.43 MHz * base_del = ca. 95.2 ns
PAL_ADD_DEL = 0.446563064859117  # ca. 100.7 ns
PAL_COLOR_DISABLE_THRESHOLD = 0.05


def pal() -> list[int]:
    """`COLOURS_PAL_Update` at the standard preset."""
    del_adj = PAL_COLOR_DELAY / 360.0

    def delay(coeff: tuple[int, int]) -> float:
        add, mult = coeff
        return PAL_BASE_DEL + PAL_ADD_DEL * add + del_adj * mult

    even_burst_del = delay(PAL_EVEN[0])
    odd_burst_del = delay(PAL_ODD[0])
    subcarrier_del = (even_burst_del + odd_burst_del + HUE) / 2.0
    burst_diff = even_burst_del - odd_burst_del
    burst_diff -= math.floor(burst_diff)
    if 0.5 - PAL_COLOR_DISABLE_THRESHOLD < burst_diff < 0.5 + PAL_COLOR_DISABLE_THRESHOLD:
        saturation_mult = 0.0
    else:
        amplitude = math.sqrt(2.0 * math.cos(burst_diff * 2.0 * math.pi) + 2.0)
        saturation_mult = math.sqrt(2.0) / amplitude

    table = []
    for cr in range(16):
        u = v = 0.0
        if cr:
            even_del = delay(PAL_EVEN[cr - 1])
            odd_del = delay(PAL_ODD[cr - 1])
            even_angle = (0.5 - (even_del - subcarrier_del)) * 2.0 * math.pi
            odd_angle = (0.5 + (odd_del - subcarrier_del)) * 2.0 * math.pi
            saturation = (SATURATION + 1) * 0.175 * saturation_mult
            # The palette averages the even- and odd-line chroma.
            u = (math.cos(even_angle) + math.cos(odd_angle)) * saturation / 2.0
            v = (math.sin(even_angle) + math.sin(odd_angle)) * saturation / 2.0
        for lm in range(16):
            y = luminance(lm)
            r = YUV2RGB[0][0] * y + YUV2RGB[0][1] * u + YUV2RGB[0][2] * v
            g = YUV2RGB[1][0] * y + YUV2RGB[1][1] * u + YUV2RGB[1][2] * v
            b = YUV2RGB[2][0] * y + YUV2RGB[2][1] * u + YUV2RGB[2][2] * v
            table.append(to_rgb24(r, g, b))
    return table


def render_table(name: str, doc: str, table: list[int]) -> str:
    rows = []
    for hue in range(16):
        rows.append(f"    // Hue ${hue:X}")
        for half in range(2):
            entries = table[hue * 16 + half * 8 : hue * 16 + half * 8 + 8]
            rows.append("    " + " ".join(f"0xFF{c:06X}," for c in entries))
    body = "\n".join(rows)
    return f"""{doc}
#[rustfmt::skip]
#[allow(clippy::unreadable_literal)]
pub const {name}: [u32; 256] = [
{body}
];
"""


def render() -> str:
    ntsc_table = render_table(
        "NTSC_PALETTE",
        "/// NTSC palette: 16 hues by 16 luminances, indexed by the colour byte.",
        ntsc(),
    )
    pal_table = render_table(
        "PAL_PALETTE",
        "/// PAL palette: 16 hues by 16 luminances, indexed by the colour byte.",
        pal(),
    )
    return f"""//! GTIA colour palettes.
//!
//! Generated by `tools/gtia-palette.py` from atari800's colour model at its
//! standard preset. Do not edit by hand -- rerun the script.
//!
//! A GTIA colour byte is a hue in the high nibble and a luminance in the
//! low nibble; each table has 256 entries indexed by the byte itself, so
//! the sixteen luminances of GTIA mode 9 are all distinct. ARGB32 format:
//! `0xAARRGGBB` (alpha always `0xFF`).

{ntsc_table}
{pal_table}"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help=f"write here (default {OUT})")
    parser.add_argument("--check", action="store_true", help="compare with the committed file")
    parser.add_argument("--dump", action="store_true", help="print the raw RGB values, one per line")
    args = parser.parse_args()

    if args.dump:
        print("NTSC")
        print("\n".join(f"{c:06X}" for c in ntsc()))
        print("PAL")
        print("\n".join(f"{c:06X}" for c in pal()))
        return 0

    rendered = render()
    out = args.out or OUT
    if args.check:
        if out.read_text(encoding="utf-8") != rendered:
            print(f"{out} does not match tools/gtia-palette.py; rerun it", file=sys.stderr)
            return 1
        print(f"{out} matches its generator")
        return 0
    out.write_text(rendered, encoding="utf-8")
    print(f"256-entry NTSC and PAL palettes -> {out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
