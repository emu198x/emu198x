# Capturing Amiga boot-path golden frames

This process answers how the Amiga boot-path regression images are captured,
compared, and interpreted.

The boot-path goldens preserve known framebuffer output for selected Kickstart
and Workbench waypoints. They are regression baselines. A golden is an
independent accuracy reference only when its provenance records an external
capture; an image produced by Emu198x remains an Emu198x regression baseline.

## Storage and consumer

Committed images live in:

```text
crates/runtime-commodore-amiga/tests/goldens/
```

They are consumed by
[`golden_matrix.rs`](../../crates/runtime-commodore-amiga/tests/golden_matrix.rs).
The matrix selects the machine model, firmware, optional disk image, boot flow,
and settle-frame count for each row.

Firmware and media remain external. A row whose required local asset is absent
reports a skip during an ordinary test run. The profile-specific Test Kit video
gates have different failure semantics and are documented separately.

## Comparison geometry

The Amiga runtime renders a 768 × 576 PAL Standard framebuffer. The golden
matrix compares the centred 752 × 572 region used by the registered FS-UAE
captures:

- eight pixels are removed from each horizontal edge;
- two scanlines are removed from each vertical edge;
- the remaining pixels are encoded as eight-bit RGB;
- no scaling, interpolation, scanline effect, shader, or colour filter is
  applied by the comparison.

Reference PNGs must therefore decode to exactly 752 × 572 pixels. A dimension
mismatch fails rather than being scaled or cropped implicitly.

This mutable boot-golden geometry is not the A1200 Test Kit reference format.
That strict gate validates doubled row pairs and retains one row from each,
producing an immutable 752 × 286 comparison image from its registered FS-UAE
capture. Its references cannot be updated through the boot-golden mechanism.

## External FS-UAE capture

Use an unfiltered one-times output with nearest-neighbour presentation:

```text
video_resolution = 1x
video_format = rgba
video_aspect_ratio = auto
zoom = full
filter = nearest
scanlines = 0
shader = none
```

Record the FS-UAE version, complete machine configuration, firmware and media
checksums, capture waypoint, source viewport, and crop applied to the source.
The checked-in 752 × 572 image must be derived from that declared capture
without resampling.

The current Kickstart 1.2 and 1.3 insert-disk rows settle at frame 250. Other
rows carry their own settle counts in the matrix and must be captured at the
matching guest waypoint. A visually similar screen reached at an unspecified
time is not an interchangeable reference.

An evidence-backed timing correction can move the frame at which a stable
guest waypoint is reached. In that case, retain the existing golden when the
later capture is pixel-exact, record the timing cause, and move only the row's
settle count. A changed frame number alone does not justify replacing the
image.

## Validating a golden

Run the matrix from the repository root:

```sh
cargo test -p runtime-commodore-amiga --test golden_matrix -- --nocapture
```

When a comparison differs, the harness writes these ignored diagnostics beside
the committed image:

- `<name>.actual.png`, containing the cropped Emu198x frame;
- `<name>.diff.png`, containing a red mask for mismatched pixels.

The failure reports the number and percentage of differing pixels. Inspect the
actual frame and diff before changing either the emulator or the baseline.

## Updating a regression baseline

`EMU198X_UPDATE_GOLDENS=1` rewrites boot-path goldens from current Emu198x
output. It is a maintenance mechanism, not an independent-reference capture
path.

Before retaining an updated image:

1. establish why the rendered output changed;
2. compare the affected region with an external implementation or primary
   evidence;
3. record whether the resulting image is externally captured or
   Emu198x-produced;
4. review the complete image rather than only the expected changed region.

Do not use this update mechanism for the Test Kit v1.21 conformance references.
That lane admits references only through its provenance contract.

## Result interpretation

A passing boot-path matrix establishes that current output matches the
registered regression baselines for every row that ran. It does not establish
that skipped rows passed, that every baseline has independent provenance, or
that unexercised chipset modes are pixel accurate.

## Related documents

- [Amiga Test Kit v1.21 video conformance](amiga-test-kit-video-conformance.md)
- [Amiga Test Kit v1.12 verification](amiga-test-kit-verification.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)
