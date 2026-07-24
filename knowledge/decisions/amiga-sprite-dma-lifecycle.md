# Decision: Derive Amiga sprite DMA requests from comparator state

**Date**: July 2026

## The decision

Agnus stores each sprite's VSTART, VSTOP and latent data-active state.
It derives control and data requests from those values and the current
beam position:

- on OCS, the fixed control-refetch boundary is line 25 for the
  configured PAL region and line 20 for NTSC;
- on ECS and AGA with `BEAMCON0.VARVBEN` clear, `BEAMCON0.PAL` selects
  the line-25 or line-20 fixed boundary;
- with `VARVBEN` set, `VBSTOP` replaces the fixed boundary regardless
  of `VARBEAMEN` or region;
- that boundary requests control words without overwriting VSTOP;
- a VSTOP match requests control words and disables data state;
- a VSTART match enables data state when VSTOP does not also match;
- no VSTART/VSTOP comparison or ordinary data request occurs inside
  the selected vertical-blank interval;
- fixed timing clears data state at the final field line; programmable
  timing derives reset from its explicit `VBSTOP` event and may carry
  active state across counter wrap.

VSTOP takes precedence when VSTART and VSTOP match on the same line.
The selected reset boundary likewise takes precedence over VSTART and
stale data state, so the sprite fetches control words there.

Programmable blanking treats `VBSTRT` as the inclusive blank start and
`VBSTOP` as the exclusive blank stop plus the one-line reset event. A
start greater than the stop wraps across line zero. Equal start and
stop values describe an empty blank level, but the stop event still
resets sprites and requests control. An explicit `VBSTOP` of zero
therefore refetches control on line zero; data can begin on line one.

The programmed blank level is an edge-driven latch, not a range
reconstructed from the current register values. On each line entry,
`VBSTRT` sets the hidden level before `VBSTOP` clears it and emits the
one-line sprite-reset event. The programmed generator advances
independently of `VARVBEN`; that bit selects either the accumulated
programmed state or the fixed state.

Writes during a line do not create, cancel or replay line-entry events.
Writing `VBSTOP` equal to the current line therefore does not
manufacture a reset, and changing it after a stop edge does not cancel
the event already held for that line. This historical state can cross
the field-counter wrap when the programmed stop line is unreachable.

Comparator state evolves independently of `DMAEN` and `SPREN`.
`SPREN` does not create a request: together with `DMAEN`, it only lets
an existing control or data request claim the sprite's scheduled bus
opportunities. Enabling sprite DMA after VSTART and before VSTOP can
therefore expose an already-active data request. Leaving it disabled
through VSTOP removes that latent request.

Both direct `SPRxPOS`/`SPRxCTL` writes and DMA-fetched control words
re-evaluate the comparator state for the current line. This keeps the
CPU, Copper and sprite-DMA paths on the same state machine.

The implementation does not claim physical reset values for the
write-only programmable comparators. ECS and AGA seed untouched
`VBSTRT` and `VBSTOP` shadows to an out-of-domain sentinel, following
WinUAE. Once any supported programmable vertical register has been
written, line-entry comparisons begin, but an untouched blank edge
remains unarmed. This prevents an unrelated `VTOTAL`, `VSSTRT` or
`VSSTOP` write from manufacturing a line-zero blank event.

This decision currently covers the shared nine-bit sprite VSTART and
VSTOP comparator. ECS VSTART[9] and VSTOP[9] remain unimplemented.
Programmable blanking for sync outputs, VERTB, CIA timing and Copper
restart is separate from this sprite-DMA decision.

## Evidence boundary

The *Amiga Hardware Reference Manual*, third edition, documents:

- the fixed vertical-blank boundaries as line 20 NTSC and line 25 PAL
  (printed page 219, PDF page 234);
- VSTART activation, VSTOP termination and the control/data sequence
  (printed pages 124–126, PDF pages 139–141);
- the visible consequence of disabling sprite DMA between VSTART and
  VSTOP (printed page 109, PDF page 124);
- programmable `VBSTRT`/`VBSTOP` and vertical total behaviour (printed
  page 303, PDF page 318);
- the `BEAMCON0.VARVBEN` selector, independently of `VARBEAMEN`
  (printed page 304, PDF page 319).

The *A500/A2000 Technical Reference Manual* identifies `SPREN` as the
sprite-DMA enable and permits processor writes to `SPRxPOS`/`SPRxCTL`
(pages 208 and 210–211).

Those manuals do not directly expose the hidden comparator state while
`SPREN` is clear, nor do they explicitly name the selected blank-end
line as the automatic control-refetch strobe. Equal, wrapping and zero
programmable-edge behaviour is also not specified there. These
implementation details are corroborated by independent code paths in:

- WinUAE `c32694e338fa5f34977f522eb4898adb069d2e73`, principally
  `custom.cpp`;
- Minimig-AGA `3ab91cd9220d4d047886d215b515227cbe568bdd`, principally
  `agnus_spritedma.v` and `agnus_beamcounter.v`;
- vAmiga `60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`, principally
  `Agnus.cpp` and `AgnusRegs.cpp`.

WinUAE and Minimig agree with Commodore's 25/20 regional boundaries.
The inspected vAmiga revision uses 25/19 despite a neighbouring comment
that says 25/20; it is not used as the NTSC boundary authority.

WinUAE is the principal implementation corroboration for programmable
corner cases. Its changelog records `VBSTOP` as the sprite-reset and
first-control-load line, with `VBSTOP + 1` as the first possible data
line. Minimig independently ties its blank-end event to sprite control
fetches, but its programmable blank generator is incomplete and is not
used for `VARVBEN`-alone or wrapping semantics. The inspected vAmiga
revision does not implement programmable vertical blanking.

The reset sentinel and the rule that any programmed vertical-register
access enables comparisons follow WinUAE's implementation. They are
not asserted as manufacturer-documented power-on behaviour.

## Consequences

- Each fixed or programmed per-slot request is derived. The programmed
  event generator itself has historical state that cannot be recovered
  from registers and beam position. ECS and AGA snapshots preserve the
  vertical-accessed flag, blank-active latch and line-held edge events.
- The Amiga runtime postcard schema is version 2. Version-1 snapshots
  are rejected rather than migrated.
- The per-request `SpriteDmaVerticalTiming` value remains transient.
  The current-CCK sprite bus-use latch is serialized because a snapshot
  can preserve the machine at the second half of a CCK.
- VSTOP remains the value last supplied by control data; it is no
  longer temporarily replaced to manufacture a reset-line match.
- ECS and AGA arbitration, DMA service, direct comparator writes and
  per-line lifecycle all receive the same derived vertical timing.
- Idle sprite opportunities continue through Agnus's normal priority
  chain to the blitter or CPU.
- A request that performs a fetch remains authoritative for the whole
  CCK even if the fetched control word changes the comparator state.
  See the single-slot authority decision for that arbitration rule.

## Drift triggers

Reject these patterns:

- gating VSTART/VSTOP state updates on `SPREN`;
- treating every sprite opportunity as owned because `SPREN` is set;
- forcing every stored VSTOP to 25 to trigger a new-frame control fetch;
- using the PAL reset boundary for an NTSC Agnus;
- continuing to use a fixed regional reset while `VARVBEN` is set;
- requiring `VARBEAMEN` before programmable blank affects sprites;
- treating equal `VBSTRT`/`VBSTOP` as removing the independent stop
  event;
- manufacturing an extra field-end sprite reset in programmable mode;
- evaluating VSTART before an equal VSTOP and leaving the sprite active;
- reconstructing the programmed blank latch or line-held events solely
  from current registers and beam position;
- serializing a derived per-slot request instead of the underlying
  comparator and programmed-event state;
- dropping current-CCK bus ownership while restoring at half-CCK
  precision.

## Related documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md)
