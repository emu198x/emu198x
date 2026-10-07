# Decision: Delay AGA Lisa colour writes at pixel output

**Date:** 2026-08-01
**Status:** BINDING

## The question

When does an AGA `COLORxx` register write become visible at Lisa's pixel
output?

## Evidence

The inspected FS-UAE 5.0.7 source is derived from the WinUAE implementation
family. Its AGA display path states that colour changes are delayed by one
hires pixel and applies the preceding palette value for that output sample.
This is one software-implementation family, not independent agreement between
FS-UAE and WinUAE.

The registered Amiga Test Kit v1.21 A1200 AGA PAL lane supplies the end-to-end
observation. Before the delay was represented, the gradients and EBU-bar cases
disagreed at beam-raced palette changes. The current reference geometry is
fixed from beam coordinates rather than searched from image content. Under
that absolute mapping, introducing one hires sample of Lisa colour-output delay
makes the palette boundaries exact without changing the independently produced
reference pixels.

The Test Kit result establishes the visible phase for the registered patterns.
It does not expose Lisa's internal gates or establish the behaviour of every
AGA mode and register sequence.

## The decision

An AGA `COLORxx` write updates Lisa's register mirror and 256-entry palette
immediately. Register reads and diagnostics therefore report the new value as
soon as the custom-register write is dispatched.

Pixel output retains the preceding value of the addressed palette entry for
exactly one hires output sample. The next sample observes the new value. Any
output sample consumes the pending delay, including a sample that selects a
different palette entry; the stage is a time delay, not a wait for the changed
index to be used.

The rule applies wherever the changed palette entry can contribute to the
implemented AGA output path:

- ordinary indexed colour uses the preceding 24-bit palette value;
- HAM8 direct-colour selection uses the preceding 24-bit value;
- EHB and HAM6 direct-colour selection use the preceding 24-bit value;
- a winning sprite uses the preceding direct-palette value after the hidden
  playfield sample has advanced any HAM hold state; and
- a later `COLORxx` write before the next sample makes the earlier new register
  value the preceding output value and supersedes the earlier pending stage.

A write sampled while `BPLCON2.RDRAM` is set changes neither the palette nor
the pending output stage. AGA EHB and HAM6 remain Lisa-owned 24-bit modes;
`BPLCON3.LOCT` precision is retained and must not be interpreted as
`KILLEHB`. AGA `KILLEHB` is read from BPLCON2.

The delayed value is pixel-pipeline state, not register state. It must not be
implemented by postponing the palette write, moving the Copper event, or
shifting the framebuffer.

## Persistence and inspection

A pending colour write can affect the next output sample after a save-state
boundary. Runtime snapshots therefore serialize the palette index and its
preceding 24-bit value, compatibility 12-bit value and transparency/genlock
flag. The complete 256-entry transparency/genlock table is also machine state.
Snapshot schema version 31 rejects version 30 because the older positional
payload cannot preserve this state.

The same pending stage is available through the canonical
`denise.delayed_color_write` query and the AGA-compatible
`aga.delayed_color_write` query. Inspection reports no pending value after the
consuming output sample. The complete transparency/genlock table is available
through `denise.palette_genlock` and `aga.palette_genlock`.

## Evidence boundary

The current evidence is exact for the registered A1200 AGA PAL Test Kit
patterns and agrees with one UAE-family software implementation. It is not a
physical-hardware measurement, a second-family consensus, or general proof of
all AGA palette, HAM, border and blanking behaviour.

Stronger hardware evidence may refine the rule for combinations not exercised
by the gate. It must not be represented as disagreement between independent
FS-UAE and WinUAE implementations because they share implementation ancestry.

## Verification

Focused Lisa tests establish that:

- the previous indexed colour appears for one hires sample and the new colour
  appears on the following sample;
- an output sample selecting another index still consumes the delay;
- output outside retained framebuffer storage consumes the delay;
- EHB and HAM6 retain the preceding RGB24 value, including LOCT precision;
- HAM8 direct colour uses the preceding RGB24 value;
- a winning sprite bypasses HAM and EHB decoding while the hidden playfield
  stream still advances;
- RDRAM reads return the selected bank and LOCT half, including the high-half
  transparency bit;
- an RDRAM-protected write leaves palette, transparency and delay state
  unchanged; and
- consecutive writes retain one well-defined pending stage.

Query and snapshot tests preserve and expose that stage. In the A1200 AGA PAL
Test Kit lane, the EBU colour boundaries are exact after this decision. The
other registered patterns additionally exercise bitplane, display-window and
sprite timing; their current assertion status is recorded by the conformance
process rather than attributed to this colour stage.

## Drift triggers

Reject these patterns:

- delaying the register mirror instead of the pixel result;
- retaining the old colour until that palette index is selected;
- applying the delay only to ordinary indexed output;
- feeding a winning sprite index through HAM or EHB decoding;
- consuming the pending stage once per vertically duplicated host row;
- dropping the pending stage during save or restore; or
- presenting UAE-family agreement as physical-hardware proof.

## Related Documents

- [Advance the Denise pipeline across the full projected raster](amiga-denise-full-raster-pipeline.md)
- [Separate Copper colour writes from post-output writes](amiga-denise-color-output-phase.md)
- [Lisa bitplane and display-window output phase](amiga-lisa-bitplane-diw-output-phase.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Amiga programmable-HBLANK conformance](../processes/amiga-programmable-hblank-conformance.md)
- [Save-state: serde the live machine](savestate-live-machine-serde.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)

## Six-plane indexed colour selection

The [Lisa EHB synthesis](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#36-ehb-and-ham8-on-lisa)
records BPLCON2.KILLEHB at bit 9. When set, six-plane single-playfield
output uses the complete post-BPLAM palette address, including independent
entries 32..63 and XOR-selected higher entries. The ECS five-bit palette
fold is not applicable to Lisa. The existing delayed palette view feeds
this lookup; changing mode does not bypass pending COLOR output state.

Regression coverage distinguishes COLOR01 from COLOR33, exercises every
post-XOR address, retains delayed writes to upper banks, and serializes
six real bitplanes through the colour compositor with four BPLAM values.
The reference agreement establishes address selection, not physical
calibration of a mid-line KILLEHB transition.

## Dual-playfield colour selection

The [Lisa playfield synthesis](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#38-playfield-logic-on-lisa-aga-extensions)
records BPLCON3.PF2OF (bits 12..10) as offsets 0, 2, 4, 8, 16, 32,
64 and 128. WinUAE `drawing.cpp::dblpfofs`/`decode_pixel_aga` and Minimig
`rtl/denise_playfields.v` independently implement this table. Lisa resets
BPLCON3 to $0C00, selecting the compatible offset 8; promoting a live ECS
instance preserves its existing BPLCON3 value.

Lisa supplies the decoded offset to the shared core at each raster output
call. The core first selects the nontransparent playfield using raw plane
codes and PF2PRI, adds the offset only if PF2 wins, and applies BPLAM XOR
last. Sprite priority and collisions continue to use raw playfield identity
and plane bits. OCS/ECS output entry points always supply offset 8.
No additional register mirror or serialized field is needed; saved states
already hold the BPLCON3 value in the ECS wrapper.

Regression tests cover all offsets, all 16 PF2 codes, transparent and deep
PF1 codes, both priorities, and two XOR masks (1,536 lores combinations),
plus every offset at hires/superhires and the fixed OCS/ECS paths. A live
serial stream resumes with the same offset across snapshot serialization.
These checks establish steady-state composition and state preservation;
the physical propagation phase of a mid-line PF2OF write is not calibrated.

## Border-sprite eligibility

The [Lisa border synthesis](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#310-border-handling-on-lisa)
records BPLCON3.BRDRSPRT at bit 1, gated by BPLCON0.ECSENA. Lisa supplies
that selector as a composition input to the shared sprite pipeline. It
bypasses both the display-window gate and the BPL1DAT enable: Minimig's
sprite input is `display_ena | brdsprt`; WinUAE clears both `sprites_hidden2`
gates for border-sprite mode. OCS/ECS continue to supply false.

The underlying playfield remains colour zero outside DIW. Border sprites
use their normal banked colour and retained serial output code; changing
eligibility does not restart the shifter. Collision matching sees zero
playfield bits in the border and the eligible sprite groups. Physical
HBLANK remains the board's downstream output mask.

This establishes BRDRSPRT eligibility and palette selection. It does not
calibrate the propagation phase of a mid-line selector write. The downstream
BRDRBLNK mask below also applies to border-sprite output.

## Border black output

The [Lisa border synthesis](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#310-border-handling-on-lisa)
records BRDRBLNK at BPLCON3 bit 5, enabled by BPLCON0.ECSENA. The original
Lisa specification defines a final blank-black selection. Minimig masks RGB
when outside DIW or before BPL1DAT enables the display. WinUAE's ordinary
output agrees; Ultra overscan exposes blanked pixels as a host option.

Lisa supplies this mask to the board, which resolves each colour sample
normally before storing black in the framebuffer. Hidden palette delays,
HAM state, sprite advancement and collision latches continue to advance.
Border sprites are masked too. The existing register and BPL1DAT latch
suffice; snapshot format remains v39.

Production-framebuffer regressions cover every ECSENA/BRDRBLNK/BPL1DAT/DIW
combination and compare masked sprite/colour state with an unmasked control.
This establishes steady-state output. The physical propagation phase of
selector writes and display-window edges remains uncalibrated.

## Counter-domain verification (2026-10-06)

The independent adjacent-COLOR00 control confirms the one-hires (two 35 ns
samples) palette delay. The separately modelled Copper early queue was one
lores tick too late and is corrected in the Copper phase decision. The earlier
raw-image mapping omitted producer line padding; it is superseded by the
counter-domain origin. The palette delay itself is retained.
