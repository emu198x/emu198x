#!/usr/bin/env python3
"""Write the oracle's test scripts to cases/.

Each script drives the chip through one area of its behaviour. The format is
a line per command:

    variant ym2612|ym3438   which chip
    readmode all|port0      whether every port reads the status
    w PORT DATA             write DATA to PORT (0-3)
    c N                     run N internal clocks
    r PORT                  read PORT and record the value
    pin 0|1                 drive the TEST pin

`reg PART ADDR DATA` is shorthand for an address write to port 2*PART, one
clock, a data write to port 2*PART+1 and 32 clocks for the busy flag to clear.
Both runners expand it the same way.

Run from this directory: python3 make_cases.py
"""

import pathlib
import random

OP_OFFSET = {1: 0x0, 2: 0x8, 3: 0x4, 4: 0xC}


def chan(ch):
    """(part, channel within part) for channel 0-5."""
    return ch // 3, ch % 3


class Script:
    def __init__(self):
        self.lines = []

    def reg(self, part, addr, data):
        self.lines.append(f"reg {part} {addr:#04x} {data:#04x}")

    def op(self, ch, op, base, data):
        part, c = chan(ch)
        self.reg(part, base + OP_OFFSET[op] + c, data)

    def ch(self, ch, base, data):
        part, c = chan(ch)
        self.reg(part, base + c, data)

    def freq(self, ch, block, fnum):
        self.ch(ch, 0xA4, (block << 3) | (fnum >> 8))
        self.ch(ch, 0xA0, fnum & 0xFF)

    def key(self, ch, ops=0xF):
        part, c = chan(ch)
        self.reg(0, 0x28, (ops << 4) | (part << 2) | c)

    def clocks(self, n):
        self.lines.append(f"c {n}")

    def read(self, port):
        self.lines.append(f"r {port}")

    def raw(self, line):
        self.lines.append(line)

    def patch(self, ch, alg, fb, ops):
        """ops: dict op -> (dt, mul, tl, ks, ar, am, dr, sr, sl, rr, ssg)."""
        self.ch(ch, 0xB0, (fb << 3) | alg)
        self.ch(ch, 0xB4, 0xC0)
        for op, (dt, mul, tl, ks, ar, am, dr, sr, sl, rr, ssg) in ops.items():
            self.op(ch, op, 0x30, (dt << 4) | mul)
            self.op(ch, op, 0x40, tl)
            self.op(ch, op, 0x50, (ks << 6) | ar)
            self.op(ch, op, 0x60, (am << 7) | dr)
            self.op(ch, op, 0x70, sr)
            self.op(ch, op, 0x80, (sl << 4) | rr)
            self.op(ch, op, 0x90, ssg)


def plain(tl, ar=31, dr=0, sr=0, sl=0, rr=15, dt=0, mul=1, ks=0, am=0, ssg=0):
    return (dt, mul, tl, ks, ar, am, dr, sr, sl, rr, ssg)


def silent():
    return plain(127)


def case_silence(s):
    s.clocks(20000)


def case_sine(s):
    s.patch(0, 7, 0, {1: silent(), 2: silent(), 3: silent(), 4: plain(0)})
    s.freq(0, 4, 0x269)
    s.key(0, 0x8)
    s.clocks(24000)
    s.key(0, 0x0)
    s.clocks(24000)


def case_algorithms(s):
    for alg in range(8):
        s.patch(0, alg, 5, {
            1: plain(24, mul=1),
            2: plain(30, mul=2, dt=3),
            3: plain(36, mul=3, dt=5),
            4: plain(8, mul=1),
        })
        s.freq(0, 4, 0x2A0)
        s.key(0)
        s.clocks(9000)
        s.key(0, 0)
        s.clocks(3000)


def case_envelopes(s):
    rng = random.Random(1)
    for ch in range(6):
        ops = {}
        for op in (1, 2, 3, 4):
            ops[op] = plain(
                tl=rng.randrange(0, 40),
                ar=rng.randrange(1, 32),
                dr=rng.randrange(0, 32),
                sr=rng.randrange(0, 32),
                sl=rng.randrange(0, 16),
                rr=rng.randrange(0, 16),
                ks=rng.randrange(0, 4),
                mul=rng.randrange(0, 16),
            )
        s.patch(ch, rng.randrange(0, 8), rng.randrange(0, 8), ops)
        s.freq(ch, rng.randrange(0, 8), rng.randrange(0, 0x800))
    for ch in range(6):
        s.key(ch)
    s.clocks(60000)
    for ch in range(6):
        s.key(ch, 0)
        s.clocks(3000)
    s.clocks(60000)
    # Retrigger during attack and during release.
    s.key(2)
    s.clocks(500)
    s.key(2, 0)
    s.clocks(700)
    s.key(2)
    s.clocks(20000)


def case_ssg_eg(s):
    for ch in range(6):
        ops = {}
        for i, op in enumerate((1, 2, 3, 4)):
            mode = 8 + ((ch * 4 + i) % 8)
            ops[op] = plain(tl=10 + 4 * i, ar=20 + ch, dr=18 + i, sr=12, sl=8, rr=10,
                            ssg=mode)
        s.patch(ch, 7, 0, ops)
        s.freq(ch, 3 + (ch % 3), 0x200 + 0x40 * ch)
        s.key(ch)
    s.clocks(120000)
    for ch in range(6):
        s.key(ch, 0)
    s.clocks(40000)


def case_lfo(s):
    for ch in range(6):
        s.patch(ch, 4, 3, {
            1: plain(30, mul=2, am=1),
            2: plain(10, am=1),
            3: plain(30, mul=3),
            4: plain(10, am=ch % 2),
        })
        s.freq(ch, 4, 0x300 + 0x20 * ch)
        s.ch(ch, 0xB4, 0xC0 | (ch % 4) << 4 | (ch + 2) % 8)
        s.key(ch)
    for freq in range(8):
        s.reg(0, 0x22, 0x08 | freq)
        s.clocks(30000)
    s.reg(0, 0x22, 0x00)
    s.clocks(10000)


def case_ch3_and_csm(s):
    s.patch(2, 7, 0, {1: plain(10), 2: plain(14), 3: plain(18), 4: plain(22)})
    s.freq(2, 4, 0x200)
    for i, (blk, fnum) in enumerate([(3, 0x1F0), (4, 0x2A0), (5, 0x150), (2, 0x7F0)]):
        s.reg(0, 0xAC + i, (blk << 3) | (fnum >> 8))
        s.reg(0, 0xA8 + i, fnum & 0xFF)
    s.reg(0, 0x27, 0x40)
    s.key(2)
    s.clocks(30000)
    s.key(2, 0)
    s.clocks(10000)
    # CSM: timer A keys channel 3 on at each overflow.
    s.patch(2, 7, 0, {1: plain(10, rr=8), 2: plain(14, rr=8), 3: plain(18, rr=8),
                      4: plain(22, rr=8)})
    s.reg(0, 0x24, 0xF0)
    s.reg(0, 0x25, 0x01)
    s.reg(0, 0x27, 0x85)
    s.clocks(60000)
    s.reg(0, 0x27, 0xC5)
    s.clocks(30000)
    s.reg(0, 0x27, 0x00)
    s.clocks(10000)


def case_timers(s):
    for port in range(4):
        s.read(port)
    s.reg(0, 0x24, 0xFF)
    s.reg(0, 0x25, 0x00)
    s.reg(0, 0x26, 0xF8)
    s.reg(0, 0x27, 0x0F)
    for _ in range(40):
        s.clocks(97)
        s.read(0)
        s.read(1)
    # Busy flag: read immediately after a data write.
    s.raw("w 0 0x30")
    s.clocks(1)
    s.raw("w 1 0x01")
    for _ in range(40):
        s.clocks(1)
        s.read(0)
    s.reg(0, 0x27, 0x3F)
    s.read(0)
    s.reg(0, 0x27, 0x0A)
    s.clocks(5000)
    for port in range(4):
        s.read(port)
    s.reg(0, 0x27, 0x30)
    s.read(0)
    # Let the status bus decay on the ports that do not drive it.
    s.clocks(310000)
    s.read(1)
    s.read(0)


def case_dac(s):
    s.patch(5, 7, 0, {1: plain(0), 2: plain(0), 3: plain(0), 4: plain(0)})
    s.freq(5, 4, 0x269)
    s.key(5)
    s.clocks(2000)
    s.reg(0, 0x2B, 0x80)
    rng = random.Random(2)
    for pan in (0xC0, 0x80, 0x40, 0x00, 0xC0):
        s.reg(1, 0xB6, pan)
        for _ in range(64):
            s.raw("w 0 0x2a")
            s.clocks(1)
            s.raw(f"w 1 {rng.randrange(256):#04x}")
            s.clocks(37)
    s.reg(0, 0x2B, 0x00)
    s.clocks(5000)


def case_detune_multi(s):
    for ch in range(6):
        ops = {}
        for i, op in enumerate((1, 2, 3, 4)):
            dt = (ch * 4 + i) % 8
            mul = (ch * 4 + i * 3) % 16
            ops[op] = plain(tl=12, dt=dt, mul=mul)
        s.patch(ch, 7, 0, ops)
        s.freq(ch, 7 - ch % 2, 0x7FF - 0x111 * ch)
        s.key(ch)
    s.clocks(40000)


def case_keyon_edges(s):
    s.patch(0, 7, 0, {1: plain(20), 2: plain(20), 3: plain(20), 4: plain(20)})
    s.patch(4, 7, 0, {1: plain(20), 2: plain(20), 3: plain(20), 4: plain(20)})
    s.freq(0, 4, 0x269)
    s.freq(4, 5, 0x150)
    for ops in (0x1, 0x2, 0x4, 0x8, 0x5, 0xA, 0xF, 0x0):
        s.key(0, ops)
        s.clocks(3000)
    # Channel codes 3 and 7 address no channel.
    s.reg(0, 0x28, 0xF3)
    s.reg(0, 0x28, 0xF7)
    s.clocks(3000)
    s.key(4)
    s.clocks(2000)
    # Writes below $10 go to the absent SSG and must change nothing.
    s.reg(0, 0x07, 0xFF)
    s.reg(1, 0x0F, 0xFF)
    s.clocks(2000)
    # Back-to-back key-ons faster than the busy flag allows.
    for i in range(16):
        s.raw("w 0 0x28")
        s.clocks(1)
        s.raw(f"w 1 {0xF0 | (i % 7):#04x}")
        s.clocks(3)
    s.clocks(20000)


def case_test_registers(s):
    s.patch(1, 4, 6, {1: plain(20, mul=3), 2: plain(10), 3: plain(20, mul=5), 4: plain(10)})
    s.freq(1, 4, 0x269)
    s.key(1)
    s.clocks(4000)
    # $21 bit 6: read internal data instead of the status.
    for value in (0x40, 0xC0, 0x41, 0xC1):
        s.reg(0, 0x21, value)
        for _ in range(24):
            s.clocks(1)
            s.read(0)
    s.reg(0, 0x2C, 0x10)
    for _ in range(24):
        s.clocks(1)
        s.read(0)
    s.reg(0, 0x2C, 0x00)
    s.reg(0, 0x21, 0x00)
    # $2C bit 5: the DAC test output; bit 3 the DAC's low bit.
    s.reg(0, 0x2A, 0x5A)
    s.reg(0, 0x2C, 0x28)
    s.clocks(3000)
    s.reg(0, 0x2C, 0x00)
    # $21 bit 2: the timers count every clock.
    s.reg(0, 0x27, 0x0F)
    s.reg(0, 0x21, 0x04)
    s.clocks(2000)
    s.read(0)
    s.reg(0, 0x21, 0x00)
    # $21 bit 5: the envelope level forced to zero, and the timer masked.
    s.reg(0, 0x21, 0x20)
    s.clocks(3000)
    s.reg(0, 0x21, 0x00)
    # $2C bit 6 with the TEST pin as the envelope clock.
    s.reg(0, 0x2C, 0x40)
    s.raw("pin 1")
    s.clocks(3000)
    s.raw("pin 0")
    s.reg(0, 0x2C, 0x00)
    s.clocks(3000)


CASES = {
    "silence": case_silence,
    "sine": case_sine,
    "algorithms": case_algorithms,
    "envelopes": case_envelopes,
    "ssg_eg": case_ssg_eg,
    "lfo": case_lfo,
    "ch3_and_csm": case_ch3_and_csm,
    "timers": case_timers,
    "dac": case_dac,
    "detune_multi": case_detune_multi,
    "keyon_edges": case_keyon_edges,
    "test_registers": case_test_registers,
}


def main():
    out = pathlib.Path(__file__).parent / "cases"
    out.mkdir(exist_ok=True)
    for name, build in CASES.items():
        for variant, readmode in (("ym2612", "port0"), ("ym3438", "all")):
            s = Script()
            s.raw(f"variant {variant}")
            s.raw(f"readmode {readmode}")
            build(s)
            path = out / f"{name}.{variant}.ym"
            path.write_text("\n".join(s.lines) + "\n")


if __name__ == "__main__":
    main()
