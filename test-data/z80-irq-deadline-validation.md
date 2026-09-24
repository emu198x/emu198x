# Z80 IRQ deadline investigation — draft, not ready to merge

## CPU finding

Perfect Z80 revision `9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217` samples IRQ
at the final T-state's rising edge, two half-cycles before the next instruction
boundary dispatches the response. A held IRQ arriving later is deferred. A
pulse covering that earlier instant is accepted even if released before the
boundary; a pulse wholly between sampling instants is not retained.

The candidate stores two previous IRQ levels alongside the boundary-pending
bit. It changes which level qualifies acceptance, retaining IFF/EI checks,
NMI priority and response lengths. This is level history, not an edge latch.

The full IM1 sweep agrees in **1,664 comparisons** over sixteen cases and four
pulse widths: ordinary instructions, HALT, all five selected block repeats,
EI;NOP, EI;HALT, EI;DI, EI;EI;NOP, and DI. The twelve-case pre-change baseline
has **129 disagreements in 976 comparisons**. The public 976-case regression
also checks snapshot continuation after both history updates. It fails on the
baseline at NOP arrival 2, width 3: the old core misses an accepted pulse.

## Why this remains a draft

The isolated baseline (`78f34c49`) passes Float48K at **14338** and Float128K
at **14364**. With this candidate they read **14337** and **14363**. Those
hardware-derived targets remain unchanged and failing. The two FUSE-based
HALT acceptance/latency tests also fail on two of eight phases; acknowledge
cost and the uncontended-window assertion still pass.

The CPU evidence challenges the settled decision
[`zilog-z80-samples-int-at-the-instruction-boundary.md`](../knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md).
FUSE's instruction-boundary event processing is not an independent pin-level
measurement. The cited CPC wording that the CPU is informed "at T4" does not,
on its own, establish arrival *after* T4's rising edge. Nevertheless, this
candidate must not replace that policy until the integrated observations are
reconciled and the decision is explicitly superseded.

Two local diagnostic experiments were discarded: refreshing the live ULA bus after each CPU tick did not restore the targets (48K 14337; 128K 14365), and generating the
ULA interrupt before counter advance, as SpecIde does, left the candidate's
14337/14363 results unchanged. No ULA or floating-bus changes are included.
These failures do not establish which remaining integration assumption is wrong.

The next experiment must record the two HALT synchronisation loops, /INT edge,
CPU acceptance edge, final IN latch and ULA data slot in the same half-cycle
coordinate system, then compare their relationships with primary ULA timing
and a signal-level Spectrum reference. Do not fit a read origin or weaken a
hardware target to accommodate the CPU change.

## Snapshot compatibility and validation

The former boundary boolean retains its binary field position and byte width.
Values 0/1 retain its meaning with empty IRQ history; values 2–7 also encode
history. Legacy JSON booleans remain readable. Missing legacy history cannot be
reconstructed, so the first two ticks after restore assume IRQ was low. New
history tags need the new reader. Unknown states are rejected.

202 ordinary CPU tests, all 1,604,000 Harte vectors, FUSE's 1,350 exact plus six
pinned differences, six Rak 1.2a exercisers, and ordinary CPC machine tests pass.
The two Spectrum tape probes and two HALT differential assertions fail as above.
All-target CPU Clippy and formatting pass. Earlier ZEX results were not rerun.

Original adapters and dated measurements are retained privately in
`ops/experiments/z80-irq-cutoff/`; interpretation lives in
`reference/by-topic/cpu-z80/z80-irq-cutoff-evidence.md`. This is selected no-WAIT
NMOS die-derived evidence, not a CMOS, physical-chip, or exhaustive IRQ/NMI-race
claim. Independent timing sweeps for IM0/IM2 remain outside this experiment.
