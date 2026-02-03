# Milestones

## Current Status

| Phase           | Progress        |
|-----------------|-----------------|
| 1. Foundation   | M1 ✅ / M2 ✅ / M3-M4 ⬜ |
| 2. ZX Spectrum  | ⬜               |
| 3. Commodore 64 | ⬜               |
| 4. NES/Famicom  | ⬜               |
| 5. Amiga        | ⬜               |
| 6. Integration  | ⬜               |
| 7. Frontend     | ⬜               |

**Next:** M3 (6502 CPU Core)

---

Each milestone has:

- **Deliverable**: What is built
- **Verification**: How to prove it works (using TOSEC where applicable)
- **Links**: Relevant documentation

A milestone is complete when ALL verification criteria pass.

---

## Phase 1: Foundation

### M1: Project Scaffolding ✅

**Deliverable:** Rust workspace with crate structure.

```text
emu198x/
├── Cargo.toml (workspace)
├── crates/
│   ├── emu-core/        # Shared traits, types
│   ├── emu-spectrum/    # ZX Spectrum
│   ├── emu-c64/         # Commodore 64
│   ├── emu-nes/         # NES/Famicom
│   ├── emu-amiga/       # Amiga
│   └── emu-frontend/    # Native UI
└── docs/
```

**Verification:**

- [x] `cargo build` succeeds
- [x] `cargo test` runs (empty tests OK)
- [x] Core traits defined per `docs/architecture.md`

**Links:** [architecture.md](architecture.md)

---

### M2: Z80 CPU Core ✅

**Deliverable:** Z80 CPU implementation with per-tick execution.

**Verification:**

- [x] ZEXDOC passes (documented instructions)
- [x] ZEXALL passes (undocumented instructions)
- [x] CPU exposes `Observable` trait
- [x] Each `tick()` advances exactly one T-state

**Implementation complete:**

- Full instruction set (~700 opcodes) with T-state accurate micro-ops
- All prefix groups: unprefixed, CB, DD, ED, FD, DDCB, FDCB
- Undocumented instructions: IXH/IXL/IYH/IYL, SLL, etc.
- Bus architecture supports memory contention (wait states)
- Observable trait exposes registers, flags, and timing state

**Links:** [spectrum.md](systems/spectrum.md)

**Verification binaries:**

```text
zexdoc.com - Tests documented Z80 instructions
zexall.com - Tests all instructions including undocumented
```

Run with: `cargo test --package emu-z80 -- --nocapture zex`

---

### M3: 6502 CPU Core

**Deliverable:** 6502 CPU implementation with per-cycle execution.

**Verification:**

- [ ] Klaus Dormann test suite passes (all documented opcodes)
- [ ] Decimal mode tests pass
- [ ] Illegal opcode tests pass (basic set)
- [ ] CPU exposes `Observable` trait
- [ ] Each `tick()` advances exactly one CPU cycle
- [ ] Memory access timing matches per-cycle expectations

**Links:** [c64.md](systems/c64.md), [nes.md](systems/nes.md)

**Verification binaries:**

```text
6502_functional_test.bin (Klaus Dormann)
6502_decimal_test.bin
```

---

### M4: 68000 CPU Core

**Deliverable:** 68000 CPU implementation.

**Verification:**

- [ ] Basic instruction tests pass
- [ ] Address modes verified
- [ ] Exception handling correct
- [ ] CPU exposes `Observable` trait
- [ ] Bus cycle timing accurate (4 clock minimum)

**Links:** [amiga.md](systems/amiga.md)

---

## Phase 2: ZX Spectrum

### M5: Spectrum Memory Map

**Deliverable:** 48K Spectrum memory layout with ROM.

**Verification:**

- [ ] 16K ROM at 0x0000-0x3FFF
- [ ] 48K RAM at 0x4000-0xFFFF
- [ ] Screen memory at 0x4000-0x5AFF (bitmap) and 0x5800-0x5AFF (attributes)
- [ ] ROM loaded from Spectrum 48K ROM image

**Links:** [spectrum.md](systems/spectrum.md)

**TOSEC:**

```text
Sinclair ZX Spectrum/Firmware/ZX Spectrum (1982)(Sinclair Research).rom
```

---

### M6: Spectrum ULA - Basic Video

**Deliverable:** ULA generating video output (no contention yet).

**Verification:**

- [ ] Border colour renders correctly
- [ ] Screen bitmap displays
- [ ] Attributes (INK/PAPER/BRIGHT/FLASH) render correctly
- [ ] Frame timing: 69888 T-states per frame (PAL)
- [ ] 312 scanlines, 224 T-states per line

**Manual verification:**

- [ ] Load and display any SCREEN$ file
- [ ] Border colour changes visible

**Links:** [spectrum.md](systems/spectrum.md)

---

### M7: Spectrum ULA - Contention

**Deliverable:** Accurate memory contention timing.

**Verification:**

- [ ] Contention pattern: 6,5,4,3,2,1,0,0 repeated during screen fetch
- [ ] Contention affects addresses 0x4000-0x7FFF
- [ ] I/O contention on ULA ports

**TOSEC verification (timing-sensitive demos):**

```text
Sinclair ZX Spectrum/Demos/Shock Megademo (1990)(Raww Arse).tap
```

**Links:** [spectrum.md](systems/spectrum.md)

---

### M8: Spectrum Keyboard

**Deliverable:** Keyboard input via I/O port scanning.

**Verification:**

- [ ] All 40 keys register correctly
- [ ] Multiple simultaneous keys work
- [ ] Keyboard I/O port 0xFE responds correctly

**Manual verification:**

- [ ] Type in BASIC prompt
- [ ] Play a game requiring keyboard

**TOSEC:**

```text
Sinclair ZX Spectrum/Games/Manic Miner (1983)(Bug-Byte Software).tap
```

---

### M9: Spectrum Tape Loading

**Deliverable:** TAP file loading.

**Verification:**

- [ ] Standard speed loading works
- [ ] LOAD "" triggers tape playback
- [ ] Edge pulses generated correctly
- [ ] Loading completes without errors

**TOSEC verification:**

```text
Sinclair ZX Spectrum/Games/Jet Set Willy (1984)(Software Projects).tap
Sinclair ZX Spectrum/Games/Elite (1985)(Firebird Software).tap
```

**Links:** [spectrum.md](systems/spectrum.md)

---

### M10: Spectrum Beeper Audio

**Deliverable:** 1-bit beeper sound output.

**Verification:**

- [ ] Port 0xFE bit 4 controls speaker
- [ ] Audio buffer generated at correct sample rate
- [ ] No crackling or timing drift

**TOSEC verification:**

```text
Sinclair ZX Spectrum/Games/Manic Miner (1983)(Bug-Byte Software).tap
```

(Has recognisable beeper music)

---

### M11: Spectrum 128K

**Deliverable:** 128K memory paging, AY-3-8912 audio.

**Verification:**

- [ ] 128K RAM paging via port 0x7FFD
- [ ] ROM switching (48K BASIC / 128K BASIC / 128K Editor)
- [ ] AY sound chip produces correct output
- [ ] 128K-specific software runs

**TOSEC verification:**

```text
Sinclair ZX Spectrum/Firmware/ZX Spectrum +2 (1986)(Amstrad).rom
Sinclair ZX Spectrum/Games/Chase H.Q. (1989)(Ocean Software)[128K].tap
```

---

### M12: Spectrum System Complete

**Deliverable:** Full Spectrum emulation ready for use.

**Verification (broad compatibility):**

- [ ] Manic Miner — plays correctly with audio
- [ ] Jet Set Willy — plays correctly
- [ ] Elite — loads and runs (multiple loaders)
- [ ] Chase H.Q. (128K) — uses AY audio
- [ ] Shock Megademo — timing-sensitive effects work
- [ ] At least 95% of tested TOSEC games boot

**Links:** [spectrum.md](systems/spectrum.md)

---

## Phase 3: Commodore 64

### M13: C64 Memory Map

**Deliverable:** C64 memory layout with banking.

**Verification:**

- [ ] BASIC ROM, Kernal ROM, Character ROM mapped correctly
- [ ] RAM banking via CPU port ($01)
- [ ] I/O area at $D000-$DFFF

**TOSEC:**

```text
Commodore C64/Firmware/Commodore 64 - Kernal (1982)(Commodore).bin
Commodore C64/Firmware/Commodore 64 - BASIC (1982)(Commodore).bin
Commodore C64/Firmware/Commodore 64 - Character (1982)(Commodore).bin
```

**Links:** [c64.md](systems/c64.md)

---

### M14: C64 VIC-II Basic Video

**Deliverable:** VIC-II generating display (no sprites/scrolling yet).

**Verification:**

- [ ] Text mode displays correctly
- [ ] Hi-res bitmap mode works
- [ ] Multi-colour bitmap mode works
- [ ] Border and background colours correct
- [ ] Frame timing: 312 lines (PAL), 63 cycles per line

**Manual verification:**

- [ ] Boot to BASIC prompt with correct colours
- [ ] POKE 53280,0 changes border to black

---

### M15: C64 VIC-II Badlines

**Deliverable:** Accurate badline timing.

**Verification:**

- [ ] CPU stalled during character fetch
- [ ] Badline occurs every 8 raster lines in visible area
- [ ] Sprite DMA steals cycles correctly

**TOSEC verification (timing-sensitive):**

```text
Commodore C64/Demos/Crest - Deus Ex Machina (1994)(Crest).d64
```

---

### M16: C64 VIC-II Sprites

**Deliverable:** Hardware sprites.

**Verification:**

- [ ] 8 sprites display correctly
- [ ] Sprite priorities (over/under background)
- [ ] Sprite-sprite collision
- [ ] Sprite-background collision
- [ ] Multi-colour sprites
- [ ] Sprite stretching (X and Y expand)

**TOSEC verification:**

```text
Commodore C64/Games/Impossible Mission (1984)(Epyx).d64
```

---

### M17: C64 CIA Timers

**Deliverable:** CIA chips with timers and I/O.

**Verification:**

- [ ] Timer A and B on both CIAs
- [ ] Timer underflow interrupts
- [ ] Keyboard scanning via CIA
- [ ] Joystick ports via CIA

**Manual verification:**

- [ ] Type on BASIC prompt
- [ ] Joystick input in games

---

### M18: C64 SID Audio

**Deliverable:** SID sound chip emulation.

**Verification:**

- [ ] Three oscillators with all waveforms
- [ ] ADSR envelope
- [ ] Filter (low-pass, high-pass, band-pass)
- [ ] Ring modulation
- [ ] Sync

**TOSEC verification:**

```text
Commodore C64/Games/Impossible Mission (1984)(Epyx).d64
```

(Famous "Another visitor. Stay a while... stay forever!" speech samples)

---

### M19: C64 1541 Disk Drive

**Deliverable:** 1541 drive emulation (D64 support).

**Verification:**

- [ ] D64 image loading
- [ ] Directory listing (LOAD "$",8)
- [ ] File loading (LOAD "FILENAME",8)
- [ ] Sequential file access
- [ ] Basic fast loader support

**TOSEC verification:**

```text
Commodore C64/Games/Boulder Dash (1984)(First Star Software).d64
```

**Links:** [c64.md](systems/c64.md)

---

### M20: C64 System Complete

**Deliverable:** Full C64 emulation ready for use.

**Verification (broad compatibility):**

- [ ] Impossible Mission — plays with speech samples
- [ ] Boulder Dash — plays correctly
- [ ] Elite — loads, plays, saves
- [ ] Crest demos — timing-sensitive effects
- [ ] At least 95% of tested TOSEC games boot from D64

**Links:** [c64.md](systems/c64.md)

---

## Phase 4: NES/Famicom

### M21: NES Memory Map

**Deliverable:** NES memory layout.

**Verification:**

- [ ] 2K internal RAM (mirrored)
- [ ] PPU registers at $2000-$2007
- [ ] APU/IO at $4000-$4017
- [ ] Cartridge space at $4020-$FFFF

**Links:** [nes.md](systems/nes.md)

---

### M22: NES PPU Background

**Deliverable:** PPU rendering backgrounds.

**Verification:**

- [ ] Nametable rendering
- [ ] Pattern table access
- [ ] Palette handling
- [ ] Scrolling (single screen)
- [ ] VBlank timing and NMI

**Manual verification:**

- [ ] Static title screens display correctly

---

### M23: NES PPU Sprites

**Deliverable:** PPU sprite rendering.

**Verification:**

- [ ] OAM sprite display
- [ ] 8x8 and 8x16 sprites
- [ ] Sprite priority
- [ ] Sprite 0 hit detection
- [ ] 8 sprite per scanline limit

---

### M24: NES PPU Timing

**Deliverable:** Cycle-accurate PPU.

**Verification:**

- [ ] PPU runs at 3× CPU rate
- [ ] Sprite 0 hit timing correct
- [ ] VBlank flag timing correct
- [ ] blargg ppu_vbl_nmi tests pass

**Test ROMs:**

```text
ppu_vbl_nmi.nes (blargg)
sprite_hit_tests_2005.10.05.nes
```

---

### M25: NES APU

**Deliverable:** Audio Processing Unit.

**Verification:**

- [ ] Two pulse channels
- [ ] Triangle channel
- [ ] Noise channel
- [ ] DMC (sample playback)
- [ ] Frame counter timing
- [ ] blargg apu tests pass

**Test ROMs:**

```text
apu_test.nes (blargg)
```

---

### M26: NES Controller Input

**Deliverable:** Controller input.

**Verification:**

- [ ] Standard controller read via $4016/$4017
- [ ] Strobe/shift register behavior

---

### M27: NES Mapper 0 (NROM)

**Deliverable:** Basic cartridge support.

**Verification:**

- [ ] 16K/32K PRG ROM
- [ ] 8K CHR ROM
- [ ] Horizontal/vertical mirroring

**TOSEC verification:**

```text
Nintendo NES/Games/Super Mario Bros. (1985)(Nintendo).nes
Nintendo NES/Games/Donkey Kong (1981)(Nintendo).nes
```

---

### M28: NES Mapper 1 (MMC1)

**Deliverable:** MMC1 mapper.

**Verification:**

- [ ] PRG banking
- [ ] CHR banking
- [ ] Mirroring control

**TOSEC verification:**

```text
Nintendo NES/Games/Legend of Zelda, The (1986)(Nintendo).nes
Nintendo NES/Games/Metroid (1986)(Nintendo).nes
```

---

### M29: NES Additional Mappers

**Deliverable:** MMC3, UxROM, CNROM.

**Verification:**

- [ ] MMC3 (Mapper 4) with IRQ
- [ ] UxROM (Mapper 2)
- [ ] CNROM (Mapper 3)

**TOSEC verification:**

```text
Nintendo NES/Games/Super Mario Bros. 3 (1988)(Nintendo).nes [MMC3]
Nintendo NES/Games/Mega Man (1987)(Capcom).nes [UxROM]
```

---

### M30: NES System Complete

**Deliverable:** Full NES emulation ready for use.

**Verification (broad compatibility):**

- [ ] Super Mario Bros. — plays correctly
- [ ] Legend of Zelda — plays, saves work
- [ ] Super Mario Bros. 3 — MMC3 IRQ effects correct
- [ ] Mega Man 2 — plays correctly
- [ ] At least 90% of tested TOSEC games boot

**Links:** [nes.md](systems/nes.md)

---

## Phase 5: Amiga

### M31: Amiga Memory Map

**Deliverable:** Amiga memory layout.

**Verification:**

- [ ] Chip RAM at $000000
- [ ] Kickstart ROM at $F80000 (or $FC0000)
- [ ] Custom chip registers at $DFF000
- [ ] CIA registers at $BFE001/$BFD000

**TOSEC:**

```text
Commodore Amiga/Firmware/Kickstart v1.3 (1987)(Commodore)(A500-A1000-A2000).rom
```

**Links:** [amiga.md](systems/amiga.md)

---

### M32: Amiga Chipset - Agnus/DMA

**Deliverable:** DMA controller and copper.

**Verification:**

- [ ] Bitplane DMA
- [ ] Sprite DMA
- [ ] Copper executes display lists
- [ ] Blitter basic operations

---

### M33: Amiga Chipset - Denise/Video

**Deliverable:** Video output.

**Verification:**

- [ ] Bitplane display (1-6 planes)
- [ ] Hardware sprites
- [ ] Colour palette
- [ ] HAM mode (Amiga 500)

---

### M34: Amiga Chipset - Paula/Audio

**Deliverable:** Audio output.

**Verification:**

- [ ] 4 DMA audio channels
- [ ] Volume and period control
- [ ] Audio interrupts

---

### M35: Amiga Disk Support

**Deliverable:** ADF floppy support.

**Verification:**

- [ ] ADF image loading
- [ ] MFM decoding
- [ ] DMA disk reads
- [ ] Boot from floppy

**TOSEC verification:**

```text
Commodore Amiga/Games/Shadow of the Beast (1989)(Psygnosis).adf
```

---

### M36: Amiga System Complete

**Deliverable:** Full Amiga 500 emulation.

**Verification (broad compatibility):**

- [ ] Workbench 1.3 boots
- [ ] Shadow of the Beast — plays with copper effects
- [ ] Turrican — plays correctly
- [ ] State of the Art demo — timing effects work
- [ ] At least 85% of tested TOSEC games boot

**Links:** [amiga.md](systems/amiga.md)

---

## Phase 6: Integration

### M37: Observability Infrastructure

**Deliverable:** State inspection across all systems.

**Verification:**

- [ ] Query CPU registers
- [ ] Query memory ranges
- [ ] Query video chip state
- [ ] Query audio chip state
- [ ] Breakpoint support
- [ ] Step-by-tick execution

**Links:** [observability.md](features/observability.md)

---

### M38: MCP Server

**Deliverable:** MCP server exposing emulator control.

**Verification:**

- [ ] Boot/reset commands
- [ ] Media insertion
- [ ] State queries
- [ ] Execution control (run, step, pause)
- [ ] Screenshot capture
- [ ] Input injection

**Links:** [mcp.md](features/mcp.md)

---

### M39: Headless Scripting

**Deliverable:** Automation without GUI.

**Verification:**

- [ ] JSON command protocol
- [ ] Batch execution
- [ ] Content capture (screenshots, video, audio)
- [ ] Integration with Code Like It's 198x pipeline

**Links:** [scripting.md](features/scripting.md), [integration.md](integration.md)

---

### M40: WASM Build

**Deliverable:** Emulators compile to WASM.

**Verification:**

- [ ] `wasm32-unknown-unknown` target builds
- [ ] JavaScript API wrapper
- [ ] Runs in browser (where legal per BIOS requirements)

**Links:** [integration.md](integration.md)

---

## Phase 7: Frontend

### M41: System Launcher UI

**Deliverable:** Each binary has a launcher screen for configuration.

**Verification:**

- [ ] Spectrum launcher: model selection (48K/128K/+2/+3), region, peripherals
- [ ] C64 launcher: region, SID revision, expansions (REU)
- [ ] NES launcher: NES/Famicom, region, FDS option
- [ ] Amiga launcher: preset models, custom config (chipset, CPU, RAM, Kickstart)
- [ ] Recent sessions list per system
- [ ] "Load File..." opens picker and can launch directly
- [ ] Command-line flags bypass launcher (--start, --model, etc.)

**Links:** [frontend.md](features/frontend.md)

---

### M42: Media Controls — Tape

**Deliverable:** Visual tape deck widget.

**Verification:**

- [ ] Play, Stop, Rewind, Fast Forward, Eject buttons
- [ ] Progress bar showing tape position
- [ ] Block counter for multi-block tapes
- [ ] Motor activity indicator
- [ ] Drag-and-drop TAP/TZX files onto deck
- [ ] All controls have programmatic equivalent

**Links:** [frontend.md](features/frontend.md)

---

### M43: Media Controls — Disk

**Deliverable:** Visual disk drive widget.

**Verification:**

- [ ] Insert/Eject functionality
- [ ] Activity LED
- [ ] Track position display
- [ ] Multiple drives for Amiga (DF0:-DF3:)
- [ ] Drag-and-drop D64/ADF files onto drive
- [ ] All controls have programmatic equivalent

**Links:** [frontend.md](features/frontend.md)

---

### M44: Input Configuration

**Deliverable:** Keyboard, joystick, and mouse mapping UI.

**Verification:**

- [ ] Keyboard mapping modes (positional, symbolic, custom)
- [ ] Joystick mapping to keyboard or gamepad
- [ ] Multi-port assignment (Port 1, Port 2)
- [ ] Mouse capture and sensitivity
- [ ] Swap ports function
- [ ] Configuration persists across sessions

**Links:** [frontend.md](features/frontend.md)

---

### M45: Display Options

**Deliverable:** Video output configuration.

**Verification:**

- [ ] Aspect ratio correction (native, 4:3, stretch)
- [ ] Scaling modes (nearest, integer, bilinear, CRT shader)
- [ ] Border options (none, visible, overscan)
- [ ] Fullscreen toggle (F11)
- [ ] Palette selection per system

**Links:** [frontend.md](features/frontend.md)

---

### M46: Web Frontend

**Deliverable:** Browser-based UI via WASM.

**Verification:**

- [ ] System selection
- [ ] Canvas rendering at correct aspect
- [ ] File input for media (fallback for no drag-drop)
- [ ] Touch controls for mobile
- [ ] Keyboard input capture
- [ ] Audio playback (Web Audio API)

**Links:** [frontend.md](features/frontend.md), [integration.md](integration.md)

---

## Milestone Status Key

- ⬜ Not started
- 🟨 In progress
- ✅ Complete
- ❌ Blocked

---

## Verification Notes

### TOSEC Collection Paths

Organise TOSEC by system for verification:

```text
tosec/
├── sinclair-zx-spectrum/
├── commodore-c64/
├── nintendo-nes/
└── commodore-amiga/
```

### Running Verification

Each milestone verification should be automated where possible:

```bash
# Run milestone verification
cargo test --package emu-spectrum milestone_m7
```

### Documenting Failures

If a milestone verification fails, document:

1. Which specific test failed
2. Expected vs actual behaviour
3. Relevant system state at failure
