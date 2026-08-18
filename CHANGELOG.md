# Changelog

All notable changes to Emu198x will be documented in this file.

Format loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
not strictly. Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [0.3.0] - 2026-08-18


### Added

- *(cpc)* Load games from tape
- *(cpc)* Add the CPC runtime, so the machine can be driven
- *(cpc)* Add the CPC frontend, so the machine is runnable
- *(nes)* Expose OAM and per-scanline sprite counts, so dropout is measurable
- *(spectrum)* Autoload tape on the 128K family, not just the 48K
- *(cpc)* Read the screen back as text
- *(cpc)* Add a 6128 model with the PAL's banked RAM
- *(cpc)* Model the Gate Array's /WAIT stretching
- Read Debug198x sidecars for symbolised debugging


### Fixed

- *(denise)* Repair the dual-playfield priority test, which never armed a playfield
- Make the Atari 800XL MCP tests run and give memory_read its advertised default
- *(spectrum)* Let a caller pin which ROMs the Spectrum boots
- *(nes)* Stop the debug PPU trapping when sprite size changes mid-line
- *(dragon)* Make a missing golden fail instead of passing quietly
- *(video)* Encode captures at a constant quantiser, not CRF
- *(audio)* Emit whole frames, so a quiet machine still has an audio track
- *(cpc)* Fit the 464's HD6845S, which reads its start address back
- *(z80)* Restore WZ = PC + 1 on the INIR/INDR repeat path
- *(spectrum)* Move the 48K floating-bus read origin to 14335
- **Breaking** — Widen the I/O trace port to the full 16-bit address bus. `emu198x_shell::IoEvent::port` is now `u16` rather than `u8`. Every consumer is in this workspace; machines with their own `u8` event type need no change, as the conversion widens.
- Correct the Debug198x banked-paging model


### Performance

- *(debug)* Stop re-rendering the framebuffer once per stepped instruction

## [0.2.3] - 2026-08-15


### Added

- *(cpc)* Generate interrupts in the Gate Array from the CRTC's HSync
- *(cpc)* Boot the CPC464 firmware to its own blue-and-yellow screen
- *(cpc)* Render the display at the dot clock
- *(cpc)* Let the CPC be typed at


### Fixed

- *(6845)* Start VSync at the beginning of row R7, not its end
- *(bbc)* Point the tape test at the UEF that was there all along
- *(uef)* Default the tape test to the UEF we already vendor
- *(cpc)* Report VSync on PPI port B, where programs look for it
- *(c64)* Stop type_string dropping characters, and wire load_basic_program

## [0.2.2] - 2026-08-14


### Added

- *(spectrum)* Read .szx snapshots
- *(spectrum)* Expose the CPU on the query surface
- *(cpc)* Add the Amstrad Gate Array's video modes and palette
- *(release)* Generate the changelog from commits, not from packages


### Fixed

- *(z80)* Hold the M1 opcode strobes to the rising edge of T3
- *(z80)* Make the M1 refresh strobe a full clock wide
- *(z80)* Hold /RFSH to the start of the next machine cycle
- *(z80)* Hold the memory read strobes to the end of T3
- *(z80)* Present each M-cycle's address on its own T1 rise
- *(z80)* Hold the memory write strobes to the end of T3
- *(z80)* Hold the I/O strobes from T2 fall to the end of the cycle
- *(z80)* Give the not-taken displacement cycle a read's pins
- *(z80)* Stop driving IR during internal cycles
- *(ula)* Arm the contention gate on the edge that drops /MREQ
- *(spectrum)* Derive the floating-bus sample instant from the I/O M-cycle
- *(ula)* Phase-lock the contention window to the ULA's fetch group
- *(ula)* Open the contention window at the fetch cycle, not the fetch
- *(shell)* Refuse a snapshot extension we do not write, and wait for the BASIC prompt
- *(spectrum)* Charge +2A contention from a measured mask
- *(spectrum)* Charge each port class the lookups FUSE charges it
- *(z80)* Sample /INT at the instruction boundary, not a half-cycle early
- *(sega)* Tick the Z80 twice per T-state, and feed /INT before the tick
- *(msx,coleco,svi)* Tick the Z80 twice per T-state, and feed /INT before it
- *(sord-m5,mtx)* Tick the Z80 twice per T-state on the CTC-vectored machines
- *(einstein)* Tick the Z80 twice per T-state, making the 4 MHz claim true
- *(zx80,zx81)* Tick the Z80 twice per T-state against a T-state ULA
- *(aquarius)* Tick the Z80 twice per T-state
- *(release)* Let every crate's work reach the suite changelog
- *(release)* Process every crate so its commits can reach the changelog
- *(release)* Write the suite changelog from the workspace, not from one machine

The per-system binaries stay at 0.x for now; library crates published to
crates.io may hit their own 1.0 on their own schedules.

## [Unreleased]

## [0.2.1](https://github.com/emu198x/emu198x/compare/v0.2.0...v0.2.1) - 2026-08-11

### Added

- *(spectrum)* boot any variant headlessly with --machine

### Other

- make the release able to ship binaries again
- Correct Amiga DMA ownership and media persistence
- inherit the Emu198x suite version
- declare the last two silent guards, both in src test modules
- give every fixture guard a voice, across the workspace
- Ship higher-CPU Amiga profiles

## [0.1.0] — 2026-05-23

Initial public release. Six per-system native verifier shells, each shipping
as its own binary for macOS (arm64 + x86_64), Linux x86_64, and Windows
x86_64.

### What works

- **Sinclair ZX Spectrum 48K** — real Z80 + ULA-driven machine loop;
  TAP/TZX loading with autoload and cycle-faithful tape turbo; live beeper +
  tape audio; real-software regressions including Manic Miner and Jet Set
  Willy. Other Spectrum variants (16K, 128K, +2, +2A/B, +3) exist as crates
  and are in active work.
- **Commodore 64** — live 6502 / CIA / VIC-II / SID board loop; KERNAL
  boots to `READY.`; TAP-backed datasette with autoload; host-side `.prg`,
  `.bas`, `.d64`, `.t64` import paths; optional live 1541 drive-8 with real
  `D64` media insertion (read-only; write path is post-launch).
- **Nintendo Entertainment System** — live 2A03 / 2C02 / APU machine loop;
  iNES cartridge loading with 14 mappers (NROM, MMC1, UxROM, CNROM, MMC3,
  MMC5, AxROM, Color Dreams, VRC2a, Action 53, BxROM, NINA-001, Sunsoft-4,
  Camerica); `nestest` passes 8991/8991.
- **Commodore Amiga A500 OCS PAL** — live board loop over `motorola-68000`,
  Agnus, Denise, Paula, Gary, dual 8520 CIAs, keyboard, DF0 floppy; live
  Paula audio; Kickstart 1.3 boots; Workbench 1.3 and 2.04 (ECS A500+)
  desktop. A1200 / AGA work is mid-flight and not yet shipping.
- **Nintendo Game Boy** — live DMG-family CPU / PPU / APU machine loop;
  `raw` / `lcd` / `crt` video presenter modes; headless cartridge runner with
  `.sav` battery-RAM sidecars; Blargg and mooneye-style verification gates.
- **Dragon 32** — real BASIC ROM boot over `motorola-6809`, dual MC6821
  PIAs, MC6883 SAM, MC6847 VDG; CAS media, ROM / DGN cartridges, DragonDOS
  VDK disks (read-only), PC-Dragon PAK snapshots; 11/12 application smoke
  matches against patched XRoar reference frames.

### Verification

- Z80 — 100% Tom Harte, ZEXDOC, ZEXALL pass
- 6502 — 100% Tom Harte
- 68000 — 100% Tom Harte (1,000,058 vectors)
- 627/629 NES ROMs survive 300 frames in the local-archive smoke matrix
- Per-system 10-title catalogue infrastructure (`emu198x-catalogue`) covers
  Spectrum, C64, NES, Amiga via TOML manifests

### Modes

Each per-system binary supports three modes:

- `--ui` (default) — native interactive shell with `wgpu` video, `cpal`
  audio, `gilrs` gamepad input, `winit` windowing
- `--script` — headless JSON-driven runner for screenshots, snapshots,
  capture, regression
- `--mcp` — JSON-RPC 2.0 MCP server over stdio, for Claude Code / other
  MCP hosts

### Not in this release

Stated honestly upfront so nothing surprises:

- Spectrum variants beyond 48K are work-in-progress
- Amiga A600 / A1200 / A3000 / A4000 / CDTV / CD32 (AGA chipset is mid-flight)
- Game Boy Color, Super Game Boy, link cable
- Dragon 64, CoCo line, DragonDOS write path, OS-9
- NES Famicom Disk System, Zapper, Game Genie, mapper coverage past 14
- C64 cartridge (CRT) support, REU, mouse / paddles, 1541 write path, C128
- Pentagon / Scorpion / Timex Spectrum variants (crates exist, deferred)
- Any system not in the six above (Atari 2600, BBC Micro, MSX, Master
  System, etc. — these are Wave 2+ per the roadmap)

### Documentation

- [README](README.md) — what the project is, how to build, how to obtain ROMs
  legally, per-system runner examples
- public docs site — system status, MCP integration, capture, scripting, and
  accuracy progress
- [`CONTRIBUTING.md`](CONTRIBUTING.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
  [`SECURITY.md`](SECURITY.md)

### Notes

- ROMs are not bundled. The README's "Getting ROMs" section covers each
  platform's legal acquisition path (Cloanto Amiga Forever, Cloanto C64
  Forever, World of Spectrum's Sinclair-permitted set, etc.).
- License is GPL-2.0-or-later workspace-wide.
- Project lives in the 198x family alongside Code Like It's 198x.

[Unreleased]: https://github.com/emu198x/emu198x/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/emu198x/emu198x/releases/tag/v0.1.0
