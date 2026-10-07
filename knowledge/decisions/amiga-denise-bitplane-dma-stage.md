# Decision: Stage bitplane DMA before Denise parallel copies

**Date:** 2026-10-03
**Status:** BINDING

## Evidence

A real Sqrxz Amiga OCS disk boot, using ordinary port-2 joystick input, exposes
a repeatable scrolling fault in the raw framebuffer. At the fine-scroll wrap,
the scenery moves -34 raster pixels instead of -2; three fields later it moves
+30 instead of -2. Both anomalies repeat sixteen fields later. Lossless PNG
texture registration finds exact RGB equality after those translations. The
extra 32 raster pixels are one sixteen-pixel low-resolution bitplane word.

The Copper log shows consistent guest updates: `BPLCON1` changes from `$0000`
to `$00FF` while the plane pointers advance by two bytes, then counts down
through `$00EE`, `$00DD`, `$00CC`. Alternating screen buffers retain the same
coarse and fine scroll relationship. The host monitor shader is downstream of
these already incorrect pixels.

The [Amiga display synthesis](../../../../reference/by-system/commodore-amiga/amiga-graphics-display.md#15-scrolling-bplcon1)
records the primary HRM's fine-delay and whole-word pointer relationship.
Implementation precedent is WinUAE `drawing.cpp`: `do_denise_cck` consumes the
normal RGA stage through `expand_drga` at `idx1` (the preceding CCK); that
stage handles `BPLxDAT` and `bpldat_docopy`. The copy comparator is tested after
pixel output. Its early stage separately handles sprite enable. vAmiga's
`AgnusEvents.cpp` also orders drawing before the simultaneous bitplane fetch.
These references establish implementation precedent, not physical-hardware
calibration of every subpixel edge.

## Decision

Agnus performs and accounts for a bitplane memory transfer in its granted bus
slot. The fetched word and any wide-fetch tail enter a bounded, serialized
transfer stage. On the following CCK, after the first output tick, the transfer
reaches Denise's holding latches; `BPL1DAT` then enables the existing pending
parallel-copy path. The following output tick tests the comparator. No CPU,
Copper, DMA arbitration, master-clock or framebuffer crop is adjusted.

This keeps all sixteen low-resolution delay values within the same fetched
stream. Reading RAM again on retirement would be incorrect: the stage retains
the word actually read in the granted slot. Widths one, two and four carry their
tails together. Runtime snapshot version 36 persists the stage; malformed plane
or width values are rejected during candidate-machine validation.

## Verification and limits

The ROM-free regression renders an asymmetric bitplane through the actual
Agnus/board pipeline and verifies all sixteen delays against the zero-delay
stream. A separate test restores an in-flight transfer at each supported width,
changes RAM after the fetch, and verifies identical subsequent chip state and
pixels. Real Sqrxz lossless captures then show exactly -2 raster pixels per
field through the formerly failing wraps. The longer raw recording shows only
normal -2 and -4 motion as the game changes speed.

A second unmodified game, Solid Gold 1.0, is booted from its author-uploaded
freeware disk through normal joystick input. Sixty-four consecutive lossless
moving-field comparisons show only -2 and -4 raster-pixel movement, with exact
RGB equality after translation and no extra word-sized jumps. This is a motion
consistency check, not a reference-emulator or physical-hardware comparison.
The diagnostic scripts, media provenance and per-field measurements are under
`target/amiga-scroll-validation/`.

The strict Amiga Test Kit v1.21 OCS and AGA reference gates also pass after this
change. OCS checkerboards, dots and crosshatch match vAmiga exactly, with the
registered colour-phase disagreements retained for gradients and EBU bars.
AGA EBU bars, dots and crosshatch match FS-UAE exactly; the other three patterns
retain their registered sprite horizontal-output-phase disagreements. These independent
pattern comparisons do not establish reference agreement for game scrolling.

The sprite-enable early path and independently propagated CPU/Copper data
writes retain their existing models. This correction does not claim that those
paths, hires scrolling, or AGA fine-scroll extensions have been calibrated
against hardware. The shared DMA transport covers OCS, ECS and AGA.

Hires, independent dual-playfield and AGA delay coverage was added on
2026-10-04; see [the scroll-mode decision](amiga-denise-scroll-modes.md). That
work corrects delay decoding and the Lisa serial stage while retaining the
one-CCK DMA transport established here.

## Manual AGA holding-register writes

Lisa decodes all eight bitplane-data words, including BPL8DAT at `$DFF11E`.
The AGA wrapper handles that final word without extending OCS register decode.
It updates holding data only; BPL1DAT at `$DFF110` still queues parallel copy.
The [primary AGA reference](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#manual-bitplane-data-register-decode)
records WinUAE/Minimig precedent. A CPU guest-bus test verifies the final
address reaches the holding register, and a pixel test verifies the eighth bit
appears after the same BPL1DAT strobe as the other planes. This does not model
CPU writes' shared-wide-bus interaction in FMODE 32/64-bit modes.


## Addressed-service integration boundary (2026-10-06)

The approved version-50 shared Agnus pipeline now has a connected bitplane
address/service adapter. A reservation retains whether terminal MOD applies;
addressing samples PT and selected MOD, while service reads RAM from the
retained address, selects the actual width/lanes from immediate FMODE,
and writes PT from that address plus actual bytes and captured signed MOD. The payload still crosses the same one-CCK normal Denise
RGA stage described above. No extra clock or memory read at addressing exists.

The [primary DMA observations](../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md)
record the compiled registered PT/MOD sweep. Automatic display reservations,
shared integration of other clients and independent raster-counter projection
remain pending. Ordinary running guests still use the legacy display grant
until the evidence-backed DDF reservation sequencer is connected. This bounded
adapter work does not close the 90 remaining DMA/busy timing rows.

Reservation mode metadata must not freeze service width. Registered service
reads live FMODE, and the existing mid-line FMODE regression requires the
new transfer width on an already selected cell. Only the fetched payload's
width/words are fixed through the later normal RGA stage. The initial adapter
assumed otherwise; the service-time width regression went red (1 vs 2 words)
and that assumption was corrected before acceptance.

The compiled service sweep also records the old FS-UAE FMODE-2 producer
branch discrepancy explicitly. It agrees on 96 rows for modes 0/1/3; the
32 mode-2 rows are retained as disagreements. The primary Lisa specification
and vendored newer WinUAE select the existing two-word page-mode behaviour.
No old-reference defect is copied into the native pipeline as a golden.


## Overlapping address/service order

The next display PT/MOD sample precedes current outgoing service, as recorded
in the [primary register-stage observations](../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md)
and both registered `do_cck` implementations. Sample immediately after the
shared Agnus stage shift and before claiming outgoing service or servicing
other clients. A same-plane overlap must retain a register rewrite that
exists at the address edge even when outgoing service subsequently updates
that register from its captured source. The earlier adapter's late sample
was corrected after an overlap regression failed with 1FFE instead of 3000.
No new clock, saved field or snapshot version is introduced by this correction.
