# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add side-effect-free diagnostic snapshots for Paula interrupt/control,
  complete audio-pipeline, UART, pot-port, and component log state.
- Add a read-only diagnostic snapshot for Paula disk registers, byte latches, DSKDAT queue, DMA state, and PLL timing.
- Count rotational read words discarded while Paula's three-word disk-DMA FIFO
  is full so media regressions cannot remain silent.
- Expose the current D0/D1/D2 disk-cell request mask in the disk diagnostic
  snapshot.

### Changed

- Widen the saved audio period counter and the public `effective_period` and
  `period_counter` diagnostic fields to `u32` so 65,536 is represented exactly.
- Stage disk read-DMA requests by FIFO occupancy: one queued word requests D2,
  two request D1/D2, and three request D0/D1/D2.

### Fixed

- Deliver attached volume at startup/high-byte entry and attached period at
  low-byte entry, using the current AUDxDAT holding word. Match DMA request
  phases, coalesce pending requests, repeat the buffer on underflow, and load
  late-arriving words on high-byte entry.

- Honour periods below 124 and reload zero as 65,536 CCKs. The recommended
  DMA sampling period no longer clamps the digital countdown.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-paula-8364-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- A1200 Stage G: revert Stage E — it was triggering Wack entry
- A1200 Stage E: Paula idle-mark fix unblocks KS 3.1 DiagAlive
- Move disk read DMA state machine into Paula
- Add Amiga postcard snapshots across the chip stack
- add native channel controls
- Apply mechanical Rust formatting cleanup
- fix workspace clippy and test hygiene
- land wb13 boot investigation and fixes
- fix ADKCON bit constants — WORDSYNC was silently the wrong bit
- Retire commodore-paula-8364-archive: the archive is now the live crate
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- Paula DSKLEN arming flip-flop + Copper HP full resolution
- Tighten Amiga CIA and floppy boot path
- Add fresh Amiga headless baseline
