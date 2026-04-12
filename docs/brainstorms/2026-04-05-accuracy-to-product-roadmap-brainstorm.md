# Brainstorm: From Accuracy to Product

**Date:** 2026-04-05
**Status:** Agreed

## What We're Building

A roadmap to turn Emu198x from a cycle-perfect Spectrum proof-of-concept into a multi-system emulator that serves two audiences in parallel:

1. **Code198x content pipeline** — headless screenshot/video capture for all four curriculum platforms
2. **Standalone product** — a polished emulator worth demoing at CRASH! Live (October 2026)

Shared infrastructure (save states, capture, serialisation) serves both goals. Product features layer on top.

## Key Decisions

### 1. Systems: all four Code198x platforms

Spectrum (done) → C64 → NES → Amiga, in that order.

- **C64 next**: 6502 CPU (different architecture from Z80, tests the model's generality), VIC-II timing is well-documented, 1541 drive emulation was working in the old codebase.
- **NES third**: also 6502-based (shares CPU core with C64), PPU timing is the challenge. Simpler system overall than Amiga.
- **Amiga last**: most complex (68000 + custom chipset + bus arbitration). The architecture must prove itself on simpler systems first. Also benefits from the most development time before October.

### 2. Accuracy bar: same as Spectrum, every system

100% CPU test pass rate. Correct timing model from day one. No "approximate now, fix later" — that's how we got the old codebase's problems.

This means each system ships slower but ships right. The fresh start philosophy applies to every system, not just Spectrum.

### 3. Must-have features for October: capture pipeline + CRT

**Capture pipeline:**
- Headless mode (no SDL window needed)
- Screenshot capture to PNG at any frame
- Video capture (frame sequence or encoded)
- Input scripting (keyboard/joystick automation for reproducible captures)
- MCP integration for Code198x skills (boot, load, capture, step)

**CRT filter:**
- The basic shader already exists in the Spectrum runner
- Extract into shared infrastructure so every system gets it
- Integer pixel scaling in Sharp mode (already a known requirement)

**Not required for October (but architecturally prepared for):**
- Save states (build serialisation into every new system from the start, but the UI can wait)
- Debugger (useful for development but not a user-facing launch feature)
- Rewind (depends on save states; impressive demo feature but not blocking)
- WASM/web embedding (post-launch; keep SDL2 behind feature flags but don't build the WASM backend yet)

### 4. Shell architecture: shared from day one

The old codebase's `emu198x-shell` pattern was right — extract all generic UI into a shared crate. Every system gets the same launcher, CRT filter, audio pipeline, capture tools, and (eventually) debugger.

Don't build the shell *after* four separate runners. Build it with the second system (C64), so NES and Amiga inherit it.

## Approach: Phased Build

### Phase 1 — Shared infrastructure (before C64)

Extract from the Spectrum runner into shared crates:
- **Rendering**: CRT shader, viewport (integer scaling), framebuffer-to-texture pipeline
- **Audio**: SDL2 audio queue, mixing pipeline
- **Capture**: headless framebuffer grab → PNG, frame sequence → video
- **Runner scaffold**: SDL2 windowed runner + native platform menus (NSMenu on macOS, etc.) via thin FFI. Not a full native app yet — that comes post-launch with SwiftUI (macOS), GTK4 (Linux), WinUI (Windows) frontends. The SDL2 runner is the cross-platform baseline.
- **Serialisation**: serde with bincode/postcard. Derive `Serialize`/`Deserialize` on every chip and machine from day one. Compact binary, automatic schema evolution, fast enough for rewind (ring buffer of snapshots every N frames).
- **System trait**: `run_frame()` is the boundary. The shared shell calls it and gets back a framebuffer + audio buffer. What happens inside is system-specific — no universal tick loop pattern. Each system's run loop must reflect how the actual hardware operates.

### Phase 2 — C64

- Port 6502 CPU from old codebase (the tick-level state machine was already complete and validated)
- VIC-II video chip (cycle-accurate, handles bus stealing / BA line)
- SID audio (6581/8580)
- CIA timers (6526)
- 1541 drive (IEC serial, D64 loading — was working in old codebase)
- Machine integration: VIC-II owns the bus (BA line halts CPU for badlines/sprites). Run loop reflects actual C64 timing.
- Validate against VICE and per-cycle test suites

### Phase 3 — NES

- Reuse 6502 core (with Ricoh 2A03 differences: no decimal mode, integrated APU)
- PPU (2C02) — cycle-accurate scanline rendering
- APU — pulse, triangle, noise, DMC channels
- Mapper framework (start with NROM, MMC1, MMC3 — covers most common games)
- Validate against Blargg test ROMs, Mesen test suite

### Phase 4 — Amiga

- Port 68000 CPU (tick-level conversion not yet started — biggest piece of work)
- Custom chipset: Agnus (DMA scheduling, copper, blitter), Denise (bitplane rendering, sprites), Paula (audio, disk, serial)
- Bus arbitration (Agnus controls who gets the bus each cycle)
- Floppy drive (MFM encoding, track timing)
- Kickstart loading, Workbench boot target
- Validate against WinUAE and Minimig FPGA core

### Phase 5 — Product polish (leading up to October)

- Shell UI: model selector, system launcher
- CRT filter refinement (per-system tuning — Spectrum scanlines differ from C64 colour bleed)
- Save states UI (infrastructure already built in Phase 1)
- Input config (keyboard mapping, gamepad support)
- MCP server for Code198x integration (all four platforms)

## Open Questions

1. **C64 SID emulation approach**: port from old codebase, or fresh implementation? The old SID wasn't cycle-accurate. reSID is the gold standard but is C++ — do we wrap it or rewrite in Rust?

2. **NES mapper coverage**: how many mappers do we need for the curriculum? NROM + MMC1 + MMC3 covers ~80% of commercial games. Curriculum games will be original, so we control the mapper choice.

3. **68000 tick-level conversion**: the old 68000 is step()-level only. Converting to tick-level for Amiga bus arbitration is the single largest piece of work. Do we port from old codebase and convert, or start fresh? (The Z80 experience suggests: port the design, not the code.)

4. **October scope**: if we're running behind, which system gets cut? Amiga is the obvious candidate (most complex, least curriculum content ready). C64 and NES are more critical for Code198x launch.

## Beyond October: Full System Coverage

The four launch systems are a starting point. The long-term goal is to rebuild all 35+ systems from the old codebase at the new accuracy standard. Every CPU core cycle-perfect. Every system correct from day one.

### Product shape

**Per-system standalone binaries + unified launcher.** Each system works on its own (`emu198x-spectrum`, `emu198x-c64`, etc.) but a unified launcher (`emu198x`) bundles them all with a system catalogue. Users download what they want. Both formats ship.

This means the shell infrastructure must be a shared crate that each system links against. The launcher is a thin wrapper that presents the catalogue and delegates to each system's `run()`.

### CPU cores needed

All cycle-perfect, signal-level, Tom Harte validated (where tests exist):

| Core | Systems |
|------|---------|
| Z80 | Spectrum (done), MSX, CPC, Master System, SG-1000, ColecoVision, ZX80, ZX81 |
| 6502 | C64, NES (2A03 variant), Atari 2600/5200/7800/800XL, BBC Micro, Electron, Atom, Oric, VIC-20, PET |
| 6809 | Dragon, CoCo, Vectrex |
| 68000 | Amiga, Atari ST, Mega Drive |
| Custom/simple | Jupiter Ace (Forth), Aquarius (Z80 variant) |

The Z80 is done. The 6502 tick-level core exists in the old codebase (validated). The 6809 has step()-delegation only. The 68000 needs full tick-level conversion.

### Wave 2: historically significant systems (post-October)

Pick by significance rather than CPU convenience:

- **Atari 2600** — the system that started it all. 6502 core, TIA video/audio chip. Simple but timing-critical (racing the beam).
- **BBC Micro** — British computing heritage. 6502 core, 6845 CRTC, SAA5050 teletext. Natural Code198x expansion.
- **MSX** — Z80 core (done), TMS9918 VDP, AY-3-8912 (done). International significance. Multiple manufacturers.
- **Master System** — Z80 core (done), Sega VDP. Gateway to Mega Drive (shares VDP lineage).

### Wave 3 and beyond

- Atari 800XL, Atari 5200, Atari 7800 (all 6502 family, ANTIC/GTIA/MARIA video)
- Amstrad CPC (Z80, 6845 CRTC, AY — shared chips)
- Mega Drive (68000 + Z80 dual CPU, VDP)
- ZX80, ZX81 (Z80, simple ULA — historically important)
- Acorn Electron, Atom (6502 family)
- Dragon, CoCo (6809)
- ColecoVision, SG-1000 (Z80, TMS9918)
- VIC-20, PET (6502 family)
- Oric Atmos (6502, AY-3-8912)
- Jupiter Ace (Z80, Forth ROM)
- Remaining obscure systems (Memotech MTX, Sord M5, SVI-328, Tatung Einstein, Aquarius)

### Shared chip reuse map

| Chip | Crate | Used by |
|------|-------|---------|
| Z80 | `zilog-z80` | Spectrum, MSX, CPC, SMS, SG-1000, ColecoVision, ZX80, ZX81, Mega Drive |
| 6502 | `mos-6502` | C64, VIC-20, PET, BBC, Electron, Atom, Atari 800XL/5200, Oric |
| 2A03 | `ricoh-2a03` | NES (6502 variant + APU) |
| AY-3-8912 | `gi-ay-3-8912` | Spectrum 128K+, MSX, CPC, Oric, ST |
| TMS9918 | `ti-tms9918` | MSX, ColecoVision, SG-1000 |
| SN76489 | `ti-sn76489` | SMS, Mega Drive, ColecoVision, BBC |
| 6845 CRTC | `motorola-6845` | CPC, BBC |
| SID | `mos-sid-6581` | C64 |
| VIC-II | `mos-vic-ii` | C64 |
| 68000 | `motorola-68000` | Amiga, Atari ST, Mega Drive |

Each system added *after* the chip crate exists is significantly cheaper. The Z80 systems after Spectrum are mostly video chip + glue logic. The 6502 systems after C64 share the CPU core and differ in their video/audio chips.

### Architecture implications for Phase 1

The shared infrastructure built in Phase 1 must accommodate 35+ systems:

- **System trait**: every system implements a common interface (run_frame, framebuffer, audio_buffer, load_media, save_state, load_state)
- **Media formats**: each system registers its supported extensions. The launcher routes files to the right system.
- **Shell config**: per-system (window title, native resolution, model list, input mapping)
- **Capture pipeline**: system-agnostic. Any system that implements the trait can be captured headlessly.

## What We're NOT Doing

- Not building WASM/web embedding for October. Post-launch.
- Not building a full debugger UI for October. The infrastructure goes in, the UI comes later.
- Not chasing feature parity with the old shell. Cherry-pick what matters.
- Not rushing 35 systems at once. Four for launch, historically significant systems next, long tail over time.
