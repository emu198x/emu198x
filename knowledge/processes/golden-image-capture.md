# Capturing boot-path golden frames

This process answers how the Amiga boot-path regression images are captured,
compared, and interpreted. The ZX80 and ZX81 follow the same rule against a
different external emulator; see *The ZX8x, against MAME* at the end.

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

## Reviewed A1200 Workbench 3.1 rebaseline

The 2026-08-08 Lisa bitplane and horizontal display-window phase correction
moved the A1200 Workbench 3.1 playfield by two host-HIRES samples while leaving
the independently clocked sprite pointer on its absolute coordinate path. The
existing golden and the first corrected capture differed at 14,550 of 430,144
pixels.

Translating the corrected capture left by two samples aligned every changed
playfield pixel except 184 pixels in a 22 x 22 rectangle at the pointer. This
is the expected separation between the corrected bitplane path and the
unchanged sprite path. It also agrees with the independently registered A1200
Test Kit result: the non-pointer EBU, dots and crosshatch cases are exact under
the beam-absolute mapping, while the remaining pointer phase is retained as a
separate disagreement.

The complete corrected Workbench image was reviewed and retained as the new
Emu198x regression baseline. It is not an external Workbench capture, and no
pointer mask was added; subsequent runs must reproduce every pixel exactly.

## Volatile Workbench memory readouts

Workbench computes the free-memory figure shown in the title bar from live
allocator state. Captures taken at nearby instruction boundaries can therefore
differ in 16-byte allocation quanta without representing a display failure.
One reviewed Workbench 1.2 comparison moved by four such quanta.

The matrix excludes only the numeric glyph field when this occurs. Its reviewed
rectangles in the cropped 752 x 572 comparison space are:

- Workbench 1.3: x 270, y 36, width 50, height 18;
- Workbench 1.2: x 319, y 36, width 60, height 18.

The pointer, title-bar chrome, surrounding text and every other framebuffer
pixel remain exact. Do not enlarge either rectangle to accommodate an unrelated
difference.

## Result interpretation

A passing boot-path matrix establishes that current output matches the
registered regression baselines for every row that ran. It does not establish
that skipped rows passed, that every baseline has independent provenance, or
that unexercised chipset modes are pixel accurate.

## Related documents

- [Amiga Test Kit v1.21 video conformance](amiga-test-kit-video-conformance.md)
- [Lisa bitplane and display-window output phase](../decisions/amiga-lisa-bitplane-diw-output-phase.md)
- [Amiga Test Kit v1.12 verification](amiga-test-kit-verification.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)

## The ZX8x, against MAME

The Amiga compares against FS-UAE. The ZX80 and ZX81 compare against **MAME
0.289**, for the same reason and under the same caveat: it is an independent
implementation, not a machine.

Capture with `tools/zx8x-mame-capture/capture.sh`, which repackages the ROMs
already staged for Emu198x's own tests into the zips MAME expects. Nothing is
downloaded. One detail decides whether the comparison means anything: **our
ZX81 ROM is MAME's `zx81a`, the 2nd revision, and MAME's default is the 3rd.**
Comparing against the default compares two different ROMs. The script passes
`-bios 2nd`.

### Aligning two rasters rather than cropping one

Unlike the Amiga, the two rasters cannot be made to match by trimming edges.
MAME renders the whole 384x311 field, closing with six blank lines of vertical
sync. We render the 320x288 window a set shows, opening `FIRST_VISIBLE_LINE`
into the field. So the comparison is over the 256x192 text area both contain,
with MAME's row *n* mapping to our row *n* - 8.

That 8 is the point. It is `FIRST_VISIBLE_LINE`, derived in #1116 from the
ROM's own pad, and until this capture nothing had checked it against anything.
Both machines' text areas come out **pixel-identical**.

### What it does not settle

Horizontal placement. MAME puts the ZX80's picture 26 pixels right of the
ZX81's; we place both in the same column, because `FIRST_CHAR_TSTATE` is fitted
per machine to a window already chosen. The tests assert the vertical agreement
and deliberately do not assert the horizontal. See #1123.
