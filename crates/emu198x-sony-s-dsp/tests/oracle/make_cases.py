#!/usr/bin/env python3
"""Write the scripted S-DSP oracle cases.

A case is a text script that both the ares harness and the Rust test read:

    ram <hex addr> <hex bytes...>   audio RAM contents before the first step
    at <steps>                      later writes happen after this many steps
    w <hex reg> <hex value>         a register write
    end <steps>                     how many steps to run

One step is one SPC700 cycle; 32 steps make a sample.
"""

import random
import sys
from pathlib import Path

SAMPLE = 32
DIR_PAGE = 0x02  # sample directory at $0200


class Case:
    def __init__(self, title):
        self.lines = [f"# {title}"]
        self.events = []
        self.steps = 0

    def ram(self, addr, data):
        for i in range(0, len(data), 32):
            chunk = " ".join(f"{b:02X}" for b in data[i : i + 32])
            self.lines.append(f"ram {addr + i:04X} {chunk}")

    def w(self, step, reg, value):
        self.events.append((step, reg, value & 0xFF))

    def end(self, steps):
        self.steps = steps

    def text(self):
        lines = list(self.lines)
        at = None
        # Stable: writes due on the same step keep the order they were made.
        for step, reg, value in sorted(self.events, key=lambda e: e[0]):
            if step >= self.steps:
                continue
            if step != at:
                lines.append(f"at {step}")
                at = step
            lines.append(f"w {reg:02X} {value:02X}")
        lines.append(f"end {self.steps}")
        return "\n".join(lines) + "\n"


def brr(rng, blocks, loop, filters=(0, 1, 2, 3), scales=range(0, 13)):
    """A BRR sample: `blocks` nine-byte blocks, the last marked end, and
    looping when `loop`."""
    out = bytearray()
    for i in range(blocks):
        last = i == blocks - 1
        header = rng.choice(list(scales)) << 4 | rng.choice(filters) << 2
        if last:
            header |= 1 | (2 if loop else 0)
        out.append(header)
        out.extend(rng.randrange(256) for _ in range(8))
    return bytes(out)


def directory(case, entries):
    """Sample directory entries: (start, loop) per source number."""
    data = bytearray()
    for start, loop in entries:
        data += bytes([start & 0xFF, start >> 8, loop & 0xFF, loop >> 8])
    case.ram(DIR_PAGE << 8, data)


def voice(case, step, n, src, pitch, vol=(0x40, 0x40), adsr=(0x8F, 0xE0), gain=0x7F):
    base = n << 4
    case.w(step, base + 0, vol[0])
    case.w(step, base + 1, vol[1])
    case.w(step, base + 2, pitch & 0xFF)
    case.w(step, base + 3, pitch >> 8)
    case.w(step, base + 4, src)
    case.w(step, base + 5, adsr[0])
    case.w(step, base + 6, adsr[1])
    case.w(step, base + 7, gain)


def common(case, step=0, flg=0x20):
    case.w(step, 0x5D, DIR_PAGE)
    case.w(step, 0x0C, 0x7F)
    case.w(step, 0x1C, 0x7F)
    case.w(step, 0x6C, flg)


def brr_loops(rng):
    case = Case("BRR decoding with every filter and scale, looping and one-shot")
    one = brr(rng, 12, loop=True)
    two = brr(rng, 7, loop=False)
    case.ram(0x1000, one)
    case.ram(0x2000, two)
    directory(case, [(0x1000, 0x1000 + 9 * 4), (0x2000, 0x2000)])
    common(case)
    voice(case, 0, 0, 0, 0x1000)
    voice(case, 0, 1, 1, 0x0C00, vol=(0x7F, 0x10))
    case.w(5, 0x4C, 0x03)
    case.w(SAMPLE * 300 + 7, 0x02, 0x34)  # retune voice 0 mid-sample
    case.w(SAMPLE * 300 + 7, 0x03, 0x21)
    case.end(SAMPLE * 1024)
    return case


def adsr(rng):
    case = Case("ADSR: attack, decay to sustain, sustain decay, KOFF release")
    case.ram(0x1000, brr(rng, 4, loop=True, filters=(0,), scales=(10,)))
    directory(case, [(0x1000, 0x1000)])
    common(case)
    for n, (a0, a1) in enumerate([(0x8F, 0xE0), (0x8A, 0x4F), (0xF3, 0x7A), (0x80, 0x1F)]):
        voice(case, 0, n, 0, 0x1000, adsr=(a0, a1))
    case.w(3, 0x4C, 0x0F)
    case.w(SAMPLE * 2000 + 13, 0x5C, 0x05)
    case.w(SAMPLE * 2600 + 2, 0x5C, 0x00)
    case.w(SAMPLE * 2600 + 2, 0x4C, 0x02)
    case.end(SAMPLE * 4096)
    return case


def gain_modes(rng):
    case = Case("GAIN: direct, linear and exponential decrease, linear and bent increase")
    case.ram(0x1000, brr(rng, 4, loop=True, filters=(0, 1), scales=(9,)))
    directory(case, [(0x1000, 0x1000)])
    common(case)
    gains = [0x40, 0x9C, 0xBC, 0xDC, 0xFC, 0x1F, 0xE0, 0xC4]
    for n, g in enumerate(gains):
        voice(case, 0, n, 0, 0x0800 + n * 0x111, adsr=(0x00, 0x00), gain=g)
    case.w(1, 0x4C, 0xFF)
    # Change the modes while they run.
    for k, step in enumerate(range(SAMPLE * 500, SAMPLE * 3000, SAMPLE * 311 + 5)):
        n = k % 8
        case.w(step, (n << 4) | 7, gains[(k + 3) % 8] ^ 0x05)
    case.end(SAMPLE * 3500)
    return case


def pitch_mod_and_noise(rng):
    case = Case("Pitch modulation chains, noise at several rates")
    case.ram(0x1000, brr(rng, 8, loop=True))
    directory(case, [(0x1000, 0x1000 + 18)])
    common(case, flg=0x20 | 0x1A)
    for n in range(8):
        voice(case, 0, n, 0, 0x0400 + n * 0x0321, vol=(0x30 + n, 0x50 - n))
    case.w(0, 0x2D, 0xAE)  # PMON on 1,2,3,5,7 (and 0, which ignores it)
    case.w(0, 0x3D, 0x41)  # noise on voices 0 and 6
    case.w(2, 0x4C, 0xFF)
    for k, rate in enumerate([0x1F, 0x05, 0x10, 0x01, 0x00, 0x1C]):
        case.w(SAMPLE * (400 + 350 * k) + k, 0x6C, 0x20 | rate)
    case.end(SAMPLE * 2600)
    return case


def echo(rng):
    case = Case("Echo: delay, feedback, the FIR, EON, and the write-enable flag")
    case.ram(0x1000, brr(rng, 6, loop=True))
    directory(case, [(0x1000, 0x1000)])
    common(case, flg=0x20)
    fir = [0x7F, 0x00, 0xE0, 0x10, 0xF8, 0x20, 0x08, 0xC0]
    for n, c in enumerate(fir):
        case.w(0, (n << 4) | 0x0F, c)
    case.w(0, 0x6D, 0x80)  # ESA $8000
    case.w(0, 0x7D, 0x03)  # EDL 3: 6 KB
    case.w(0, 0x0D, 0x60)  # EFB
    case.w(0, 0x2C, 0x50)
    case.w(0, 0x3C, 0xB0)
    case.w(0, 0x4D, 0x0B)  # EON
    for n in range(4):
        voice(case, 0, n, 0, 0x0900 + n * 0x0155, vol=(0x60, 0x20 + n * 0x10))
    case.w(4, 0x4C, 0x0F)
    case.w(SAMPLE * 64 + 9, 0x6C, 0x00)  # echo writes on
    case.w(SAMPLE * 2000, 0x7D, 0x01)  # shorter delay mid-run
    case.w(SAMPLE * 2600 + 30, 0x6C, 0x20)  # writes off again
    case.w(SAMPLE * 2900, 0x6D, 0x90)
    case.end(SAMPLE * 3400)
    return case


def kon_koff_timing():
    rng = random.Random(7)
    case = Case("KON, KOFF and ENDX written at every step position in a sample")
    case.ram(0x1000, brr(rng, 3, loop=False))
    case.ram(0x1100, brr(rng, 5, loop=True))
    directory(case, [(0x1000, 0x1000), (0x1100, 0x1100 + 9)])
    common(case)
    for n in range(8):
        voice(case, 0, n, n & 1, 0x1000 + n * 0x40)
    step = SAMPLE * 4
    for offset in range(SAMPLE):
        step += SAMPLE * 37 + offset
        voices = 1 << (offset % 8) | 1 << ((offset * 3) % 8)
        case.w(step, 0x4C, voices)
        case.w(step + 11 + offset, 0x5C, voices if offset % 3 else 0)
        if offset % 5 == 0:
            case.w(step + 2, 0x7C, 0xFF)
        if offset % 7 == 0:
            case.w(step + 64, 0x5C, 0x00)
    case.end(step + SAMPLE * 200)
    return case


def soft_reset_and_mute(rng):
    case = Case("FLG soft reset and mute, and reads of ENVX/OUTX")
    case.ram(0x1000, brr(rng, 4, loop=True))
    directory(case, [(0x1000, 0x1000)])
    common(case, flg=0xE0)
    for n in range(3):
        voice(case, 0, n, 0, 0x0F00 + n * 0x100)
    case.w(3, 0x4C, 0x07)
    case.w(SAMPLE * 10, 0x6C, 0x20)
    case.w(SAMPLE * 10, 0x4C, 0x07)
    case.w(SAMPLE * 400 + 17, 0x6C, 0x60)  # mute
    case.w(SAMPLE * 600 + 3, 0x6C, 0x20)
    case.w(SAMPLE * 800 + 25, 0x6C, 0xA0)  # soft reset
    case.w(SAMPLE * 900, 0x6C, 0x20)
    case.w(SAMPLE * 900, 0x4C, 0x05)
    case.w(SAMPLE * 1000 + 1, 0x08, 0x55)  # writes to ENVX and OUTX
    case.w(SAMPLE * 1000 + 1, 0x19, 0xAA)
    case.end(SAMPLE * 1300)
    return case


def fuzz(seed):
    rng = random.Random(seed)
    case = Case(f"Random register writes at random steps, seed {seed}")
    for k in range(6):
        case.ram(0x1000 + k * 0x200, brr(rng, rng.randrange(2, 20), loop=rng.random() < 0.7))
    directory(case, [(0x1000 + k * 0x200, 0x1000 + k * 0x200 + 9 * rng.randrange(2)) for k in range(6)])
    case.ram(0x8000, bytes(rng.randrange(256) for _ in range(0x400)))
    common(case, flg=0x20)
    case.w(0, 0x6D, 0x80)
    step = 0
    end = SAMPLE * 3000
    while step < end - SAMPLE:
        step += rng.choice([1, 2, 3, 7, 13, 32, 64, 100, 500])
        reg = rng.choice(
            [rng.randrange(0x80)] * 2
            + [(rng.randrange(8) << 4) | rng.randrange(8)] * 6
            + [0x4C, 0x5C, 0x2D, 0x3D, 0x4D, 0x0D, 0x7D, 0x7C]
        )
        value = rng.randrange(256)
        if reg == 0x6C:
            value &= 0x3F  # keep out of soft reset most of the time
        if reg == 0x6D:
            value = 0x80 | (value & 0x3F)
        if reg == 0x5D:
            value = DIR_PAGE
        case.w(step, reg, value)
    case.end(end)
    return case


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).parent / "cases")
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(1997)
    cases = {
        "brr_loops": brr_loops(rng),
        "adsr": adsr(rng),
        "gain_modes": gain_modes(rng),
        "pitch_mod_noise": pitch_mod_and_noise(rng),
        "echo_fir": echo(rng),
        "kon_koff_timing": kon_koff_timing(),
        "soft_reset_mute": soft_reset_and_mute(rng),
        "fuzz_1": fuzz(1),
        "fuzz_2": fuzz(2),
        "fuzz_3": fuzz(3),
    }
    for name, case in cases.items():
        (out / f"{name}.dsp").write_text(case.text())


if __name__ == "__main__":
    main()
