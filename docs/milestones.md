# Milestones (v1-focused, capture-first)

## Purpose

This roadmap is optimised to ship **Code Like It's 198x v1** with:

- Undeniable technical credibility
- High-quality, reproducible captured artefacts (images, video, audio)
- At least one complete, teachable path per core system

The emulators are **first-class projects**, but during v1 they are treated as
**instrumentation and content-production engines**, not general-purpose player emulators.

Emulator work beyond v1 is explicitly valuable, but **non-blocking**.

---

## Core Principles

- **Capture first**: emulator features must enable or improve observable, repeatable artefacts.
- **Demonstrable > complete**: systems are "done" when they can teach and show truth, not when they run most commercial software.
- **Observability is non-negotiable**: internal state must be inspectable.
- **Determinism beats convenience**: repeatable runs matter more than realism in v1.

---

## Status Overview

| Track   | Description                            | Status      |
|---------|----------------------------------------|-------------|
| Track A | Foundation & Instrumentation           | Complete    |
| Track B | System Demonstrability (v1)            | In progress |
| Track C | Completeness & Compatibility (Post-v1) | Sealed      |

---

## Track A: Foundation & Instrumentation (v1-blocking) ✅

All foundation milestones are complete.

### M1: Project Scaffolding ✅

Rust workspace with clear crate boundaries.

### M2: Z80 CPU Core ✅

Cycle-accurate Z80. 1,604,000/1,604,000 single-step tests pass.

### M3: 6502 CPU Core ✅

Per-cycle 6502 with illegal opcodes. 2,560,000/2,560,000 single-step tests pass.

### M4: 68000 CPU Core ✅

Full 680x0 family (68000–68040). 317,500 DL tests + 5,335,000 Musashi
cross-validation tests across 8 CPU variants, all passing.

### M37: Observability & Capture Infrastructure ✅

CPU registers, memory, video/audio chip state queryable on all systems.
Breakpoints and step-by-tick execution working.

### M38: Control Server (MCP) ✅

All four systems expose boot/reset, media insertion, run/pause/step,
screenshot/video/audio capture, and input injection via MCP JSON-RPC.

### M39: Headless Scripting ✅

All four systems accept `--script <file.json>` for batch execution.
Deterministic capture of video/audio via `save_path` parameters.

---

## Track B: System Demonstrability (v1-blocking)

A system is **Demonstrable** when it can:

- Boot deterministically
- Run a known-good or purpose-built program
- Produce stable video and audio
- Expose internal state for inspection
- Support scripted, repeatable capture

Broad commercial compatibility is **explicitly not required** for v1.

---

### ZX Spectrum — Demonstrable ✅

All required and optional features are implemented and working.

#### Implemented

- **Models**: 48K, 128K, +2, +2A, +3 (memory banking, ROM switching)
- **ULA**: Full 7 MHz video, bitmap + attributes, border, contention timing
- **Audio**: 1-bit beeper + AY-3-8910 PSG (128K+) with stereo ACB mode
- **Keyboard**: Full 48-key matrix, Kempston joystick
- **Tape**: TAP (ROM trap fast-load) + TZX (real-time signal playback)
- **Disk**: +3 FDC (NEC uPD765) plumbed in
- **Capture**: Screenshots (PNG), audio (WAV), video (MP4)
- **Runner**: Windowed + headless, MCP server, `--sna`/`--z80`/`--tap`/`--tzx`

#### Remaining for v1 exit

- One timing-sensitive visual demo (capture)
- One hero screenshot
- One audio capture
- One complete lesson draft

---

### Commodore 64 — Demonstrable ✅

Feature-complete. All required, optional, and most deferred features are implemented.

#### Implemented

- **Models**: PAL (6569) + NTSC (6567)
- **VIC-II**: All 6 display modes, XSCROLL/YSCROLL/CSEL/RSEL, sprite DMA
  cycle stealing, badline timing, raster IRQ
- **SID**: Both 6581 (non-linear filter, die-analysis waveforms) and 8580
  (linear filter, AND waveforms), 3 voices, ADSR, all waveform combinations
- **CIA**: TOD timers (model-aware dividers), keyboard matrix, FLAG pin
  (negative-edge for tape turbo loaders)
- **1541 Drive**: Full GCR encode/decode, half-track positioning, read + write,
  D64 save. Standalone 6502 + VIA1/VIA2
- **REU**: 128/256/512 KB, STASH/FETCH/SWAP/VERIFY DMA
- **Cartridges**: 7 types (Normal, Action Replay, Simon's BASIC, Ocean,
  Fun Play, Magic Desk, EasyFlash)
- **Tape**: TAP format with ROM trap + real-time pulse playback
- **Capture**: Screenshots (PNG), audio (WAV), video (MP4)
- **Runner**: Windowed + headless, MCP server, `--model`, `--sid`, `--reu`,
  `--d64`, `--prg`, `--type-text`

#### Remaining for v1 exit

- Clear visual explanation of badlines (capture)
- Recognisable SID audio example (capture)
- One hero visual
- One complete lesson draft

---

### NES/Famicom — Demonstrable ✅

Full PPU, APU, and broad mapper coverage. Runs most of the NES library.

#### Implemented

- **Regions**: NTSC (primary) + PAL
- **PPU (2C02)**: Dot-based rendering, background + sprites, sprite 0 hit,
  OAM evaluation, nametable mirroring
- **APU**: All 5 channels (2 pulse, triangle, noise, DMC), non-linear mixer,
  frame counter
- **Controllers**: Standard joypad + Zapper light gun (screen sampling)
- **Mappers** (14): NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4),
  AxROM (7), MMC2 (9), MMC4 (10), ColorDreams (11), BxROM (34),
  GxROM (66), Camerica (71), Mapper87, Mapper206
- **Battery saves**: SRAM detection from iNES header
- **Capture**: Screenshots (PNG), audio (WAV), video (MP4)
- **Runner**: Windowed + headless, MCP server, `--region ntsc|pal`

#### Remaining for v1 exit

- One pipeline-focused visual demo (capture)
- One captured sprite/timing example
- One complete lesson draft

---

### Amiga — Demonstrable ✅

Boots all OCS/ECS Kickstart versions. Workbench 1.3 reaches full desktop.

#### Implemented

- **Models**: A1000, A500, A2000 (OCS), A500+, A600, A3000 (ECS),
  A1200, A4000 (AGA — framework, display incomplete)
- **CPU**: 68000/010/020/030/040 with crystal-derived and independent clocks
- **Agnus**: OCS + ECS beam counter, DMA controller (bitplane/sprite/disk/audio),
  Copper coprocessor, Blitter (area + line mode)
- **Denise**: OCS + ECS bitplane video, display window, sprites.
  AGA: 8 bitplanes, 24-bit palette, HAM8, FMODE (partial)
- **Paula**: 4-channel audio DMA (14-bit), disk DMA, interrupt controller
- **CIA**: Two 8520s — keyboard, parallel, timers, TOD
- **Keyboard**: Full Amiga keycode encoding + handshake via CIA-A SP
- **Floppy**: ADF + IPF formats, MFM encode/decode, sector checksums,
  read + write. Motor spin-up, head stepping, disk change detection
- **Drive sounds**: Recorded samples (click + motor hum) from Freesound CC BY 4.0
- **Status bar**: Power LED + drive activity LED in runner window
- **Memory**: Chip RAM aliasing, slow RAM (A501), fast RAM (A3000 RAMSEY),
  unmapped reads return 0
- **A3000**: RAMSEY/Fat Gary stubs, PMOVE stub, 68030 instruction cache,
  reaches STRAP diagnostic display
- **Capture**: Screenshots (PNG), audio (WAV), video (MP4 with audio)
- **Runner**: Windowed + headless, MCP server, `--model`, `--chipset`,
  `--adf`, `--no-drive-sounds`

#### Kickstart boot status

| ROM | Model | Status |
|-----|-------|--------|
| KS 1.0 | A1000 | Yellow screen (no slow RAM) |
| KS 1.2 | A500, A2000 | Insert-disk screen ✅ |
| KS 1.3 | A500, A2000 | Insert-disk screen ✅ |
| KS 2.04 | A500+ | Insert-disk screen ✅ |
| KS 2.05 | A600 | Insert-disk screen ✅ |
| KS 3.1 | A500, A600, A2000 | Insert-disk screen ✅ |
| KS 2.02/3.1 | A3000 | STRAP reached, stalls on device init |
| KS 3.0/3.1 | A1200, A4000 | AGA display incomplete |
| WB 1.3 | A500 | Full desktop boot ✅ (~6 min emulated) |

#### Remaining for v1 exit

- One Copper/Blitter visual demo (capture)
- One audio DMA example (capture)
- One hero capture
- One complete lesson draft

---

## Track C: Completeness & Compatibility (Post-v1, sealed)

These milestones remain valuable but **cannot block v1 launch**.

Includes (non-exhaustive):

- Spectrum +3 disk loading, broader 128K software testing
- C64 broad D64/TOSEC compatibility
- NES additional mappers, accuracy test ROMs
- Amiga AGA display completion, A3000 SCSI/DMA stubs, broad game compatibility
- Polished emulator UI
- Web frontend via WASM

Work in this track resumes **only after**:

- Code Like It's 198x v1 ships
- At least one lesson per system is public

---

## Stop Clause (Important)

> Emulator development may continue after v1, but **Code Like It's 198x lessons and site content become the primary driver of further emulator work**.

This clause exists to be argued with — and noticed when broken.

---

## Final Note

This roadmap does not reduce ambition.

It makes ambition **pay rent** by producing artefacts, lessons, and public proof before pursuing completeness.
