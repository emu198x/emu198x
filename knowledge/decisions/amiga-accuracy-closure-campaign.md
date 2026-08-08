# Decision: Amiga accuracy closure campaign

**Date:** 2026-07-31
**Status:** ACTIVE
**Assessment revision:** `8bb2c48e`

## The question

What accuracy work must Emu198x complete before the active Amiga effort pivots
from broad improvement to failure-driven maintenance?

## Current assessment

The supported Amiga family is assessed at **7/10 overall** when 10 means that
parity with mature reference emulators is defensible across the behaviour
Emu198x claims to support.

The PAL OCS A500 and A500+A501 path, using ordinary ADF media, is the strongest
slice and is assessed at **8/10**. The implementation architecture,
deterministic state model and inspection surface are assessed separately at
**9/10**. Architectural quality is not evidence of hardware accuracy and does
not raise the family accuracy score by itself.

These numbers are a point-in-time engineering judgement. They are not a public
compatibility percentage and must not be presented as one.

## Evidence supporting the assessment

- The MC68000 has broad functional single-instruction coverage. The registered
  1,000,058-row SingleStepTests/680x0 comparison has 968,687 exact rows and
  narrowly classifies the remaining address-error disagreements. It does not
  compare exact cycle length, final prefetch state or ordered normal bus
  transactions. Later processors have large software-oracle corpora, but those
  corpora do not establish complete processor-specific cache, MMU, exception
  or physical-bus behaviour.
- The A500+A501 OCS PAL Test Kit v1.21 lane compares six stable video cases
  pixel-for-pixel and RGB4-channel-for-RGB4-channel against vAmiga 4.4b12.
  This is exact evidence for those cases, from one independent implementation
  family. It is not hardware evidence and does not generalise to ECS, AGA,
  NTSC, audio or arbitrary software.
- Directed tests cover important Agnus arbitration, Copper, blitter, sprite,
  audio-DMA and interrupt paths. Programmable-HBLANK captures add
  software-derived ECS and AGA evidence, with disagreements and single-family
  boundaries retained rather than normalised away.
- Runtime snapshots preserve the bounded mutable machine state and have
  fixed-point and forward-replay tests. Debug reads are non-driving, and the
  scheduler, CPU, chipset, peripheral and expansion state needed to investigate
  disagreements is exposed.
- Source-aware memory watches distinguish CPU, blitter D-channel and disk
  read-DMA writes. Agnus and Paula diagnostics expose both planned D0/D1/D2
  disk cells and the hardware master that actually consumed the current CCK.

## Why the family score was not higher at assessment

Two concrete shared timing shortcuts were present when this campaign was
recorded:

1. Agnus computes `disk_dma_slot_granted`, but the running driver advances
   floppy read and write transfers from an independent track pacer rather than
   consuming the granted disk-DMA cells.
2. A custom-register write to an active blitter could synchronously drain the
   blit before applying the write. This preserves ordering and final memory,
   but the beam, audio, CIAs, Copper and other DMA clients do not advance
   through the elapsed wait.

Independent proof is also uneven. OCS has the strongest video oracle; ECS and
AGA have narrower comparator coverage. Paula has directed component tests but
no independent waveform oracle, and the driver does not claim an exact Paula
sampling phase. At the time of assessment, the compatibility catalogue had
six Amiga entries and was weighted toward A500 software.

Model completeness is a separate boundary:

- A600 and A1200 machines both compose Gayle, but the current Gayle stage has
  no attached IDE-drive or PCMCIA-card backend.
- The active 68EC020 path retains approximate bus and exception details.
- MC68030 and MC68040 profiles do not yet support accuracy claims for their
  processor-specific caches, MMUs and other later-processor behaviour.
- The active floppy path accepts sector-based ADF media. It does not yet
  validate preservation-grade custom tracks, weak bits or IPF/flux media.

These boundaries must be stated in claims. They do not block accuracy work on
the narrower stock-floppy configurations.

## Ordered closure campaign

Work proceeds in this order:

1. Bind floppy transfers to Agnus's authoritative disk-DMA grants while
   retaining rotational pacing, WORDSYNC behaviour, write capture and
   deterministic state. Add machine-level regressions for grant-gated reads,
   writes, pointer movement, completion interrupt and CPU contention.
2. Remove synchronous active-blitter draining. Let CPU and Copper writes land
   through their ordinary arbitration without inventing a CPU wait, and keep
   every blitter stage in the machine scheduler. Add regressions for CPU,
   Copper and replacement-size writes during an active blit.
3. Establish independent A1200/AGA visual evidence and an independent Paula
   waveform comparison. Record comparator provenance and the exact assertion
   boundary.
4. Complete the established ten-title Amiga catalogue target with
   representative OCS, ECS and AGA software, including timing-sensitive
   Copper/blitter, audio and track-loading cases.
5. Re-run the Test Kit, programmable-HBLANK, compatibility and deterministic
   replay lanes at the resulting revision. Preserve machine-readable reports
   and classify every disagreement.

Implementation changes in this campaign are committed separately. A later
step does not absorb an unexplained regression from an earlier one.

## Non-goals

This campaign does not keep expanding until every planned Amiga peripheral or
processor exists. In particular, IDE, PCMCIA, SCSI, CD-ROM, RTG, later-CPU
MMUs and later-CPU caches remain separate implementation work unless one is
required by a selected closure-catalogue case.

The campaign also does not claim physical-hardware conformance where the
registered evidence is software-derived. Hardware measurements may strengthen
or replace an oracle later without reopening unrelated completed steps.

## Pivot gate

The broad Amiga push ends when:

- both shared timing shortcuts above have been removed and regression-tested;
- the exact OCS video lane remains green;
- the claimed AGA display cases have an independent green oracle;
- the Paula comparison has a recorded oracle, tolerance and provenance;
- the ten-title OCS/ECS/AGA catalogue and deterministic replay gates pass; and
- every remaining comparator disagreement is either fixed, explicitly scoped
  out, or recorded as blocked on stronger evidence.

After that gate, Amiga work becomes failure-driven. New work must begin from a
real-software failure, a comparator disagreement, new primary or hardware
evidence, or an explicit expansion of the supported configuration claim.
General refactoring and speculative peripheral breadth do not keep the
campaign open.

## Progress log

| Date | Step | Result |
| --- | --- | --- |
| 2026-07-31 | Campaign recorded | Assessment, ordered work and pivot gate accepted at revision `8bb2c48e`. |
| 2026-07-31 | 1. Disk DMA arbitration | Commit `24b20ce4` separates the rotational stream from chip-memory traffic, adds Paula's bounded three-word FIFO, and binds reads and writes to Agnus cells `$07/$09/$0B`. Component, three-profile library, machine arbitration and full-track ADF write-back tests pass. Snapshot and query compatibility advance with the same step. |
| 2026-07-31 | 2. Mid-blit register writes | Reference inspection rejected the original CPU-wait premise. CPU and Copper writes now use normal arbitration without synchronously completing the blitter. Replacement-size and no-hidden-time regressions cover OCS, ECS and AGA; exact per-channel effects of other mid-blit writes remain evidence-bound. |
| 2026-07-31 | 3. Paula evidence foundation | A project-authored three-case waveform corpus now boots through the complete A500 path and measures cadence, stereo routing and the 64:32 volume ratio. Its first current-source run exposed and fixed a factor-of-two `ADKCON.FAST` disk-stream regression. The Emu198x self-consistency gate is green; independent waveform evidence is still required before this step closes. |
| 2026-07-31 | 3. Paula stereo routing | A prototype vAmiga 4.4b12 capture disagreed with Emu198x's channel assignment. The primary hardware manual confirms channels 1 and 2 on the left output and channels 0 and 3 on the right. The reversed Emu198x mapping is corrected and covered for all four channels. The repeatable vAmiga producer and registered capture package remain outstanding. |
| 2026-07-31 | 3. Registered Paula comparison | The reproducible vAmiga adapter and an audited three-case package now retain vAmiga 4.4b12 revision `60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`, source WAVs, configurations, logs, hashes, and semantic records. Emu198x agrees on output assignment, programmed cadence within approximately 0.0098 percent, and the half/full volume relationship. This satisfies the Paula portion at the single-family software-evidence level; it does not establish hardware or two-family consensus. |
| 2026-08-01 | 3. Registered A1200 visual comparison | A reproducible FS-UAE 5.0.7 package now supplies seven images for six A1200 AGA PAL Test Kit v1.21 cases. The strict Emu198x lane agrees exactly on every RGB8 pixel. Closing the comparison exposed and corrected Lisa's one-hires-pixel `COLORxx` output delay and a board wrapper that paused Denise's bitplane pipeline before the retained viewport. The A500 OCS lane remains exact after both corrections. This completes step 3 at the declared single-family software-evidence level; it does not establish physical-hardware accuracy, UAE-family independence from WinUAE, or general AGA compatibility. |
| 2026-08-01 | 4. Workbench 1.3 baseline audit | The A500+A501 row remained at the AmigaDOS startup window at frames 2,500 and 3,000 on clean revision `85d3dd05`. The desktop appears by frame 3,500 and is pixel-exact with the existing golden. The last preserved Jul-29 binary reached the same golden at frame 2,500. This is an expected waypoint shift after normal MFM pacing moved from 28 to 112 CCKs per word, not a Lisa/full-raster regression or a new golden. The matrix now captures frame 3,500. The host-time-backed A501 RTC was recorded as a separate deterministic-execution gap. |
| 2026-08-01 | 4. Deterministic RTC timebase | The RTC now advances from completed PAL or NTSC system ticks after taking its initial timestamp. Clock mode, whole seconds, subsecond phase and phase rate survive version-32 snapshots; grouped and leaf queries remain equal between machine ticks. Host-synchronized progression is an explicit component mode. Supplying a fixed epoch through a future runtime builder remains separate interface work. |
| 2026-08-08 | 1. Disk-cell request and actual-use refinement | Paula now stages read requests across D2, D1/D2 or D0/D1/D2 according to the three-word FIFO occupancy, while Agnus retains ownership of the fixed cell decode. An actual-use latch prevents the final serviced word from returning the same CCK to the CPU after Paula clears its live request. Track changes retain rotational word phase, and media mount or eject invalidates the encoded-track cache. WinUAE supports the selected read-stage mapping; vAmiga differs, so direct hardware confirmation remains desirable. Separate write-stage timing is not claimed. |
| 2026-08-08 | 1/2. Actual-owner Copper and blitter arbitration | A waiting or throttled Copper yields an eligible cell to a blitter when the CPU is idle; a mature CPU chip-RAM request outranks non-nasty DMA, while `BLTPRI` may pre-empt it. Both fetch cells of an active Copper instruction remain Copper-owned. Directed common and machine regressions cover active-fetch, idle-CPU, non-nasty CPU-priority and nasty-mode paths. |
| 2026-08-08 | Inspection provenance | The common Amiga watch records CPU, blitter D-channel and disk read-DMA writes with source, CCK, address, value, width and concurrent CPU context. Shared shell filters can select one source and an inclusive CCK window without changing the legacy CPU-only stream. |
| 2026-08-08 | Snapshot media fixed point | Version-34 snapshots preserve DF0 writability and reattach the skipped ADF object after restoring the drive mechanism and encoded-track state. Restore no longer synthesizes a disk insertion. Live guest writes are encoded; a successful flush invalidates stale MFM without changing word phase; read-only and writable media both retain byte-level fixed points, and a restored writable image accepts another persisted write. |
| 2026-08-08 | 4. Bounded release-screen navigation | The catalogue now passes release screens, trainers, selectors and prompts through sequential bounded waits and ordinary guest input. Every release receives one guest-visible frame, preventing adjacent actions from collapsing into one host batch. The Ackerlight *Arkanoid: Revenge of Doh* entry retained its established frame and audio goldens in the first review run; the ten-entry requalification below was performed against the final arbitration refinements. |
| 2026-08-08 | 4. Final-core catalogue requalification | Commit `891c236a` is the arbitration and writable-media baseline. In the complete ten-entry Amiga sweep, all ten entries passed byte-fixed-point snapshot plus frame/audio replay; six also retained their exact frame/audio oracles immediately. The four shifted deterministic phases were captured and reviewed, then each passed an exact individual rerun after requalification. Workbench 1.3 excludes only its 50 x 18 allocator-derived free-memory digits; every other pixel remains exact. Banshee uses the midpoint of matching 100-frame samples across an 800-frame POWERUPS-page span rather than a dissolve. The catalogue now spans OCS PAL, OCS NTSC, ECS and AGA, but remains compatibility evidence for those entries rather than an independent hardware oracle. |

## Related Documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga disk rotation and DMA arbitration](amiga-disk-dma-fifo-arbitration.md)
- [Amiga Paula stereo routing](amiga-paula-stereo-routing.md)
- [Lisa colour-output delay](amiga-lisa-color-output-delay.md)
- [Denise full-raster pipeline](amiga-denise-full-raster-pipeline.md)
- [Amiga register writes during an active blit](amiga-mid-blit-register-writes.md)
- [Amiga blitter completion pipeline](amiga-blitter-completion-pipeline.md)
- [M68k test-oracle strategy](m68k-test-oracle-strategy.md)
- [Amiga Test Kit video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Amiga programmable-HBLANK conformance](../processes/amiga-programmable-hblank-conformance.md)
- [Amiga programmable-HBLANK write timing](../processes/amiga-programmable-hblank-write-timing.md)
- [Amiga Paula-audio conformance](../processes/amiga-paula-audio-conformance.md)
- [Amiga RTC time source](amiga-rtc-time-source.md)
- [Catalogue startup navigation](catalogue-startup-navigation.md)
- [Registered vAmiga Paula-audio package](../../test-data/commodore/amiga/paula-audio/references/vamiga-4.4b12-60fd1e6b/README.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [October catalogue](october-catalogue.md)
