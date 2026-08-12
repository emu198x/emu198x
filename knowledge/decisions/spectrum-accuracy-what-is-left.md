# What is left on Spectrum accuracy, and the order to take it

**Date:** 2026-08-12
**Status:** PLAN — nothing here is started
**Follows:** [`spectrum-contention-the-way-out.md`](spectrum-contention-the-way-out.md),
which is now largely a record of what happened rather than a plan.

The contention campaign ended with something it did not start with:
**visibility**. Every machine has an arrival-resolved differential against
FUSE, two have a graded real-software survey, the floating bus is measured
end to end on both, and all of it runs nightly with ratchets. This
document is what that visibility now says, and what to do about it.

## The scoreboard

| gate | 48K | 128K | +2A |
|---|---|---|---|
| ZXSpectrum4.net survey | 13 of 70 failing | 10 of 67 | — |
| memory differential vs FUSE | 18 of 370,024 | 17 of 375,406 | **149,185** of 442,666 |
| I/O differential vs FUSE | 21,510 of 294,153 | — | — |
| live floating bus vs FUSE | **0** of 69,888 | **0** of 70,908 | — |
| `IN`-path byte vs FUSE | **0** | — | — |
| first-display-byte probe | 14337 (want 14338) | 14364 ✓ | — |
| floatspy | 72 px, one byte | — | — |

## The single most important thing this now shows

**Six of the 48K's thirteen failing cases, and four of the 128K's ten, are
the I/O instruction family failing *uncontended*.**

```
48K   test 32  INI; INIR; IND; INDR        Uncontended + Contended
48K   test 33  OUTI; OTIR; OUTD; OTDR      Uncontended + Contended
48K   test 35  IN A,(n); OUT (n),A; ...    Uncontended + Contended
128K  test 32  INI; INIR; IND; INDR        Uncontended + Contended
128K  test 33  OUTI; OTIR; OUTD; OTDR      Uncontended + Contended
```

An uncontended failure is **not a contention result**. There is no ULA in
it. It is the instruction's own T-state structure in `zilog-z80`, and it
is wrong on two machines in the same way because it is one defect in a
shared crate.

That reframes the board. The contention gate has been the whole story for
weeks, and a third of what the surveys are complaining about was never
about contention at all. It only became visible because the 128K survey
landed and showed the *same* family failing the same way — one machine's
failure is a case, two machines' identical failure is a cause.

## The order, and why

### 1. Block and port I/O instruction timing — do this first

**What.** `INI/INIR/IND/INDR`, `OUTI/OTIR/OUTD/OTDR`, and the plain
`IN`/`OUT` group have the wrong T-state structure in `zilog-z80`.

**Why first.** It is the largest identified group on the board (10 failing
cases across two machines), it needs no ULA, it has a documented reference
in Zilog UM0080 and the vendored SpecIde/FUSE sources, and it is exactly
the method that has worked five times: derive from the source, lock a
golden waveform, then measure.

**How.** Extend `zilog-z80`'s `bus_pin_waveform` to cover the block-I/O
M-cycle shapes the way Phase 1 covered `M1`, memory read/write and plain
I/O. `INI` is documented as `M1(4) M1(5) IO(4) MW(3)` — the five-T-state
second `M1` is the giveaway, and a core that gives it four is wrong by one
per iteration, which is what a `loop=` count in the survey measures.

**Proves it.** Both surveys' tests 32/33 turning green in **Uncontended**
first. If uncontended goes green and contended does not, the residue is
contention and belongs with item 3.

**Disproves it.** If the M-cycle structure already matches Zilog, the
defect is in `R` register increments per iteration — the survey reports
`R=` separately and it is wrong on these cases too.

### 2. The +2A contention mask — the biggest single number, and a known shape

**What.** `DELAY_TABLE_PLUS2A` has three `true` entries where the pattern
needs fourteen. The gate undercharges everywhere: `NOP` costs 4.00
T-states inside the contended window against FUSE's mean 9.00 — a
single-M-cycle instruction is *never* contended at any arrival T-state.

**Why second.** 149,185 of 442,666 is the largest number on the board, and
unlike everything else here its shape is already established: #856's phase
question is closed in the negative (the offset sweep is flat — no phase
agrees), so the mask's *length* is the whole story and it is derivable.

**How.** Rebuild the mask from FUSE's `contention_pattern_76543210` and
the Amstrad gate's own polarity — it contends on `/MREQ` **asserted**,
where the Sinclair ULAs contend on it inactive, so the run must span the
half-cycles `/MREQ` is low rather than the one before it.

**Proves it.** The +2A differential dropping sharply, and its offset sweep
developing a **minimum** where it is currently flat. The flatness is the
diagnostic: a gate that agrees at no offset is not mis-phased.

**Watch.** `amstrad-ula-40077`'s older `fuse_differential` reports a frame
maximum of 1 against FUSE's 7. It should reach 7 as a side effect. If it
reaches 7 while the arrival-resolved differential stays flat, the mask has
been fitted to a maximum — which is the trap #856 named.

### 3. The I/O contention instrument question — think before touching the gate

**What.** The I/O differential sits at 21,510 of 294,153 and `$40FF` gets
two withheld runs where FUSE charges four.

**Why not first.** Three separate terms have now been disconfirmed —
`contend_port_late`'s odd-port branch (byte-identical no-op), mutual
exclusion (worse, 78,339), and the `IOREQTW3` port (worse, 84,099). Three
disconfirmations is a signal about the *question*, not the answers.

**The question.** Is "maximal run of withheld half-cycles" the right
counterpart to FUSE's charge points for a level-sensitive gate at all? Its
own doc calls it a lower bound, because a run merges two adjacent charges.
If the metric is wrong then `$40FF`'s "2 against 4" is not a defect and
the last three experiments were scored against a ruler that cannot express
the answer.

**How.** Settle the metric before changing the gate. Take a case where
FUSE's charge count is unambiguous and the engine's stall pattern is
known, and establish by hand what the correct mapping between them is.
`contention_arming` already records half-cycle by half-cycle, so the data
is there.

**Do not** add a fourth term before this is settled.

### 4. Snow — implemented five times, tested twice

**What.** `snow_address` is wired into five ULAs with two unit tests in
`ula_engine.rs` and no program-level coverage at all.

**Why it matters now.** Snow is driven by `/RFSH`, and Phase 1 found
`/RFSH` ending a T-state early. That pin was wrong for the entire life of
the snow implementation, and nothing would have reported it.

**How.** Eight snow programs are already staged in the `zx-spectrum-tests`
corpus — `Snow`, `Snow Contention`, `Snow48`, `Snow128N`, `Snow R`,
`Snow Hold`, `Snow Tests`, `ULA Snow Crash` — several with reference
screenshots. Start with `Snow48` and `Snow Contention`; they are the two
with the clearest pass/fail output.

**Expect** this to find real defects rather than confirm correctness. It
is new coverage over code that has never been exercised.

## Blocked on evidence we do not hold

### The 128K interrupt anchor

`CONFIG_128K.int_start_pixel` puts the `/INT` edge two T-states from
`top_left_pixel`, where the 48K's lands exactly on it — all three configs
share `int_scan` 248 and `int_start_pixel` 1, and only a 224-T-state line
makes that come out right. The live floating bus is byte-exact at
`top_left_pixel` and 18,432 wrong at the `/INT` anchor, so the raster is
right and the interrupt is early.

It cannot be moved alone: setting `int_start_pixel` to 5 takes `Float128K`
from 14364 to 14362, measured. The probe tracks the interrupt one for one,
and 14364 is a community-reference coordinate whose own note says primary
hardware capture provenance is incomplete.

**What would settle it:** `Float128K` run under FUSE itself, or a capture
on real 128K hardware. Both are outside this repo. Until then the
disagreement is asserted by
`the_int_anchor_still_disagrees_with_the_bus`, so reconciling it fails a
test rather than silently outdating the record.

### floatspy's remaining byte, and Float48K's 14337

The entire floating-bus read path is byte-exact against FUSE — tables,
model, frame origin, sample instant, and the byte `IN` returns, each
scored at every T-state or arrival T-state in the frame. floatspy reads
the *correct byte for the T-state it reads at*, so the T-state is wrong,
and both programs synchronise on the interrupt and count from it.

That points at the same place as the 128K anchor: the relationship between
the interrupt and everything else. Worth revisiting **after** item 1, in
case interrupt-adjacent instruction timing is part of it.

## Judgement calls that are not mine

- **Super HALT Invaders' golden** is red at 5,936 of 104,192 pixels
  against one blessed at `9d2ef79e`, the commit that pinned the 128K
  contention — it predates the whole rework and the tape to re-check it
  was absent throughout. The two captures are the same title screen at
  different points in its animation, with the live one missing the title
  line. Someone should eyeball it and decide; it is deliberately out of
  the nightly until then.
- **The 128K suite's test 2** stops with `4 Out of memory, 5070:1` on its
  contended pass. Unknown whether that is the suite running out of room on
  a 128K in 48K mode or something this engine does to it. Recorded as an
  exact set in `KNOWN_INCOMPLETE`, so it cannot drift either way
  unnoticed.

## What not to do

- **Do not re-open the 48K contention window.** Its phase is derived from
  the HDL's fetch-to-`CLKWAIT` relationship and its edge from `Border_n`.
  Moving it one T-state to match the corrected frame origin was tried and
  rejected by both origin-independent oracles — survey 13 → 17, floatspy
  72 px → 140. The one T-state between the contention oracles' origin and
  the raster's is the harness's arrival label, and it comes out the same
  on three machines with no shared geometry.
- **Do not re-bless a golden to make a gate green.** Both surviving golden
  disagreements — floatspy and Super HALT Invaders — are the only evidence
  we have that something is wrong.
- **Do not fit a constant to a frame maximum.** That is what #856 spent
  four experiments learning.
