# Emu198x

A cycle-accurate emulator suite for vintage computing education.

## Why This Exists

Emu198x is the emulation engine for **Code Like It's 198x** (`~/Projects/Code198x`),
an educational site that teaches retro game development through complete, playable
games. Every architectural decision here — crystal-accurate timing, observable
state, MCP integration, scripting, WASM builds — exists to serve that teaching
mission. The emulators provide:

- **Automated capture** — screenshots and video for unit content
- **Browser embedding** — WASM builds for interactive lessons
- **Observability** — learners inspect registers, memory, and chip state live
- **Deterministic replay** — input scripts produce repeatable results
- **Save states** (planned) — lesson checkpoints and debugging workflows

Code198x has its own `CLAUDE.md` with curriculum structure, voice guidelines,
and build tooling. Its `docs/emulators/EMULATOR-ROADMAP.md` predates this
project and describes a different architecture — treat Emu198x docs as
authoritative for emulator design.

## Project Principles

1. **Crystal-accurate timing.** Every emulator ticks at the system's master crystal frequency. All component timing derives from this. No exceptions. No "good enough" approximations. See `docs/roadmap.md`. Do not try to take any shortcuts, for
instance by modelling instruction-level accuracy.

2. **Observable by design.** Internal state is always queryable. CPU registers, memory, video chip state, audio state — all exposed for education and debugging.

3. **Four systems first.** C64, ZX Spectrum, NES, Amiga. Everything else is future scope.

4. **Verification matters.** Every milestone has concrete pass/fail criteria using real software from TOSEC collections.

## Documentation Structure

Start from `docs/README.md` for navigation. Key entry points:

```text
docs/
├── README.md            # Documentation map and navigation
├── roadmap.md           # Active work and current priorities
├── status.md            # High-level support snapshot (dashboard)
├── inventory.md         # Architecture notes, crate inventory, testing strategy
├── testing-policy.md    # Verification standards for components and machines
├── testing-audit.md     # Crate-by-crate audit against the testing policy
├── future-systems.md    # Systems beyond the core four (out of scope)
├── systems/
│   ├── spectrum.md      # ZX Spectrum specifics and Emu198x status
│   ├── c64.md           # Commodore 64 specifics and Emu198x status
│   ├── nes.md           # NES/Famicom specifics and Emu198x status
│   └── amiga.md         # Amiga specifics, model matrix, Kickstart boot status
├── features/
│   ├── frontend.md      # UI, media controls, input mapping
│   ├── observability.md # State inspection, debugging
│   ├── mcp.md           # MCP server integration
│   ├── scripting.md     # Automation, headless operation
│   └── capture.md       # Screenshots, video, audio
└── solutions/           # Implementation, testing, and debugging notes
```

## Critical Constraints

**DO:**

- Tick at crystal frequency
- Derive all component timing from master clock
- Verify milestones with real software
- Expose internal state for observation
- Track phase relationships between components

**DO NOT:**

- Step by instruction instead of by tick
- Run CPU then "catch up" video
- Use floating-point for timing
- Assume CPU is the timing master
- Skip ticks for performance (skip frames instead)
- Treat cycle accuracy as optional

## Current Focus

See `docs/roadmap.md` for active priorities and `docs/status.md` for the support
snapshot. Spectrum and C64 are production-ready; NES and Amiga are usable with
known gaps.

## System Variants

Systems have variants. Don't model 50 machines — model the axes:

- **CPU variant** (same family, different speed/features)
- **Chipset generation** (different video/audio capabilities)
- **Memory configuration**
- **Peripherals**
- **Region** (PAL/NTSC)

See `docs/inventory.md` for the clock domain model and crate conventions.

Primary targets (what lessons target):

- Spectrum 48K PAL
- C64 PAL (SID 6581)
- NES NTSC
- Amiga 500 PAL (OCS, Kickstart 1.3)

Extended support (run user software):

- Spectrum 128K, +2, +2A, +3
- C64 NTSC, SID 8580, REU
- NES PAL, Famicom
- Amiga 500+, 600, 1200, 2000, 3000 (ECS, AGA)
- Accelerated configs (faster CPU + Fast RAM)

## Future Systems

**NOT IN SCOPE** until all four primary systems are complete.

See `docs/future-systems.md` for the full list, priority bias, component reuse
matrix, and candidate expansion order. Do not add these to milestones. Do not
build infrastructure for them. Structure code so they're *possible*, then forget
about them until the core four ship.

## Technology

- **Language:** Rust
- **Targets:** Native (Linux, macOS, Windows), WASM
- **Licence:** MIT

## Crate Structure

See `docs/inventory.md` for the full crate inventory, naming conventions, and
architecture notes. Summary of the workspace layout:

```text
emu198x/
├── Cargo.toml (workspace)
├── crates/
│   ├── emu-core/              # Shared traits: Bus, Cpu, Observable, Tickable, Machine
│   │
│   │   CPUs
│   ├── mos-6502/              # 6502 CPU
│   ├── zilog-z80/             # Z80 CPU
│   ├── motorola-68000/        # 68000–68040 family
│   ├── motorola-68010/        # 68010 wrapper
│   ├── motorola-68020/        # 68020 wrapper
│   │
│   │   Amiga custom chips
│   ├── commodore-agnus-ocs/   # Agnus OCS (beam, DMA, copper, blitter)
│   ├── commodore-agnus-ecs/   # Super Agnus (wraps OCS)
│   ├── commodore-agnus-aga/   # Alice (wraps ECS; FMODE, 8-plane)
│   ├── commodore-denise-ocs/  # Denise OCS (video output, bitplanes)
│   ├── commodore-denise-ecs/  # Super Denise (wraps OCS)
│   ├── commodore-denise-aga/  # Lisa (wraps ECS; 24-bit palette, HAM8)
│   ├── commodore-paula-8364/  # Paula (interrupts, audio/disk DMA)
│   ├── mos-cia-8520/          # CIA 8520 (Amiga)
│   │
│   │   Amiga support chips and peripherals
│   ├── commodore-gayle/       # Gayle (IDE + PCMCIA)
│   ├── commodore-dmac-390537/ # DMAC 390537 (A3000 SCSI stub)
│   ├── drive-amiga-floppy/    # 3.5" DD floppy
│   ├── peripheral-amiga-keyboard/ # Keyboard controller
│   │
│   │   Shared chips
│   ├── gi-ay-3-8910/          # AY-3-8910 PSG (Spectrum 128+)
│   ├── mos-sid-6581/          # SID 6581/8580 (C64)
│   ├── mos-vic-ii/            # VIC-II 6567/6569 (C64)
│   ├── mos-cia-6526/          # CIA 6526 (C64)
│   ├── mos-via-6522/          # VIA 6522 (1541 drive)
│   ├── sinclair-ula/          # Spectrum ULA (video, contention, INT)
│   ├── nec-upd765/            # uPD765 FDC (Spectrum +3)
│   ├── ricoh-ppu-2c02/        # PPU 2C02 (NES)
│   ├── ricoh-apu-2a03/        # APU 2A03 (NES)
│   │
│   │   Format crates
│   ├── format-adf/            # Amiga Disk File
│   ├── format-ipf/            # Interchangeable Preservation Format
│   ├── format-d64/            # Commodore D64 disk image
│   ├── format-gcr/            # Commodore 1541 GCR encoding
│   ├── format-c64-tap/        # C64 TAP tape image
│   ├── format-prg/            # C64 PRG file
│   ├── format-spectrum-tap/   # Spectrum TAP tape image
│   ├── format-tzx/            # TZX tape image
│   ├── format-sna/            # Spectrum SNA snapshot
│   ├── format-z80/            # Spectrum Z80 snapshot
│   ├── nes-cartridge/         # iNES cartridge + 14 mappers
│   │
│   │   Machine and runner crates
│   ├── machine-amiga/         # Amiga system (library)
│   ├── emu-amiga/             # Amiga runnable package
│   ├── emu-spectrum/          # Spectrum runnable package
│   ├── emu-c64/               # C64 runnable package
│   ├── emu-nes/               # NES runnable package
│   │
│   │   Test tooling
│   └── m68k-test-gen/         # Musashi cross-validation test generator
└── docs/
```

Each system is a **separate binary**. Libraries are shared — e.g. `emu-spectrum`
depends on `emu-core` + `zilog-z80` + `sinclair-ula` + `gi-ay-3-8910`, and
`machine-amiga` depends on `emu-core` + `motorola-68000` + the Amiga chip crates.
WASM builds are per-system.
