# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Serialize the shared two-CCK blitter-startup phase and expose its
  progress outcome separately from a serviced channel operation
- Serialize installed original-Agnus revision identity and the line-held hard
  vertical-blank force-off state that cannot be reconstructed from VPOSR,
  beam position and live interlace registers
- Add regression coverage for the non-empty idle register-equal
  DDFSTRT/DDFSTOP transition
- Serialize the original-Agnus vertical display-window latch so machine
  integrations and save states preserve comparator history

### Fixed

- Begin internal blitter activity immediately on every revision, consume two
  accepted/free startup CCKs before the first channel operation, reload BZERO
  on the first accepted CCK, and delay only A1000-visible BBUSY to that point
- Select the A1000 hard vertical-blank close on line zero and the later
  original-Agnus close on the final physical PAL/NTSC field line, including
  LOF-dependent interlaced fields; force-off wins over a coincident VSTART
  and terminates an unstopped DDF run
- Drive original-Agnus vertical bitplane eligibility from VSTART/VSTOP
  events instead of reconstructing a circular range, and terminate an
  unstopped DDF run when VSTOP closes the latch
- Let a genuinely later DDFSTRT comparator establish a fresh
  original-Agnus run after DMA terminated the old run, without
  treating DMA re-enable itself as a resume
- Terminate an unstopped original-Agnus DDF run when effective
  bitplane DMA is disabled so same-line re-enable cannot resume its
  stale fetch phase
- Preserve the proven next-line start-inhibition result when an
  original-Agnus phase-shifted `$E3` terminal endpoint crosses a
  short-line wrap, without assigning an unverified terminal bus slot
- Carry original Agnus's horizontal DDF hard-start gate across line
  boundaries: `$18` opens it, in-line terminal completion closes it,
  and a missed pre-`$18` comparator is not replayed
- Let enhanced-chipset wrappers select the shared `$D8` bitplane-DMA
  stop event without duplicating the OCS fetch sequencer
- Evaluate original Agnus's fixed `$D8` data-fetch stop as a beam event
  and freeze its terminal fetch unit so later or missed DDFSTOP
  comparators cannot overrun into end-of-line bus slots
- Treat DDFSTOP as a serialized comparator event for ordinary
  start-before-stop fetch regions and freeze the terminal fetch endpoint,
  so current, past or post-match register writes cannot rewrite line history
- Start and phase each line's bitplane fetches from a serialized
  DDFSTRT comparator match instead of the live register value, so
  current or past writes cannot retroactively create DMA
- Require bitplane DMA and an active vertical display window when
  early OCS Agnus observes the DDFSTRT comparator
- Derive sprite control and data requests from one shared regional
  vertical-timing path, preserving current-CCK bus ownership in snapshots
- Select early-OCS nine-bit or Fat Agnus 8372A ten-bit sprite vertical
  comparators from the Agnus identity

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-agnus-ocs-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-j: correct chipset identification across OCS / ECS / AGA
- Open Emu198x for public release
- Amiga NTSC: chip-layer line alternation + 5 NTSC OCS variants
- Add Amiga postcard snapshots across the chip stack
- fix workspace clippy and test hygiene
- land wb13 boot investigation and fixes
- WB 1.3 diag: pinpoint the silent failure — chained QBlits never run
- Port Blitter into the machine (tasks #134–#147)
- Retire commodore-agnus-ocs-archive: the archive is now the live crate
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- VIC-II unused-bit read mask; Agnus NTSC short/long line constants
- Paula DSKLEN arming flip-flop + Copper HP full resolution
- Add fresh Amiga headless baseline
