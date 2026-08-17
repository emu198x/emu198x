# Decision: a machine stalls its Z80 by `/WAIT` or by its clock, and the CPC's is the clock

**Date:** 2026-08-17
**Status:** ACTIVE
**Applies to:** every Z80 machine in this workspace — how it holds its CPU back
**Supersedes the mechanism chosen in:** #972, which put the Amstrad CPC on the
`/WAIT` pin

## The decision

There are two ways to stall a Z80 here, and which one a machine uses is not a
matter of taste:

| Mechanism | Whose behaviour it is | Who uses it |
|---|---|---|
| **`/WAIT` pin** | the Z80's — extend an M-cycle at `T2` while the pin is held | machines that genuinely assert the pin: a device asking for more time |
| **Clock gating** | the *machine's* — the video chip owns the CPU clock and decides when it advances | Ferranti ULA (whole Spectrum family), and the Amstrad CPC |

**The Amstrad CPC belongs in the second row**, not the first. Its microsecond
quantisation cannot be produced by any `/WAIT` pattern, and the proof is below.

Do not add a third mechanism. If a machine seems to need one, the question to
ask first is which of these two its chip actually implements.

## The proof

The CPC stretches every Z80 M-cycle onto a 1 µs grid, so each M-cycle costs a
multiple of four T-states. Longshot's *CRTC Compendium* §27.7.2 describes the
means as the Gate Array holding `/WAIT` for three T-states in four, leaving the
Z80 "free" for one. **That description cannot be the whole mechanism.**

Every instruction begins on the grid, because every M-cycle before it ended on
it. So every instruction's first M-cycle — the opcode fetch — arrives at the
same microsecond phase. Across the 251 measurable base opcodes, that fetch is
naturally:

| `M1` natural length | opcodes | must cost | required stall |
|---|---|---|---|
| 4 | 215 | 4 | **0** |
| 5 | 21 | 8 | **3** |
| 6 | 9 | 8 | **2** |
| 7 | 2 | 8 | **1** |
| 11 | 4 | 12 | **1** |

A `/WAIT` pattern sampled at `T2` has exactly one input: the arrival phase. It
is *identical* in all five rows. One input, five required outputs — so no
pattern can do it. The fact that distinguishes the rows is the fetch's own
length, and the Z80 has not expressed that by `T2`.

Confirmed exhaustively as well as algebraically. Searched against the 238
opcodes where Caprice32's `cc_op` and this workspace's own M-cycle
decomposition agree, for a `/WAIT` assertion pattern that reproduces every one:

- a function of microsecond phase — **no solution**;
- allowed to differ for `M1`, which the Gate Array could do since it sees
  `/M1` — **no solution**;
- allowed to depend on the M-cycle's natural length, which is *more* than the
  hardware could know — **no solution**.

Under both the strict criterion (same cost from any starting phase) and the
correct one (some aligned phase where every instruction matches and returns to
that phase).

The Gate Array also generates the Z80's clock. That is where the quantisation
has to come from.

## What this means in practice

**The `/WAIT` pin stays, and stays correct.** Sampling at `T2` and extending
while asserted is the Z80's own behaviour, identical on every Z80 ever made. A
machine whose device genuinely asks for time — an MSX or Master System
inserting VDP wait states — wants exactly this and nothing else. The pin is not
the problem; using it for a job that is not `/WAIT`'s was.

**The CPC moves to clock gating**, joining the eight Spectrum-family machines
that already work that way through `SpectrumDriver::cpu_clock_active`. The rule
to implement is that an M-cycle may only *begin* on a microsecond boundary,
which makes each one cost a multiple of four as a consequence rather than as a
table.

**Not a per-instruction table.** Caprice32 and Arnold both encode the stretched
costs as opcode tables, which is why neither could serve as an oracle for the
mechanism. A table is the answer written down; it teaches nothing transferable
and would make the CPC a special case forever.

## What this does not settle

The interrupt acknowledge. Under M-cycle rounding it costs 16, where §27.4,
Caprice32 and this machine's own grid invariant all say 20. The standing
hypothesis is that the acknowledge's two automatic wait states form a stretch
unit of their own — `round4(5) + round4(2) = 12`, giving `12+4+4 = 20` for IM 1
and `12+4+4+4+4 = 28` for IM 2, both matching Caprice32's hardcoded figures.
Two confirmations of one idea, still a hypothesis. Tracked in #971.

## Drift triggers

Re-read this entry when any of these appears:

- "just assert `/WAIT` for the CPC" — it cannot work; see the proof.
- "add a cycle table for the CPC" — that is the outcome, not the mechanism.
- "the Compendium says the Z80 is free 1 T in 4" — true as a description of the
  pin, insufficient as an account of the timing.
- a third stalling mechanism, or a machine-specific hook on the Z80 core for
  timing that is really the machine's own clock.
- "clock gating measured worse" — it was measured worse once, from a prototype
  with an off-by-one. That result is void.

## Sources

- *CRTC Compendium* §27.4, §27.7.2 (interrupt costs; the `/WAIT` description).
- CPC firmware guide, on M-cycles being stretched to a multiple of 4 T-states.
- `emulators/amstrad-cpc/caprice32/src/z80.cpp` — `cc_op[256]`, and the
  hardcoded `iCycleCount` of 20 and 28 for the interrupt modes.
- `crates/machine-amstrad-cpc/tests/microsecond_grid.rs` — the invariant and
  what currently falls off it.
