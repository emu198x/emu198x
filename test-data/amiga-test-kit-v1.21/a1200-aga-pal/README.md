# A1200 AGA PAL Test Kit v1.21 FS-UAE reference

This directory answers which independently produced FS-UAE frames form the
registered A1200 AGA PAL Amiga Test Kit v1.21 video reference.

## Scope

The reference covers six Test Kit video paths on an unexpanded PAL A1200. It
is consumed by a separate explicit A1200 video-conformance lane. It does not
extend the A500+A501 OCS result to AGA and must not be updated from Emu198x
output.

The frames were produced by FS-UAE 5.0.7 at revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`. FS-UAE identifies its underlying
core as derived from WinUAE 6.0.1, so this reference belongs to the UAE
implementation family.

| Property | Registered value |
|---|---|
| machine | Commodore Amiga A1200 |
| processor | 68EC020 |
| chipset and region | AGA PAL |
| chip RAM | 2,097,152 bytes |
| expansion RAM | none |
| firmware | Kickstart 3.1 revision 40.068 |
| firmware image size | 524,288 bytes |
| Test Kit | version 1.21 |

[`manifest.json`](manifest.json) is the machine-readable authority for these
values and all content hashes.

## Capture sequence

Every reference case starts from a fresh boot. The producer runs 600 PAL
fields before pressing the first navigation key. It holds each key for three
fields, runs one field after release, and waits a further 50 fields between
navigation keys. The selected screen then settles for 150 fields for
`gradients` or 100 fields for every other case.

Three adjacent complete chipset framebuffers are captured. Static cases must
be byte-identical. The alternating checkerboard must have an A-B-A
relationship. Its A and B names record capture order only; a consumer compares
the two phases as an unordered pair.

The retained capture adapter is
[`tools/fs-uae-test-kit-video-capture/`](../../../tools/fs-uae-test-kit-video-capture/README.md).
It pins the producer source, patch, binary, configuration, external input
hashes, key events, field labels, and raw geometry.

## Viewport normalisation

The capture hook copies FS-UAE's completed 756 × 576 BGRA8888 chipset buffer
before frontend processing. The canonical reference is derived without
searching for an alignment:

- crop the producer buffer at `(2, 0)` to 752 × 572 pixels;
- require both rows in every vertically doubled pair to be identical;
- retain the first row from each checked pair;
- convert BGRA channel order to packed RGB; and
- write the resulting 752 × 286 RGB8 image without scaling, filtering, colour
  correction, palette conversion, or tolerance.

The corresponding Emu198x crop is `(8, 2)` in its 768 × 576 runtime
framebuffer. The fixed mapping is established by the registered bitplane-only
checkerboard, dots, and crosshatch images, which share pixel positions without
an alignment search. Beam-raced `COLORxx` screens are not used to choose the
crop because Lisa applies a separate one-hires-pixel colour-output delay.

All retained producer pixels have opaque alpha. Alpha is validated and then
discarded; it is not part of the RGB comparison. The AGA reference preserves
all eight captured bits per channel. It does not use the OCS reference's RGB4
presentation normalisation.

## Evidence status

This family is independent of Emu198x because its pixels were produced by
FS-UAE. It remains one software implementation family. It does not establish
physical-hardware output, analogue-video behaviour, an independent WinUAE
vote, or accuracy outside the registered A1200 configuration and patterns.

The A500+A501 vAmiga family exercises a different machine and is not a second
vote for this A1200 result.

## Expected contents

- `manifest.json`: provenance, geometry, navigation, timing, and checksums;
- `package.py`: strict raw-run packaging and committed-reference verification;
- one RGB8 PNG for each static pattern and two for the alternating pattern;
- this README.

The Test Kit ADF, Kickstart image, patched FS-UAE binary, raw capture runs, and
diagnostic output are not included.

## Related documents

- [Reference collection](../README.md)
- [Amiga Test Kit v1.21 fixture identity](../../amiga-test-kit-v1.21.md)
- [Amiga Test Kit v1.21 video conformance](../../../knowledge/processes/amiga-test-kit-video-conformance.md)
- [FS-UAE capture adapter](../../../tools/fs-uae-test-kit-video-capture/README.md)
