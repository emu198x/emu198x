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
2. `common-nintendo-game-boy` — timing constants, `MemoryBus`
   trait over the CPU's single bus, palette helpers (DMG four
   shades + CGB 15-bit RGB), joypad matrix. No host-boundary
   types; the runtime layer maps host input.
3. `nintendo-game-boy-ppu` — dot-level renderer. Port `ppu.zig`
   (471 LOC). Modes 0/1/2/3, LCDC + STAT, DMA, OAM scan. Feeds
   a 160×144 framebuffer.
4. `nintendo-game-boy-apu` — 4-channel mixer. Port `apu.zig`
   (783 LOC). Frame sequencer, length counters, envelopes,
   sweep, wave RAM, noise LFSR.
5. `nintendo-game-boy-timer` — DIV / TIMA / TMA / TAC. Port
   `timer.zig` (138 LOC).
6. `nintendo-game-boy-mbc` — MBC1 / MBC3 / MBC5 first. Others
   as demand surfaces from tests.
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
