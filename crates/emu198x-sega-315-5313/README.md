# emu198x-sega-315-5313

The Sega 315-5313, the Mega Drive's video display processor, stepped slot by
slot from the master clock.

**Status: being built in stages** (emu198x/emu198x#2033). This release has
the chip's bus side and clock, and the planes and window. Sprites and Mode 4
are still to come.

- VRAM, CRAM and VSRAM, with the odd-address byte swap and the 128 KB mode wiring.
- The control port's two-word command latch and the 24 registers.
- The four-entry write FIFO. It drains one access per external slot, after a
  three-slot wait, with a VRAM word taking two slots.
- The read prefetch, including CRAM and VSRAM's missing bits taken from the FIFO.
- DMA:
  - VRAM fill, which writes the high byte to the address with bit 0 flipped;
  - VRAM copy, which alternates read and write slots;
  - 68000 transfers, with the VDP as bus master and the source wrapping
    inside a 128 KB window.
- The H and V counters, their latch, interlace, the status register, and the
  V-int, H-int and external interrupts.
- Planes A and B and the window:
  - full-screen, per-cell and per-line horizontal scroll;
  - full-screen and two-cell vertical scroll;
  - plane sizes, H and V flips, priority, and the column-0 mask;
  - shadow and highlight;
  - interlace mode 2's 16-line tiles.
- Each tile is fetched in its own slot. Output goes to an ARGB framebuffer
  through CRAM as it is at that moment, so mid-line palette changes and CRAM
  dots appear where they land.

## How it is modelled

A line is 3420 master clocks in both widths: 210 slots in H40 and 171 in H32.
Each slot is one VRAM access. During the picture most slots fetch for the
display; the CPU gets only the external slots: 18 a line in H40 and 16 in
H32. On blank lines, or with the display off, every slot but the refresh
slots is external. The crate steps the line slot by slot, so the FIFO, DMA
and reads happen exactly when the line's external slots come round. The slot
positions are hardware facts from Nemesis's measurements and BlastEm's
test-ROM-checked model. This crate's code is its own.

## Using it

Call `tick()` once per master clock. A full FIFO makes `write_data` return
`None`, and a read that isn't ready makes `read_data` return `None`; the
machine holds the CPU's DTACK and retries. During a 68000-to-VDP DMA,
`bus_request()` asks for the 68000's bus. `dma_read()` names the word the VDP
wants, and the machine reads it and passes it to `dma_word()`. The chip owns
no bus and makes no callbacks, the same as every Emu198x chip.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulator cores. This crate is versioned
independently of the Emu198x suite.

Licensed GPL-2.0-or-later.
