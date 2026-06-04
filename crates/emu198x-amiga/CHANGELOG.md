# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-amiga-v0.2.0) - 2026-06-04

### Added

- *(emu198x-amiga)* accept --model a600|a1200|a2000 in script mode

### Fixed

- *(input)* [**breaking**] number joystick ports by the documented hardware labels

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- consolidate Amiga into one emu198x-amiga binary (UI/script/MCP)
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-w: wake_task tool
- A1200 Stage AE-v: signal_task (manual signal injection)
- A1200 Stage AE-u: disasm_around + dump_msgport_messages
- A1200 Stage AE-t: stack + library inspection toolkit
- A1200 Stage AE-r: resolve_lvo MCP tool + NDK 3.2 LVO tables
- A1200 Stage AE-s: decode Process struct + label node types
- A1200 Stage AE-q: memory_scan MCP tool
- A1200 Stage AE-o: query_exec_ports — public MsgPort inspector
- A1200 Stage AE-m: query_exec_tasks — ExecBase / task-list inspector
- A1200 Stage AE-h + AE-i: investigation tooling — chipset write log + CPU instruction trace
- A1200 Stage AE-g: --model CLI flag for the MCP server
- A1200 Stage AE-f: rename AmigaA1200Session → AmigaSession
- A1200 Stage AE-e: mirror BPLCON0 / palette / chipset-read tracers onto OCS + ECS
- A1200 Stage AE-d: lift more MCP tools off the A1200 downcast
- A1200 Stage AE-c: route cross-cutting MCP tools through AmigaLiveAccess
- A1200 Stage AE-b: AmigaLiveAccess trait — chipset-agnostic chip access
- A1200 MCP session migrates to family runtime
- Add Reset { kind: hard|soft } across ScriptStep, MCP, and script binaries
- A1200 Stage AD: agnus_id PAL/NTSC swap fix + AGA rendering punch list
- A1200 Stage AC: chipset reads + AGA Alice agnus_id — KS now goes full AGA
- A1200 Stage AB: watchpoint + poke tools — render path proven correct
- A1200 Stage AA: fs-uae cross-check — gap confirmed
- A1200 Stages Y + Z: palette trace + MCP restart tool
- A1200 Stage X: hook up VideoRecorder for live boot recording
- A1200 Stage W: framebuffer dump — the Amiga actually boots Workbench
- A1200 Stage V: BPLCON0 write trace — the boot brings up a screen
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- A1200 Stage S: zip-aware insert_media + the wedge isn't a wedge
- A1200 Stage R: insert_media + the wedge is reframed
- A1200 Stage Q4: integration test + Stage Q findings
- A1200 Stage Q2/Q3: step, watchpoints, copper-list dump, deeper chip queries
- A1200 Stage Q1: --mcp mode with 11 debugging tools
- Open Emu198x for public release
- Phase 0 closed: WB 2.04 desktop + verifier-binary dispatch
- Wire AmigaEcs machine + AmigaEcsRuntime; reclassify A500+ as ECS
- Convert runtime-commodore-amiga to AmigaRuntime<M: AmigaMachine>
- add native presentation filters
- migrate native windows to wgpu presenter
- add amiga joystick controls
- add native channel controls
- share native audio output
- preserve stereo in native audio conversion
- play live audio in native shell
- wire native mouse input
- add native verifier window
