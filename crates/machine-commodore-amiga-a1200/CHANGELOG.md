# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- [breaking] Store the stock MC68EC020 through the active-CPU boundary and
  run its serialized clock domain at two CPU edges per 7 MHz Amiga system
  tick in the raw `AmigaA1200Snapshot` postcard schema; runtime envelopes
  version this as V24
- Retain a bounded, non-snapshot instruction-boundary queue for runtime
  tracing
- Complete MC68020 chip-RAM data phases through Alice's 32-bit path, with
  independent arbitration and once-only write side effects per phase;
  unresolved ROM and MMIO widths retain compatibility dispatch

### Fixed

- Route Copper horizontal comparison through Alice's inherited programmable
  beam projection
- Reset generic Autoconfig state on CPU RESET without clearing expansion RAM
- Preserve Alice's delayed finish source and serialized final-D completion
  through live execution and synchronous register-write ordering
- Consume the shared two accepted blitter-startup CCKs before the first
  Alice channel operation while retaining immediate BBUSY visibility
- Apply the enhanced `$D8` bitplane-DMA stop and horizontal hard-limit
  bypass policy through the Alice machine loop
- Preserve Alice's observed ordinary DDFSTOP and pending current-line
  fetch endpoint through register rewrites and snapshots
- Preserve Alice's current-line DDFSTRT fetch origin in arbitration,
  wide-fetch rendering and snapshots
- Gate rendered output with Alice's serialized vertical display-window
  state, including an explicitly programmed zero `DIWHIGH`
- Route Alice's ten-bit sprite vertical coordinates through guest
  register writes and snapshots

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-commodore-amiga-a1200-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Fixed

- DENISEID $FFF8 → $00F8 for AGA Lisa
- clear clippy warnings hidden behind the muda compile failure

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-l: restore real AGA Alice agnus_id ($2300 PAL / $3300 NTSC)
- A1200 Stage AE-k: ECS blitter extension registers — WB content draws on ECS
- A1200 Stage AE-j: correct chipset identification across OCS / ECS / AGA
- A1200 Stage AE-h + AE-i: investigation tooling — chipset write log + CPU instruction trace
- A1200 Stage AE-e: mirror BPLCON0 / palette / chipset-read tracers onto OCS + ECS
- A1200 Stage AE-b: revert agnus_id to OCS — restore working WB render
- A1200 Stage AD: agnus_id PAL/NTSC swap fix + AGA rendering punch list
- A1200 Stage AC: chipset reads + AGA Alice agnus_id — KS now goes full AGA
- A1200 Stage AB: watchpoint + poke tools — render path proven correct
- A1200 Stages Y + Z: palette trace + MCP restart tool
- A1200 Stage V: BPLCON0 write trace — the boot brings up a screen
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- A1200 Stage T: wire AGA registers to the chipset bus
- A1200 Stage O: blitter activity diagnostic
- A1200 Stage O: copper-list dump — STRAP is mid-render, BPU=0
- A1200 Stage O: CIA / display diagnostics — boot stuck in STRAP
- A1200 Stage N: CPU interrupts_taken counter — IRQs ARE firing (89K/10s)
- A1200 Stage N: track mask raises — the IRQ loop is self-blocking
- A1200 Stage N: CPU mask vs Paula IPL — the IRQ-acceptance gap
- A1200 Stage N diagnostics: surface the IRQ gap
- A1200 Stage M: BF on (An)/(An)+/-(An) — +6.8K unique PCs to FC1xxx
- A1200 Stage L: 68010+ interrupt frames push F/V word; +17K unique PCs
- A1200 Stage J+K: 68010+ RTE pops F/V word; 68020+ Format-$A group-0
- cargo fmt --all across the workspace
- Open Emu198x for public release
- A1200 Stage I: failing validation is TST.L D7; BMI — guru-alert loop
- A1200 Stage H: decoded the reboot trampoline; KS perpetually resets
- A1200 Stage G: revert Stage E — it was triggering Wack entry
- A1200 Stage F: KS 3.1 wedge confirmed as Wack-style debugger
- A1200 Stage E: Paula idle-mark fix unblocks KS 3.1 DiagAlive
- A1200 Stage D: KS 3.1 boot diagnostics + root-cause hypothesis
- A1200 Stage C: load KS 3.1, observe first failure
- A1200 Stage B: Cpu68020 swapped into the A1200 machine
- A1200 Stage A: AGA chipset + Gayle + machine scaffold
