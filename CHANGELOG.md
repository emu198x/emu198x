# Changelog

All notable changes to Emu198x will be documented in this file.

Format loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
not strictly. Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The per-system binaries stay at 0.x for now; library crates published to
crates.io may hit their own 1.0 on their own schedules.

## [Unreleased]

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
