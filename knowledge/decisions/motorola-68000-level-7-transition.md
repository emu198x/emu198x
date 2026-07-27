# Decision: retain MC68000 lower-to-level-7 transitions

**Date:** 2026-07-25
**Status:** implemented

## Question

How does the MC68000 core distinguish a new level-7 transition from a
request that remains continuously held at level 7?

## Decision

The core samples the external `ipl` input on every CPU tick. It retains:

- `sampled_ipl`, the most recently observed request level;
- `level7_transition_pending`, a pending transition from a lower level
  to level 7.

When the sampled level changes from 0-6 to 7, the pending latch is set.
The latch remains set until the CPU accepts that level-7 interrupt at an
instruction or STOP boundary. A boolean is intentional: it represents a
pending condition and does not invent a counted queue of unserviced
transitions.

## Recognition

A pending level-7 transition is accepted regardless of the active
interrupt mask and takes priority over the ordinary live-level
comparison. Acceptance consumes the latch and records level 7 in the
normal accepted-interrupt state.

A request held continuously at level 7 does not set the latch again.
While the active mask remains 7, it therefore cannot create another
interrupt at every instruction boundary.

The ordinary comparison remains separate. If an instruction lowers the
active mask while the live request is still level 7, the held request is
greater than the new mask and becomes eligible without another input
transition. This includes an RTE that restores a lower mask.

Sampling occurs before state dispatch on every tick, including bus cycles
and internal execution. A transition that arrives before an instruction
boundary remains pending until the boundary. Interrupt acceptance still
occurs only at the existing instruction and STOP boundaries.

## Exception precedence

A pending floating-point exception retains its existing precedence at
the instruction boundary. Taking that exception does not consume the
level-7 latch; the interrupt remains pending for the next eligible
boundary.

## Reset boundary

Architectural reset clears the pending transition and synchronizes
`sampled_ipl` with the current input. This prevents reset from preserving
an old pending event or manufacturing a lower-to-level-7 transition when
the input was already held at 7.

The Motorola interrupt description does not settle the electrical case
of level 7 held throughout reset. Synchronizing to the current input is
the deterministic boundary for this pin-level model. The RESET
instruction is not architectural reset and does not clear the latch; it
only asserts the external RESET output.

## Save-state compatibility

Both fields affect future execution and are serialized. Omitting
`sampled_ipl` could invent a transition after restoring a held level 7.
Omitting `level7_transition_pending` could lose a transition observed
during an instruction or bus wait.

Amiga runtime snapshots advance from schema 19 to schema 20 because the
nested CPU postcard layout changes. Version 19 is rejected before full
payload decoding. Paula produces request levels 0-6 for the currently
modelled Amigas, but every OCS, ECS and A1200 snapshot still embeds the
changed CPU layout.

The MC68010-through-MC68040 wrappers serialize their nested shared core,
so the retained state propagates without wrapper-specific fields. This
does not claim the MC68020-or-later electrical input synchronizer,
debounce timing or `IPEND` signal; those remain variant pin-timing work.

## Verification

The regression suite verifies:

- one lower-to-level-7 transition is accepted once while level 7 remains
  held and the active mask stays 7;
- a later lower-to-level-7 transition creates a second interrupt;
- a held level 7 is accepted through ordinary comparison after an
  instruction lowers the mask;
- a transition wakes STOP even when the mask is 7;
- sampled held-level history and an unconsumed pending transition both
  survive serialization without inventing or losing an interrupt;
- reset clears pending state and synchronizes the sampled input.

## Evidence basis

The canonical rule is defined by
[MC68000 Interrupt Processing](../../../../codex/05-specifications/processors/m68000/interrupt-processing.md),
based on section 6.3.2 of the Motorola ninth-edition user's manual.

## Related Documents

- [Accepted MC68000 interrupt level](motorola-68000-interrupt-acknowledge-level.md)
- [MC68000 spurious interrupt response](motorola-68000-spurious-interrupt-response.md)
- [Save-state live-machine serde](savestate-live-machine-serde.md)
- [CPU bus interface](cpu-bus-interface.md)
