# Fixed PET board owners

Adopted 2026-10-31 under the approved PET correction and split plans in
emu198x/docs (2026-10-11 and 2026-10-31).

The discrete 32K 2001N/3032, graphics-keyboard 4032 and business-keyboard 8032
have independent machine owners over `common-commodore-pet`. Each owner binds
one board and validates restored state before installation. ROM identity does
not select wiring. The 4032/8032 profiles retain their old 40/80 IDs.

This explicitly overrides `runtime-internal-shape.md`'s generic runtime axis for
PET: its existing four concerns remain shared, with a fifth `machine.rs` closed
enum holding three fixed owners. This follows the fixed-board split pattern and
keeps family lifecycle, input, queries and snapshot policy in one place.
No mutable dereference exposes replaceable raw board state.

Version 5 contains the board/video phases, pending PRG and undelivered samples.
Version 4 is rejected before decoding, as approved; it cannot reconstruct the
omitted state exactly. No intermediate v5 layout is published.

Second PIA, IEEE-488 peripherals, cassette and full 6545 interlace remain
separate work. Three owners do not mean the whole PET family is complete.
