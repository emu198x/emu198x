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

## Applied 2026-08-17: the 48K floating-bus read origin

First use of this decision, and it settled a question three issues had been
carrying (#939, #940, #851).

The `IN` sample origin for the 48K floating bus was libspectrum's
`top_left_pixel`, 14336. Three hardware-derived oracles say 14335:

| Oracle | at 14336 | at 14335 |
|---|---|---|
| Woody's Float48K (hardware, WoS 17551) | 14337 | **14338** ✓ |
| Spectron `floatspy_48.png` | fails | **matches** ✓ |
| Spectron `halt2int_48.png` | `Float: Unknown`, 49104/49152 | **`Early`, 49152/49152** ✓ |

FUSE is the only witness for 14336, and it is a reference emulator's constant
rather than a measurement. Hardware wins; the origin moved.

**What it is not.** The ULA's bus *content* was never in question — it is
byte-exact against FUSE across the whole frame at 14336, and
`float_bus_oracle`'s frame-wide differential passes either side of the change.
What moved is when the CPU *samples*, which is why the two sample-instant
differentials now carry `FUSE_SAMPLE_OFFSET = 2` against FUSE's 3. That single
constant is where the divergence is stated.

**Not the shared Z80 lead, and that was measured rather than assumed.** Taking
`IO_READ_DATA_LATCH_LEAD_TSTATES` from 2 to 1 gives the same 48K answer, so it
looked like the tidier fix — but it moves Float128K from 14365 to 14366, away
from the 14364 that machine wants. The 128K's own one-T-state gap (#942) is a
separate question and stays open.

**Cost:** two FUSE sample-instant assertions restated by one constant. Nothing
else in the Spectrum family moved; the 128K, +2A, +3 and runtime suites are
bit-identical either side.

## Applied 2026-08-30: the 128K floating-bus read origin

The 128K read path had remained at libspectrum's `top_left_pixel` origin,
14362, and Float128K consequently observed the first non-idle byte at 14365.
[Mark Woodmass's published hardware-derived timing table](https://sourceforge.net/p/fuse-emulator/bugs/360/)
gives **14364** for the 128K, alongside the already adopted 48K figure of
14338. The matching community table and test program are independently
preserved by the
[redcode/ZXSpectrum documentation](https://github.com/redcode/ZXSpectrum/wiki/2A-3-Floating-Bus-Test).

The read origin therefore moves to 14363. With the shared two-T-state Z80
latch lead, Float128K now reports 14364. This does **not** move the live ULA
bus: the frame-wide differential remains byte-exact against FUSE at
libspectrum's 14362 `top_left_pixel`. It changes when the CPU samples that bus,
using the same explicit live-bus/read-path distinction as the 48K core.

**Verification:** Float128K reaches 14364 from a real 128K ROM and the original
multi-block tape; the full-frame live-bus differential remains 0 wrong of
70,908 T-states. This closes #942 without reverting the independently settled
Z80 interrupt-boundary decision or changing shared Z80 timing.
