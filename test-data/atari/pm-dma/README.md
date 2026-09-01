# Player/missile DMA probe (Atari 800XL)

`atari-800xl-pm-dma.bin` is an 8 KB cartridge this project wrote, built from
[`atari-800xl-pm-dma.s`](atari-800xl-pm-dma.s) by [`build.py`](build.py).
It carries no code from any commercial image, so it sits in the repository
and runs in CI with no ROM at all: the 800XL starts a cartridge directly when
it has no OS.

## What it is for

A player reaches the screen through three parts — ANTIC fetches its bitmap
over DMA, the machine hands the bytes to GTIA, GTIA gates and positions them
— and each part has its own tests that write registers by hand. This
cartridge runs the chain end to end: it programs PMBASE, a display list and
the P/M bitmaps, and lets the hardware draw. The test that drives it is
[`crates/machine-atari-800xl/tests/pm_dma.rs`](../../../crates/machine-atari-800xl/tests/pm_dma.rs).

## How a test uses it

The three registers under test are read from zero page when the program
starts, so one image covers every combination:

| Address | Register |
|---|---|
| `$80` | DMACTL |
| `$81` | GRACTL |
| `$82` | VDELAY |

A test pokes them before the first frame, runs three frames, and compares
the set of pixels that are not the background colour with the set the
registers should light. The playfield is blank mode 4, so nothing else is on
screen.

Each object is written into both P/M layouts at positions that cover the
same scan lines, so the expected picture is the same at one- and two-line
resolution and the difference under test is the address arithmetic ANTIC
does for each. Player 2 sits in the blank lines above the playfield and
player 3 below the display list's jump, because P/M DMA runs on every line
of the display, not only on mode lines.

## Regenerating

```sh
python3 test-data/atari/pm-dma/build.py          # write the image
python3 test-data/atari/pm-dma/build.py --check  # CI: compare with the source
```

Needs `asm198x` on the path. Deterministic; regenerating should produce no
diff.
