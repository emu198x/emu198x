# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Connect addressed bitplane transfers to live service and saved ownership,
  retaining the sampled pointer/modulo and bus lanes through Denise's normal
  RGA delay; automatic display request generation remains pending

- Retain equalisation and vertical-blank sync latches for automatic refresh timing-register admission

- Derive service-cell ownership from an admitted timing-strobe descriptor so
  CPU and other DMA cannot reuse that cell after the strobe retires

- Carry typed STRHOR, STRVBL and STREQU identity in the shared DMA stages for
  Denise counter propagation

- Retain typed display reservation, address and service descriptors on the
  ordinary CCK clock, rejecting duplicate advancement and occupied-cell replacement;
  live DMA adapter integration remains separate

- Retain and serialize the complete DMA service plan across both halves of a CCK, rejecting duplicate service admission
- Expose side-effect-free Agnus identity, beam, original-chipset latch,
  fixed-sync event and complete sprite-DMA diagnostic snapshots
- Add side-effect-free arbitration, DDF-sequencer and complete implemented
  blitter diagnostic snapshots, including authoritative per-CCK bus use,
  pending channel operations, line/fill runtime and buffered final-D state
- Serialize line-mode ONEDOT row eligibility, B texture phase and
  pre-service nasty ownership for deterministic mid-line restore and
  same-CCK CPU arbitration
- Expose serialized blitter completion phase, Copper-specific busy,
  remaining completion CCKs, final-D pending state and per-CCK bus use
- Serialize the shared two-CCK blitter-startup phase and expose its
  progress outcome separately from a serviced channel operation
- Serialize installed original-Agnus revision identity and the line-held hard
  vertical-blank force-off state that cannot be reconstructed from VPOSR,
  beam position and live interlace registers
- Add regression coverage for the non-empty idle register-equal
  DDFSTRT/DDFSTOP transition
- Serialize the original-Agnus vertical display-window latch so machine
  integrations and save states preserve comparator history
- Accept Paula's per-cell D0/D1/D2 request mask when constructing the
  machine-facing bus plan
- Serialize and expose per-CCK disk bus use so a completed final word remains
  authoritative for later CPU arbitration in the same cell

### Fixed

- Admit the final free odd Copper request cell, preventing line-end MOVEs from slipping into the next line

- Preserve DDFSTRT suppression and the old DDFSTOP comparison during register
  writes, committing their shared pending entry after comparator evaluation

- Clock Alice's area-blit source finish independently of a blocked final D
  write, preserving the pending transfer and one-shot interrupt through restore

- Preserve area-blitter channel/fill idle and holding stages: prime D before
  writing, overlap source reads with the previous held result, and apply
  channel modulos at their own row ends. Idle cells remain available to CPU
  transfers while occupied DMA cells stall the pipeline.

- Preserve the four CCK stages of standard line drawing, service optional B
  DMA with its modulo and reserved cycle, and save generated D results until
  their write stage. Internal stages leave the CPU bus available; C-enable
  gates line writes and later pixels use the C destination pointer.

- Preserve Alice register and arbitration stages across mid-line BPLCON0/FMODE writes, separating slot cadence from live transfer width

- Select AGA sprite DMA word lanes by FMODE/address and advance control fetches over full transfer padding

- Project the physical horizontal beam into the Copper comparator's two-CCK
  lead and PAL/NTSC line-parity wrap
- Suppress complete later D transfers in an ONEDOT horizontal row while
  retaining minterm, BZERO, line-state and final would-be-write completion
- Consume the preloaded `BLTBDAT` line texture with SRCB disabled, start
  at the selected BSH bit, decrement its phase per pixel and leave the
  data register unchanged
- Separate pre-AGA main finish, BZERO result generation and final D,
  delay Alice's finish source until final D, and retain distinct DMACONR
  and Copper completion observations
- Update BZERO from every generated D result even when D DMA is disabled,
  and release pre-AGA blitter-nasty ownership at main finish
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
