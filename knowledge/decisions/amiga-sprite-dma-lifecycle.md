# Decision: Derive Amiga sprite DMA requests from comparator state

**Date**: July 2026

## The decision

Agnus stores each sprite's VSTART, VSTOP and latent data-active state.
It derives control and data requests from those values and the current
beam position:

- the fixed control-refetch boundary is line 25 on PAL and line 20 on
  NTSC;
- that boundary requests control words without overwriting VSTOP;
- a VSTOP match requests control words and disables data state;
- a VSTART match enables data state when VSTOP does not also match;
- no VSTART/VSTOP comparison occurs inside fixed vertical blank;
- the final field line clears data state.

VSTOP takes precedence when VSTART and VSTOP match on the same line.
The reset boundary likewise takes precedence over stale data state, so
the sprite fetches control words there.

Comparator state evolves independently of `DMAEN` and `SPREN`.
`SPREN` does not create a request: together with `DMAEN`, it only lets
an existing control or data request claim the sprite's scheduled bus
opportunities. Enabling sprite DMA after VSTART and before VSTOP can
therefore expose an already-active data request. Leaving it disabled
through VSTOP removes that latent request.

Both direct `SPRxPOS`/`SPRxCTL` writes and DMA-fetched control words
re-evaluate the comparator state for the current line. This keeps the
CPU, Copper and sprite-DMA paths on the same state machine.

This decision covers fixed PAL and NTSC timing. Coupling the reset event
to programmable `VBSTOP` when `BEAMCON0.VARVBEN` is enabled is separate
work.

## Evidence boundary

The *Amiga Hardware Reference Manual*, third edition, documents:

- the fixed vertical-blank boundaries as line 20 NTSC and line 25 PAL
  (printed page 219, PDF page 234);
- VSTART activation, VSTOP termination and the control/data sequence
  (printed pages 124–126, PDF pages 139–141);
- the visible consequence of disabling sprite DMA between VSTART and
  VSTOP (printed page 109, PDF page 124).

The *A500/A2000 Technical Reference Manual* identifies `SPREN` as the
sprite-DMA enable and permits processor writes to `SPRxPOS`/`SPRxCTL`
(pages 208 and 210–211).

Those manuals do not directly expose the hidden comparator state while
`SPREN` is clear, nor do they explicitly name the fixed blank-end line
as the automatic control-refetch strobe. The implementation details are
corroborated by independent code paths in:

- WinUAE `custom.cpp` (`generate_sprites`, `sprstartstop` and fixed
  vertical-blank boundary handling);
- Minimig-AGA `agnus_spritedma.v` and `agnus_beamcounter.v`;
- vAmiga `Agnus.cpp` and `AgnusRegs.cpp`.

WinUAE and Minimig agree with Commodore's 25/20 regional boundaries.
The inspected vAmiga revision uses 25/19 despite a neighbouring comment
that says 25/20; it is not used as the NTSC boundary authority.

## Consequences

- Fixed reset control requests are derived events, so no new snapshot
  field or snapshot-version migration is required.
- VSTOP remains the value last supplied by control data; it is no
  longer temporarily replaced to manufacture a reset-line match.
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
- evaluating VSTART before an equal VSTOP and leaving the sprite active;
- adding serialized state for a request that can be derived from region,
  beam position and comparator state.

## Related documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md)
