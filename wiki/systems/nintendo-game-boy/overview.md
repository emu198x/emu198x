# Nintendo Game Boy

> Status as of 2026-04-24: **DMG core and runtime are through the
> Phase 2 gate.** The Rust port now includes CPU, PPU, APU, timer,
> cartridge format, MBCs, machine integration, runtime integration,
> skipped-boot profiles for DMG0 / DMG ABC / MGB / SGB / SGB2, and
> local verification. The ignored Phase 2 harness currently passes
> Blargg `cpu_instrs`, `instr_timing`, `mem_timing` v1/v2,
> `dmg-acid2`, and a broad mooneye-gb sweep of 103 ROMs
> (`acceptance`, `emulator-only/mbc1`, `emulator-only/mbc2`,
> `emulator-only/mbc5`). `emu198x-game-boy` provides a minimal native
> verifier window with live stereo audio plus host-side APU channel
> toggles/gain controls, `.sav` sidecar persistence for battery-backed
> cartridge RAM, and `emu198x-script-game-boy` exposes the runtime as a
> headless cartridge runner with screenshots, audio capture, shared
> scripts, snapshots, and the same `.sav` sidecar convention. CGB,
> boot-ROM execution, full OAM-DMA non-HRAM bus blocking, link cable,
> native UI, and long-tail cartridge hardware remain later work.

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
   falling-edge glitches are modelled. Overflow holds TIMA at
   `$00` for one m-cycle, then reloads from TMA and latches a
   strobe the machine OR's into `IF` bit 2; the mooneye-gb
   `tima_reload`, `tima_write_reloading`, and `tma_write_reloading`
   cases pass. 17 unit tests passing.
6. **`nintendo-game-boy-mbc`** — done. ROM-only, MBC1, MBC2, MBC3, and
   MBC5 implemented end-to-end. `Cartridge` owns the ROM image and
   external RAM and dispatches reads / writes to the chosen
   `Mbc` enum variant. MBC1 covers the bank-zero-rewrite quirk,
   the 2-bit secondary's dual role (ROM bits 5-6 in mode 0 / RAM
   bank in mode 1), and large-ROM bank-0 windowing in advanced
   mode. MBC2 covers the internal 512×4-bit RAM and address-bit-8
   RAM-enable / ROM-bank split. MBC3 handles the 7-bit ROM bank,
   four RAM banks, and the RTC register snapshot/latch; RTC values
   are modelled as registers but are not yet advanced from wall-clock
   time. MBC5 covers the 9-bit bank split (low 8 bits at
   $2000-$2FFF, bit 9 at $3000-$3FFF), 4-bit RAM bank, and
   crucially permits bank 0 in the switchable window (no MBC1-style
   rewrite). 18 tests pass.
7. **`format-nintendo-game-boy-cartridge`** — done. `CartridgeHeader::parse`
   decodes the $0100-$014F header, validates ROM length matches
   the size byte, recomputes the $0134-$014C header checksum, and
   surfaces clear errors for unknown / unsupported mapper bytes
   (MMM01, MBC6/7, Camera, Tama5, HuC1/3 are explicitly refused;
   MBC1/2/3/5 + ROM-only are accepted). `load(rom)` convenience
   builds a fully-loaded `Cartridge` from the `nintendo-game-boy-mbc`
   crate. 14 tests.
8. **`machine-nintendo-game-boy`** — done. `GameBoy` composes the
   SM83 + PPU + APU + timer + cartridge + 8 KiB WRAM + 8 KiB VRAM
   + 160 B OAM + 127 B HRAM + IF/IE + joypad + serial. Bus
   dispatch covers the full DMG memory map (incl. `$E000-$FDFF`
   echo RAM and `$FEA0-$FEFF` reads-as-`$FF`). Per-m-cycle
   orchestration ticks timer / APU / PPU at T-cycle rate, OR's
   IRQ sources from timer overflow / PPU VBlank / PPU STAT /
   joypad rising-edge / serial-transfer-complete into IF, then
   services the CPU's pin-level bus and ticks. Serial writes with
   `SC = $81` capture the byte to a buffer (Blargg's reporting
   channel) and latch the serial IRQ on the DIV-derived serial
   clock. OAM DMA is paced one byte per m-cycle with restart timing;
   CPU access to OAM/VRAM is blocked during the relevant PPU modes.
   The remaining DMA gap is full non-HRAM CPU bus blocking during
   OAM DMA. Boot-ROM execution is still deferred in favour of
   skipped-boot profiles. 22 unit tests passing.
9. **`runtime-nintendo-game-boy`** — done. `GameBoyRuntime` wraps
   the DMG-class machine behind `MachineCore`. `load_media` accepts a
   `Cartridge` image at slot `cartridge`, parses it via
   `format-nintendo-game-boy-cartridge`, and rebuilds the machine.
   `run_until` ticks `GameBoy::run_frame` until the requested
   `MachineTime` is reached, pushes `Indexed8` frames against the
   four-shade `DMG_GREYSCALE_RGBA` palette, and drains the APU's
   48 kHz interleaved-stereo float buffer per frame. Snapshots are
   postcard envelopes versioned + profile-id-checked so a CGB
   snapshot won't deserialise into a DMG runtime once that lands.
   Joypad input maps `a/b/select/start/up/down/left/right` (case-
   insensitive, accepted from either `Key` or `Button` events) to
   `JoypadButton`. The family catalogue exposes skipped-boot
   profiles for DMG0, DMG ABC, MGB, SGB, and SGB2. 13 tests passing.

Phase 1 is complete: 9 of 9 crates landed. The Game Boy now
boots through the runtime boundary on any header-valid
ROM-only / MBC1 / MBC2 / MBC3 / MBC5 cartridge.

The native verifier window uses `emu198x-native-video` for shared
`wgpu` presentation, matching the other current native windows. That
provides nearest-neighbour GPU presentation with centred integer
scaling and optional presentation filters via `--video raw|lcd|crt`.
Raw remains the default and headless captures remain runtime framebuffer
captures; the LCD mode is a host-side visual treatment, not a change to
the emulated framebuffer.

### Phase 2 — verification (done for current DMG scope)

Acceptance tests, in order:

- Blargg `cpu_instrs` — all 11 sub-tests passing. Gates opcode
  correctness.
- Blargg `instr_timing` — passing. Gates m-cycle counts per
  instruction.
- Blargg `mem_timing` (v1 + v2) — passing. Gates bus-access timing.
- mooneye-gb `acceptance/` — passing locally at m-cycle precision.
- mooneye-gb broad sweep — 103/103 passing locally across
  `acceptance`, `emulator-only/mbc1`, `emulator-only/mbc2`, and
  `emulator-only/mbc5`.
- `dmg-acid2.gb` — passing as a non-trivial PPU rendering smoke test.

The verification harness lives at
`crates/runtime-nintendo-game-boy/tests/phase2_verification.rs`.
It is `#[ignore]`'d by default and reads local corpora from:

- `EMU198X_GB_BLARGG_ROOT`
- `EMU198X_GB_MOONEYE_ROOT`
- `EMU198X_GB_DMG_ACID2_ROM`

### Phase 3 — CGB (later)

DMG is now green across the planned Phase 2 gate. The next major
family step is CGB. That's the trigger for the family-driver lift
called out in
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
