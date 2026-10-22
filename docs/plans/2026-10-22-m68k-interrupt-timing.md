# 68000 interrupt timing

Lane: engineering frontier (680x0 accuracy). Steve decided on 2026-10-22
to fix it, after the instruction and exception timing work (#1980, #1996,
#2000) left interrupts as the one sequence still short.

## Problem

An autovectored 68000 interrupt took 32 clocks and wrote the whole
six-byte frame before the acknowledge cycle. The MC68000 User's Manual
(Table 8-14) gives 44 clocks with a four-clock acknowledge, and vAmiga's
Moira (`execInterrupt`, `jumpToVector`) sequences them:

| Clocks | Step |
|---|---|
| 6 | idle |
| 4 | write the PC's low word (SP+4) |
| 4 | interrupt acknowledge |
| 4 | idle |
| 8 | write SR (SP), then the PC's high word (SP+2) |
| 8 | read the vector |
| 4 | handler prefetch |
| 2 | idle |
| 4 | handler prefetch |

Neither SingleStepTests corpus has interrupt rows, so Moira and the manual
are the oracles (RULES.md rule 32).

The decision record `motorola-68000-spurious-interrupt-response.md`
described the old order. Its rule (BERR during the acknowledge selects
vector 24, keeping the six-byte frame) still holds; its account of when
the frame is written is amended.

## Change

- `initiate_interrupt_exception`, 68000 branch: six idle clocks, the PC's
  low word, the acknowledge, then `TAG_EXC_IACK_68000` keeps the vector,
  waits four clocks and joins the group-1 sequence for SR and the PC's high
  word (`TAG_EXC_SYNC_SR`, `TAG_EXC_SYNC_PC_HI`).
- `TAG_EXC_FINISH` waits two clocks between the handler prefetches for
  interrupts too, now that they retain their vector.
- 68010+ are unchanged.

The Amiga answers the acknowledge as a four-clock cycle. A real Amiga's
autovector synchronises with the E clock, which can lengthen it; neither
oracle models that, and it is outside this change.

## Verification

- `autovectored_interrupt_takes_44_clocks_in_the_manuals_order` pins the
  order, addresses and 44 clocks; it fails on the old sequence.
- The existing interrupt tests (levels, level 7, device vectors, spurious
  BERR, STOP, trace) still pass.
- SingleStepTests, both MAME sweeps, the 680x0 and Amiga suites, the
  ROM-backed Amiga tests and the six Dizzy games.
