# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add a source-aware Amiga memory-write record and the narrow shared-driver
  hook for observing disk read-DMA writes into chip RAM
- Add non-driving active-map memory peeks and a payload-free diagnostic snapshot
  of installed memory topology, ROM state, overlay, and floating-bus value
- Add a shared side-effect-free controller-port input snapshot
- Expose the pending CPU-domain motherboard admission slot through a
  non-consuming diagnostic getter
- Add shared read-only scheduler and encoded-track-stream snapshots, and
  centralise the bounded instruction-boundary queue capacity
- Add a side-effect-free RTC diagnostic snapshot exposing stored and effective
  time, decoded calendar fields, and control state without the host anchor
- Drive the selected CPU through its exact clock domain, preserve every
  instruction boundary crossed inside a system tick, and separate
  accelerator-local responders from synchronized motherboard cycles
- Retain partially consumed CPU-domain edges so exact instruction stepping
  cannot discard faster-CPU time or advance the chipset twice
- Add the closed active-CPU type and serialized rational CPU-clock
  accumulator needed by stock and accelerated Amiga configurations
- Add an optional responder-sized CPU-bus completion path while preserving
  byte/word compatibility dispatch for unchanged machines and address regions

### Changed

- Preserve actual disk-DMA bus use across both CPU phases of a CCK, including
  the final transfer cell after Paula clears its live request
- [breaking] Extend the public `AmigaDriver` implementation surface with
  active-CPU clock-domain state and instruction-boundary recording

### Fixed

- Reserve both modeled Copper instruction-fetch cells so the first word-fetch
  phase cannot be double-allocated to the blitter or CPU
- Offer a Copper-eligible cell that a waiting or throttled Copper did not use
  to the blitter before returning it to the CPU. A non-nasty blitter yields to
  a mature CPU chip-RAM request; nasty mode may pre-empt it.
- Consume machine-composed horizontal-blank levels for each output sample
  instead of reconstructing chipset comparator intervals inside the renderer
- Propagate board-level `COLORxx` writes through Denise's serialized early
  display stage without delaying other Copper or custom-register effects
- Project Denise's real post-wrap output onto the preceding physical raster
  row and defer line-local display reset until the hardwired HBLANK boundary
- Perform the HBLANK line reset before a coincident phase-zero bitplane fetch
  so an AGA wide-transfer tail remains part of the new line
- Preserve Denise's one-low-resolution-pixel sprite load-to-output phase in
  the board-level video path
- Feed Copper `WAIT` and `SKIP` the installed Agnus's comparator-visible
  horizontal position, including its two-CCK lead and active-line parity wrap
- Reject restored active-CPU state whose serialized instruction-cache
  presence is impossible for the selected processor family
- Consume CPU RESET output after every active-CPU edge and provide a shared
  external-device reset hook
- Select an Amiga autovector from the interrupt level encoded by the
  current shared acknowledge cycle instead of mutable live IPL inputs
- Preserve pre-service nasty ownership across both half-CCK phases so a
  bus-free ONEDOT would-be D remains available to the 68000 after the line
  engine advances
- Advance the blitter completion pipeline every CCK, feed Copper its
  later BFD observation, raise Paula only on the one-shot finish source
  and retain actual same-CCK blitter bus use in CPU arbitration
- Feed the revision-correct visible blitter-busy signal to Copper `WAIT` and
  `SKIP`, apply BFD's blitter-idle condition to both instructions, and defer
  `SKIP` comparison until the post-fetch decision phase
- Consume original Agnus's frozen hard-stop grants without reconstructing
  a separate Denise-side DDF limit
- Keep Denise fetch, pointer and pixel integration aligned with Agnus's
  comparator-driven DDFSTOP termination
- Phase Denise's bitplane pipeline from Agnus's matched DDFSTRT origin
  rather than a mutable register value
- Gate board-level Amiga output with the concrete Agnus or Alice vertical
  display-window state instead of re-decoding legacy OCS bounds

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/common-commodore-amiga-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-a: HIRES dma_claim schedule
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- A1200 Stage T: wire AGA registers to the chipset bus
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Amiga Seam 1.7: move copper.rs to common-commodore-amiga
- Amiga Seam 1.6: Denise wrapper goes generic over DeniseChip
- Amiga Seam 1.5: add DeniseChip trait
- Amiga Seam 1.4: move memory.rs into common-commodore-amiga
- Amiga Seam 1.3: move cia.rs into common-commodore-amiga
- Amiga Seam 1.2: move rtc.rs into common-commodore-amiga
- Amiga Seam 1.1: scaffold common-commodore-amiga crate
