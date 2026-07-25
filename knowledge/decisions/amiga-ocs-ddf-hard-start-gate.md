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

For the phase-shifted `DDFSTRT=$1C` case, the fixed right stop produces a
logical terminal endpoint at `$E3`. A short PAL or NTSC line exposes physical
positions only through `$E2`. Original Agnus masks DDF positions with `$FC`,
so the next architectural start positions are `$00`, `$04`, `$08`, `$0C`,
`$10`, `$14` and `$18`.

The pinned WinUAE revision carries the old run, stopping phase and fetch
counter across wrap. The old run prevents `$00` from establishing a fresh
run, and its normal-stop path closes the limiter before `$04` is considered.
The pinned vAmiga revision carries `bprun`, `lastFu`, `shw` and the fetch
counter across the same boundary. It independently rejects a fresh `$00`
run and clears the original-chipset start permission before `$04`.

These implementations agree on start admission. They do not agree closely
enough to identify one externally visible next-line bitplane slot, pointer
advance or modulo event.

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
- when the one-CCK logical `$E3` endpoint lies beyond a short physical line,
  line wrap projects its proven start-admission result into a closed effective
  gate before the next line's DDF comparators are evaluated;
- end-of-line alone does not reset the gate;
- a DDFSTRT comparator before `$18` can start only while the carried gate is
  open;
- a comparator missed while the gate is closed is not replayed when `$18`
  subsequently opens it.

An idle line has no terminal completion, so it carries an open gate into the
next line. A line whose terminal unit completes after its `$18` opening and
before wrap carries a closed gate.

For the projected short-line case, `$00` cannot establish a fresh run and
the effective gate is closed for `$04` through `$14`. `$18` still reopens it
before a coincident start is considered. This is a compressed start-admission
model. It does not place the old terminal fetch on a physical next-line bus
cell.

Enhanced Agnus and Alice do not consume this original-chipset field. Their
left hard window belongs to the enhanced multi-region sequencer.

Emu198x initializes the gate open to preserve its established deterministic
first-line behaviour. This is an implementation compatibility choice, not a
claim about the hardware power-on level.

The serialized gate was introduced in runtime postcard schema version 9.
This additional short-line transition does not change the nested postcard
layout, but runtime postcards advance to schema version 10. A version-9
snapshot taken immediately after the old wrap behaviour has already lost
the `$E3` endpoint and records an open gate indistinguishable from a
legitimate idle-line carry. That state cannot be repaired during restore, so
version-9 snapshots are rejected before payload decoding.

The runtime uses one global Amiga envelope version. Version-9 ECS and AGA
runtime snapshots are therefore also rejected even though those chipsets do
not consume this original-chipset gate transition.

Raw postcards of the public `AmigaOcsSnapshot`, `AmigaEcsSnapshot` and
`AmigaA1200Snapshot` machine values are unversioned. Their positional layout
did not change, so they remain decodable. An early-OCS raw snapshot captured
after the old buggy wrap can silently restore the stale open gate; ECS and AGA
raw states are not semantically affected by this transition. Durable save
states must use the runtime envelope.

## Deferred behaviour

This decision defines only the start-admission result of the phase-shifted
terminal unit. Emu198x does not carry the old run, stopping phase and fetch
counter into next-line bus arbitration. The exact terminal bitplane slot,
pointer advancement, modulo timing and contention during `$00` through `$03`
remain unresolved. Closing the effective gate at line entry must not be read
as a claim that the physical limiter changes at that position.

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
- all legal pre-`$18` comparators after a short-line `$E3` logical terminal,
  including `$00`, with no replay at `$18`;
- the same logical `$E3` endpoint being represented in-line on an NTSC long
  line;
- postcard round-trip immediately before short-line wrap, followed by the
  deterministic closed-gate result;
- rejection of version-9 runtime postcards.

## Related documents

- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Idle register-equal DDF boundaries](amiga-idle-equal-ddf-boundaries.md)
- [Enhanced Agnus horizontal DDF hard limits](amiga-enhanced-ddf-hard-limits.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Save State Format](save-state-format.md)
