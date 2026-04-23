# Nintendo Game Boy

> Status as of 2026-04-23: **CPU done, the rest of the family is
> next.** The [Sharp LR35902](../../chips/sharp-lr35902.md) CPU crate
> is ported and validated against the 49,600-test Adam Tennant
> single-step corpus. PPU / APU / timer / MBC / cartridge format /
> machine / runtime are still to come. The port lifts the existing
> Zig implementation at `~/Projects/Emu198x-Zig/` into the fresh
> Rust tree under the
> [within-family layering](../../decisions/within-family-layering.md)
> shape.

Nintendo's 8-bit handheld, released 1989 (DMG) and refreshed 1998
(CGB). Runs on a Sharp LR35902 system-on-chip CPU — a custom core that
sits between the Z80 and the 8080 in feature set. Cartridge-driven,
with memory-bank controllers handling paging for ROMs larger than
32 KiB.

- **CPU:** [Sharp LR35902](../../chips/sharp-lr35902.md) at 4.194304 MHz
  DMG / 8.388608 MHz CGB-fast. M-cycle-grained
  ([decision](../../decisions/sm83-abstraction-level.md)). Integrates
  boot ROM, PPU tie-ins, APU, timer, DMA, serial, and joypad
  controller on the same die.
- **Memory:** 8 KiB work RAM (DMG) / 32 KiB banked (CGB), 8 KiB
  VRAM (DMG) / 16 KiB banked (CGB), 160 B OAM, 127 B HRAM, plus
  whatever the cartridge exposes via MBC.
- **Video:** 160×144 LCD, 4 shades (DMG) / 32 768 colours (CGB),
  tile + sprite based. PPU runs on the same 4 MHz master clock as
  the CPU.
- **Audio:** 4 channels — two pulse with sweep, one programmable
  wave table, one LFSR noise. Mixed internally, stereo output via
  SO1/SO2 panning registers.
- **Cartridge:** MBC1 / MBC2 / MBC3 (with RTC) / MBC5 / MBC6 / MBC7
  / HuC1 / HuC3 / MMM01, plus the unbanked "no MBC" mode. Save RAM
  battery-backed on most carts that have it.

## Port plan

Lifted from the Zig implementation at `~/Projects/Emu198x-Zig/` under
the [archive-port methodology](../../decisions/archive-port-methodology.md):
characterise → port-with-tests → integrate.

### Phase 0 — docs and shape (done)

- [SM83 abstraction level](../../decisions/sm83-abstraction-level.md) —
  m-cycle, not T-cycle.
- This overview.
- [Sharp LR35902](../../chips/sharp-lr35902.md) — chip page.
- [Game Boy timing](timing.md) — master-clock constants and m-cycle
  derivations.

### Phase 1 — family crates

1. **`sharp-lr35902`** — done. Pin-level m-cycle state machine over
   the full opcode table (incl. CB sub-table), interrupt dispatch
   (5 m-cycles, lowest set bit wins, cancelled-IRQ → $0000), HALT
   bug latch, EI one-instruction delay. Public pin fields per
   [CPU bus interface](../../decisions/cpu-bus-interface.md). 92
   unit tests + 49,600 Adam Tennant single-step tests passing.
2. **`common-nintendo-game-boy`** — done for the DMG. Timing
   constants (master clock, m-cycles per frame / scanline, PPU
   mode dots, OAM DMA budget, screen size), `MemoryBus` trait,
   four-shade DMG palette helpers (`BGP`/`OBP*` decoder), joypad
   matrix with the action / direction group multiplexing on
   `$FF00`. Hardware-only; the runtime layer will map host events.
   CGB-specific bits (15-bit RGB palettes, double-speed knob)
   deferred until the second machine arrives.
3. **`nintendo-game-boy-ppu`** — done. Pixel-FIFO renderer ticked
   per-dot. 4-state BG/window fetcher, OAM scan with 10-sprite
   limit and DMG X-priority sort, full LCDC / STAT / SCY / SCX /
   LYC / BGP / OBP0 / OBP1 / WX / WY register set, 160×144
   framebuffer of post-palette 2-bit shades. STAT IRQ rising-edge
   detection on the OR of LYC + mode 0/1/2 enable bits; VBlank
   IRQ latches at LY=144. LCD-off freezes timing and blanks the
   framebuffer per hardware. 24 unit tests passing. OAM DMA
   blocking and per-mode VRAM/OAM access blocking are deferred to
   the machine layer (which owns the actual address decode).
4. **`nintendo-game-boy-apu`** — done. Four channels (CH1 square
   with sweep, CH2 square, CH3 wave-table playback, CH4 LFSR
   noise), full register block, frame sequencer driven by the
   timer's DIV bit 12 (length 256 Hz, sweep 128 Hz, envelope
   64 Hz). NR50/NR51 mixer + per-channel L/R panning, NR52
   power-off behaviour with length-counter preservation. All four
   "first half" length-period quirks, sweep negate-clear quirk,
   DAC-off-disables-channel, wave-RAM corruption on retrigger.
   Stereo `f32` output at 48 kHz via fractional accumulator over
   the real 4.194304 MHz master (the Zig was 2× off; corrected
   here). 18 unit tests passing.
5. **`nintendo-game-boy-timer`** — done. 16-bit free-running
   counter ticked at the master clock rate; DIV is the high byte,
   TIMA increments on the falling edge of `(timer_enable AND
   selected_counter_bit)`. Both the DIV-write and TAC-write
   falling-edge glitches are modelled. Overflow latches a strobe
   the machine OR's into `IF` bit 2. The 1-m-cycle reload delay
   that mooneye-gb's `tima_reload` family verifies is deferred
   (matching the Zig source's TODO) until the rest of the machine
   layer exists. 17 unit tests passing.
6. **`nintendo-game-boy-mbc`** — done. ROM-only, MBC1, MBC3, and
   MBC5 implemented end-to-end. `Cartridge` owns the ROM image and
   external RAM and dispatches reads / writes to the chosen
   `Mbc` enum variant. MBC1 covers the bank-zero-rewrite quirk,
   the 2-bit secondary's dual role (ROM bits 5-6 in mode 0 / RAM
   bank in mode 1), and large-ROM bank-0 windowing in advanced
   mode. MBC3 handles the 7-bit ROM bank, four RAM banks, and the
   RTC register snapshot/latch (RTC values aren't advanced yet —
   the machine layer can drive them when it lands). MBC5 covers
   the 9-bit bank split (low 8 bits at $2000-$2FFF, bit 9 at
   $3000-$3FFF), 4-bit RAM bank, and crucially permits bank 0 in
   the switchable window (no MBC1-style rewrite). 18 tests pass.
   MBC2 is deferred.
7. `format-nintendo-game-boy-cartridge` — header parse
   (Nintendo logo, title, cartridge type, ROM size, RAM size,
   destination, version, header checksum, global checksum),
   MBC selection, save-RAM loader.
8. `machine-nintendo-game-boy` — composes CPU + PPU + APU +
   timer + MBC + work RAM + VRAM + OAM + HRAM. Implements
   `pub fn run_frame()` directly.
9. `runtime-nintendo-game-boy` — bespoke runtime over the one
   machine. `Model` enum with `Dmg` (and later `Cgb`);
   `profile_for` + `profiles()` in `profiles.rs` even with one
   entry, per [within-family layering](../../decisions/within-family-layering.md).

### Phase 2 — verification

Acceptance tests, in order:

- Blargg `cpu_instrs` — all 11 sub-tests. Gates opcode
  correctness.
- Blargg `instr_timing` — gates m-cycle counts per instruction.
- Blargg `mem_timing` (v1 + v2) — gates bus-access timing.
- mooneye-gb `acceptance/` — hardware-behaviour tests at
  m-cycle precision.
- `dmg-acid2.gb` — PPU rendering smoke test.

### Phase 3 — CGB (later)

When DMG is green across all four Blargg tests and the
mooneye acceptance suite, the second machine arrives. That's
the trigger for the family-driver lift called out in
[within-family layering](../../decisions/within-family-layering.md):
extract a `GameBoyDriver` trait, extract `GameBoyMachine`, make
the runtime generic, add a `GameBoyCgbRuntime` type alias.

## Related

- [Within-family layering](../../decisions/within-family-layering.md)
  — the five-piece structure this port fills in.
- [Archive-port methodology](../../decisions/archive-port-methodology.md)
  — the discipline for lifting the Zig source.
- [SM83 abstraction level](../../decisions/sm83-abstraction-level.md)
  — why the CPU is m-cycle.
- [CPU bus interface](../../decisions/cpu-bus-interface.md) — the
  pin-level rule the SM83 obeys.
- [Sharp LR35902](../../chips/sharp-lr35902.md) — chip page.
- [Game Boy timing](timing.md) — master clock, m-cycle constants,
  frame timing.
