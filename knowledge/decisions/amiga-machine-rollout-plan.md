# Decision: Amiga machine rollout plan

**Date:** 2026-05-22
**Status:** A1200 Stages A + B + C landed 2026-05-22. Stage D
(diagnose the $F80E60 stall) pending.

## What this is

The order in which the remaining Amiga variants get built, and the
chip-extraction queue that minimises rework across them.

This document is **sequencing**, not architecture. The architectural
seams (chip substrate, 68k family, display surface, storage zoo,
boot CI) are already captured in
[`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md).
This plan picks an order for working through them.

## Variant zoo with deltas vs current OCS / ECS baseline

| Machine | Chipset | CPU | New I/O chips | Firmware |
|---|---|---|---|---|
| **A1200** | AGA (Alice + Lisa) | 68EC020 | Gayle | KS 3.0 / 3.1 |
| **A600** | ECS (already done) | 68000 (done) | Gayle (shared w/ A1200) | KS 2.05 |
| **A2000B** | ECS or OCS (done) | 68000 (done) | Zorro-II extras only | KS 1.3 / 2.04 |
| **CDTV** | OCS (done) | 68000 (done) | DMAC, CD-ROM | KS 1.3 + CDTV ROM |
| **A4000-030** | AGA (shared w/ A1200) | 68030 | Fat Gary, Ramsey | KS 3.0 / 3.1 |
| **A3000** | ECS (done) | 68030 (shared w/ A4000) | Fat Gary, Ramsey, Buster | KS 1.4 → 2.04 |
| **CD32** | AGA (shared w/ A1200) | 68EC020 (shared w/ A1200) | Gayle (shared), **Akiko**, CD-ROM | KS 3.1 + CD32 ROM |
| **A4000-040** | AGA (shared) | 68040 | (shared w/ A4000-030) | KS 3.1 |
| **Vampire V2 / V4** | AGA + FPGA RTG | AC68080 | RTG framebuffer, SD card | KS 3.x + Apollo OS |

## Chip extraction queue, ordered by cumulative unlock

1. **Gayle** — unlocks A600, A1200, CD32. Three-way win. Most of the donor's
   2334-line crate is NE2000 PCMCIA (drop it); the minimum-viable subset is
   ID register + IDE-empty status + Gayle CS register, ~500 lines.
2. **AGA Alice + Lisa** — unlocks A1200, A4000, CD32. Donor crates are 278 +
   372 lines, both already wrap-don't-clone over their ECS counterparts;
   they should compose with the Seam-1 `DeniseChip` trait without changes.
3. **Fat Gary + Ramsey** — unlocks A3000 + A4000. Not yet ported from any donor.
4. **DMAC** — unlocks CDTV. Not yet ported.
5. **Akiko** — unlocks CD32. Heaviest of the listed chips; chunky-to-planar
   conversion + CD-ROM controller in one die.
6. **Buster** — unlocks A3000 SCSI/DMA.
7. **A2091 / GVP SCSI** — A2000/A3000 with hard disks (post-base-boot only).
8. **AC68080** — Vampire targets. Separate clean-room project; not a donor port.

## Rollout order

1. **A1200** — *current.* High leverage: validates Cpu68020 in a real
   machine, extracts Gayle (unlocks A600, CD32), extracts Alice + Lisa
   (unlocks A4000, CD32) all in one push.
2. **A600** — cheap follow-on. ECS chipset already wired; reuses Gayle from
   A1200 extraction. No CPU change. Mostly a KS 2.05 firmware swap.
3. **CDTV** — orthogonal. OCS + 68000 already done. New work is DMAC +
   CD-ROM peripheral + CDTV firmware. Validates the CD-ROM lane on the
   simpler chipset before CD32 combines everything at once.
4. **A4000-030** — validates Cpu68030 wiring in a real host (the same way
   A1200 validated Cpu68020). Reuses Alice + Lisa from A1200.
5. **CD32** — combines A1200 chipset + CDTV CD-ROM lane + Akiko. The Akiko
   chunky-to-planar implementation is the headline new work.
6. **A3000** — Cpu68030 in an ECS host. Adds Fat Gary + Ramsey + Buster
   (most of the new chip work) on a chipset (ECS) that's already done.
7. **A4000-040** — Cpu68040 swap on the existing A4000-030 chassis.
   Incremental; mostly validates the Cpu68040 wrapper.
8. **Vampire V2 / V4** — separate track. AC68080 + RTG + SD storage.
   Long-horizon per `project_amiga_long_term_scope.md`.

## Sequencing rationale

- **A1200 before A600** even though A600 is cheaper, because A1200's
  Cpu68020 validation is the high-leverage thing — once that holds, A600
  is "swap KS 2.05 firmware and wire Gayle".
- **CDTV before CD32** so CD-ROM peripheral is exercised on a familiar
  (OCS) chipset before CD32 stacks it onto AGA + Akiko.
- **A4000-030 before CD32** so Cpu68030 is validated in a real host before
  CD32 adds Akiko as a separate axis of change.
- **A3000 after A4000-030** so Fat Gary + Ramsey are validated on AGA
  first (where there's more catalogue demand), then transplanted to ECS
  for A3000.

## Per-machine "minimum viable" budget

Each machine targets *"KS reaches the startup screen"* as its Stage C
deliverable. No floppy boot, no Workbench, no software catalogue. The
budget per machine is roughly:

- Extract new chip crates (porting from donor if available)
- Scaffold new `machine-*` crate (parallel to ECS/OCS shape)
- Wire CPU variant if it differs from base (68020/68030/68040)
- Load Kickstart ROM at the right address
- Run N frames and document the first crash/hang

Workbench / catalogue / floppy / IDE boot are post-Stage-C and tracked
per-machine on demand, not in this rollout.

## A1200 Stage C findings — 2026-05-22

Loading `kick31a1200.rom` (KS 3.1 r40.068, 512 KiB) into the A1200
machine and running 50 PAL frames (~1s emulated) produces:

- **Initial PC** $F800D2 (reset vector → KS entry point).
- **Final PC** $F80E60 — ~3.6 KB into the ROM.
- **667 unique PCs visited.** Healthy boot progress; not a 2-byte
  tight loop.
- **1056 PC excursions below $F80000.** KS jumped into chip-RAM
  trampolines or jumps that exited the ROM window — expected during
  exec setup.
- **10 custom-register writes**, **4 INTENA writes** — chipset and
  interrupt-controller surface is being exercised.
- **A4 = $00F3686C** at the stall — pointer into the
  diagnostic-ROM area ($F00000-$F7FFFF), which is unmapped in our
  A1200 build. KS 3.x scans this region during early init looking
  for a third-party diagnostic image.
- **SR = $2701** — supervisor mode, IPL mask 7 (interrupts masked).
  The chipset is writing INTENA but the CPU mask hasn't dropped, so
  even if VBL fires the CPU won't service it. Likely KS hasn't
  reached its IPL-lowering step yet.

**Disassembly at the stall** (PC = $F80E60):

```
$F80E60: 57C9 FFF8   DBEQ  D1, *-6      ; decrement-and-branch
$F80E64: 6610        BNE   *+18
$F80E66: 4BEC FFFE   LEA   -2(A4), A5   ; A4 = $F3686C → A5 = $F3686A
$F80E6A: BBD4        CMP.L (A5), D5     ; D5 = 0
$F80E6C: 66EE        BNE   *-16
$F80E6E: 610C        BSR   *+14
```

This is a memory-scan loop comparing 32-bit words against zero,
walking A4 backward through the $F00000-$F7FFFF region. Without
either a diagnostic ROM image or open-bus reads returning zero,
the inner BNE never falls through and the outer DBEQ counts D1
down — but D1 (low word $0002 at the report time) is depleting
slowly and may eventually fall through. Whether it does within
"reasonable" boot time is Stage D's first question.

## A1200 Stage D — what to investigate next

1. **Run for 500–1000 frames** and see whether D1 depletes and the
   boot exits this loop, or whether it's a genuine wedge.
2. **Map the $F00000-$F7FFFF region** so reads return open-bus
   ($FFFFFFFF) consistently — currently they may return chip-RAM
   mirror garbage if Gary's decoder hasn't been taught to refuse
   the address space. WinUAE returns open bus here for stock A1200
   configs.
3. **Check whether the $E00000-$E7FFFF ROM mirror is needed.** KS
   3.x ROMs were sometimes built with internal references to the
   lower half; if any branch / jump targets land there and read
   garbage, boot will diverge invisibly.
4. **Look at how `motorola-68000`'s `reset_to()` flows when the SSP
   read at $000000 hits the OVL-mirrored ROM.** Our reset routing
   should mirror the SSP fetch to the ROM-at-$F80000 image.
5. **Compare to WinUAE booting the same ROM** and trace where their
   PC sequence diverges from ours after the first few hundred
   instructions.

Stage D's deliverable: either KS 3.1 reaches the "insert workbench"
screen (best case) or we have a definite list of "next thing to
fix" issues with the chipset / memory map.

## What this plan does not cover

- **PMOVE / PFLUSH / PTEST** (68030 + 68040 MMU instructions). Tracked in
  [`m68k-test-oracle-strategy.md`](m68k-test-oracle-strategy.md). Will
  surface as a Stage C failure on A4000-030 or A3000.
- **FPU FLINE handling** for 68040. The 705-line `motorola-68040/src/fpu.rs`
  exists but isn't wired through `variant_decode_hook`. Will surface as a
  Stage C failure on A4000-040 if KS 3.1 probes for the FPU.
- **WinUAE second-oracle** ([`m68k-test-oracle-strategy.md`](m68k-test-oracle-strategy.md)
  Mitigation B). Becomes timely once A1200 boots — extracting WinUAE's
  CPU as a callable library is its own project.
- **RTG framebuffer surface.** Captured by Seam 3 of the architecture
  review; lands when Vampire or Picasso-class cards become a real target.

## Drift triggers

- **A new Amiga variant appears** that doesn't fit the chipset / CPU
  axes above. PiStorm is the obvious candidate (68000 emulated on Pi
  silicon + real Amiga hardware) — it should land its own track here.
- **A chip turns out to be heavier than the donor estimate.** Gayle's
  donor crate is 2334 lines but most is NE2000; if the IDE / PCMCIA
  side surfaces unexpected complexity at Stage A, that's a sequencing
  signal to re-cost.
- **Akiko ends up gating CD32 longer than expected.** The chunky-to-planar
  path is fundamental; if it's not portable from donor / WinUAE, CD32
  may slip behind A3000.

## Related

- [Amiga full-family architecture review](amiga-full-family-architecture-review.md) — the architectural seams this plan sequences through.
- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md) — the wrap-don't-clone pattern that makes CPU swaps mechanical.
- [Motorola 68k test-oracle strategy](m68k-test-oracle-strategy.md) — the verification ladder, with Mitigation B gated on the first real 68020 machine (i.e., A1200 Stage C).
