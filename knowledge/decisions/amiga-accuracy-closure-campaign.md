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

## Why the family score is not higher

Two concrete shared timing shortcuts remain:

1. Agnus computes `disk_dma_slot_granted`, but the running driver advances
   floppy read and write transfers from an independent track pacer rather than
   consuming the granted disk-DMA cells.
2. A custom-register write to an active blitter can synchronously drain the
   blit before applying the write. This preserves ordering and final memory,
   but the beam, audio, CIAs, Copper and other DMA clients do not advance
   through the elapsed wait.

Independent proof is also uneven. OCS has the strongest video oracle; ECS and
AGA have narrower comparator coverage. Paula has directed component tests but
no independent waveform oracle, and the driver does not claim an exact Paula
sampling phase. The compatibility catalogue currently has six Amiga entries
and is weighted toward A500 software.

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
2. Replace synchronous active-blitter draining with scheduler-visible waiting
   that advances every machine domain. Add regressions for CPU and Copper
   writes during a blit and for the elapsed beam, CIA, audio, interrupt and
   DMA state.
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

## Related documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga blitter completion pipeline](amiga-blitter-completion-pipeline.md)
- [M68k test-oracle strategy](m68k-test-oracle-strategy.md)
- [Amiga Test Kit video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Amiga programmable-HBLANK conformance](../processes/amiga-programmable-hblank-conformance.md)
- [Amiga programmable-HBLANK write timing](../processes/amiga-programmable-hblank-write-timing.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [October catalogue](october-catalogue.md)
