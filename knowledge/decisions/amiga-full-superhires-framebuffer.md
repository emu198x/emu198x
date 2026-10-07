# Full Lisa superhires framebuffer

Status: Implemented, 2026-10-05.

Lisa's serializer produces four 35 ns samples per lores period. The hires
framebuffer discarded samples 1 and 3 before palette/HAM resolution, so a
correct chip stream could still lose narrow sprite pixels and HAM operations.

The board now retains a fixed 1536×576 ARGB framebuffer for Lisa, while OCS/ECS
retain their existing 768×576 buffers. Resolution changes do not resize the
raster. Slower source samples are held across the appropriate native samples.
Each Lisa sample participates in palette/HAM resolution before final border
and horizontal blanking. Non-interlaced row duplication does not advance the
chip twice. Lisa's fine HBLANK comparator keeps all eight positions per CCK;
WinUAE `drawing.cpp::update_hblank` likewise preserves its three fine bits
when the host resolution is superhires.

A normal COLOR write keeps the previous palette value for two 35 ns samples,
preserving the calibrated one-hires-period delay. Copper's early RGA stage
continues to hold its previous palette value through the current board tick.
The generic resolution hook defaults to the existing OCS/ECS behaviour.

Frame packets, RGB signal timing, UI framebuffer sizing and framebuffer dumps
use the selected chipset's width. Lisa's sample clock doubles with its width,
so the PAL television geometry remains physically unchanged; monitor geometry
continues to use the host's display contract. Native screenshots preserve the
1536-wide sample raster. Snapshot v42 rejects v41's incomplete framebuffer.

The primary [35 ns DMA sprite observations](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#aga-full-superhires-sprite-capture--2026-10-05)
constrain solid/patterned sprite rates and all quarter positions. Native DMA
checks use those recorded serial positions at fixed marker-relative offsets;
there is no alignment search. Existing Test Kit and Workbench golden captures
remain hires references, so their comparators select the fixed even phase of
Lisa's buffer. They do not verify the intervening samples. Chip/board tests
cover all four sprite samples, HAM8 progression, colour delay, fine blanking,
snapshot restoration and the frame packet's width/sample clock/aspect ratio.

Validation artifacts and native probe outputs for this session are retained
under `/private/tmp/emu198x-full-superhires-validation/`.

The [correctly sized superhires playfield diagnostics](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#aga-superhires-playfield-dma-address-selection--2026-10-05)
close the original malformed-buffer disagreement. They also expose a separate
Alice wide-fetch issue: word-aligned pointers and FMODE=2 can repeat selected
word lanes instead of reading consecutive words. Bitplane DMA now samples the
reference's selected lanes during the granted transfer, before entering the
existing pending stage. Pointer increments, grants, serializer phases and
snapshot schema remain unchanged. The regression covers all four FMODE modes
and every even pointer offset in an eight-byte block, including restoration
and later RAM writes. Full-sample serial/scroll tests cover all fetch widths.

The corrected guest builder and full-resolution reference host patch are
source-controlled. Their captures and validation records are retained under
`/private/tmp/emu198x-superhires-playfield/`. These remain software diagnostics,
not an admitted silicon-calibrated corpus. Mid-line register propagation and independent hardware calibration
remain accuracy work.


The [wide sprite and full-raster observations](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#wide-sprite-dma-and-full-raster-origins--2026-10-05)
now cover sprite grant-time lane selection and padded control-word strides.
Address-sensitive tests cover all sixteen sprite FMODE/alignment combinations.
Lisa selects fixed $10..$5D horizontal blanking when its propagated ECSENA
and EXTBLKEN selectors do not jointly select the programmable comparator.
No serializer, DMA grant, lifecycle or snapshot layout changes were required.

Sixteen sprite and eight playfield guests pass 72 whole-common-raster
comparisons at recorded native sample origins (16,2), including border and
blanking. FS-UAE 5.0.7's separate sprite page-mode producer bug is preserved
as failed evidence; its exploratory correction is an explicit one-line port
from registered newer WinUAE. Primary observations distinguish both producers.
Artifacts and verification hashes live at
`/private/tmp/emu198x-wide-sprite-validation/verification.json`.
Historical hires Test Kit/Workbench references still verify only their
retained even phase; the native diagnostics supply the intervening samples.


## Mid-line register propagation

The [primary mid-line register and transfer observations](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#mid-line-display-register-discovery--2026-10-05)
reproduced four resolution-transition errors, a FMODE 32→64-bit error hidden
by constant words, and a ten-native-sample-early BPLCON4 XOR edge.

The user approved extending the existing chip stages and versioning save
states on 2026-10-05. Alice retains raw BPLCON0 and a DMA-side copy after the
two-CCK register and two-CCK pending-arbitration path. FMODE's two pending
cadence slots remain distinct from its live transfer width. Lisa's BPLCON0
and bitplane FMODE selectors cross the existing normal output stage; BPL1DAT
then latches the copy width for its data. Board synchronization and blanking
feed the raw input without overwriting Lisa's timed serializer copy.

Lisa's serial history advances at the fixed 35 ns clock. A continuous
four-period accumulator for each playfield preserves its residual shift phase
across resolution changes; parallel loads restart only their own group. The
history ring retains 512 native periods for the fixed output transport.
The [independent scroll decision](amiga-denise-scroll-modes.md) supersedes
its earlier programmable-delay tap. Cursor and phase
values outside their rings are rejected during deserialization.

Parallel copy uses the installed fetch-width comparator on the physical
Denise counter, independently of the DDF-relative storage coordinate. The
Workbench 64-bit hires row detects the incorrect origin: it otherwise moves
the playfield right by 32 hires samples and clips its right edge. Each transfer
replaces the complete holding group, including its wide tail, while the
active FIFO continues draining. BPLAM reaches palette selection after ten
native periods without recolouring border or sprite output. Raw register
mirrors remain inspectable before their timed copies change.

Snapshot v45 persists all these in-flight copies, independent source phases,
complete pending words and the full 32-bit shifter; v44 and older Amiga save
states are explicitly rejected. The separate scroll-stage approval covers this
compatibility break. The regression probes, original failed captures, transfer
logs, rebuilt output and verification records live under
`/private/tmp/emu198x-midline-display/`.

These remain software-reference diagnostics. Independent silicon calibration
and other unmeasured mid-line register combinations remain accuracy work.

Verification: all 72 mid-line and 72 static sprite/playfield reference-field
comparisons pass over the complete 1512 × 574 common raster (124,975,872 RGB
pixels, zero differences). The release regression run passes 799 tests; all
eight strict-asset boot-matrix checks pass. Both explicit Test Kit video gates
pass all six patterns exactly. Formatting, Ruff and strict Clippy pass. The
physical-counter regression was verified red with the old DDF-relative
input, then green after restoring the correction. The broad suite retains
31 explicitly ignored fixture/campaign tests; only the two video gates were
invoked separately. Hashes and logs are recorded in
`/private/tmp/emu198x-midline-display/fix-verification.json`.

## Register-phase sweep and retained narrow data

The [primary phase-sweep observations](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#display-register-phase-sweep--2026-10-05)
cover 26 guests: six resolution directions, four width directions and three
controls, at DDF starts $30/$38. Thirty-two even WAIT positions per guest
span $60..$9E on successive scanlines. These are programmed wait positions;
actual register deliveries remain subject to Copper arbitration.

The sweep exposed one additional error: FMODE 0→1 turns the preceding word
into black at sixteen alternate phases. Lisa’s 16-bit and 32-bit modes share
a 32-bit register with output taps at bits15/31. Shifting a narrow word moves
it into the upper half, where widening can expose it before parallel copy.
The shared core now retains and clocks this full register independently of
the narrow sixteen-bit countdown. Parallel copies install the transported
word/group; the existing 64-bit path remains separate. Its full state is
serialized and discoverable as `denise.bitplanes.shift_data_32`.

The user approved snapshot v44 for this additional state. The word37 replay
regression was observed failing before the correction; a restore regression
checks that consumed narrow bits survive and reappear at the wide tap. All
78 new reference-field comparisons and the preceding 144 comparisons pass
with zero RGB differences. Original guests and independent reference buffers
remain unchanged. Artifacts live under
`/private/tmp/emu198x-display-phase-sweep/`.

The sweep does not cover every legal Copper delivery phase, FMODE=2
transitions, other plane counts, scrolling/priority/HAM combinations or
physical silicon calibration. Those require their own measured probes.

Phase-sweep verification is recorded in
`/private/tmp/emu198x-display-phase-sweep/verification.json`: 222 reference
fields, 192,671,136 RGB pixels, zero differences; 801 release regression
tests pass, all eight strict-asset boot checks pass, and both explicit Test
Kit video gates match all six patterns exactly. Build, formatting, Ruff and
strict Clippy pass. The broad suite leaves 31 explicit fixture/campaign
tests ignored; the two video gates were invoked separately.

## BPLAM counter-origin correction (2026-10-07)

The [counter-qualified residual observations](../../../../reference/by-system/commodore-amiga/2026-ecs-colour-blanking-observations.md#aga-bplam-residual-counter-qualified-2026-10-07)
supersede the ten-native-period BPLAM claim above. The earlier measurement
included four samples of reference storage padding. The existing early-RGA
input reaches palette selection after six native samples: counter 260 to
261.5 in the retained guest. Lisa now reads tap 4 of its existing ten-sample
history. History encoding/clocking and the raw BPLCON4 mirror are unchanged;
snapshot 54 keeps its layout. Border and sprite output remain separate.

## Strobe-driven Lisa vertical blanking (2026-10-07)

The [top-field observations](../../../../reference/by-system/commodore-amiga/2026-ecs-colour-blanking-observations.md#aga-first-visible-line-vertical-blanking-2026-10-07)
identify a missing Lisa vertical-blank latch. The existing normal RGA strobe
now feeds Lisa before output blanking is composed. STRHOR requests release;
leaving STRHOR requests blanking. Fixed and programmable comparator paths
retain independent pending events and levels. A selected horizontal start
consumes a pending start; its stop consumes a pending release. The final
mask does not stall palette, HAM, sprite or serial advancement.

The user approved saving this state in snapshot 55, rejecting version 54.
The board's existing strobe descriptor and horizontal-counter stages remain
authoritative; no additional clock or raster-derived vertical state is added.
All ten retained AGA blanking guests now agree over thirty complete common
rasters. The archive retains eighteen pre-fix failures as negative controls.
This is UAE-family agreement; exact silicon timing remains unmeasured.
