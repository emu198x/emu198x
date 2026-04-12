---
title: "Per-System Trait Implementation, Save States, and Rewind"
type: feat
date: 2026-03-21
---

# Per-System Trait Implementation, Save States, and Rewind

## Context

The unified app has all the UI panels, commands, and infrastructure in
place, but most per-system trait methods return defaults. The palette
viewer shows "no data", save states return None, the tape deck has no
transport, and sprite/tile viewers are empty. This plan implements the
trait methods system by system, then builds save state serialisation
and the rewind feature on top.

## Principles

- Work system by system, not feature by feature. Completing one system
  end-to-end is more valuable than implementing one method across 13
  systems.
- Start with ZX Spectrum (simplest state model), then NES (best test
  coverage), then C64 (most complex of the core four), then Amiga.
- Each system gets a single commit covering all its trait implementations.
- Save states and rewind build on the per-system work.

## Phase 13: Spectrum Trait Implementation

The Spectrum 48K has the simplest state: Z80 registers + 48KB RAM +
ULA state + beeper + optional AY.

### 13a: Chip inspector methods
- `palette_info()` — fixed 16-colour palette (8 colours × bright bit)
- `sprite_info()` — N/A (Spectrum has no hardware sprites)
- `pattern_table()` — character cell patterns from screen memory
  ($4000–$57FF attribute area drives colour, $4000–$5AFF is bitmap)
- `memory_map()` — ROM $0000–$3FFF, RAM $4000–$FFFF (48K), or
  banked layout for 128K
- `input_state()` — keyboard matrix state + Kempston joystick
- `debug_read()` — already implemented
- `debug_write()` — write to RAM (skip ROM range)

### 13b: Tape transport
- `tape_status()` — position, length, playing state from TapeDeck
- `tape_command()` — play, stop, rewind, eject mapped to TapeDeck methods

### 13c: Peripheral status
- `peripheral_status()` — tape motor LED, border colour

### 13d: Save state
- Binary format: version byte + Z80 registers + RAM + ULA state +
  AY registers + master clock + tape position
- `save_state()` / `load_state()` implementation
- Verify: save during boot, load, continue — screen should be identical

## Phase 14: NES Trait Implementation

### 14a: Chip inspectors
- `palette_info()` — NES palette: 64 system colours + 32 PPU palette
  RAM entries. Both returned (system palette for reference, active
  palette for what's on screen)
- `sprite_info()` — read all 64 OAM entries. Each has x, y, tile,
  attributes (flip, priority, palette). Render sprite pixels from
  pattern table using the sprite's palette.
- `pattern_table()` — two 256-tile pattern tables from CHR ROM/RAM.
  Render each tile using greyscale (or palette 0).
- `memory_map()` — $0000–$07FF RAM, $2000–$2007 PPU, $4000–$4017 APU,
  $4020–$FFFF cartridge (banked by mapper)
- `input_state()` — controller 1/2 button states from Controller struct
- `debug_write()` — write to RAM range

### 14b: Save state
- Version byte + CPU registers + 2KB RAM + PPU state (VRAM, OAM,
  palette, scroll, control/mask/status registers) + APU state +
  mapper state (bank registers, PRG/CHR RAM)
- Mapper state is the challenge — each mapper has different fields.
  Add a `save_state()`/`load_state()` to the Mapper trait.

## Phase 15: C64 Trait Implementation

### 15a: Chip inspectors
- `palette_info()` — fixed VIC-II 16-colour palette
- `sprite_info()` — 8 VIC-II sprites from $D000–$D01F registers +
  sprite data pointers from screen memory. Render 24×21 (or 48×21
  multicolour) sprite pixels.
- `pattern_table()` — character ROM (256 8×8 characters)
- `memory_map()` — complex C64 banking: RAM $0000–$FFFF with
  ROM/IO overlays controlled by $01 processor port
- `input_state()` — keyboard matrix + joystick ports
- `debug_write()` — write to RAM (bypass banking)

### 15b: Tape and drive status
- `tape_status()` — C64TapeDeck position/state
- `tape_command()` — transport controls
- `peripheral_status()` — 1541 drive LED, motor, track number

### 15c: Printer hookup
- `has_printer()` → true
- `printer_output()` — intercept IEC device #4 serial bus output,
  buffer bytes for the virtual printer

### 15d: Save state
- CPU + 64KB RAM + VIC-II state (all registers, raster position,
  sprite DMA state) + SID state + CIA×2 state + optional 1541
  drive state (CPU + 2KB RAM + VIA×2 + GCR state)

## Phase 16: Amiga Trait Implementation

### 16a: Chip inspectors
- `palette_info()` — OCS: 32 colours from $DFF180–$DFF1BE.
  AGA: 256 colours from 24-bit palette RAM.
- `sprite_info()` — 8 hardware sprite channels from SPRxPT/SPRxPOS/
  SPRxCTL. Render sprite data from DMA pointers.
- `memory_map()` — chip RAM, slow RAM, fast RAM, ROM, custom
  registers, CIA space — varies by model
- `input_state()` — keyboard + mouse + joystick from CIA/custom regs

### 16b: Save state
- Largest state: CPU + chip RAM (512K–2MB) + fast RAM + all custom
  chip registers + CIA×2 + floppy drive state + disk contents
- Compression essential — delta encoding or zlib on the RAM blobs

## Phase 17: Remaining Systems

Apply the same pattern to the 9 remaining systems. Group by shared
chip (e.g. TMS9918 systems share palette/pattern logic):

- **TMS9918 group** (SG-1000, MSX, ColecoVision): shared palette
  (fixed 16 colours), shared pattern table rendering, shared
  sprite rendering (32 sprites, 8×8 or 16×16)
- **Atari group** (2600, 5200, 7800, 800XL): ANTIC/GTIA/POKEY
  shared state, TIA palette for 2600
- **SMS**: Sega VDP palette (32 colours from CRAM), pattern tables,
  64 sprites from SAT
- **BBC Micro**: 8 video modes, character ROM, 16-colour palette
  via Video ULA

## Phase 18: Rewind / Time-Travel Debugging

Depends on save states working for at least one system.

### 18a: Ring buffer infrastructure
- `RewindBuffer` struct: fixed-size ring of `(frame_number, state_blob)` pairs
- Configurable snapshot interval (default: every 60 frames = ~1/sec)
- Configurable depth (default: 30 seconds = 30 snapshots for Spectrum,
  fewer for Amiga due to size)
- Input recording: store key events between snapshots for replay

### 18b: Rewind commands
- `Command::Rewind(target_frame)` — find nearest prior snapshot,
  load it, replay frames + input events to reach the target
- `Command::StepBack` — rewind one instruction (snapshot + replay
  to one instruction before current)

### 18c: Rewind UI
- Timeline scrubber below the viewport (horizontal bar showing
  snapshot positions)
- Step Back button in debugger toolbar alongside Step Forward
- "Hold to rewind" button that plays backwards in real time
- Frame number display on the timeline

## Phase 19: Flow Analysis

Recursive descent disassembler that builds a control flow graph.

### 19a: CFG builder
- Start from known entry points (reset vector, NMI vector, IRQ vector)
- Linear sweep until branch/jump/return
- At conditional branches: fork both paths
- At calls: follow target, continue after call
- At indirect jumps: stop (mark as unresolved)
- Output: list of basic blocks with edges

### 19b: CFG UI
- Scrollable graph view (egui doesn't have a graph layout library,
  so use a simple vertical list with indented call targets)
- Click a block to jump to it in the disassembly view
- Colour code: entry points, subroutines, interrupt handlers

### 19c: Function detection
- Heuristic: any address that's a CALL/JSR target is a function entry
- Name auto-generation: `sub_$XXXX` for unlabelled functions
- Merge with symbol table — user labels override auto-generated names

## Phase 20: WASM Integration

### 20a: Spectrum WASM
- `machine-sinclair-zx-spectrum` compiles to WASM with no changes
- JS shell: canvas for framebuffer, AudioWorklet for audio,
  keyboard events → handle_key()
- Embed ROM_48K in the WASM binary (it's in emu-sinclair-zx-spectrum)

### 20b: Lesson harness
- Standardised JS API: `create(system)`, `loadMedia(data)`,
  `runFrame()`, `framebuffer()`, `handleKey(key, pressed)`
- Embeddable `<emu198x-player>` web component
- State inspection API for lesson validation

## Implementation Order

| Order | Phase | Effort | Depends on |
|-------|-------|--------|------------|
| 1 | 13 Spectrum traits | 1 day | — |
| 2 | 14 NES traits | 1–2 days | — |
| 3 | 15 C64 traits | 2 days | — |
| 4 | 18a–c Rewind | 1–2 days | 13d or 14b |
| 5 | 16 Amiga traits | 2–3 days | — |
| 6 | 17 Remaining systems | 3–4 days | — |
| 7 | 19 Flow analysis | 2–3 days | — |
| 8 | 20 WASM integration | 2–3 days | 13 |

Phases 13–15 can be parallelised (independent systems). Rewind (18)
can start as soon as any one system has save states. Flow analysis (19)
and WASM (20) are independent of each other.
