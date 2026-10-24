#!/usr/bin/env python3
"""Write the PPU oracle cases into cases/.

A case is a text script, one command a line:

    region ntsc|pal
    frames N                    frames to run
    vram ADDR VALUE             poke a VRAM word (hex) before the run
    vrand SEED ADDR COUNT       poke COUNT VRAM words from the case PRNG (hex)
    crand SEED INDEX COUNT      poke COUNT CGRAM entries from the PRNG (hex)
    cgram INDEX VALUE           poke a CGRAM entry (hex)
    oam ADDR VALUE              poke an OAM byte (hex)
    w F V H REG VALUE           write $21REG at frame F, line V, master clock H
    r F V H REG MDR             read $21REG there, the CPU open bus being MDR
    latch F V H LEVEL           drive the external latch pin (WRIO bit 7)

Times are decimal. F counts completed frames; a frame completes as V=240
starts, so lines from 240 on carry the next frame's number. H is a
multiple of 4 because the model is clocked by the dot. Commands at one time
run in order. A command whose time never comes is reported by both sides.

The PRNG is xorshift64* seeded with SEED | 1, taking the top 16 bits of
each output; the harness and the Rust test implement it identically.

The cases are deterministic: rerunning this rewrites them unchanged.
"""

import random
from pathlib import Path

HERE = Path(__file__).resolve().parent


def at(f, v, h):
    """Check a time is one the model reaches, numbering the frame."""
    assert h % 4 == 0 and h < 1360, h
    assert 0 <= v < 313, v
    return f + (1 if v >= 240 else 0), v, h


class Case:
    def __init__(self, name, frames=2, region="ntsc"):
        self.name = name
        self.lines = [f"region {region}", f"frames {frames}"]
        self.pokes = []
        self.events = []

    def vrand(self, seed, addr=0, count=0x8000):
        self.pokes.append(f"vrand {seed:x} {addr:04x} {count:x}")

    def crand(self, seed, index=0, count=256):
        self.pokes.append(f"crand {seed:x} {index:02x} {count:x}")

    def oam(self, addr, value):
        self.pokes.append(f"oam {addr:03x} {value & 0xff:02x}")

    def _event(self, f, v, h, text):
        f, v, h = at(f, v, h)
        self.events.append((f, v, h, len(self.events), text.format(f=f, v=v, h=h)))

    def w(self, reg, value, f=0, v=0, h=0):
        self._event(f, v, h, f"w {{f}} {{v}} {{h}} {reg:02x} {value & 0xff:02x}")

    def r(self, reg, f, v, h, mdr=0x5A):
        self._event(f, v, h, f"r {{f}} {{v}} {{h}} {reg:02x} {mdr:02x}")

    def latch(self, level, f, v, h):
        self._event(f, v, h, f"latch {{f}} {{v}} {{h}} {level}")

    def w16(self, reg, value, **when):
        """A write-twice register: low byte, then high."""
        self.w(reg, value & 0xFF, **when)
        self.w(reg, value >> 8 & 0xFF, **when)

    def w2(self, reg, value, **when):
        """A register pair: `reg` takes the low byte, `reg + 1` the high."""
        self.w(reg, value & 0xFF, **when)
        self.w(reg + 1, value >> 8 & 0xFF, **when)

    def save(self):
        # A frame number covers V=240 of the frame before up to V=239, so
        # its lines from 240 on come first.
        order = sorted(self.events, key=lambda e: (e[0], e[1] < 240, e[1], e[2], e[3]))
        events = [text for *_, text in order]
        text = "\n".join(self.lines + self.pokes + events) + "\n"
        (HERE / "cases" / f"{self.name}.ppu").write_text(text)


def seed(rng):
    return rng.randrange(1 << 63)


def random_memory(case, rng):
    case.crand(seed(rng))
    case.vrand(seed(rng))


def background_setup(case, rng, mode, extra=0):
    """Random VRAM and CGRAM, every background on, scrolled at random."""
    random_memory(case, rng)
    case.w(0x05, mode | extra)
    for layer in range(4):
        case.w(0x07 + layer, (0x40 + layer * 8) | rng.randrange(4))
    case.w(0x0B, rng.randrange(256))
    case.w(0x0C, rng.randrange(256))
    for layer in range(4):
        case.w16(0x0D + 2 * layer, rng.randrange(0x400))
        case.w16(0x0E + 2 * layer, rng.randrange(0x400))
    case.w(0x2C, 0x1F)
    case.w(0x2D, 0x00)
    case.w(0x00, 0x0F)


def sprites(case, rng, count, size_select=0, y_range=(0, 224), x_range=(0, 256)):
    """`count` random sprites; the rest parked below the picture."""
    case.w(0x01, size_select << 5 | 0x02)
    for i in range(128):
        if i < count:
            case.oam(i * 4 + 0, rng.randrange(*x_range) & 0xFF)
            case.oam(i * 4 + 1, rng.randrange(*y_range) & 0xFF)
            case.oam(i * 4 + 2, rng.randrange(256))
            case.oam(i * 4 + 3, rng.randrange(256))
        else:
            case.oam(i * 4 + 0, 0)
            case.oam(i * 4 + 1, 0xF0)
    for i in range(32):
        case.oam(0x200 + i, rng.randrange(256) if i * 4 < count else 0)


def cases():
    out = []

    # Every mode, with each tile size and the mode 1 BG3 priority bit.
    for mode in range(7):
        for extra in (0x00, 0x08, 0xF0):
            rng = random.Random(f"mode{mode}-{extra}")
            c = Case(f"mode{mode}_{extra:02x}")
            background_setup(c, rng, mode, extra)
            out.append(c)

    # Direct colour on BG1.
    for mode in (3, 4):
        rng = random.Random(f"direct{mode}")
        c = Case(f"direct_colour_mode{mode}")
        background_setup(c, rng, mode)
        c.w(0x30, 0x01)
        out.append(c)

    # Mode 7: matrices, flips, the repeat modes, EXTBG and mid-line writes.
    for variant in range(6):
        rng = random.Random(f"mode7-{variant}")
        c = Case(f"mode7_{variant}")
        random_memory(c, rng)
        c.w(0x05, 0x07)
        c.w(0x33, 0x40 if variant >= 3 else 0x00)
        c.w(0x1A, [0x00, 0x03, 0x80, 0xC0, 0x41, 0x82][variant])
        for reg in (0x1B, 0x1C, 0x1D, 0x1E):
            if variant % 2:
                value = rng.randrange(0x10000)
            else:
                value = rng.choice([0x100, 0x80, 0x200, 0xFF00])
            c.w16(reg, value)
        for reg in (0x1F, 0x20, 0x0D, 0x0E):
            c.w16(reg, rng.randrange(0x2000))
        c.w(0x2C, 0x13)
        c.w(0x2D, 0x02)
        c.w(0x00, 0x0F)
        if variant >= 4:
            for line in range(1, 225, 7):
                c.w16(0x1B, rng.randrange(0x10000), f=1, v=line, h=rng.randrange(0, 260) * 4)
                c.w16(0x1E, rng.randrange(0x10000), f=1, v=line, h=1100)
        c.r(0x34, 1, 100, 1200)
        c.r(0x35, 1, 100, 1204)
        c.r(0x36, 1, 100, 1208)
        out.append(c)

    # Windows and colour math, with sprites and mid-line changes later on.
    for variant in range(10):
        rng = random.Random(f"window-{variant}")
        c = Case(f"window_math_{variant}")
        background_setup(c, rng, variant % 2)
        for reg in range(0x23, 0x2B):
            c.w(reg, rng.randrange(256))
        c.w(0x2B, rng.randrange(16))
        for reg in (0x2C, 0x2D, 0x2E, 0x2F):
            c.w(reg, rng.randrange(32))
        c.w(0x30, rng.randrange(256) & 0xF2)
        c.w(0x31, rng.randrange(256))
        for channel in (0x20, 0x40, 0x80):
            c.w(0x32, channel | rng.randrange(32))
        if variant >= 6:
            sprites(c, rng, 48)
        if variant >= 8:
            for line in range(1, 225, 5):
                c.w(0x26, rng.randrange(256), f=1, v=line, h=rng.randrange(0, 300) * 4)
                c.w(0x31, rng.randrange(256), f=1, v=line, h=rng.randrange(0, 300) * 4)
        out.append(c)

    # Mosaic on backgrounds, hi-res and Mode 7, changed mid-frame.
    for variant, mode in enumerate([1, 3, 5, 7]):
        rng = random.Random(f"mosaic-{variant}")
        c = Case(f"mosaic_{variant}")
        background_setup(c, rng, mode)
        c.w(0x06, rng.randrange(256) | 0x0F)
        c.w(0x06, 0x00, f=1, v=60, h=200)
        c.w(0x06, 0x7F, f=1, v=100, h=600)
        c.w(0x06, 0xF3, f=1, v=150, h=0)
        out.append(c)

    # Brightness and force blank, toggled mid-frame and mid-line.
    rng = random.Random("blank")
    c = Case("brightness_blank", frames=3)
    background_setup(c, rng, 1)
    sprites(c, rng, 64)
    for i, line in enumerate(range(10, 225, 13)):
        c.w(0x00, (i & 15) | (0x80 if i % 3 == 0 else 0), f=1, v=line, h=(i * 97 % 339) * 4)
    c.w(0x00, 0x0F, f=2, v=0, h=0)
    c.w(0x00, 0x8F, f=2, v=100, h=1100)
    c.w(0x00, 0x0F, f=2, v=101, h=1200)
    out.append(c)

    # Sprites: each size pair, X across the whole range, name select.
    for size in range(8):
        rng = random.Random(f"sprites-{size}")
        c = Case(f"sprites_size{size}")
        random_memory(c, rng)
        c.w(0x05, 0x01)
        sprites(c, rng, 128 if size % 2 else 40, size_select=size, x_range=(0, 512))
        c.w(0x01, size << 5 | 0x02 | rng.randrange(4) << 3)
        c.w(0x2C, 0x10)
        c.w(0x00, 0x0F)
        c.r(0x3E, 1, 230, 0)
        out.append(c)

    # Too many sprites on a line, and the priority rotation.
    rng = random.Random("sprite-overflow")
    c = Case("sprite_overflow")
    random_memory(c, rng)
    sprites(c, rng, 128, size_select=1, y_range=(40, 60))
    c.w(0x2C, 0x10)
    c.w(0x00, 0x0F)
    c.r(0x3E, 0, 30, 0)
    c.r(0x3E, 0, 80, 0)
    c.r(0x3E, 1, 100, 0)
    c.w2(0x02, 0x8000 | 37, f=1, v=10, h=0)
    c.r(0x3E, 1, 230, 0)
    out.append(c)

    # Interlace, sprite interlace, pseudo-hires, overscan and PAL.
    for variant, (region, setini, mode) in enumerate([
        ("ntsc", 0x01, 5),
        ("ntsc", 0x03, 1),
        ("ntsc", 0x08, 1),
        ("ntsc", 0x04, 1),
        ("pal", 0x00, 1),
        ("pal", 0x05, 5),
        ("pal", 0x0B, 6),
    ]):
        rng = random.Random(f"frame-{variant}")
        c = Case(f"frame_{variant}_{region}", frames=4, region=region)
        background_setup(c, rng, mode)
        sprites(c, rng, 40)
        c.w(0x33, setini)
        c.r(0x3F, 1, 10, 0)
        c.r(0x3F, 2, 10, 0)
        out.append(c)

    # The ports through a frame, display on and off, and the counter latch.
    rng = random.Random("ports")
    c = Case("ports", frames=3)
    background_setup(c, rng, 1)
    sprites(c, rng, 32)
    c.w(0x15, 0x80)
    for i, line in enumerate(range(0, 262, 11)):
        h = (i * 53 % 300) * 4
        c.w2(0x16, rng.randrange(0x8000), f=1, v=line, h=h)
        c.w(0x18, rng.randrange(256), f=1, v=line, h=h)
        c.w(0x19, rng.randrange(256), f=1, v=line, h=h)
        c.r(0x39, 1, line, h + 4)
        c.r(0x3A, 1, line, h + 4)
        c.w(0x21, rng.randrange(256), f=1, v=line, h=h + 8)
        c.w(0x22, rng.randrange(256), f=1, v=line, h=h + 8)
        c.w(0x22, rng.randrange(256), f=1, v=line, h=h + 8)
        c.r(0x3B, 1, line, h + 12)
        c.r(0x3B, 1, line, h + 12)
        c.w2(0x02, rng.randrange(0x200), f=1, v=line, h=h + 16)
        c.w(0x04, rng.randrange(256), f=1, v=line, h=h + 16)
        c.w(0x04, rng.randrange(256), f=1, v=line, h=h + 16)
        c.r(0x38, 1, line, h + 20)
        for reg in (0x37, 0x3C, 0x3C, 0x3D, 0x3D, 0x3F):
            c.r(reg, 1, line, h + 24)
        for reg in (0x04, 0x10, 0x15, 0x26, 0x30, 0x2E):
            c.r(reg, 1, line, h + 28)
    c.latch(0, 2, 50, 400)
    c.r(0x3C, 2, 50, 404)
    c.r(0x3C, 2, 50, 404)
    c.r(0x37, 2, 60, 1300)
    c.r(0x3F, 2, 60, 1300)
    c.r(0x3F, 2, 61, 0)
    c.latch(1, 2, 70, 0)
    c.r(0x37, 2, 80, 1312)
    for reg in (0x3C, 0x3C, 0x3F):
        c.r(reg, 2, 80, 1316)
    c.w(0x00, 0x8F, f=2, v=120, h=0)
    for i in range(20):
        c.w(0x18, i, f=2, v=120 + i, h=500)
    c.w(0x00, 0x0F, f=2, v=150, h=0)
    out.append(c)

    # Sprite interlace, where the tall small sizes halve.
    for size in (6, 7):
        rng = random.Random(f"sprite-interlace-{size}")
        c = Case(f"sprite_interlace_size{size}", frames=3)
        random_memory(c, rng)
        sprites(c, rng, 64, size_select=size)
        c.w(0x01, size << 5 | 0x02)
        c.w(0x33, 0x03)
        c.w(0x2C, 0x10)
        c.w(0x00, 0x0F)
        out.append(c)

    # Force blank toggled while the sprite tiles are being fetched. Each
    # sprite skipped while blanked costs 8 clocks, so a long blank can push
    # the fetch past the end of the line, where ares and the model part
    # (see `obj_fetch_abandon`); these sprites are small enough that it
    # never does.
    rng = random.Random("fetch-blank")
    c = Case("fetch_blank")
    random_memory(c, rng)
    sprites(c, rng, 128, size_select=0, y_range=(0, 200))
    c.w(0x2C, 0x10)
    c.w(0x00, 0x0F)
    for line in range(2, 220, 3):
        start = rng.randrange(270, 320) * 4
        c.w(0x00, 0x8F, f=1, v=line, h=start)
        c.w(0x00, 0x0F, f=1, v=line, h=start + rng.choice([4, 8, 12, 16, 24, 40]))
    c.r(0x3E, 1, 230, 0)
    out.append(c)

    # Frame lengths: the short NTSC line and the extra interlaced line,
    # found through the counter latch.
    for region, setini in (("ntsc", 0x00), ("ntsc", 0x01), ("pal", 0x01)):
        c = Case(f"frame_length_{region}_{setini}", frames=5, region=region)
        c.w(0x33, setini)
        c.w(0x00, 0x0F)
        last = 311 if region == "pal" else 261
        for frame in range(4):
            for line, h in ((240, 1352), (last, 1352), (last, 1300)):
                c.latch(0, frame, line, h)
                c.latch(1, frame, line, h)
                for reg in (0x3C, 0x3C, 0x3D, 0x3D, 0x3F):
                    c.r(reg, frame, line, h)
        if setini:
            for frame in (0, 2):
                c.latch(0, frame, last + 1, 0)
                c.latch(1, frame, last + 1, 0)
                for reg in (0x3D, 0x3D, 0x3F):
                    c.r(reg, frame, last + 1, 0)
        out.append(c)

    # Raster effects: a scroll, then a mode and a layer change, mid-line.
    for variant in range(3):
        rng = random.Random(f"raster-{variant}")
        c = Case(f"raster_{variant}")
        background_setup(c, rng, 1)
        for line in range(1, 225, 3):
            h = rng.randrange(14, 230) * 4
            c.w16(0x0D, rng.randrange(0x400), f=1, v=line, h=h)
            if variant >= 1:
                c.w(0x05, rng.choice([0, 1, 2, 3, 5]), f=1, v=line, h=h + 400)
            if variant >= 2:
                c.w(0x2C, rng.randrange(32), f=1, v=line, h=h + 200)
        out.append(c)

    return out


def main():
    (HERE / "cases").mkdir(exist_ok=True)
    for old in (HERE / "cases").glob("*.ppu"):
        old.unlink()
    for case in cases():
        case.save()


if __name__ == "__main__":
    main()
