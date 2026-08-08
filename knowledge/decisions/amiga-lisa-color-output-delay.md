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
