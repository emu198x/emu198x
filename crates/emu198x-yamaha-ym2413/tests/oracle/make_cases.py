#!/usr/bin/env python3
"""Write the oracle's test scripts to cases/.

Each script drives the chip through one area of its behaviour, and every
script is written for both variants. The format is a line per command:

    variant ym2413|vrc7    which chip (resets it)
    w PORT DATA            write DATA to PORT (0 address, 1 data)
    c N                    run N internal clocks

`reg ADDR DATA` is shorthand for an address write, 3 internal clocks (the
12 input clocks the datasheet asks for), a data write and 21 internal clocks
(84 input clocks). Both runners expand it the same way.

Run from this directory: python3 make_cases.py
"""

import pathlib
import random

SAMPLE = 18


class Script:
    def __init__(self, variant):
        self.lines = [f"variant {variant}"]

    def reg(self, addr, data):
        self.lines.append(f"reg {addr:#04x} {data:#04x}")

    def raw(self, line):
        self.lines.append(line)

    def clocks(self, n):
        self.lines.append(f"c {n}")

    def samples(self, n):
        self.clocks(n * SAMPLE)

    def note(self, ch, block, fnum, inst=1, vol=0, key=True, sus=False):
        self.reg(0x30 + ch, (inst << 4) | vol)
        self.reg(0x10 + ch, fnum & 0xFF)
        self.reg(0x20 + ch, (int(sus) << 5) | (int(key) << 4) | (block << 1) | (fnum >> 8))

    def key_off(self, ch, block, fnum, sus=False):
        self.reg(0x20 + ch, (int(sus) << 5) | (block << 1) | (fnum >> 8))

    def user(self, regs):
        for addr, data in enumerate(regs):
            self.reg(addr, data)


def every_instrument(s):
    """Each ROM instrument in turn on channel 0, keyed on, held, released."""
    for inst in range(1, 16):
        s.note(0, 4, 0x1AC, inst=inst)
        s.samples(1500)
        s.key_off(0, 4, 0x1AC)
        s.samples(800)


def all_channels(s):
    """Nine channels at once, different instruments, notes and volumes."""
    for ch in range(9):
        s.note(ch, 2 + ch % 5, 0x100 + ch * 37, inst=1 + ch, vol=ch)
    s.samples(4000)
    for ch in range(9):
        s.key_off(ch, 2 + ch % 5, 0x100 + ch * 37)
    s.samples(2000)


def user_instrument(s):
    """The user instrument across its fields: multiple, KSR, AM, VIB, EG
    type, KSL, total level, rectification, feedback and the four rates."""
    patches = [
        [0x21, 0x21, 0x1A, 0x07, 0xF0, 0xF0, 0x00, 0x0F],
        [0xE3, 0x61, 0x8F, 0xDD, 0xA3, 0x84, 0x26, 0x47],
        [0x7F, 0x3C, 0x40, 0x1F, 0x55, 0x6B, 0x9C, 0x3A],
        [0x01, 0x8A, 0xC3, 0xC5, 0xFF, 0xFF, 0x0F, 0x0F],
        [0x40, 0x40, 0x00, 0x00, 0x11, 0x11, 0xFF, 0xFF],
    ]
    for regs in patches:
        s.user(regs)
        s.note(1, 3, 0x0F0, inst=0, vol=2)
        s.samples(1500)
        s.key_off(1, 3, 0x0F0)
        s.samples(1000)


def rhythm(s):
    """Rhythm mode: each drum alone, then all five, with their volumes and
    the frequencies of channels 6-8 that tune them."""
    s.reg(0x16, 0x20)
    s.reg(0x26, 0x05)
    s.reg(0x17, 0x50)
    s.reg(0x27, 0x05)
    s.reg(0x18, 0xC0)
    s.reg(0x28, 0x01)
    s.reg(0x36, 0x03)
    s.reg(0x37, 0x24)
    s.reg(0x38, 0x51)
    for bit in range(5):
        s.reg(0x0E, 0x20 | (1 << bit))
        s.samples(1200)
        s.reg(0x0E, 0x20)
        s.samples(400)
    s.reg(0x0E, 0x3F)
    s.samples(2000)
    s.reg(0x0E, 0x20)
    s.samples(800)
    # Melody on channels 0-5 alongside, then rhythm mode off again.
    for ch in range(6):
        s.note(ch, 4, 0x150 + ch * 20, inst=ch + 2)
    s.reg(0x0E, 0x35)
    s.samples(2000)
    s.reg(0x0E, 0x00)
    s.samples(1500)


def envelopes(s):
    """Sustain on and off, EG type, key-off during attack and decay,
    re-key during release, and a key-on with no gap."""
    s.user([0x01, 0x01, 0x00, 0x00, 0x52, 0x63, 0x34, 0x45])
    for sus in (False, True):
        s.note(0, 4, 0x120, inst=0, sus=sus)
        s.samples(60)
        s.key_off(0, 4, 0x120, sus=sus)
        s.samples(600)
        s.note(0, 4, 0x120, inst=0, sus=sus)
        s.samples(900)
        s.key_off(0, 4, 0x120, sus=sus)
        s.samples(30)
        s.note(0, 4, 0x120, inst=0, sus=sus)
        s.samples(500)
        s.key_off(0, 4, 0x120, sus=sus)
        s.samples(2500)
    # EG type 0 (percussive) against 1 (sustained).
    s.user([0x01, 0x21, 0x00, 0x00, 0xA4, 0xA4, 0x37, 0x37])
    s.note(2, 3, 0x180, inst=0)
    s.samples(2500)
    s.user([0x21, 0x01, 0x00, 0x00, 0xA4, 0xA4, 0x37, 0x37])
    s.samples(2500)
    s.key_off(2, 3, 0x180)
    s.samples(1500)


def rates_and_scaling(s):
    """Every attack and decay rate, with key-scaled rates across blocks."""
    for rate in range(16):
        s.user([0x11 if rate & 1 else 0x01, 0x11, 0x10, 0x00,
                (rate << 4) | rate, (rate << 4) | (15 - rate), 0x22, 0x23])
        block = rate % 8
        s.note(3, block, 0x0C0 + rate * 9, inst=0)
        s.samples(300)
        s.key_off(3, block, 0x0C0 + rate * 9)
        s.samples(200)


def lfo(s):
    """AM and vibrato on both operators, held long enough for the LFOs to
    cycle more than once."""
    s.user([0xC1, 0xC1, 0x10, 0x05, 0xF2, 0xF2, 0x02, 0x03])
    s.note(4, 4, 0x1C0, inst=0)
    s.samples(30000)
    s.key_off(4, 4, 0x1C0)
    s.samples(1000)


def key_scale_level(s):
    """KSL at all four settings across the blocks and F-numbers."""
    for ksl in range(4):
        s.user([0x01, 0x01, (ksl << 6) | 0x08, (ksl << 6), 0xF0, 0xF0, 0x00, 0x00])
        for block in range(8):
            s.note(5, block, 0x050 + block * 60, inst=0)
            s.samples(120)
        s.key_off(5, 7, 0x1A0)
        s.samples(100)


def feedback(s):
    """Feedback 0-7 with and without rectification."""
    for fb in range(8):
        for rect in (0x00, 0x18):
            s.user([0x01, 0x01, 0x00, rect | fb, 0xF0, 0xF0, 0x0F, 0x0F])
            s.note(6, 4, 0x160, inst=0)
            s.samples(400)
            s.key_off(6, 4, 0x160)
            s.samples(100)


def write_timing(s):
    """Writes without the datasheet's pauses, writes to unmapped and mirror
    addresses, a data write with no address and back-to-back address
    writes."""
    s.raw("w 0 0x30")
    s.raw("w 1 0x40")
    s.clocks(1)
    s.raw("w 0 0x10")
    s.clocks(2)
    s.raw("w 1 0x9C")
    s.clocks(5)
    s.raw("w 0 0x20")
    s.raw("w 1 0x18")
    s.clocks(40)
    s.samples(800)
    # Unmapped registers and the ghost addresses $19-$1F, $29-$2F, $39-$3F.
    for addr in (0x09, 0x0A, 0x0D, 0x19, 0x1F, 0x29, 0x2E, 0x39, 0x3F, 0x40, 0xC0):
        s.reg(addr, 0xFF)
        s.samples(50)
    # Each register at every slot position.
    for skew in range(18):
        s.raw("w 0 0x21")
        s.clocks(skew)
        s.raw("w 1 0x17")
        s.clocks(18)
        s.raw("w 0 0x21")
        s.clocks(3)
        s.raw("w 1 0x07")
        s.clocks(17 + skew)
    s.raw("w 1 0x55")
    s.clocks(30)
    s.samples(600)


def test_register(s):
    """The test register's bits: zero attenuation, LFO reset, phase hold,
    and the envelope timer's test input."""
    s.note(7, 4, 0x140, inst=3)
    s.samples(300)
    for bits in (0x01, 0x02, 0x04, 0x08, 0x0F, 0x00):
        s.reg(0x0F, bits)
        s.samples(400)


def slow_envelopes(s):
    """The slowest rates, held long enough for the envelope timer to carry
    into its high bits: the rate-1 to rate-3 steps only show their full
    cadence once the timer has rolled past bit 13, after about 65,000
    samples. Key-scaled rates with odd low bits make that cadence visible:
    a rate-1 step depends on the timer's high bits only when the rate's low
    bits are odd. Sustain level 15 keeps the decay running throughout."""
    s.user([0x11, 0x11, 0x00, 0x00, 0xF1, 0xF1, 0xF1, 0xF1])
    s.note(0, 1, 0x1A0, inst=0)
    s.note(1, 0, 0x120, inst=0)
    s.note(2, 3, 0x0A0, inst=0)
    s.samples(150000)
    s.key_off(0, 1, 0x1A0)
    s.key_off(1, 0, 0x120)
    s.key_off(2, 3, 0x0A0)
    s.samples(80000)


def fuzz(s, seed):
    """Random writes to the whole register map at random intervals."""
    rng = random.Random(seed)
    s.reg(0x0E, 0x20)
    for _ in range(700):
        addr = rng.choice(
            list(range(0x00, 0x08)) + [0x0E]
            + list(range(0x10, 0x19)) + list(range(0x20, 0x29)) + list(range(0x30, 0x39))
        )
        if addr == 0x0E:
            data = rng.randrange(0x40)
        else:
            data = rng.randrange(0x100)
        s.reg(addr, data)
        s.clocks(rng.randrange(0, 600))


CASES = {
    "every_instrument": every_instrument,
    "all_channels": all_channels,
    "user_instrument": user_instrument,
    "rhythm": rhythm,
    "envelopes": envelopes,
    "rates_and_scaling": rates_and_scaling,
    "lfo": lfo,
    "key_scale_level": key_scale_level,
    "feedback": feedback,
    "write_timing": write_timing,
    "test_register": test_register,
    "slow_envelopes": slow_envelopes,
    "fuzz_a": lambda s: fuzz(s, 1),
    "fuzz_b": lambda s: fuzz(s, 2),
}


def main():
    out = pathlib.Path(__file__).parent / "cases"
    out.mkdir(exist_ok=True)
    for old in out.glob("*.opll"):
        old.unlink()
    for variant in ("ym2413", "vrc7"):
        for name, build in CASES.items():
            s = Script(variant)
            build(s)
            (out / f"{name}_{variant}.opll").write_text("\n".join(s.lines) + "\n")


if __name__ == "__main__":
    main()
