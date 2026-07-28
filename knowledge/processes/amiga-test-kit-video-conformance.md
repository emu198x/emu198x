# Verifying Amiga video with Amiga Test Kit v1.21

This process answers how the Amiga Test Kit v1.21 video-conformance lane is run
and how its result may be interpreted.

The lane is separate from the Test Kit v1.12 identity and replay gate. Version
1.12 verifies guest machine detection, input progress, and deterministic replay.
Version 1.21 supplies stable video patterns for comparing Emu198x with a
registered independent implementation.

## Required inputs

The lane requires:

- the registered Amiga Test Kit v1.21 ADF, supplied directly or inside a ZIP
  through `EMU198X_AMIGA_TEST_KIT_V121_ADF`;
- Kickstart 1.3 revision 34.005, supplied through
  `EMU198X_AMIGA_KICKSTART_13_ROM` or resolved from
  `EMU198X_AMIGA_ROM_DIR`;
- the registered reference manifest and its independently sourced images;
- a release build of the `runtime-commodore-amiga` integration test.

[`test-data/amiga-test-kit-v1.21.sha256`](../../test-data/amiga-test-kit-v1.21.sha256)
pins the normalised ADF and ROM bytes. The reference manifest separately pins
the reference producer, machine configuration, capture geometry,
synchronisation rule, and image identity. Delivery archive names do not replace
payload checksums.

An explicitly invoked lane is strict. A missing file, ambiguous ZIP, checksum
mismatch, invalid provenance record, missing reference, or unexpected image
geometry is a failure rather than a skip.

## Invocation

Run the complete lane from the repository root:

```sh
EMU198X_AMIGA_TEST_KIT_V121_ADF=/path/to/amiga-test-kit-v1.21.adf \
EMU198X_AMIGA_KICKSTART_13_ROM=/path/to/kick13.rom \
scripts/verify-amiga-test-kit-video.sh
```

The ADF variable may instead name a ZIP containing the registered image.
`EMU198X_AMIGA_KICKSTART_13_ROM` may be omitted when `kick13.rom` is available
through the normal Amiga ROM-directory resolution.

The wrapper verifies the normalised inputs before running the ignored
integration test in release mode with one test thread. Direct invocation of the
test remains strict and does not acquire ordinary skip-if-missing behaviour.

## Registered machine

The first conformance profile is an A500 with an A501 expansion, OCS PAL
chipset, MC68000, 512 KiB chip RAM, 512 KiB slow RAM, and Kickstart 1.3
revision 34.005. CPU, RAM, chipset, region, firmware, and Test Kit identities
are part of the reference record rather than assumptions inferred from an
image filename.

ECS, AGA, NTSC, accelerated, and expanded profiles require separate reference
records. A result from the A500 OCS PAL lane must not be generalised to those
configurations.

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
- producer boot, key-release, inter-key, and final-capture timing;
- the separate boot, key-hold, key-release, inter-key, and final-settle field
  counts used by the executable Emu198x procedure;
- source viewport, comparison crop, pixel encoding, and any normalisation;
- source PNG checksum and decoded-pixel checksum.

The first manifest records one vAmiga capture family. This is independent of
Emu198x, but it is not cross-implementation consensus and must not be described
as such. A later consensus manifest requires agreement with another
independent implementation or physical hardware after the declared
normalisation. FS-UAE shares UAE-family implementation ancestry with WinUAE and
does not provide a second independent family alongside WinUAE.

An Emu198x-produced frame may be retained as diagnostic output or a regression
baseline, but it cannot be registered as an independent source and the
conformance test provides no golden-update mode.

## Pixel comparison

The comparison operates on unscaled digital pixels. The reference manifest
declares the exact source viewport and crop; the harness applies that geometry
without searching for an alignment. Alpha and PNG encoding metadata are not
part of the comparison after the image has been decoded.

The Emu198x framebuffer contains vertically doubled rows. The harness verifies
that both runtime rows in every canonical scanline are identical before
retaining one. It does not silently discard an unchecked row.

OCS exposes four bits per colour channel. The registered vAmiga capture stores
each nibble in the high half of an eight-bit channel, while Emu198x replicates
the nibble across both halves. The harness reduces both encodings to the
underlying four-bit channel value before comparison. This normalises a
framebuffer representation choice; it does not introduce colour tolerance.
The pinned vAmiga conversion may be one byte below its 16-value step. Emu198x
must emit an exact 17-value step. A channel outside those declared encoding
bounds fails before pixel comparison.

Every pixel and every four-bit channel must match. There is no percentage
threshold for a passing case. On a pixel or temporal mismatch the lane
records:

- the relevant Emu198x frame or phase sequence;
- a pixel-difference mask for each compared pair;
- the comparison outcomes and differing-pixel counts;
- the first differing coordinate where a compared pair differs;
- the case and reference identities.

Diagnostics are written below
`target/accuracy/amiga-test-kit-v1.21/a500-a501-ocs-pal/`. They are evidence for
investigation and are never promoted automatically to expected images.

## Result interpretation

A passing lane establishes that Emu198x produced the registered vAmiga digital
pixel output for every executed A500+A501 OCS PAL Test Kit v1.21 case, using
the pinned machine, firmware, media, navigation, registered phase pair and
alternation, and crop.

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
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)
