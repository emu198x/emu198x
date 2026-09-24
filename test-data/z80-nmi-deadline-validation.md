# Z80 NMI acceptance deadline

## Observation and correction

The pinned Perfect Z80 die-derived model (`9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217`)
defers NMI edges arriving in the final two half-cycles before the next T1 rising
edge. The previous core accepted them at that boundary, one instruction early.
This affects ordinary instructions and HALT as well as repeating block operations.

Capture remains edge-triggered. A four-state latch records empty, eligible, and
the two intervening half-cycles. Boundary decisions consume only eligible edges;
a late pulse remains pending. Instruction durations and the IRQ sampling path
are unchanged. This is an eligibility model supported by the measured cutoff,
not a claim that those states correspond to particular physical circuit nodes.

## Differential evidence

Original adapters sweep NOP, LD A,n, LD (HL),A, ADD HL,BC, JR, HALT, INIR, INDR,
OTIR, OTDR and CPDR. Each input is held or pulsed for one, two or three
half-cycles. All **1,272 comparisons** agree on the handler's post-PUSH-AF fetch
phase, stacked return PC, pushed AF and SP. The seven-case pre-change sweep has
56 disagreements out of 584; all occupy the two late-arrival phases.

The public regression reproduces the seven-case 584-input sweep. It fails on
the pre-change core at NOP arrival phase 5: the handler fetch occurs at phase
52 instead of 60. It also restores snapshots at both not-yet-eligible latch
states and compares subsequent CPU state and memory. A separate regression
checks a pulse during WAIT and eligible NMI priority over IRQ.

## Snapshot representation

The former latch field retains its binary position and byte width. Values 0/1
retain the old boolean meanings: empty/already eligible. New values 2/3 encode
the in-flight states. The decoder also accepts legacy JSON booleans and rejects
unknown states. A legacy snapshot cannot supply an edge age it never recorded;
an already latched legacy request remains immediately eligible. New snapshots
containing in-flight tags require the new reader.

## Validation scope

The CPU suite, Tom Harte vectors, FUSE final-state compatibility, and ordinary
ZX81 machine/ULA tests are checked alongside the differential. Original probes,
measurements and run hashes live privately in the umbrella under
`ops/experiments/z80-nmi-cutoff/`. Primary-layer interpretation is in
`reference/by-topic/cpu-z80/z80-nmi-cutoff-evidence.md`.

This is selected no-WAIT NMOS die-derived evidence, not physical-chip or CMOS
measurement. The local WAIT/priority regression is not an independent hardware
oracle. Sub-half-cycle pulses, arbitrary repeated-edge streams and exhaustive
IRQ/NMI races remain outside the differential's scope.
