# Decision: Original Agnus hard vertical-blank close

**Date:** July 2026

## The question

Where should each original Agnus revision force the vertical display-window
latch closed?

## Evidence

The 1985 *Amiga Hardware Reference Manual* states that vertical blank starts
at line zero and that display data cannot appear in the vertical-blanking
area. It documents the A1000-era external contract, but does not distinguish
the 8361/8367 timing from the later 8370/8371 original Agnus or expose the
hidden display-window force-off signal.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` stores a line-held hard vertical
blank signal as `agnus_bsvb`. It asserts that signal:

- on line zero for the A1000 Agnus; and
- on `maxvpos + lof_store - 1`, the final physical field line, for later
  Agnus revisions.

WinUAE uses that signal as `forceoff` while evaluating the vertical
display-window latch. Force-off suppresses a coincident VSTART and also
acts as an end event, so the close wins. The signal is selected at line
entry and remains stable for the line rather than being reconstructed from
later register writes.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` independently forces its
vertical display-window flip-flop clear near the field boundary. Its clear
event also terminates the original-chipset bitplane run. That revision does
not distinguish the A1000 boundary and uses a generic regional line, so it
corroborates forced close and run termination but is not the authority for
the revision-specific line.

The repository has no primary hardware trace that compares these silicon
revisions. The manufacturer manual establishes the A1000-era line-zero
blanking contract. The exact later-revision distinction and LOF-dependent
final-line selection rely on the pinned WinUAE implementation.

## The decision

Store original-Agnus revision identity separately from the VPOSR revision
bits. VPOSR distinguishes PAL from NTSC but gives the A1000 8361/8367 the
same regional values as the later 8370/8371.

`OriginalAgnusRevision` has two values:

- `A1000` for the 8361/8367 installed by the A1000 builder; and
- `Later` for the 8370/8371 selected by the existing generic original-OCS
  constructors.

This identity is a serialized field inside `Agnus`. It does not add or
reorder an `InstalledAgnus` variant.

At every physical line entry, original Agnus replaces a serialized,
line-held hard-blank force-off state:

- `A1000` asserts it when `vpos == 0`; and
- `Later` asserts it when `vpos` is the final line of the current field.

Construction begins at line zero. The A1000 constructor therefore starts
with force-off asserted, while later-original and enhanced constructors
start with it clear.

The later boundary uses the field length already selected for that line
transition. A non-interlaced field and an interlaced short field use the
regional 312-line PAL or 262-line NTSC total. An interlaced long field adds
the LOF line, making line 312 PAL or line 262 NTSC the final line.

The held force-off state participates in the same vertical-latch evaluation
as VSTART and VSTOP. It suppresses VSTART and acts as a stop, so it wins over
a coincident VSTART. Current-line `DIWSTRT` and `DIWSTOP` writes consume the
held state. They do not recompute hard blank from live `BPLCON0.LACE` or LOF
state and therefore cannot replace a line-entry event retrospectively.

A forced active-to-inactive transition has the established VSTOP
termination result. If an original-Agnus DDF run has started and no
terminal endpoint is pending, the transition marks that run aborted.
Reopening the vertical latch does not resume the stale fetch phase.

The dedicated A1000 bootstrap-ROM constructors select `A1000`. Existing
A500-family and other later original-OCS constructors retain `Later`.
Enhanced Agnus and Alice serialize the nested identity and held state but
do not consume them; their existing enhanced vertical-latch logic remains
unchanged.

## Save-state compatibility

The revision identity and held force-off state change every nested Amiga
machine postcard. The Amiga runtime envelope advances to schema version 15
and rejects version 14 before payload decoding.

A version-14 A1000 snapshot contains regional VPOSR identity, beam position
and display-window history, but cannot say that line zero rather than the
final field line is its hard-close boundary. A mid-line snapshot also
cannot reconstruct the held force-off event safely from live interlace
registers. Both pieces of state must therefore be serialized.

Raw postcards of `Agnus`, `AgnusEcs`, `AgnusAga`,
`AmigaOcsSnapshot`, `AmigaEcsSnapshot` and `AmigaA1200Snapshot` remain
unversioned and change positional layout. Durable save states must use the
versioned runtime envelope.

## Deferred behaviour

This decision does not define:

- the exact sub-CCK delay between a physical comparator and the internal
  force-off level;
- the final residual Denise pixel or already-fetched shifter output;
- Copper, sprite, VERTB interrupt or external sync timing at the same
  boundary;
- the result of a future beam-position write that moves the beam onto or
  away from a hard-blank line;
- vertical close after a terminal fetch endpoint is already pending; or
- enhanced-chipset programmable hard limits and `HARDDIS`.

The implementation assigns bitplane eligibility and DDF termination at
line-entry granularity. It does not claim an exact final bus cell or pixel
cutoff.

## Verification

Hermetic tests cover:

- PAL and NTSC A1000 line-zero close;
- PAL and NTSC later-original-Agnus final-line close;
- non-interlaced, interlaced short-field and interlaced long-field totals;
- A1000 force-off winning over line-zero VSTART;
- A1000 force-off releasing for a line-one VSTART;
- later original Agnus allowing line-zero VSTART after its final-line close;
- a current-line DIW write being unable to reopen A1000 line zero;
- the held later-Agnus event surviving a same-line `LACE` change;
- hard close preventing DDFSTRT admission and terminating an unstopped run;
- model-level A1000-versus-later builder behaviour;
- postcard round-trip of A1000 revision identity followed by deterministic
  line-zero close, plus round-trip of the asserted line-held force-off state
  before a matching DIW write; and
- rejection of version-14 runtime postcards.

## Related documents

- [Original Agnus vertical display-window latch](amiga-ocs-vertical-diw-latch.md)
- [Original Agnus DDF run termination on DMA disable](amiga-ocs-ddf-dma-disable.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga sprite DMA lifecycle](amiga-sprite-dma-lifecycle.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
