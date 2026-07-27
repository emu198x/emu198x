# Decision: Amiga blitter line-mode ONEDOT

**Date:** July 2026

## The question

How does `BLTCON1.ONEDOT` affect line-mode result generation, D
transfers, chip-bus ownership and completion?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed page 191,
defines ONEDOT as selecting a single bit per horizontal row. Its
line-mode register setup says to set ONEDOT when one pixel per row is
required and clear it otherwise. Printed page 192 repeats that rule in
the setup summary. This is primary evidence for which pixels may reach
memory. The manual does not state whether a suppressed D transfer owns
the bus, whether its generated value reaches BZERO or which internal
cycle emits completion.

The inspected WinUAE revision
`69df7fed523f9e79c5641ea4cdfe80eae5c32967` keeps a one-dot latch. It
allows a pixel when ONEDOT is clear or the latch is clear, sets the
latch after the pixel decision and clears it on a vertical step. It
stages that eligibility before bus allocation. A suppressed D operation
uses an idle internal channel rather than a bus write.

WinUAE still performs line minterm processing and BZERO accumulation
for the suppressed operation. Its line path finishes on the final D
cycle or on the cycle where final D would have occurred when ONEDOT
suppresses it.

The inspected Minimig revision
`d5f541e0f6bedf950b233a0075a21cf861b5dd78` independently describes
the line write state as a potentially free bus cycle in one-dot mode.
Its D request is conditional on first-pixel eligibility, while the
state machine still advances to completion.

The inspected vAmiga revision
`f9e34ca4f199172df77b7109c3fe1f380b87833b` also permits the first
pixel in a horizontal segment, rearms that permission after vertical
movement, updates BZERO from every generated D value and writes memory
only when the pixel is eligible. These implementations corroborate the
internal effects that the hardware manual does not expose.

No primary trace in the repository measures the chip-bus request and
BZERO result of a suppressed pixel. The memory-visible one-pixel rule
is a hardware-manual fact. The bus, BZERO and exact completion rules are
compatibility choices supported by three independent implementations.

## The decision

The line engine keeps a serialized “dot already drawn in this
horizontal row” latch.

For every logical line step it:

1. reads C through the existing line pipeline;
2. generates D from A, the current B texture bit, C and the minterm;
3. updates BZERO from the generated D value;
4. decides whether D is eligible for transfer;
5. advances texture, error, pointer and line-count state; and
6. rearms ONEDOT only if that step moved vertically.

D is eligible when ONEDOT is clear or no dot has yet been drawn in the
current horizontal row. The first eligible step marks that row as
drawn. A vertical step clears the latch for the next logical pixel.

Suppression removes the complete D transfer. It does not mask the
generated word, preserve BZERO, freeze texture or error state, or add a
replacement write. A non-zero suppressed result therefore clears
BZERO even though destination memory is unchanged.

The pending logical `WriteD` remains visible to the line scheduler, but
its bus requirement is determined before arbitration. A suppressed
operation does not grant the chip bus to the blitter and reports no
actual blitter bus use. The 68000 may use that CPU/free cell even with
BLTPRI set.

The machine retains the pre-service nasty-ownership decision for both
master/4 phases of the CCK. This is necessary because the line engine
can advance from the suppressed write to its next C request before the
CPU polls. Recomputing from that next request would retroactively turn
the already-free cell into a blitter-owned cell.

If the final line step is suppressed, the blit still finishes on its
would-be-write CCK on original Agnus, ECS Agnus and Alice. The source
interrupt fires once, internal busy clears, and the ordinary
completion-observer holds begin. No AGA area-D completion tail is
added.

## Save-state compatibility

The current-row ONEDOT latch, current-CCK pre-service nasty ownership
and whether that ownership observation has been recorded cannot be
reconstructed from pointers, error terms, memory or the next scheduler
request. All three are serialized.

The Amiga runtime envelope advances to schema version 18 and rejects
version 17 before payload decoding. Restoring version 17 during an
active line could emit a suppressed write, omit an eligible first-row
write or change whether the CPU owns the current cell.

Raw postcards of the affected Agnus and machine types remain
unversioned and change positional layout. Durable save states must use
the versioned runtime envelope.

## Model boundary

The current line scheduler represents each pixel as a strict C read
followed by a logical D operation. A suppressed D operation advances on
an admitted CPU/free CCK but does not drive its bus. This is the
implemented compatibility boundary; it does not claim every internal
line microcycle of a physical Agnus recipe.

This decision does not change the existing non-nasty blitter policy.
CPU-request-aware three-denial arbitration remains separate work.

## Deferred behaviour

This decision does not define:

- the optional line-mode B DMA recipe;
- behaviour of unsupported line setups with C DMA disabled;
- the exact physical idle microcycle preceding D eligibility; or
- general CPU and non-nasty blitter arbitration.

## Verification

Hermetic tests cover:

- a non-ONEDOT horizontal control writing every generated pixel;
- ONEDOT writing only the first pixel in one horizontal row;
- vertical movement rearming the first-pixel permission;
- a non-zero suppressed result clearing BZERO without changing memory;
- final suppressed completion and bus use on pre-AGA and Alice identity;
- the CPU reusing a non-final suppressed nasty-mode D cell after the
  live line request has advanced to C in the OCS machine;
- the A1200 Kickstart 3.1 and Workbench 3.1 golden frame changing only
  in the previously incomplete horizontal-scroll arrow glyphs, followed
  by a stable rerun; and
- runtime snapshot round-trip after the first dot and C read, before
  the suppressed logical D operation.

## Drift triggers

Reject these patterns:

- using ONEDOT as a mask on the generated D word;
- writing D after the current horizontal row already emitted its dot;
- rearming ONEDOT on horizontal movement;
- preserving BZERO because a D transfer was suppressed;
- charging a suppressed D operation as actual chip-bus use;
- recomputing current-CCK nasty ownership from the next line request;
- delaying a final suppressed line completion into the area-D tail; or
- reconstructing ONEDOT row state during snapshot restore.

## Related Documents

- [Amiga blitter line texture phase](amiga-blitter-line-texture-phase.md)
- [Amiga blitter completion pipeline](amiga-blitter-completion-pipeline.md)
- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
