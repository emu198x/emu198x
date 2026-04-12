# Systems Roadmap

What each target system needs, in implementation order. Each system reuses the shared infrastructure (clock tree, scheduler, audio mixer, tape transport, capture pipeline, MCP, config, rewind). The work is primarily CPU cores and system-specific hardware chips.

---

## CPU Cores Required

| CPU | Systems | Status |
|-----|---------|--------|
| Z80 | Spectrum, MSX, CPC, Master System, Game Boy | **Done** — 1356 FUSE tests, full undocumented behaviour |
| 6809 | Dragon 32/64, TRS-80 CoCo 1/2/3, Vectrex | **Done** — full instruction set + illegal opcodes, 27 tests |
| 6502 | C64, NES, BBC Micro, Apple II, Atari 2600/8-bit | **Done** — all official + undocumented opcodes, BCD, 20 tests |
| 68000 | Amiga, Atari ST, Mega Drive | **Done** — full instruction set, all EA modes, supervisor/user, interrupts, 14 tests |

---

## Phase 1: Spectrum (playable)

**Status: Playable.** 48K and 128K with ULA, beeper+MIC+AY audio, tape loading (TAP/TZX/WAV), snapshots (SNA/Z80), screenshots, save states, rewind, GIF/video capture, MCP server, Kempston joystick, turbo mode.

### Remaining for Spectrum product release:
- 128K tape loading (auto-LOAD needs USR 0 sequence for 128K mode)
- Spectrum +2/+2A/+3 (different ULA, +3DOS, floppy)
- CRT shader (defined in emu-display, not yet implemented)
- Debug panels (egui, types defined in emu-debug-views)

---

## Phase 2: Dragon 32/64 and TRS-80 CoCo

**Status: Boots to BASIC, keyboard and sound working.** Dragon 32 runs BASIC ROM, keyboard input with full shift-aware mapping, SOUND command works, PMODE graphics verified. Save states, rewind, screenshots. See `docs/systems/dragon.md` for details.

---

## Phase 3: Commodore C64 (boots to BASIC)

**Status: Boots to BASIC, keyboard and SID audio working.** C64 boots KERNAL ROM, displays READY prompt with blinking cursor, accepts keyboard input with full PETSCII mapping, produces SID audio. PRG file loading with auto-RUN. See `docs/systems/c64.md` for details.

---

## Phase 4: Commodore Amiga

**Status: CPU done, custom chipset not started.** The 68000 CPU core is complete (`cpu-m68k`). The Amiga custom chipset (Agnus, Denise, Paula) is the largest remaining piece of work. See `docs/systems/amiga.md` for details.

---

## Build Order Summary

| Priority | System | CPU needed | Complexity | Community |
|----------|--------|-----------|------------|-----------|
| **Done** | Spectrum 48K+128K | Z80 (done) | — | Large (UK) |
| **Done** | Dragon 32 | 6809 (done) | — | Small but dedicated |
| **Done** | C64 | 6502 (done) | — | Largest globally |
| **Next** | CoCo 1/2 | 6809 (done) | Low | Medium (US) |
| **Then** | NES | 6502 (done) | Medium | Large (homebrew) |
| **Later** | BBC Micro | 6502 (done) | Medium | Medium (UK education) |
| **Later** | Amiga | 68000 (done) | High | Large |
| **Later** | Atari ST | 68000 (done) | Medium | Medium |
| **Later** | Mega Drive | 68000+Z80 (done) | Medium | Large |
