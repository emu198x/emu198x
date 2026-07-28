# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Allow bounded `DebugTarget::step_instruction` implementations to report
  consumed complete ticks without implying that an instruction boundary was
  crossed; zero ticks can represent partial progress by a faster CPU
- Add an explicit optional monotonic boundary counter to `DebugTarget` and
  count only completed instructions in shared script and MCP observations;
  targets without a counter retain the exact-step guarantee

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-shell-v0.2.0) - 2026-06-04

### Added

- 68000 disassembler — full ISA + effective-address strictness
- *(dragon)* wire the 6809 debug target — first 6809 isa-disasm consumer
- *(debug)* disassemble the 6502 family via the Asm198x isa_disasm spec crate
- debug-target macros + SG-1000 on the shared debug tools
- shared DebugTarget MCP debug tools + MSX/VIC-20 pilot
- *(colecovision)* operational parity — runtime crate, MCP server, shell-backed script
- *(msx)* operational parity — runtime crate, MCP server, shell-backed script
- sub-frame tick step for cycle-exact MCP debugging
- shared MCP tool registry in emu198x-shell
- type_string MCP tool for Spectrum

### Fixed

- *(spectrum)* three Code198x-authoring speed bumps

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(debug)* frame the Amiga/68000 gap as a revisit trigger, not a permanent state
- *(debug)* record the two debug-surface tiers (shared DebugTarget vs bespoke)
- *(debug)* make the debug-target macros storage-agnostic
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
- cargo fmt + clippy clean across the workspace
- press_key — single-step replacement for press / wait / release
- watch_ay_* — AY register-write tracer
- port_read / port_write — direct bus-level Z80 I/O
- step / run_until_pc / disasm — second half of Z80 debug suite
- query_cpu — read every Z80 register in one MCP call
- full-trace memory_read / poke / watch_memory tools
- Add ScriptStep::QueryAy — decoded AY-3-8912 register state in one call
- Add start/stop audio recording — mirrors video-recording ergonomics
- Spectrum follow-ups: generalise autoload helpers + frame-tick hook
- Add Reset { kind: hard|soft } across ScriptStep, MCP, and script binaries
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Bump cpal 0.15 → 0.17
- Bump png 0.17 → 0.18
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- route .sna / .z80 / .zip through portable parsers
- Add MCP server dispatcher + stdio loop
- Add shell-side MCP framework: wire types, Tool trait, registry
- Add ScriptStep::LoadBasicProgram with system-specific dispatch
- Trim pre-recording audio and fade recording boundaries
- Wire video recording into HeadlessSession + ScriptStep vocabulary
- Add shell-side VideoRecorder driving ffmpeg for MP4 capture
- Add SetMachine and AutoloadTape ScriptStep variants
- Fix new clippy lints introduced by Rust 1.95.0
- Add initial DragonDOS VDK disk support
- Add DragonDOS BIN program loading
- Add continuous Dragon gamepad axes
- directed-test passes on actionable workspace gaps
- Add Dragon cartridge and PAK snapshot media
- Add Dragon XRoar-compatible smoke screenshots
- Implement Dragon cassette playback
- Add Dragon machine runtime and native shell
- share native gamepad button mapping
- share native audio output
- preserve stereo in native audio conversion
- Add runtime-nintendo-game-boy: Game Boy at the host boundary
- Tighten 1541 IEC behavior and trace helpers
- Add C64 D64 container import support
- Add C64 VIC colour-write trace
- Add C64 tape autoload and T64 import
- Add C64 host-side program import
- Add shared query-bool waits and tape-stop alias
- Add Spectrum tape autoload workflow
- Add Spectrum UI verifier shell
- Add shared boot wait workflow
- Add Spectrum family query namespace
- Add shared session queries and script observations
- Add shared headless session and JSON scripts
- Add shared headless capture helpers
- Formalize firmware bootstrap and media transport control
- Add stable Rust CI
- Bootstrap workspace and documentation baseline
