# Game Boy timing

> Status as of 2026-04-24: **implemented for DMG-class timing.**
> These constants are now represented in `common-nintendo-game-boy`
> and exercised through the CPU, timer, PPU, APU, machine, and
> runtime crates. The current verification gate passes Blargg timing
> ROMs and the local mooneye-gb acceptance sweep; CGB double-speed
> timing remains future work.

## Master clock

The DMG runs on a single crystal-derived master clock at
**4.194304 MHz** (2²² Hz exactly). The CGB keeps the same clock in
normal-speed mode and doubles it to **8.388608 MHz** in
double-speed mode (KEY1 bit 0 = 1).

From that master clock, everything else derives:

| Domain            | Rate                        | Notes |
|-------------------|-----------------------------|-------|
| Master clock      | 4.194304 MHz                | 1 T-cycle |
| M-cycle           | 1.048576 MHz (master / 4)   | CPU tick, bus op grain |
| DIV register      | 16.384 kHz (master / 256)   | top 8 bits of internal 16-bit divider |
| APU frame seq     | 512 Hz (master / 8192)      | drives length/envelope/sweep clocks |
| Dot clock (PPU)   | 4.194304 MHz                | 1 dot = 1 T-cycle |
| Scanline          | 456 dots (114 m-cycles)     | fixed across all modes |
| Frame             | 70 224 dots (17 556 m-cycles) | 154 scanlines × 456 dots |
| Refresh           | 59.7275 Hz                  | master / 70 224 |

Double-speed mode doubles the CPU / timer / serial / DMA rates but
**leaves the PPU and APU at the original rate** — CGB double-speed
is a CPU-domain knob only.

## M-cycle as the base unit

Per [the decision](../../decisions/sm83-abstraction-level.md), the
emulator's CPU ticks at m-cycle grain. A "frame" for the machine's
run loop is **17 556 m-cycles**. A scanline is **114 m-cycles**.

Instruction timings live in the LR35902 opcode tables and all
resolve to whole m-cycles. Blargg's `instr_timing` test verifies
each instruction takes the documented m-cycle count; `mem_timing`
verifies bus reads and writes land on the correct m-cycle *within*
a multi-cycle instruction.

## PPU modes per scanline

Each visible scanline (LY 0..143) divides into:

| Mode | Dots          | M-cycles     | Description |
|------|---------------|--------------|-------------|
| 2    | 80            | 20           | OAM scan — OAM inaccessible to CPU |
| 3    | 172–289       | 43–72        | pixel transfer — OAM + VRAM inaccessible |
| 0    | rest of 456   | to scanline end | HBlank — everything accessible |

VBlank (LY 144..153) is 10 scanlines of mode 1 = 4 560 dots = 1 140
m-cycles. Mode 3's length varies with sprite count on the line,
window activation, and SCX fine-scroll alignment.

## Timer (DIV / TIMA / TMA / TAC)

Internally a single 16-bit counter incremented every T-cycle. DIV is
the high byte of that counter (visible at $FF04). TIMA ($FF05) is
incremented when the selected bit of the internal counter falls
from 1 to 0 (edge-triggered on a specific bit):

| TAC low 2 bits | Frequency    | Internal counter bit |
|----------------|--------------|----------------------|
| 00             | 4096 Hz      | 9                    |
| 01             | 262144 Hz    | 3                    |
| 10             | 65536 Hz     | 5                    |
| 11             | 16384 Hz     | 7                    |

TIMA overflow takes one m-cycle to reload from TMA and raise the
timer interrupt. The "TMA write during reload" edge case and the
"TAC change while running" edge case are both m-cycle-visible and
covered by mooneye-gb.

## Audio frame sequencer

Clocked at 512 Hz (master / 8192), driven by the DIV register's bit
5 (or bit 4 in double-speed mode). Step phases:

| Step | Length | Envelope | Sweep |
|------|--------|----------|-------|
| 0    | tick   | —        | —     |
| 1    | —      | —        | —     |
| 2    | tick   | —        | tick  |
| 3    | —      | —        | —     |
| 4    | tick   | —        | —     |
| 5    | —      | —        | —     |
| 6    | tick   | —        | tick  |
| 7    | —      | tick     | —     |

Length at 256 Hz, envelope at 64 Hz, sweep at 128 Hz.

## OAM DMA

Writing to $FF46 starts a 160-m-cycle DMA that copies $xx00–$xx9F
to OAM ($FE00–$FE9F), one byte per m-cycle. During DMA, the CPU can
only access HRAM ($FF80–$FFFE); all other reads return $FF on real
hardware.

Current implementation status: the machine paces the transfer one
byte per m-cycle, handles restart timing, and blocks CPU OAM access
while DMA is active. The remaining DMA accuracy gap is full non-HRAM
CPU bus blocking during the transfer.

## Related

- [SM83 abstraction level](../../decisions/sm83-abstraction-level.md)
  — why m-cycle.
- [Nintendo Game Boy](overview.md) — family home.
- [Sharp LR35902](../../chips/sharp-lr35902.md) — chip page.
