# Decision: Original Agnus cross-line DDF hard-start gate

**Date:** July 2026

## The question

How should original Agnus decide whether a DDFSTRT comparator before `$18`
may start bitplane DMA after a line boundary?

## Evidence

The *Amiga Hardware Reference Manual* documents the normal horizontal
data-fetch limits. It does not define the original-chipset cross-line
limiter state or the early-start anomaly.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` explicitly describes the
original-chipset limiter as persistent state. Its `$18` event opens the
limiter. DDFSTOP and the fixed right edge only request termination of an
active run. The limiter closes in the normal-stop path after the terminal
fetch unit completes. An idle line therefore leaves it open for the next
line. WinUAE names *Ode to Ramon* as a compatibility example for this
effect.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` independently models the same
state as `DDFState::shw`. `$18` sets it. Completion of the last original-
chipset fetch unit clears it. End-of-line handling preserves it. A DDFSTRT
event can start only while it is set.

The inspected Minimig-AGA revision
`3ab91cd9220d4d047886d215b515227cbe568bdd` instead opens and closes a
rectangular hard window at `$18` and `$D8` on every line. It does not model
the original-chipset idle-line carry. This is conflicting emulator evidence,
not corroboration.

The repository has no primary hardware trace for this transition. WinUAE
and vAmiga agree on the state change and cross-line result, but do not
establish a shared exact bus position for a phase-shifted terminal unit that
finishes after horizontal wrap.

The initial power-on value is also unresolved. WinUAE's C++ static
zero-initialisation produces its open state, while vAmiga's resetter clears
`shw` to produce its closed state. Neither establishes hardware power-on
behaviour.

## The decision

Represent the original-Agnus horizontal hard-start gate as serialized
state, separate from the current-line DDF comparator fields.

For original Agnus:

- `$18` opens the gate before a DDFSTRT event at the same architectural
  position is considered;
- DDFSTOP and the fixed right edge request termination but do not close
  the gate;
- completion of a terminal fetch unit whose endpoint occurs on the current
  physical line closes the gate;
- end-of-line does not reset the gate;
- a DDFSTRT comparator before `$18` can start only while the carried gate is
  open;
- a comparator missed while the gate is closed is not replayed when `$18`
  subsequently opens it.

An idle line has no terminal completion, so it carries an open gate into the
next line. A line whose terminal unit completes after its `$18` opening and
before wrap carries a closed gate.

Enhanced Agnus and Alice do not consume this original-chipset field. Their
left hard window belongs to the enhanced multi-region sequencer.

Emu198x initializes the gate open to preserve its established deterministic
first-line behaviour. This is an implementation compatibility choice, not a
claim about the hardware power-on level.

The new serialized field changes every nested Amiga machine postcard.
Runtime postcard snapshots therefore advance to schema version 9. Version-8
snapshots are rejected before payload decoding because the nested positional
postcard layout has changed.

Raw postcards of the public `AmigaOcsSnapshot`, `AmigaEcsSnapshot` and
`AmigaA1200Snapshot` machine values are unversioned and have no migration or
version-8 rejection gate. Durable save states must use the runtime envelope.

## Deferred behaviour

This decision does not define a terminal fetch unit whose phase-relative
endpoint lies beyond the last physical CCK of a short line.

WinUAE carries live run, stopping and fetch-phase state through horizontal
wrap. vAmiga also preserves its full sequencer state, but the inspected
oracles do not agree on an exact externally visible next-line fetch
position. Emu198x still clears the current-line start and terminal fields at
wrap, so it neither continues the pending terminal unit nor closes the gate
at its eventual completion. This phase-shifted case remains a known
implementation gap.

The following also remain deferred:

- the hardware power-on level of the hard-start gate;
- exact comparator and output-pipeline sub-CCK latency;
- live DMA or vertical-window changes during an active run;
- stop-before-start and already-running equality cases;
- multiple DDF regions;
- the enhanced-chipset `$18` hard-window state.

## Verification

Hermetic tests cover:

- an idle prior line carrying an open gate and allowing next-line
  `DDFSTRT=$10`;
- an in-line terminal completion carrying a closed gate and rejecting the
  same `$10` comparator;
- a rejected `$10` comparator not being replayed at `$18`;
- `$18` opening the gate before a coincident DDFSTRT event;
- the deterministic first-line open policy and enhanced-chipset bypass;
- postcard round-trip of the open and non-default closed states, followed
  by deterministic accepted and rejected next-line comparators;
- rejection of version-8 runtime postcards.

## Related documents

- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Idle register-equal DDF boundaries](amiga-idle-equal-ddf-boundaries.md)
- [Enhanced Agnus horizontal DDF hard limits](amiga-enhanced-ddf-hard-limits.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Save State Format](save-state-format.md)
