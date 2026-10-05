# DLI timing probe (Atari 800XL)

`atari-800xl-dli-timing.bin` is an 8 KB cartridge this project wrote, built
from [`atari-800xl-dli-timing.s`](atari-800xl-dli-timing.s) by
[`build.py`](build.py). It carries no code from any commercial image, so it
sits in the repository and runs in CI with no ROM at all: the 800XL starts a
cartridge directly when it has no OS.

## What it is for

ANTIC reads a text line's glyph data during the line, not at its start
(Altirra Hardware Reference Manual, "Character mode playfield DMA": names
from cycle 18 at normal width and 26 at narrow, glyph data three cycles
later). A CHBASE write that lands in the cycles before that fetch shapes the
line it lands on, and a display-list interrupt is how programs make such
writes. This cartridge makes the write visible: the screen is six mode 2
lines of character 0, drawn from a font whose glyph 0 is solid, and a DLI
switches CHBASE to a font whose glyph 0 is empty, so every scan line shows
which font it was drawn with. The test that drives it is
[`crates/machine-atari-800xl/tests/dli_timing.rs`](../../../crates/machine-atari-800xl/tests/dli_timing.rs).

## How a test uses it

How the interrupt makes the write is read from zero page when the program
starts, so one image covers every case:

| Address | Meaning |
|---|---|
| `$80` | DMACTL: `$22` for a normal playfield, `$21` for a narrow one |
| `$81` | `STA WSYNC`s before the stores: 0, 1 or 2 |
| `$82` | Number of four-cycle padding stores (0-7) before the CHBASE store, after a wait |

With WSYNC the CHBASE store spills past the end of the interrupt's line
into the first cycles of the next, which is then drawn with the new font.
Without WSYNC the write lands as early in the interrupt's own line as an
interrupt can make one; on a narrow playfield that is before the glyph
fetch, so the interrupt's own line changes font. Two waits let the first scan
line of the next text line go by, so the stores spill into its second,
where ANTIC fetches only glyph data, one character every two cycles. Enough
padding puts the write among those fetches, and that scan line splits at
the first character whose glyph ANTIC fetches two or more cycles after the
write (Altirra Hardware Reference Manual, "Character set storage": a CHBASE change
takes effect two cycles after the write). A test pokes the three bytes,
runs three frames, and reads one pixel of every character on every scan
line of the text.

The cartridge carries the OS's run vector and flags at `$BFFA-$BFFF`, so it
also starts under the real OS, but the OS's own DLI dispatch moves the
no-WSYNC write later in the line; the test runs without one.

## Regenerating

```sh
python3 test-data/atari/dli-timing/build.py          # write the image
python3 test-data/atari/dli-timing/build.py --check  # CI: compare with the source
```

Needs `asm198x` on the path. Deterministic; regenerating should produce no
diff.
