# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- The crate depends only on the OCS and ECS Denise crates. `DeniseChip`,
  `HorizontalBlanking` and the timing strobe now come from
  `emu198x-commodore-denise-ocs` instead of the Amiga family's glue, and
  `retire_timing_strobe` takes that crate's `TimingStrobe`.
- `DeniseAga::vertical_blanking_mut` and `DeniseAgaVerticalBlanking::edge`
  are public, so a board can drive Lisa's blanking latch directly. The two
  tests that drive Lisa through the glue's board wrapper moved to the glue.
- First release under the `emu198x-commodore-denise-aga` name. Until now the crate was
  `commodore-denise-aga`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

### Added

- Expose a complete side-effect-free diagnostic snapshot of Lisa-owned
  register, 24-bit palette, HAM8 and programmable-blanking state
- Expose Lisa's serialized early colour and normal programmable-blanking
  register propagation stages

### Fixed

- Remove an extra lores period from mid-line BPLCON4 palette-XOR timing, using the independently traced Lisa counter phase while preserving snapshot 54.

- Compare programmed horizontal blanking against the next lores counter, including all four fine samples, matching counter-traced coarse and fractional edges.

- Align window, bitplane, sprite, Copper colour and fixed blanking output with
  independently traced Denise counters, removing a reference-padding offset
  from Lisa timing while retaining fractional edges and snapshot 53 layout

- Gate fractional display-window edges through native composition and palette XOR before resolving HAM and sprite colours.

- Propagate BPLCON1 through Lisa’s normal selector stage while retaining its immediate register mirror.

- Propagate Lisa display controls through normal output stages, latch bitplane width on BPL1DAT, and delay playfield XOR at native palette selection

- Restore fixed horizontal blanking through the $05D lores edge when programmable blanking is not selected

- Preserve all 35 ns output samples, palette-write delay and fine horizontal-blank edges in the full Lisa framebuffer

- Render explicit superhires sprites and SPRxCTL quarter-lores positioning at 35 ns boundaries, including priority and collision timing

- Render explicit/automatic hires sprites independently of playfield resolution and honour SPRxCTL bit 4 positioning

- Honour ECSENA-gated BRDRBLNK as a final black output mask while retaining hidden colour, sprite and collision advancement

- Honour ECSENA-gated BRDRSPRT, allowing sprites outside DIW and before BPL1DAT while preserving their serial phase

- Decode BPLCON3.PF2OF for dual-playfield colour selection and reset Lisa to the compatible offset 8

- Preserve the full palette address when KILLEHB selects ordinary six-plane indexed output

- Route manual BPL8DAT writes to the eighth bitplane holding register and retain BPL1DAT as the copy strobe

- Decode CLXCON2 so BP7/BP8 participate in collision matching; base CLXCON writes clear the extension

- Align Lisa sprite output with its bitplane phase using retained codes rather than an HSTART coordinate offset

- Apply AGA extended and fractional scroll delays to a retained serial stream, preserving the extra output tick without shifting its copy comparator

- Drive Lisa programmable horizontal blanking from a serialized fine-phase
  comparator latch, with ECSENA and EXTBLKEN sampled at the event boundary
- Preserve immediate AGA colour-register reads while delaying display output
  by Denise's early stage plus Lisa's existing one-hires-sample stage
- Propagate Alice HBSTRT/HBSTOP copies through Lisa's normal display path

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-denise-aga-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Fixed

- DENISEID $FFF8 → $00F8 for AGA Lisa

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- A1200 Stage T: wire AGA registers to the chipset bus
- cargo fmt --all across the workspace
- Open Emu198x for public release
- A1200 Stage A: AGA chipset + Gayle + machine scaffold
