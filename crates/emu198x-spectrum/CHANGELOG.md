# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-spectrum-v0.2.0) - 2026-06-04

### Added

- *(spectrum)* wire Pentagon / Scorpion / Timex variant dispatch
- type_string MCP tool for Spectrum

### Fixed

- *(spectrum)* three Code198x-authoring speed bumps
- type_string adds extra settle for repeated keys
- allow MachineKind::all() dead on Linux
- suppress dead_code on Linux where muda gating leaves items unused
- rephrase Linux stub doc comment to dodge doc_lazy_continuation lint
- gate muda to non-Linux targets so Linux builds compile

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
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
- Spectrum MCP at family level — SpectrumRuntimeKind + set_machine
- Add Reset { kind: hard|soft } across ScriptStep, MCP, and script binaries
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Wire gamepad input through to the running emulator
- Script runner: extend portable LoadSnapshot dispatch to +2 / +2A / +3
- Script runner: support 128K LoadSnapshot via runtime pre-selection
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- route .sna / .z80 / .zip through portable parsers
- Open zipped tapes / snapshots / disks straight from the rfd dialogs
- Wire portable .sna / .z80 snapshot import; rename State menu honestly
- Fix +3 disk-slot wiring: rename slot to disk-a, accept .edsk
- Match the runtime helper's two-frame edge timing on autoload taps
- Drive LOAD "" through the editor for File > Open Tape autoload
- Wire View menu: Window scale 1×–4× + Video filter Raw/LCD/CRT
- Add native File / State / Help menus driving rfd dialogs
- Replace MCP stub with real server: 18 tools wired through execute_step
- Wire LoadBasicProgram through the Spectrum script runner
- Cargo feature gate: default = ['ui']
- Stub --mcp mode in src/mcp/
- Add headless script mode + mode-flag dispatcher
- Restructure emu198x-spectrum into src/ui/ subdirectory
- Map host arrow keys to Caps Shift + 5/6/7/8
- Wire SwitchMachine to actually swap runtimes (native-menu Phase 2)
- Gate FPS counter behind EMU198X_FPS=1 env var
- Pace at frame level — runtime advances in whole-frame increments
- Fix FPS counter — count per-slice completions, not catch-up bursts
- Add FPS counter to stderr (1-second windows)
- Drop boot/prompt screen-decode from per-frame window title
- Suppress dead_code on AppMenu.root for non-macOS
- Add macOS application menu with About/Hide/Quit before Machine
- Track 1C Phase 1: native Machine menu shell + AppCommand channel
- add native presentation filters
- migrate native windows to wgpu presenter
- add native channel controls
- share native audio output
- preserve stereo in native audio conversion
- Standardize native shell tape hotkeys
- Step native shells in sub-frame slices
- Rename Spectrum native shell crate path
