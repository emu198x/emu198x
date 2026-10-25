#!/usr/bin/env python3
"""Write the ymfm oracle's test scripts to cases/.

Each script drives the two ADPCM units through one area of their behaviour.
The format is a line per command:

    rom SEED SIZE   fill SIZE bytes of sample ROM from a linear congruential
                    generator: state = state * 1103515245 + 12345 (32-bit),
                    byte = bits 16-23 of the new state
    a REG DATA      write DATA to ADPCM-A register REG ($00-$2F)
    b REG DATA      write DATA to ADPCM-B register REG ($0-$F)
    s N             run N sample steps

A sample step is one FM sample: ADPCM-A runs on every third step and
ADPCM-B on every step, then both are mixed. The runners print a hash of the
mixed output, ADPCM-A's end bits and ADPCM-B's status for every 1024 steps.

Run from this directory: python3 make_cases.py
"""

import pathlib
import random

ROM_SIZE = 0x200000


class Script:
    def __init__(self, seed):
        self.lines = [f"rom {seed} {ROM_SIZE:#x}"]

    def a(self, reg, data):
        self.lines.append(f"a {reg:#04x} {data:#04x}")

    def b(self, reg, data):
        self.lines.append(f"b {reg:#03x} {data:#04x}")

    def steps(self, n):
        self.lines.append(f"s {n}")

    def a_channel(self, ch, start, end, control=0xDF):
        self.a(0x08 + ch, control)
        self.a(0x10 + ch, start & 0xFF)
        self.a(0x18 + ch, start >> 8)
        self.a(0x20 + ch, end & 0xFF)
        self.a(0x28 + ch, end >> 8)

    def b_sample(self, start, end, delta_n, level=0xFF, pan=0xC0):
        self.b(0x1, pan)
        self.b(0x2, start & 0xFF)
        self.b(0x3, start >> 8)
        self.b(0x4, end & 0xFF)
        self.b(0x5, end >> 8)
        self.b(0x9, delta_n & 0xFF)
        self.b(0xA, delta_n >> 8)
        self.b(0xB, level)

    def text(self):
        return "\n".join(self.lines) + "\n"


def a_single():
    s = Script(1)
    s.a(0x01, 0x3F)
    s.a_channel(0, 0x0010, 0x0011)
    s.a(0x00, 0x01)
    s.steps(3 * 1100)
    return s


def a_all_channels():
    s = Script(2)
    s.a(0x01, 0x30)
    pans = [0xDF, 0x9A, 0x55, 0xC7, 0x1F, 0xF0]
    for ch in range(6):
        s.a_channel(ch, 0x0100 * (ch + 1), 0x0100 * (ch + 1) + ch, pans[ch])
    s.a(0x00, 0x3F)
    s.steps(2000)
    # Key two off, restart one mid-sample, change levels while playing.
    s.a(0x00, 0x80 | 0x06)
    s.a(0x01, 0x3F)
    s.steps(500)
    s.a(0x00, 0x02)
    s.a(0x0D, 0xC0)
    s.steps(9000)
    return s


def a_levels():
    s = Script(3)
    rng = random.Random(3)
    s.a_channel(0, 0x0040, 0x0050)
    s.a(0x00, 0x01)
    for _ in range(64):
        s.a(0x01, rng.randrange(0x40))
        s.a(0x08, rng.choice([0x00, 0x40, 0x80, 0xC0]) | rng.randrange(0x20))
        s.steps(60)
    return s


def a_end_wraps_at_20_bits():
    # The end compare uses 20 bits: start $10F000 stops at $10F300 because
    # the end register says $00F2.
    s = Script(4)
    s.a(0x01, 0x3F)
    s.a_channel(3, 0x10F0, 0x00F2)
    s.a(0x00, 0x08)
    s.steps(3 * 1600)
    return s


def b_basic():
    s = Script(5)
    s.b_sample(0x0020, 0x0021, 0x4000)
    s.b(0x0, 0x80)
    s.steps(2200)
    return s


def b_rates_and_levels():
    s = Script(6)
    rng = random.Random(6)
    s.b_sample(0x0300, 0x0340, 0x1234)
    s.b(0x0, 0x80)
    for _ in range(40):
        dn = rng.randrange(0x10000)
        s.b(0x9, dn & 0xFF)
        s.b(0xA, dn >> 8)
        s.b(0xB, rng.randrange(0x100))
        s.b(0x1, rng.choice([0x00, 0x40, 0x80, 0xC0]))
        s.steps(250)
    return s


def b_repeat():
    s = Script(7)
    s.b_sample(0x0050, 0x0050, 0xC000)
    s.b(0x0, 0x90)
    s.steps(4096)
    return s


def b_stop_reset_restart():
    s = Script(8)
    s.b_sample(0x0070, 0x0070, 0xFFFF)
    s.b(0x0, 0x80)
    s.steps(700)
    # Plays to its end, then stopping clears the end flag.
    s.b(0x0, 0x00)
    s.steps(10)
    s.b(0x0, 0x80)
    s.steps(100)
    # The reset bit while playing.
    s.b(0x0, 0x81)
    s.steps(50)
    s.b(0x0, 0x80)
    s.steps(300)
    return s


def mixed():
    s = Script(9)
    rng = random.Random(9)
    s.a(0x01, 0x38)
    for ch in range(6):
        start = rng.randrange(0x1F00)
        s.a_channel(ch, start, start + rng.randrange(4), 0xC0 | rng.randrange(0x20))
    s.b_sample(0x0800, 0x0806, 0x6000, 0xC0)
    s.a(0x00, 0x3F)
    s.b(0x0, 0x80)
    s.steps(6000)
    return s


CASES = {
    "a_single": a_single,
    "a_all_channels": a_all_channels,
    "a_levels": a_levels,
    "a_end_wraps_at_20_bits": a_end_wraps_at_20_bits,
    "b_basic": b_basic,
    "b_rates_and_levels": b_rates_and_levels,
    "b_repeat": b_repeat,
    "b_stop_reset_restart": b_stop_reset_restart,
    "mixed": mixed,
}


def main():
    here = pathlib.Path(__file__).resolve().parent
    out = here / "cases"
    out.mkdir(exist_ok=True)
    for name, make in CASES.items():
        (out / f"{name}.adpcm").write_text(make().text())


if __name__ == "__main__":
    main()
