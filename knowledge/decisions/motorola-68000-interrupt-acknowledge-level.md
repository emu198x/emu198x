# Decision: carry the accepted MC68000 interrupt level across the bus boundary

**Date:** 2026-07-25
**Status:** implemented

## Question

How does the interrupt level accepted by the MC68000 reach the Amiga
autovector responder if the live IPL inputs change before the acknowledge
cycle completes?

## Decision

The CPU retains the level sampled at the instruction boundary in
`target_ipl`. When it starts `MicroOp::InterruptAck`, it presents that
accepted level on address lines A3-A1 and drives every other address line
high:

```text
address = $FFFFF1 | (accepted_level << 1)
```

The machine layer derives the acknowledged level from those address pins:

```text
accepted_level = (address >> 1) & 7
```

It must not read the CPU's live `ipl` input or reach into `target_ipl`.
This preserves the pin-level CPU boundary: the CPU owns interrupt
acceptance and bus presentation; the machine owns the response to the
presented cycle.

Level zero is not a valid interrupt-acknowledge address. Encountering it
is an implementation invariant failure, not a spurious interrupt.

Motorola requires a requester to hold IPL stable until acknowledge to
guarantee recognition. Deassertion or a level change before IACK is
therefore outside the requester's guaranteed protocol. Once this
implementation has accepted a request, retaining and presenting that
accepted identity is nevertheless the deterministic CPU/machine boundary.
This decision does not claim an additional silicon guarantee for a
non-conforming requester.

## CPU state

`target_ipl` is captured once when interrupt processing begins. At the
same boundary the CPU:

- copies the pre-interrupt SR for the exception frame;
- enters supervisor mode;
- clears trace;
- changes the active SR mask to the accepted level.

The saved SR therefore retains the old mask while the active CPU state
already carries the accepted mask. Later changes to the external IPL pins
do not alter the in-progress acknowledge, selected vector or handler mask.

No new serialized CPU field is required. `target_ipl` and the active
`State::BusCycle` address were already part of the live machine state.

## Amiga autovector response

The Amiga board requests an autovector and does not supply a device
vector. For the MC68000 Amiga path, the implementation collapses external
VPA assertion and the CPU's internal `$18 + level` generation into
`BusStatus::Ready(24 + accepted_level)`. `Ready` does not mean literal
DTACK and bus data for this cycle. The shared later-variant path should be
described as an autovector response until each variant has its own
external bus protocol.

This is a bounded implementation collapse: it does not model VPA as a
separate input pin, but the selected vector is derived from the level
presented by the CPU. Interrupt acknowledge does not clear a Paula or CIA
request and does not identify a source within a shared level.

## Why live IPL is insufficient

Before this change every interrupt-acknowledge micro-operation used
`$FFFFFF`, which encodes level 7, while the shared Amiga driver computed
the vector from the current `cpu.ipl` value. Paula refreshes that input
before every CPU tick.

If a request changed between acceptance and acknowledge, the machine
could fetch one handler while `target_ipl`, the active SR mask and a
68010-or-later format word described another level. The steady-level
interrupt tests did not expose the mismatch because the live input still
matched the accepted request.

## Snapshot compatibility

Amiga postcard snapshots advance from schema 18 to schema 19.

A schema-18 snapshot taken during interrupt acknowledge can contain the
old fixed `$FFFFFF` address. It can also contain an already-ready vector
computed from the old live IPL value. Restoring either state under the
new address-decoding rule can select the wrong handler. Schema 19 rejects
the semantically incompatible version-18 payload instead of adding a
migration that rewrites in-flight IACK address and response state. The
bump is semantic; the postcard layout did not change.

A snapshot taken earlier in interrupt entry can also contain the old
active SR mask. The current continuation no longer changes that mask at
vector completion, so accepting schema 18 could resume with the correct
handler and an incorrect mask. The same schema rejection covers this
case.

## Verification

The regression suite pins four boundaries:

- all levels 1-7 map to their required A3-A1 address encodings;
- a level-3 request remains `$FFFFF7` after live IPL is withdrawn;
- the Amiga responder returns vector 27 for `$FFFFF7` even while live IPL
  carries level 6;
- a snapshot taken during the accepted level-3 acknowledge round-trips
  byte-identically and continues identically.

Because the current 68010-through-68040 wrappers inherit `Cpu68000`'s
compatibility bus surface, their corpus harnesses now decode that surface
consistently. Per-variant external acknowledge protocols remain separate
work; in particular, this does not claim that an MC68040 carries the
accepted level on A3-A1.

## Deferred boundaries

This decision does not correct the separate level-7 edge-latching rule.
The current CPU can accept a continuously held level 7 repeatedly; that
requires explicit serialized transition/pending state and its own tests.

BERR during interrupt acknowledge is handled separately by
[MC68000 spurious interrupt response](motorola-68000-spurious-interrupt-response.md).

Externally supplied vectors on 68010-or-later machines remain outside the
current Amiga target boundary. Those machines need an IACK-first frame
path before their format-and-vector word can represent a device-supplied
vector.

## Evidence basis

The accepted-level address, active-mask and autovector rules are defined
canonically by the Codex's
[MC68000 Interrupt Processing](../../../../codex/05-specifications/processors/m68000/interrupt-processing.md)
specification, based on the Motorola ninth-edition user's manual. The
Amiga-side level and autovector boundary is defined by
[Commodore Amiga Interrupt Architecture](../../../../codex/05-specifications/systems/commodore-amiga/interrupt-architecture.md).

## Related Documents

- [CPU bus interface](cpu-bus-interface.md)
- [MC68000 spurious interrupt response](motorola-68000-spurious-interrupt-response.md)
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md)
- [Save-state live-machine serde](savestate-live-machine-serde.md)
- [68k test-oracle strategy](m68k-test-oracle-strategy.md)
- [Amiga machine rollout plan](amiga-machine-rollout-plan.md)
