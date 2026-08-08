# A500+A501 OCS PAL Test Kit v1.21 vAmiga reference

This directory answers which independently produced vAmiga frames form the
first A500+A501 OCS PAL Amiga Test Kit v1.21 video reference.

## Scope

The reference covers six Test Kit video paths on an A500 OCS PAL
configuration with an A501 expansion. It is consumed by the explicit Test Kit
video-conformance lane. It is separate from the boot-path golden matrix and
must not be updated from Emu198x output.

The frames were produced by vAmiga 4.4b12 at revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`, using its
`A500_OCS_1MB` regression configuration:

| Property | Registered value |
|---|---|
| machine | Commodore Amiga A500 with A501 |
| processor | MC68000 |
| chipset and region | OCS PAL |
| chip RAM | 524,288 bytes |
| slow RAM | 524,288 bytes |
| firmware | Kickstart 1.3 revision 34.005 |
| Test Kit | version 1.21 |

[`manifest.json`](manifest.json) is the machine-readable authority for these
values and all content hashes.

## Capture sequence

Each reference capture starts from a fresh boot. vAmiga runs the machine for
12 simulated seconds before entering Test Kit's menus. Its keyboard command
releases each key automatically after 500 milliseconds, and consecutive
navigation keys are separated by one simulated second.

| Frame | Test Kit navigation | Producer final wait |
|---|---|---:|
| `gradients` | main F6, video F1 | 3 seconds |
| `static-checkerboard` | main F6, video F2 | 2 seconds |
| `alternating-checkerboard`, phase A | main F6, video F3 | 2 seconds |
| `alternating-checkerboard`, phase B | main F6, video F3 | 3 seconds |
| `ebu-bars` | main F6, video F4, full-field F6 | 2 seconds |
| `dots` | main F6, video F5 | 2 seconds |
| `crosshatch` | main F6, video F6 | 2 seconds |

The two alternating-checkerboard phases are separate vAmiga captures. Neither
was derived from the other. Their A and B labels do not prescribe which phase
Emu198x must emit first.

The executable Emu198x procedure uses field counts rather than the producer's
simulated-second commands. It boots for 600 PAL fields, holds each key for
three fields, runs one field after releasing it, waits a further 50 fields
between navigation keys, and then settles for 150 fields for `gradients` or
100 fields for every other case. The one-field release interval also precedes
the final settle. The manifest records the producer timing and executable
timing separately.

## Viewport normalisation

vAmiga's regression capture emits packed, row-major RGB bytes at 716 × 285
pixels. Its registered source viewport is texture coordinates
`[196, 912) × [26, 311)`, corresponding to beam positions
`[$31, $E4) × [26, 311)`. The corresponding Emu198x comparison geometry is
derived without searching for an alignment:

- start at `(20, 2)` in the 768 × 576 runtime framebuffer;
- retain 716 × 570 pixels;
- require each pair of vertically doubled runtime rows to be identical;
- retain one row from each validated pair;
- compare the resulting 716 × 285 RGB8 image.

The horizontal offset maps vAmiga horizontal positions `$31` through `$E3`
onto the Emu198x runtime origin at `$2C`. The vertical offset maps vAmiga's
first captured line, line 26, onto the doubled runtime framebuffer whose first
line is line 25.

The PNGs are eight-bit true-colour images without alpha. Conversion from the
raw capture applies no scaling, interpolation, palette optimisation, colour
correction, or filtering. Each manifest `rgb_sha256` is the checksum of the
decoded packed RGB bytes and therefore also the checksum of its source raw
capture.

OCS supplies four bits per colour channel. vAmiga places those nibbles in the
high four bits of each stored byte, while Emu198x replicates each nibble across
both halves of the byte. The harness reduces both representations to the
underlying four-bit channel before comparison: vAmiga channels use a step of
16, Emu198x channels use a step of 17, and both use nearest-value rounding.
The registered vAmiga conversion may land one byte below its step; the
Emu198x channel must land exactly on its declared step. Any larger deviation
is an encoding failure rather than a tolerated colour difference. This
removes an eight-bit presentation convention without relaxing any OCS colour
bit, pixel position, or timing comparison.

## Evidence status

This family is independent of Emu198x because its pixels were produced by
vAmiga. All six frames nevertheless share one producer and one implementation
family. They do not establish cross-emulator consensus, physical-hardware
output, analogue-video behaviour, or accuracy outside the registered
configuration and patterns.

The executable comparison is deliberately mixed. Static checkerboard, both
alternating-checkerboard phases, dots, and crosshatch agree exactly after RGB4
normalisation. Gradients and EBU bars retain the exact
`denise-ocs-color-output-phase` disagreement signature. The latter is a pinned
implementation-family disagreement, not an accepted pixel tolerance or a
claim of vAmiga conformance.

[`assertions.json`](assertions.json) binds these classifications and every
observed signature to the exact producer-manifest bytes. Unexpected agreement
or any change to a registered disagreement fails the gate pending review.

A second independently configured implementation family must agree after the
same declared normalisation before these frames can represent implementation
consensus.

## Expected contents

- `manifest.json`: provenance, geometry, navigation, and checksums;
- `assertions.json`: exact and registered-disagreement comparison contracts;
- one strict RGB8 PNG for each static pattern and two for the alternating
  pattern;
- this README.

## Related documents

- [Amiga Test Kit v1.21 fixture identity](../../amiga-test-kit-v1.21.md)
- [Amiga Test Kit v1.21 video conformance](../../../knowledge/processes/amiga-test-kit-video-conformance.md)
- [Accuracy corpora](../../accuracy-corpora.md)
