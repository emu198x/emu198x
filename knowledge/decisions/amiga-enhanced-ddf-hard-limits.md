# Decision: Enhanced Agnus horizontal DDF hard limits

**Date:** July 2026

## The question

When should enhanced Agnus and Alice apply the fixed horizontal data-fetch
limits inherited from original Agnus?

## Confirmed behaviour

Enhanced Agnus retains the nominal `$18` and `$D8` horizontal data-fetch
limits while the horizontal hard-limit circuit is enabled.

The inspected WinUAE implementation disables that circuit when any of these
conditions is active:

- `BEAMCON0.HARDDIS`;
- `BEAMCON0.VARBEAMEN`;
- `BPLCON0.SHRES`;
- `BPLCON0.UHRES`.

`BEAMCON0.VARVBEN` disables the vertical hard limit only. It does not disable
the horizontal DDF limits.

The third-edition *Amiga Hardware Reference Manual* establishes the nominal
DDF limits on pages 79-80 and defines the programmable-beam and hard-disable
controls on page 304. It does not describe the whole horizontal disable
predicate in one place. The combined predicate is therefore implementation
evidence corroborated by the separate hardware definitions, rather than a
direct quotation from one manual table.

The principal implementation evidence is WinUAE
`c32694e338fa5f34977f522eb4898adb069d2e73`, particularly
`custom.cpp::check_harddis`. The inspected vAmiga revision does not implement
these BEAMCON0 semantics and is not an oracle for this decision. The inspected
Minimig-AGA revision combines `VARVBEN` with the horizontal disable output;
that conflicts with the vertical-only meaning of `VARVBEN` and is not used
for this predicate.

## The decision

Apply the enhanced-chipset fixed right-hand stop at beam count `$D8` by
default. Compute the enable policy from the current ECS/Alice register state
before advancing the shared Agnus sequencer for that CCK.

The ECS wrapper passes a transient boolean policy into the shared OCS
sequencer. The sequencer remains the sole owner of the stop event, terminal
fetch calculation and bus release. No derived policy is stored in serialized
chip state.

When enabled, the event has the same ordering and phase-relative terminal
policy as original Agnus:

- it samples only a run that existed on entry to `$D8`;
- an ordinary DDFSTOP comparator at `$D8` is recorded first;
- the terminal endpoint remains relative to the matched DDFSTRT phase;
- a later register write cannot revoke the frozen endpoint.

Fat Agnus 8372A, native ECS machines and Alice all reach this policy through
the ECS timing wrapper. Alice coverage in this change is limited to the
16-bit fetch mode; AGA wide-fetch terminal states remain separate work.

Runtime postcard snapshots advance to schema version 8. A version-7 snapshot
captured after `$D8` on an enhanced chip can contain an active fetch origin
without the terminal endpoint now required by the default hard limit.
Restoring it would otherwise continue the run past the fixed boundary.

## Deferred behaviour

This decision implements only the right-hand `$D8` event.

The enhanced `$18` opening edge cannot be represented faithfully by the
current `ddf_start_match` field because that field records both a comparator
event and the effective fetch-phase origin. A complete enhanced sequencer
needs separate soft-enable, hard-window, run-origin and terminal state. That
state is also required for multiple data-fetch regions in one line.

The following remain deferred:

- the enhanced `$18` hard-window opening event;
- multiple enhanced DDF regions;
- live same-edge hard-limit control-write latency;
- register-equal comparators with a pre-existing run, and
  stop-before-start comparators;
- AGA wide-fetch terminal states;
- exact modulo timing.

## Verification

Hermetic tests cover:

- default ECS/Fat Agnus termination at `$D8`;
- `VARVBEN` retaining the horizontal limit;
- `HARDDIS`, `VARBEAMEN`, `SHRES` and `UHRES` bypassing it;
- a `$1C` phase retaining its `$E3` calculated endpoint;
- Fat Agnus machine-level pointer advancement and post-`$DF` bus release;
- Alice inheritance through the ECS wrapper;
- Fat Agnus postcard round trips immediately before `$D8` and while the
  terminal unit is pending;
- rejection of version-7 postcard snapshots.

## Related documents

- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Idle register-equal DDF boundaries](amiga-idle-equal-ddf-boundaries.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Save State Format](save-state-format.md)
