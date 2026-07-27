# Decision: interpret BERR during MC68000 interrupt acknowledge as spurious

**Date:** 2026-07-25
**Status:** implemented

## Question

How should the MC68000 core respond when the machine asserts BERR during
an interrupt-acknowledge cycle?

## Decision

`BusStatus::Error` during `MicroOp::InterruptAck` completes the
acknowledge with spurious interrupt vector 24. It does not begin the
ordinary vector-2 bus-error exception.

This rule applies only to an explicit response from the machine layer.
`BusStatus::Wait` continues to hold the acknowledge cycle indefinitely.
The CPU does not invent an acknowledge timeout or infer BERR from elapsed
cycles.

The current bus-status abstraction does not represent the separate
BERR-plus-HALT retry handshake. `BusStatus::Error` therefore means a
terminal BERR response without a retry request.

## Exception sequence

The MC68000 interrupt sequence has already saved the pre-interrupt SR and
PC in the ordinary six-byte frame before it starts the acknowledge cycle.
When BERR terminates that cycle, the core retains this frame, the accepted
interrupt level and the active interrupt mask. It substitutes vector 24
for the acknowledge result and continues with the existing vector fetch.

Calling the ordinary bus-error entry path here would incorrectly select
vector 2 and push a second, 14-byte group-0 frame.

All other `BusStatus::Error` responses continue to enter the ordinary
bus-error path. This distinction belongs in the CPU because the meaning
of BERR depends on the cycle the CPU is executing.

## Machine boundary

A machine reports a failed acknowledge by explicitly driving
`BusStatus::Error`. A machine that requests autovectoring returns its
collapsed autovector response through `BusStatus::Ready`.

An uninitialized interrupt is a different response. A peripheral
completes acknowledge normally while supplying vector 15, represented as
`BusStatus::Ready(15)`. It is not a missing response and does not select
the spurious vector. The MC68000 core accepts this and other supplied
vectors through the same `Ready` path.

The current Amiga machine drivers always respond to a valid acknowledge
with the selected autovector. They do not naturally create the BERR path,
and this decision does not add an artificial Amiga timeout.

## Later-family boundary

The MC68010-through-MC68040 wrappers now retain the acknowledge result
before constructing their Format/Vector word. A BERR response therefore
selects vector 24 for both the stacked `$0060` offset and the handler
fetch.

Those wrappers still inherit a shared compatibility bus surface. This
decision does not claim that it models each later processor's acknowledge
address, termination signals or retry protocol. The architectural frame
rule and remaining bus boundary are defined in
[MC68010+ acknowledged interrupt vectors](motorola-68010-acknowledged-interrupt-vector.md).

## Snapshot compatibility

The MC68000 response change itself added no serialized field or layout and
did not change the Amiga snapshot schema. The later-family frame fix adds
a reachable post-IACK continuation state, so the shared Amiga envelope
subsequently advances from version 20 to 21.

## Verification

The pin-level regression terminates an accepted level-3 acknowledge with
`BusStatus::Error` after withdrawing the live IPL input. It verifies:

- vector 24 is selected instead of vector 2 or the level-3 autovector;
- the accepted level and active interrupt mask remain level 3;
- the saved SR and PC remain in the original six-byte interrupt frame;
- a stack canary below that frame is not overwritten.

A control regression terminates an ordinary supervisor-data read with
the same response and verifies vector 2 and the 14-byte group-0 frame.
Another control supplies vector 15 normally and verifies the distinct
uninitialized-interrupt entry.

## Evidence basis

The canonical behaviour is defined by
[MC68000 Interrupt Processing](../../../../codex/05-specifications/processors/m68000/interrupt-processing.md),
based on the Motorola ninth-edition user's manual.

## Related Documents

- [Accepted MC68000 interrupt level](motorola-68000-interrupt-acknowledge-level.md)
- [MC68010+ acknowledged interrupt vectors](motorola-68010-acknowledged-interrupt-vector.md)
- [CPU bus interface](cpu-bus-interface.md)
- [68k test-oracle strategy](m68k-test-oracle-strategy.md)
