# Decision: Original Agnus vertical display-window latch

**Date:** July 2026

## The question

How should original Agnus preserve vertical display-window history for
bitplane DMA?

## Evidence

The *Amiga Hardware Reference Manual* defines the `DIWSTRT` and `DIWSTOP`
register fields. It does not expose the hidden vertical comparator state,
the effect of rewriting a vertical boundary on the current line or the
interaction with an already-running bitplane sequencer.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` represents the original-chipset
vertical window with `vdiwstate`. Its decoded VSTART and VSTOP events set
and clear that state. The cleared state terminates an active original-
chipset `bprun`; restoring the vertical window does not restart the old
run. WinUAE persists the state in save data.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` independently stores the
vertical flip-flop as `DDFState::bpv`. Its vertical start and stop signals
change that state, and the clear signal stops the original-chipset
bitplane run. A later horizontal start event is required to establish a
new run. The sequencer state, including `bpv`, is serialized.

Both implementations evaluate the new line's vertical event before that
line's horizontal DDFSTRT event. Both give VSTOP precedence over a
coincident VSTART. Both initialize their emulator state inactive. That
reset result is suitable for deterministic Emu198x construction; it is not
a claim about an unobserved silicon power-on voltage.

The implementations differ in their internal event delays and exact
current-line register-write timing. Their forced late-field close also
depends on revision. That separate boundary and its evidence are recorded
in
[Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md).

Neither implementation treats the vertical latch as an immediate
per-pixel Denise shifter clear. The shared evidence establishes bitplane
sequencer admission and termination, not the exact last residual pixel
after a vertical close.

## The decision

Store original Agnus vertical display-window state as a serialized
comparator-driven latch.

Legacy original-chipset vertical decoding is:

- VSTART is the high byte of `DIWSTRT`, with V8 clear;
- VSTOP is the high byte of `DIWSTOP`, with V8 the inverse of V7; and
- equal decoded values are closed because VSTOP has precedence.

The latch starts inactive. At each line entry, Agnus compares the new
`vpos` with the decoded boundaries before evaluating that line's horizontal
DDF comparators. A VSTART match opens the latch. A VSTOP match closes it.
Lines matching neither boundary preserve the previous state.

The installed original-Agnus revision also supplies a line-held hard
vertical-blank close. It participates as a stop event and takes precedence
over a coincident VSTART. The A1000 and later physical boundaries, builder
selection and save-state consequences are defined in
[Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md).

A changed `DIWSTRT` or `DIWSTOP` write re-evaluates the decoded comparators
against the current `vpos`, with VSTOP priority. If neither matches, the
latch is preserved. Moving VSTOP away from the current line can expose an
unchanged matching VSTART and open the latch. Other numeric relationships
between VSTART, VSTOP and the beam do not reconstruct the state. A VSTART
numerically greater than VSTOP is therefore event history, not a circular
range that is automatically active near the top of the field.

When VSTOP or revision-specific hard force-off closes an active
original-Agnus DDF run before an ordinary or fixed terminal endpoint has
been requested, it sets the existing current-line run-abort latch. The
observed DDFSTRT remains the frozen display-phase origin, but it owns no
further bitplane slots. A later VSTART can reopen vertical eligibility but
cannot resume that stale run. A genuinely later DDFSTRT comparator may
replace the old origin and establish a fresh run when DMA, vertical
eligibility and the horizontal hard-start permission all admit it.

The enhanced-chipset wrappers serialize the shared inner OCS field because
it is part of the nested postcard layout, but do not consume it. Fat Agnus,
ECS Agnus and Alice retain their existing DIWHIGH-aware vertical latches.

Tests observe current-line writes after an eight-CCK settling interval and
future DDF activity eight CCKs after its comparator. This bounds the
eventual state without claiming zero write latency or an exact trigger
cell.

## Save-state compatibility

This latch changed every nested Amiga machine postcard when it was
introduced. The Amiga runtime envelope therefore advanced to schema version
13 and rejected version 12 before payload decoding.

A version-12 OCS snapshot can contain a current beam position and restored
`DIWSTRT`/`DIWSTOP` values that look geometrically active even though a
current-line VSTOP already closed the hidden latch and terminated the old
DDF run. The missing history cannot be inferred during restore. The global
runtime version also rejects version-12 ECS and AGA envelopes even though
those chipsets do not consume the new OCS field.

Raw postcards of `Agnus`, `AgnusEcs`, `AgnusAga`, `AmigaOcsSnapshot`,
`AmigaEcsSnapshot` and `AmigaA1200Snapshot` remain unversioned and change
positional layout. Durable save states must use the versioned runtime
envelope.

Schema version 15 later adds installed original-Agnus revision identity and
the line-held hard-blank force-off state, as recorded in
[Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md).

## Deferred behaviour

This decision does not define:

- exact `DIWSTRT` or `DIWSTOP` write latency;
- the final in-flight bitplane fetch or exact pixel cutoff;
- whether already-fetched or manually written BPLDAT remains visible after
  vertical close;
- vertical close after a terminal fetch endpoint is already pending;
- exact modulo timing; or
- enhanced-chipset multi-region sequencing.

## Verification

Hermetic tests cover:

- deterministic inactive reset state;
- normal VSTART opening and VSTOP closing on line entry;
- VSTOP precedence for equal decoded boundaries;
- start-after-stop values remaining inactive at an early line without
  wrapping-range reconstruction, then closing at the installed revision's
  hard-blank boundary;
- non-matching current-line register writes preserving latch history and a
  moved VSTOP exposing an unchanged matching VSTART;
- a matching current-line VSTOP terminating an unstopped DDF run;
- a future DDFSTRT crossed while vertically closed remaining missed after
  reopen;
- restoring register geometry and then reopening vertically without
  resuming the stale fetch origin;
- a later eligible DDFSTRT establishing fresh bus and pointer activity;
- machine-level pointer stability and fresh-phase advancement;
- postcard round-trip of a closed but geometrically active-looking state,
  followed by deterministic reopen and re-arm; and
- the current runtime envelope rejecting the preceding schema before
  payload decoding.

## Related documents

- [Original Agnus DDF run termination on DMA disable](amiga-ocs-ddf-dma-disable.md)
- [Original Agnus cross-line DDF hard-start gate](amiga-ocs-ddf-hard-start-gate.md)
- [Original Agnus DDF hard-stop terminal policy](amiga-ocs-ddf-hard-stop.md)
- [Original Agnus hard vertical-blank close](amiga-original-agnus-hard-vertical-blank.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
