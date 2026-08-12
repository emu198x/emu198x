# The I/O contention metric is settled: FUSE counts charges, our gate holds a level

**Date:** 2026-08-12
**Status:** SETTLED — answers the open question in
[`spectrum-accuracy-what-is-left.md`](spectrum-accuracy-what-is-left.md)
item 3, which said "settle the metric before changing the gate" and
"do not add a fourth term before this is settled".

## The question

Three terms had been disconfirmed in a row — `contend_port_late`'s
odd-port branch, mutual exclusion, and the `IOREQTW3` port. Three
disconfirmations is a signal about the question, not the answers, so item
3 asked whether "maximal run of withheld half-cycles" is even the right
counterpart to FUSE's charge points. If the ruler cannot express the
answer, `$40FF`'s "2 runs against 4 charges" is not a defect.

## The answer

**The ruler is fine. The gate cannot express what is being measured.**

Two different instruments were being conflated:

- `contention_arming` records half-cycle by half-cycle and reports
  *runs*. A run merges adjacent charges, so it genuinely cannot separate
  two charges that abut. This is the instrument the "2 against 4"
  observation comes from, and it should not be used to score gate
  changes.
- `io_contention_oracle` measures the engine's **actual per-instruction
  cost** and scores it against FUSE at every arrival T-state. It has no
  run-merging step. This is the instrument to trust.

The oracle's own results prove it is capable of expressing the right
answer, because in three of its five classes it already does:

| port class | FUSE shape | wrong | of |
|---|---|---|---|
| contended, ULA | `C:1 C:3` | 9,219 | 54,531 |
| contended, odd | `C:1 C:1 C:1 C:1` | **0** | 54,531 |
| uncontended, ULA | `N:1 C:3` | 12,291 | 57,603 |
| uncontended, odd | `N:4` | **0** | 63,744 |
| floatspy `$00FF` | `N:4` | **0** | 63,744 |

Zero disagreement across 182,019 samples in three classes is not what a
broken ruler produces.

## Why those two classes and not the others

FUSE's port contention is transcribed in its own core-test harness
(`z80/coretest.c`, `contend_port_preio` / `contend_port_postio`), and
reading it settles what `C:n` means:

```c
contend_port_preio( port ) {
  if( ( port & 0xc000 ) == 0x4000 ) { /* charge here */ }
  tstates++;
}

contend_port_postio( port ) {
  if( port & 0x0001 ) {
    if( ( port & 0xc000 ) == 0x4000 ) {
      /* charge */ tstates++;  /* charge */ tstates++;  /* charge */ tstates++;
    } else { tstates += 3; }
  } else {
    /* charge */ tstates += 3;
  }
}
```

`C:n` is **one contention lookup, then advance n T-states**. It is not a
duration. So the four classes differ in *how many discrete lookups*
happen inside the I/O M-cycle:

| page | A0 | lookups |
|---|---|---|
| contended | even | **2** |
| contended | odd | **4** |
| uncontended | even | **1** |
| uncontended | odd | **0** |

Our gate is a level test —

```text
(cpu_iorq || e.z80_iorq_prev) && io_even_port && e.z80_clock_high
```

— held across half-cycles, with one shape and one test. A level
reproduces the two degenerate counts exactly: zero lookups (never
asserted) and a lookup at every T-state (asserted throughout). It cannot
reproduce an intermediate count, because a count of discrete events is
not a property a level has.

That is precisely the observed failure pattern. We are exact at 0 and 4,
and every one of the 21,510 disagreements is in the classes needing 1 and
2. The engine charges the same 30.35 for both contended classes where
FUSE separates them by −5.18, because a level cannot charge a
two-lookup port less than a four-lookup one.

## What this licenses, and what it forbids

**Licensed.** Replacing the I/O gate with something that applies a
*count* of contention lookups at the canonical T-state offsets, selected
by the two address bits that classify the port (`A0`, and whether
`port & 0xC000 == 0x4000`). That is a structural change, not a fourth
constant, and item 3's prohibition was on the latter.

**Still forbidden.** Tuning a term and re-scoring against
`contention_arming`'s run count. Three terms died that way. Score against
`io_contention_oracle`'s cost, per class, and require the two currently
exact classes to *stay* at zero — they are the regression guard, and any
change that moves them has broken something that was right.

## Expected result

The two failing classes going to zero, and the closing gap table
resolving: engine gap `0.00` → `-5.18` on the contended page, and `6.90`
→ `7.79` on the uncontended page. The gap between two ports differing
only in their low bit is stated in a form the origin offset cannot reach,
which is why it is the load-bearing number.

## The target, derived

Reading `contend_port_preio` / `contend_port_postio` as positions rather
than counts gives the lookup offsets inside the I/O M-cycle directly.
`preio` charges at offset 0 when the page is contended; `postio` charges
at offset 1 always for an even port, and at offsets 1, 2 and 3 for an odd
port on a contended page.

| page | A0 | off 0 | off 1 | off 2 | off 3 | lookups |
|---|---|---|---|---|---|---|
| contended | even | ● | ● | | | 2 |
| contended | odd | ● | ● | ● | ● | 4 |
| uncontended | even | | ● | | | 1 |
| uncontended | odd | | | | | 0 |

So offset 0 is asserted by the page bits alone, and offsets 2 and 3 by
`A0` and the page together. That is the whole specification.

## Two things found while scoping the change

**The contended-odd class is exact by accident.** The 48K gate is

```rust
let mem_contention = contended_addr && e.gate_arms_this_halfcycle() && !cpu_mreq;
let io_contention  = (cpu_iorq || e.z80_iorq_prev) && ula_io && e.z80_clock_high;
```

`io_contention` never consults the page. The page dependence the engine
shows comes from `mem_contention`, whose `!cpu_mreq` term is true
throughout an I/O M-cycle — so a port address in `$4000..$8000` trips
*memory* contention. That leak is what charges the contended-odd class
its four lookups, and it is why that class scores zero.

The consequence for sequencing: the leak cannot be closed on its own.
Removing it without first giving `io_contention` the page term takes a
currently-exact class off zero. Both halves must land in one commit, and
the contended-odd class's zero is the check that the new gate reproduces
what the leak was supplying.

**The engine cannot yet express the table.** `UlaEngine` carries
`z80_iorq_prev` and `z80_iorq_prev2` — two half-cycles of `IORQ` history.
An I/O M-cycle is four T-states, eight half-cycles, so no combination of
the present latches distinguishes offset 1 from offset 3. The change
therefore needs new state: a position counter for the I/O M-cycle,
maintained in `track_z80_clock` alongside `mreq_t23` and `ioreq_tw3`.

That counter has a subtlety worth settling before it is written. On the
Z80, `IORQ` is asserted from `T2` through `T3` — three T-states, not
four — while FUSE's offsets run from the start of the cycle, `preio`
landing before `IORQ` falls. So the counter cannot simply count
half-cycles of asserted `IORQ`; its origin has to be pinned against the
same trace `contention_arming` records. Getting that origin wrong shifts
every offset in the table by one and would look exactly like a phase
error, which is the failure mode this whole file exists to avoid.
