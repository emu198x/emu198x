# Decision: Decode independent scroll fields and retain Lisa's serial stream

**Date:** 2026-10-04
**Status:** BINDING

## Evidence

The [primary display synthesis](../../../../reference/by-system/commodore-amiga/amiga-graphics-display.md#15-scrolling-bplcon1)
cites the HRM PF1H/PF2H layout: low nibble delays odd-numbered planes,
high nibble delays even-numbered planes. The old chip tests deliberately
preserved an archive implementation that reversed those fields.

Vendored WinUAE `drawing.cpp::update_bplcon1` masks delays by fetch width
and resolution. A 16-bit hires fetch uses seven lores delay clocks; each
clock is two hires pixels. The archive discarded bit zero instead of masking
bit three. DMA-rendered regressions fail for both the inverted fields and
that hires rule before the correction.

The [AGA synthesis](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#23-extended-horizontal-scroll-bplcon1)
records the non-contiguous six-bit delay layout, fractional fields and
FMODE-dependent mask. Its implementation precedent is Minimig
`denise_bitplanes.v` and `denise_bitplane_shifter.v`: fetched data feeds a
serial shifter, then a history scroller. WinUAE independently decodes the
same high and fractional fields. These are implementation precedents, not
physical-hardware calibration.

An AGA DMA regression also fails at delay 15 even after the field decoding
is fixed: subtracting a tick from the parallel-copy comparator selects the
neighbouring word. Wider fetches expose the same problem at extended delays.

## Decision

OCS/ECS pending parallel copies use PF1H for BPL1/3/5 and PF2H for BPL2/4/6.
Hires masks each nibble to seven lores clocks. The legacy immediate-load
helper uses the same fields and converts hires clocks into serial pixels.

Lisa compares the physical horizontal counter against each playfield's own
masked integer and fractional BPLCON1 offset at every 35 ns output period.
Each matching group copies its pending holding words and restarts only its
source clock. BPL1DAT freezes the entire pending group, including the wide
fetch tails; subsequent DMA transfers cannot mix a new tail with an old head.
The fixed output transport delay follows the two independently clocked streams
before playfield priority, sprites, collisions, HAM and palette composition.
This replaces the earlier programmable history-tap model, which agreed for
fixed offsets but changed in-flight data retroactively on mid-line writes.

BPLCON1's raw register mirror remains immediate; Lisa's comparator sees the
copy leaving the existing normal register propagation stages, as BPLCON0 and
FMODE do. OCS/ECS scroll behavior is unchanged. FMODE selects the installed
16-, 32- or 64-pixel fetch mask; resolution masks and fractional quantization
remain derived from the registered reference implementation.

The user approved extending these existing stages and breaking snapshot
compatibility on 2026-10-05. Snapshot version 45 preserves independent source
clocks, pending complete words and BPLCON1 selector stages, and explicitly
rejects version 44. Invalid phases and pending fetch-tail lengths fail
deserialization. Diagnostics expose both clocks, pending tails and the raw
and visible selector copies.

## Verification and limits

`runtime-commodore-amiga/tests/scroll_dma.rs` renders asymmetric chip-RAM
textures through actual Agnus grants and board output. It checks every OCS/ECS
nibble, both resolutions, each playfield separately and simultaneous overlapping
playfields. AGA adds every supported wide-fetch delay, masked aliases,
one-hires-pixel fractional steps and mid-line serialization with live history
and DMA. Expected pixels are translations of the same fetched source stream,
not hashes blessed from the implementation.

Strict Test Kit reference gates and boot goldens remain independent checks.
Their colour/sprite phases now agree after the separate neutral-probe corrections;
no independent reference image is replaced. This campaign covers ordinary lores/hires scrolling
with a standard aligned fetch window. It does not establish physical calibration
of superhires, arbitrary DDF phase changes or mid-line resolution/FMODE changes.

## Mid-line reference discrepancy — 2026-10-05

The [independent scroll sweep](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#independent-mid-line-playfield-scroll-sweep--2026-10-05) establishes a limit of the history-selector decision above. All six fixed-offset controls match the reference exactly; all six changing-offset guests differ in three stable fields. Changing the history tap immediately is not equivalent to changing the physical parallel-copy comparator while holding data is pending. The registered reference independently copies and restarts each group on its selected phase.

The user approved the extension of the existing per-group timing stages and snapshot v45. The decision above supersedes the programmable history-tap model. Whole-raster verification of the implementation is recorded below.

## Completed verification — 2026-10-05

All 54 new diagnostic guests rebuild byte-identically: 18 mid-line guests at
DDF $38, and 18 static extended/fractional guests at each of DDF $30/$38.
They cover all three resolutions and distinct 16/32/64-bit fetch widths. Their
162 unchanged reference fields match exactly; the preceding 222 fields remain
exact. The combined 384 fields contain 333,268,992 RGB samples with no fitted
alignment or interior exclusions.

The primary observations demonstrate why DDF $30 cannot support every earlier
translation assertion: some wide offsets select the preceding word phase.
The AGA translation fixtures use DDF $38, while the independent full-raster
sweeps retain both origins. Board tests also assert that a mid-line write to
the other playfield does not affect the active DMA texture. A copy-timing test
fails before the correction; pending-wide-word and normal-selector-stage tests
cover the two related pipeline boundaries. Restore tests preserve independently
phased streams, pending data and selector stages, reject invalid phases/tails,
and explicitly reject v44 runtime saves.

All 808 affected release tests and eight strict boot checks pass, plus both
explicit six-pattern Test Kit video gates. Thirty-one broad-suite fixture/campaign
tests remain ignored. Build, formatting, Ruff and strict Clippy pass. Evidence
and hashes are in `/private/tmp/emu198x-midline-scroll/final-verification.json`.
The registered reference was restored after its byte-identical logging capture.
The bounds above remain software-reference coverage; they do not calibrate silicon
or establish arbitrary fractional pairs, other plane counts/priority/HAM,
FMODE=2 or combined mid-line selector changes.
