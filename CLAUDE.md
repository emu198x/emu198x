# Emu198x

A cycle-accurate emulator suite for vintage computing education.

## Project Principles

1. **Crystal-accurate timing.** Every emulator ticks at the system's master crystal frequency. All component timing derives from this. No exceptions. No "good enough" approximations. See `docs/architecture.md`. Do not try to take any shortcuts, for
instance by modelling instruction-level accuracy.

2. **Observable by design.** Internal state is always queryable. CPU registers, memory, video chip state, audio state — all exposed for education and debugging.

3. **Four systems first.** C64, ZX Spectrum, NES, Amiga. Everything else is future scope.

4. **Verification matters.** Every milestone has concrete pass/fail criteria using real software from TOSEC collections.

## Documentation Structure

```text
docs/
├── architecture.md      # Timing model, core traits, constraints
├── milestones.md        # Granular milestones with verification
├── integration.md       # Code Like It's 198x integration
├── future-systems.md    # Out of scope until Phase 6
|-- constraints.md       # Things you must never do
├── systems/
│   ├── c64.md           # Commodore 64 specifics
│   ├── spectrum.md      # ZX Spectrum specifics
│   ├── nes.md           # NES/Famicom specifics
│   └── amiga.md         # Amiga specifics
└── features/
    ├── frontend.md      # UI, media controls, input mapping
    ├── observability.md # State inspection, debugging
    ├── mcp.md           # MCP server integration
    ├── scripting.md     # Automation, headless operation
    └── capture.md       # Screenshots, video, audio
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

See `docs/milestones.md` for current work.

## System Variants

Systems have variants. Don't model 50 machines — model the axes:

- **CPU variant** (same family, different speed/features)
- **Chipset generation** (different video/audio capabilities)
- **Memory configuration**
- **Peripherals**
- **Region** (PAL/NTSC)

See `docs/architecture.md` for the configuration approach.

Primary targets (what lessons target):

- Spectrum 48K PAL
- C64 PAL (SID 6581)
- NES NTSC
- Amiga 500 PAL (OCS, Kickstart 1.3)

Extended support (run user software):

- Spectrum 128K, +2, +3
- C64 NTSC, SID 8580
- NES PAL, Famicom
- Amiga 500+, 600, 1200 (ECS, AGA)
- Accelerated configs (faster CPU + Fast RAM)

## Future Systems

**NOT IN SCOPE** until Phase 6 (all four primary systems) is complete.

These are plausible future additions based on shared CPU cores:

**6502 family (after C64/NES):**

- VIC-20 — Minimal extra work, simpler VIC
- BBC Micro — UK educational importance, different video (6845)
- Atari 8-bit (400/800/XL/XE) — ANTIC/GTIA video
- Atari 2600 — TIA, racing-the-beam paradigm
- Apple II — Important historically

**Z80 family (after Spectrum):**

- Amstrad CPC — Big in UK/Europe, Gate Array video
- Master System — VDP video, cartridge-based
- MSX — TMS9918 video, Japanese market
- SAM Coupé — Spectrum successor, niche

**68000 family (after Amiga):**

- Mega Drive — 68000 + Z80, both CPUs done
- Atari ST — Same CPU, no custom chips, simpler than Amiga
- Neo Geo — Arcade hardware

**New CPUs (if time permits):**

- Dragon 32/64 — 6809, UK/Welsh made, beautiful CPU
- Game Boy — Sharp LR35902 (Z80 variant)

Do not add these to milestones. Do not build infrastructure for them. Structure code so they're *possible*, then forget about them until the core four ship.

## Technology

- **Language:** Rust
- **Targets:** Native (Linux, macOS, Windows), WASM
- **Licence:** MIT

## Crate Structure

```text
emu198x/
├── Cargo.toml (workspace)
├── crates/
│   ├── emu-core/        # Shared traits, types (library)
│   ├── emu-6502/        # 6502 CPU core (library)
│   ├── emu-z80/         # Z80 CPU core (library)
│   ├── emu-68000/       # 68000 CPU core (library)
│   ├── emu-spectrum/    # ZX Spectrum (binary)
│   ├── emu-c64/         # Commodore 64 (binary)
│   ├── emu-nes/         # NES/Famicom (binary)
│   └── emu-amiga/       # Amiga (binary)
└── docs/
```

Each system is a **separate binary**. Each binary includes:

- System launcher (variant/option selection)
- Full emulator with UI
- Media controls specific to that system

Libraries are shared:

- `emu-spectrum` depends on `emu-core` + `emu-z80`
- `emu-c64` depends on `emu-core` + `emu-6502`
- `emu-nes` depends on `emu-core` + `emu-6502`
- `emu-amiga` depends on `emu-core` + `emu-68000`

WASM builds are per-system — embed only what you need.
