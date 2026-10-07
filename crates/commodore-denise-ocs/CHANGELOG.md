# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Expose a side-effect-free snapshot of all implemented Denise register,
  bitplane, sprite, collision, HAM and wide-fetch pipeline state

### Fixed

- Apply Lisa window visibility to each native playfield, sprite-priority and collision sample while retaining uniform-gate entry points.

- Clock Lisa’s two playfield copies independently and freeze complete pending wide words at BPL1DAT.

- Retain Lisa’s full 32-bit shifter when switching between its 16- and 32-bit output taps, preserving bits exposed by mid-line widening

- Preserve Lisa serial phase across resolution changes, clock scroll history in 35 ns periods, and replace complete wide holding groups before fetch-width parallel copy

- Retain chipset-selected framebuffer sampling through shared board composition; Lisa uses four samples and OCS/ECS keep two

- Extend Lisa's serialized sprite clock and output queue to four 35 ns samples per lores period; OCS/ECS sequencing is unchanged

- Compose Lisa sprite priority and collisions from simultaneous hires sample codes while retaining the OCS/ECS lores sequencer

- Accept Lisa border-sprite eligibility as an output control while OCS/ECS retain their display-window and BPL1DAT gates

- Let Lisa supply its dual-playfield palette offset while OCS/ECS retain the fixed offset 8

- Compare collision conditions per actual source pixel and require the even group for single-playfield odd-group sprite collisions

- Retain Lisa sprite output codes for one further lores step, preserving comparator timing and aligning visible/collision output

- Correct independent playfield scroll fields and hires delay masking; retain Lisa serial scroll history for AGA wide fetches

- Require a current-line `BPL1DAT` arrival before normal sprite display and
  collision contribution while continuing to advance hidden sprite shifters
- Delay newly loaded sprite display and collision data by one low-resolution
  pixel after the `SPRxPOS`/`SPRxCTL` horizontal comparison
- Let armed manual sprite data repeat on every line until `SPRxCTL`
  disarms it, leaving VSTART/VSTOP lifecycle decisions to Agnus

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-denise-ocs-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Apply cargo fmt across in-tree edits + refresh Cargo.lock
- Cov-5c wave 2: directed-test passes across five chip crates
- Split Denise into chip / debug / viewport modules
- Add Amiga postcard snapshots across the chip stack
- fix workspace clippy and test hygiene
- land wb13 boot investigation and fixes
- Retire commodore-denise-ocs-archive: the archive is now the live crate
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- Lock in chip-only investigation: tests, fixes, golden framework, restart plan
- Add fresh Amiga headless baseline
