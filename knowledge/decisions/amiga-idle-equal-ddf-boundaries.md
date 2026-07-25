# Decision: Idle register-equal DDF boundaries

**Date:** July 2026

## The question

What should happen when an idle bitplane sequencer encounters equal
DDFSTRT and DDFSTOP register values between the nominal `$18` and `$D8`
horizontal hard edges?

## Evidence

The *Amiga Hardware Reference Manual* gives word-count formulas for ordinary
start-before-stop windows. It does not define register equality or an
OCS/ECS distinction. Applying the ordinary formula to this case would
therefore be an inference, not documented behaviour.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` does not treat equality as a
zero-width special case. After chipset masking, DDFSTOP compares on the even
phase and DDFSTRT on the following odd phase. For an idle sequencer, the stop
phase requests nothing and the start phase can begin a run. WinUAE's
changelog explicitly says that a simultaneous equality case cannot occur in
this pipeline.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` combines equal comparator
signals into one state transition. Its OCS and ECS branches both sample the
old BPRUN state for the stop request, then start an idle sequencer. This
produces the same clean-state result through a different abstraction. The
source labels the relevant ECS path a likely fix and does not present it as
a documented hardware result.

The inspected Minimig-AGA revision
`3ab91cd9220d4d047886d215b515227cbe568bdd` agrees for OCS. Its ECS/AGA path
instead contains an explicit equality special case that stops after one
fetch unit. That rule was added in commit
`7cd8a4221f34a718009d753e2193d30a657897e3` as part of a compatibility fix
for *Sanity: Roots 2.0*. The compatibility change does not include an
isolated trace or initial-state record proving the clean-idle transition
defined here.

The repository has no real-hardware trace that resolves the enhanced-chip
disagreement.

## The decision

Preserve the WinUAE/vAmiga clean-state behaviour.

For a stable, register-equal pair between the nominal hard edges:

- an idle stop phase does not request termination;
- DDFSTRT starts a run when the variant's DMA and vertical gates permit it;
- `ddf_stop_match` remains empty because no active run consumed the stop;
- with fixed horizontal limits enabled, `$D8` requests the normal
  phase-relative terminal fetch unit;
- with an enhanced horizontal bypass active, no terminal endpoint is
  created before the end of the current line.

For lores `$38/$38` with fixed limits enabled, the selected logical fetch
model produces 21 words per plane and an inclusive terminal endpoint of
`$DF`.

Emu198x represents the two comparator phases as ordered work on one
CCK-granularity beam entry. Stop samples the pre-existing run before start
is latched. This preserves the selected state transition without claiming
that the emulator exposes WinUAE's internal even/odd decision positions or
its later output pipeline directly.

Alice inherits the same idle transition through the ECS wrapper. The
verification here uses 16-bit fetch mode and does not settle AGA wide-fetch
terminal behaviour.

No new state was introduced by this decision, so it did not advance runtime
postcard schema version 8.

## Deferred behaviour

This decision does not define:

- equality when a fetch run is already active;
- retained enhanced soft-enable state on subsequent lines;
- equality at the `$18` or `$D8` hard edges;
- equality while a register is rewritten near either comparator;
- cross-line behaviour while enhanced horizontal limits are disabled;
- AGA wide-fetch terminal state;
- exact comparator-to-bus pipeline latency.

Those cases require separate soft-enable, hard-window, run-origin and
terminal state. They belong with the full multi-region sequencer, not an
equality-specific branch.

## Verification

Hermetic tests cover:

- an idle original-Agnus `$38/$38` run producing 21 lores words and
  terminating at `$DF`;
- default ECS and Alice runs reaching the fixed `$D8` stop;
- ECS and Alice `HARDDIS` runs retaining no current-line terminal endpoint;
- original Agnus, Fat Agnus and Fat Agnus with `HARDDIS` advancing
  machine-level bitplane pointers by the expected current-line amounts.

## Related documents

- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Enhanced Agnus horizontal DDF hard limits](amiga-enhanced-ddf-hard-limits.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Save State Format](save-state-format.md)
