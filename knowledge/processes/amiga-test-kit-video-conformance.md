# Verifying Amiga video with Amiga Test Kit v1.21

This process answers how the Amiga Test Kit v1.21 video-conformance lanes are
run and how their results may be interpreted.

The video lanes are separate from the Test Kit v1.12 identity and replay gate.
Version 1.12 verifies guest machine detection, input progress, and deterministic
replay. Version 1.21 supplies stable video patterns for comparing Emu198x with
a registered implementation independent of Emu198x. Two profile-specific lanes
share the ADF but retain separate machines, firmware, reference families,
normalisation rules, wrappers, and claims.

## Required inputs

Both lanes require:

- the registered Amiga Test Kit v1.21 ADF, supplied directly or inside a ZIP
  through `EMU198X_AMIGA_TEST_KIT_V121_ADF`;
- the selected profile's registered Kickstart image;
- that profile's reference manifest and independently produced images;
- that profile's strict `assertions.json` comparison contract; and
- a release build of the `runtime-commodore-amiga` integration test.

| Profile | Firmware variable | Input manifest | Reference family |
|---|---|---|---|
| A500+A501 OCS PAL | `EMU198X_AMIGA_KICKSTART_13_ROM` | [`amiga-test-kit-v1.21.sha256`](../../test-data/amiga-test-kit-v1.21.sha256) | vAmiga 4.4b12 |
| A1200 AGA PAL | `EMU198X_AMIGA_KICKSTART_31_A1200_ROM` | [`amiga-test-kit-v1.21-a1200-aga-pal.sha256`](../../test-data/amiga-test-kit-v1.21-a1200-aga-pal.sha256) | FS-UAE 5.0.7 |

Each manifest pins the normalised ADF and profile-specific ROM bytes. The
reference manifest separately pins the producer, machine configuration,
capture geometry, synchronisation rule, and image identity. Delivery archive
names do not replace payload checksums. Either wrapper may resolve its ROM from
`EMU198X_AMIGA_ROM_DIR` when the direct variable is absent.

An explicitly invoked lane is strict. A missing file, ambiguous ZIP, checksum
mismatch, invalid provenance record, missing reference, or unexpected image
geometry is a failure rather than a skip.

## Invocation

Run one profile lane from the repository root.

For A500+A501 OCS PAL:

```sh
EMU198X_AMIGA_TEST_KIT_V121_ADF=/path/to/amiga-test-kit-v1.21.adf \
EMU198X_AMIGA_KICKSTART_13_ROM=/path/to/kick13.rom \
scripts/verify-amiga-test-kit-video.sh
```

For A1200 AGA PAL:

```sh
EMU198X_AMIGA_TEST_KIT_V121_ADF=/path/to/amiga-test-kit-v1.21.adf \
EMU198X_AMIGA_KICKSTART_31_A1200_ROM=/path/to/kick31a1200.rom \
scripts/verify-amiga-test-kit-video-a1200.sh
```

The ADF variable may instead name a ZIP containing the registered image. A
direct ROM variable may be omitted when `kick13.rom` or `kick31a1200.rom`, as
appropriate, is available through the normal Amiga ROM-directory resolution.

Each wrapper verifies the normalised inputs before running only its ignored
integration test in release mode with one test thread. Direct invocation of a
test remains strict and does not acquire ordinary skip-if-missing behaviour.

## Registered profiles

The A500 profile uses an A500 with an A501 expansion, OCS PAL chipset, MC68000,
512 KiB chip RAM, 512 KiB slow RAM, and Kickstart 1.3 revision 34.005.

The A1200 profile uses an unexpanded PAL A1200, AGA chipset, 68EC020, 2 MiB
chip RAM, and Kickstart 3.1 revision 40.068.

CPU, RAM, chipset, region, firmware, and Test Kit identities are part of each
reference record rather than assumptions inferred from an image filename.

ECS, NTSC, accelerated, and other expanded profiles require separate reference
records. Neither registered result may be generalised to the other profile or
to an unregistered configuration.

## Video patterns

The lane boots to the Test Kit main menu and enters the video menu with F6. The
registered cases cover:

- F1: RGB gradients and PAL display extents;
- F2: a static per-pixel checkerboard;
- F3: an alternating per-pixel checkerboard;
- F4, then F6: EBU 100/0/100/0 colour bars;
- F5: dots;
- F6: crosshatch.

Each navigation step uses the emulated keyboard. Static cases must remain
unchanged across consecutive settled fields. The alternating checkerboard must
produce two distinct adjacent phases and repeat those phases in order. This
prevents an arbitrary field from being treated as the sole reference for a
two-phase pattern. Both valid phases have independently produced reference
captures; the harness compares them as an unordered pair because the producer
and Emu198x need not assign the same phase to the first captured field.

## Reference provenance

A reference is admissible only when its manifest records:

- reference emulator, version, revision, and implementation family;
- machine model, processor, RAM, chipset, region, and firmware configuration;
- Test Kit ADF checksum;
- menu path;
- the producer and executable gate timing schedules, recorded separately or as
  one shared field-count schedule as appropriate to the profile;
- source viewport, comparison crop, pixel encoding, and any normalisation;
- reference PNG checksum and decoded-pixel checksum, plus raw-capture checksums
  when raw pixels are the capture authority.

The A500 manifest records one vAmiga capture family. The A1200 manifest records
one FS-UAE capture family whose core is derived from WinUAE. Both are
independent of Emu198x, but they exercise different machine configurations and
therefore do not form cross-implementation consensus. FS-UAE and WinUAE are one
UAE implementation family and must not be counted as independent votes. A
consensus result for either profile requires another independent implementation
or physical hardware after the declared normalisation.

An Emu198x-produced frame may be retained as diagnostic output or a regression
baseline, but it cannot be registered as an independent source and the
conformance test provides no golden-update mode.

Each `assertions.json` is a separate executable claim about the comparison. It
is byte-bound to its producer manifest and covers every registered case and
phase exactly once. It does not turn Emu198x output into reference evidence.

## Pixel comparison

The comparison operates on unscaled digital pixels. The reference manifest
declares the exact source viewport and crop; the harness applies that geometry
without searching for an alignment. Alpha and PNG encoding metadata are not
part of the comparison after the image has been decoded.

The Emu198x framebuffer contains vertically doubled rows. The harness verifies
that both runtime rows in every canonical scanline are identical before
retaining one. It does not silently discard an unchecked row.

For A500 OCS output, the canonical image is 716 × 285 pixels. OCS exposes four
bits per colour channel. The registered vAmiga capture stores each nibble in
the high half of an eight-bit channel, while Emu198x replicates the nibble
across both halves. The harness reduces both encodings to the underlying
four-bit channel value before comparison. This normalises a framebuffer
representation choice; it does not introduce colour tolerance. The pinned
vAmiga conversion may be one byte below its 16-value step. Emu198x must emit an
exact 17-value step. A channel outside those declared encoding bounds fails
before pixel comparison.

For A1200 AGA output, the canonical image is 752 × 286 RGB8 pixels. The fixed
Emu198x crop begins at `(10, 2)` in the 768 × 576 framebuffer. Manifest schema
2 records the beam-absolute transform `Emu x = FS-UAE raw x + 8`: FS-UAE raw
`x=0` represents horizontal-blank coarse coordinate 46, while Emu198x `x=0`
represents CCK 44. The earlier `(8, 2)` crop was derived from bitplane content
and hid a two-host-sample bitplane-phase error. The correction changes the
consumer crop and bitplane timing, not the registered producer pixels. The
comparison retains all eight bits per channel and permits no channel
tolerance or alignment search.

Each profile contains six cases and seven reference images because the
alternating checkerboard retains two phases. There is no percentage threshold.
The contract classifies each case and phase as either:

- `exact`, which requires every normalised channel byte to equal the producer
  reference; or
- `registered-disagreement`, which requires a non-zero difference and an exact
  match for the complete normalised Emu198x frame hash, one-byte-per-pixel
  difference-mask hash, differing-pixel count, first differing pixel, and
  bounding box.

Unexpected agreement fails a registered-disagreement assertion. A changed
disagreement also fails even when it has fewer differing pixels. This prevents
an unresolved comparator question from becoming an open-ended tolerance.

The current contract is:

| Profile | Exact cases | Registered disagreement |
|---|---|---|
| A500+A501 OCS PAL | static checkerboard, both alternating-checkerboard phases, dots, crosshatch | gradients and EBU bars: `denise-ocs-color-output-phase` |
| A1200 AGA PAL | EBU bars, dots, crosshatch | gradients, static checkerboard and both alternating-checkerboard phases: pointer-only `aga-sprite-horizontal-output-phase` |

On a changed pixel or temporal result the lane records:

- the relevant Emu198x frame or phase sequence;
- a pixel-difference mask for each compared pair;
- the comparison outcomes and differing-pixel counts;
- the first differing coordinate where a compared pair differs;
- the case and reference identities.

Diagnostics are written below the selected profile directory:

- `target/accuracy/amiga-test-kit-v1.21/a500-a501-ocs-pal/`;
- `target/accuracy/amiga-test-kit-v1.21/a1200-aga-pal/`.

They are evidence for investigation and are never promoted automatically to
expected images.

## Result interpretation

A passing lane establishes that every exact case still agrees with its
registered producer and every unresolved case still has precisely its reviewed
disagreement signature. It does not mean that all six cases agree with the
producer. The closure runner additionally requires all six ordered outcome
markers for each profile, so a zero exit status without the declared case set
cannot satisfy the revision-wide closure.

Each result includes only its pinned machine, firmware, media, navigation,
registered phase pair and alternation, crop, producer-manifest bytes, and
assertion contract. The A500 gradients and EBU bars remain unresolved against
vAmiga. The A1200 pointer phase remains unresolved against FS-UAE in three
patterns. Those questions require stronger independent or physical evidence;
they are not relaxed comparisons.

It does not establish:

- accuracy for another Amiga model, chipset, region, or expansion;
- behaviour of modes not exercised by the registered patterns;
- analogue RGB or composite signal accuracy;
- monitor geometry, colour calibration, phosphor response, or filtering;
- Paula audio accuracy;
- general software compatibility.

## Related documents

- [Amiga Test Kit v1.21 fixture identity](../../test-data/amiga-test-kit-v1.21.md)
- [Amiga Test Kit v1.12 verification](amiga-test-kit-verification.md)
- [Amiga boot-path golden capture](golden-image-capture.md)
- [Lisa colour-output delay](../decisions/amiga-lisa-color-output-delay.md)
- [Lisa bitplane and display-window output phase](../decisions/amiga-lisa-bitplane-diw-output-phase.md)
- [Denise colour-output phase](../decisions/amiga-denise-color-output-phase.md)
- [Sprite horizontal-output phase](../decisions/amiga-sprite-horizontal-output-phase.md)
- [Denise full-raster pipeline](../decisions/amiga-denise-full-raster-pipeline.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)
