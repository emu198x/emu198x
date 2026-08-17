# Decision: a hardware measurement outranks FUSE

**Date:** 2026-08-17
**Status:** ACTIVE
**Applies to:** every Spectrum-family timing question where a direct hardware
measurement and a reference emulator disagree
**Amends:** [`fuse-governs-the-contended-window.md`](fuse-governs-the-contended-window.md)

## The decision

When a **measurement taken on real hardware** conflicts with a **reference
emulator**, the hardware measurement wins.

FUSE remains the working reference for everything it is the only account of,
which is nearly all of it. This decision changes the tie-break, not the daily
practice: where FUSE is the only witness it still governs, and where silicon
has spoken it does not.

## Why now

`fuse-governs-the-contended-window.md` named the condition that would reopen
it, and was explicit that nothing else could:

> **A measurement on real hardware.** This is the only thing that can
> arbitrate between the two, because it is a question about silicon rather
> than about models. Not currently available.

It is now available, and it has been available all along in a place nobody
connected to this question. `float_bus.rs` records:

> **Real 48K hardware:** Float48K prints `14338` (Woody, WoS forum 17551) —
> that is the canonical Smith Ch 12 / Ch 21 "fetched byte on the data bus"
> tap. **Our engine:** prints `14337`.

That shortfall has been carried as a documented residual. What was missing was
any way to close it, so it read as an open puzzle rather than as a choice.

## What made it a choice

Investigating #939 turned up the lever. The Z80's step parity relative to the
host T-state grid decides whether a CPU T-state sits inside one raster T-state
or straddles two. Moving it (measured, not reasoned):

| oracle | current parity | flipped | reference |
|---|---|---|---|
| **Float48K** | 14337 | **14338** | **14338 — hardware** |
| Float128k | 14365 | 14366 | 14364 — derived |
| I/O contention vs FUSE | **0 wrong** of 297,734 | 1536 wrong | 0 |
| floatspy vs Spectron | 48207/49152 | 48138/49152 | 49152 |

So the engine *can* reach the hardware figure, and what stops it is agreement
with FUSE. That is a trade, and until now it had never been stated as one — it
is simply where the constants landed.

## What this decision does not do

**It does not flip the parity.** One of four oracles improves; contention
regresses against FUSE and floatspy gets worse. Acting on this is work, not a
switch, and doing it badly would trade a known-good state for a worse one.

**It does not settle the 14335-versus-14338 window question directly.** Woody's
figure is a *floating-bus* measurement — the first non-`$FF` T-state — not a
contention-window measurement. It arbitrates the CPU/raster alignment, which
couples to both, and the coupling is why it bears on the window at all. The
gate-level-versus-FUSE argument in the amended record stands on its own
evidence and is not overturned here.

**It does not demote FUSE.** RULES.md #32's requirement to validate against a
reference emulator is unchanged. A reference emulator is still a prerequisite
for timing work. This says only what happens when it loses to silicon.

## What it does do

It fixes the ordering, so the open one-T-state issues stop being framed as
inconsistencies between equals:

- **#939** — floatspy against Spectron. The parity lever is the live question.
- **#942** — the 128K floating bus, where three origins disagree by one and the
  bus origin is the one anchored to `top_left_pixel`.
- **#944** — the frame origin, recorded because the contention origin scores 0
  against FUSE frame-wide and could not be moved on that evidence alone.

Each of those was left recorded rather than resolved because no reference
outranked another. That is no longer true, and each should be re-read with the
hardware figure as the target rather than as a residual.

## What would reopen this

- A hardware measurement contradicting another hardware measurement, which is
  a question about method rather than about precedence.
- Evidence that Woody's 14338 is not what it is taken to be. It is a forum
  post, not a paper, and `float_bus.rs` already flags that "capture provenance
  remains incomplete" for the 128K figure derived alongside it. The 48K number
  is better attested than the 128K one; if that changes, so does this.

## See also

- [`fuse-governs-the-contended-window.md`](fuse-governs-the-contended-window.md)
  — the decision this amends, and the source of the reopening condition.
- [`spectrum-contention-vs-floating-bus.md`](spectrum-contention-vs-floating-bus.md)
  — the campaign the one-T-state family came out of.
- [`zilog-z80-samples-int-at-the-instruction-boundary.md`](zilog-z80-samples-int-at-the-instruction-boundary.md)
  — settled the other way round: a datasheet's literal reading lost to
  hardware behaviour on a second machine.
