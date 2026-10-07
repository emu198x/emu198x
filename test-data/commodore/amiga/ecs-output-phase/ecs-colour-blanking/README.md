# ECS colour and programmed blanking

The latest [residual correction](residuals/README.md) closes the selector delay,
guest-label sampling race and obsolete static consumer. All ten timed and
fourteen static cases pass; snapshot 54's layout is unchanged. The retained
sections below record the earlier measurements and their evidence boundaries.

Two bounded corrections remove an extra ECS Copper colour tick and an extra
Lisa programmed-blank comparator tick. Neither changes framebuffer geometry
or snapshot 53's saved layout. The subsequently approved ECS blanking stages
are implemented in snapshot 54; `stage54/` retains their separate results.

The primary observation record is
[`2026-ecs-colour-blanking-observations.md`](../../../../../../../reference/by-system/commodore-amiga/2026-ecs-colour-blanking-observations.md).
The implementation, approved design and remaining work are in
[`2026-10-07-amiga-ecs-colour-blanking.md`](../../../../../../docs/plans/2026-10-07-amiga-ecs-colour-blanking.md).

The wider sweep completes at 125/128 exact guests (375/384 exact fields).
Only the three palette-XOR variants differ, by 128 RGB samples per field.
`regression/` retains every field result and before/after-verified input and
producer hashes. Its common origin comes from the shared source/counter
calibration; retained fields do not all carry fresh per-case origin traces.
`validation.json` records the completed checks and remaining failures.

## Evidence

- `ecs/color-moves/`: one lores tick late before correction; all three final
  common-raster fields exact. The failing component test is retained in logs.
- `aga/`: ten neutral blanking controls. Every horizontal interval agrees on
  all 200 active lines after correction. Six programmed controls still differ
  by 1,304 RGB samples on one top-of-field beam line; the four other controls
  are exact over the complete common raster. Full-field reports remain red
  where appropriate; the active-line count is an additional measurement.
- `ecs/`: seven blanking controls. Central and wrapping edges are seven lores
  periods early in all three fields. Five remaining controls are exact.
- `ocs/color-moves/`: colour edges agree, but the far-right fixed-blank margin
  differs at four superhires samples on 572 rows. This is an independent
  residual, unchanged by these fixes.
- `timed-baseline/` and `timed-final/`: original portable-corpus consumer
  failure and requalified results. All five AGA cases pass. ECS has three
  semantic failures and two rejected non-adjacent guest-field sequences.
- `xor-check.json`: fresh counter-traced captures reproduce two broad-sweep
  palette-XOR residuals at 128 pixels per field. Their raw fields are byte-
  identical to the retained earlier producer captures.

`identities.json` binds guest, native images, source, configuration and raw
reference hashes. `producers.json` records the final native executable and
both instrumented reference executables/source files. All reference captures
use the unchanged FS-UAE 5.0.7/WinUAE-derived producer documented by the parent
investigation. These are software observations, not physical measurements.

Each raw framebuffer and authored guest ADF is retained as `.gz`, compressed
with a zero timestamp. Decompression restores the original bytes and SHA-256;
it performs no image transform. Native PNGs are unfiltered. ECS/OCS doubled
PNGs repeat each 70 ns sample twice for comparison on the 35 ns grid.
No firmware is included.

## Reproduce

Rebuild the colour guest using the parent `build_color_control.py` and the
window builder's legacy guest. Then build the blanking guests:

```sh
python3.13 test-data/commodore/amiga/ecs-output-phase/tools/build_blank_control.py /tmp/color-moves /tmp/counter-blanking
```

All seventeen profile/guest outputs were rebuilt byte-identically. Capture
with the existing SPHX full-resolution adapter and counter-origin patch.
`compare_phase.py` requires three complete fields, matching guest identities,
600 active counter-origin rows and native 1536x576 pixels. The fixed mapping
is native (16,2) against raw (4,0), width 1508 and height 574. It does not search
for matching image content.

For archived data, decompress `probe.adf.gz`, put the decompressed BGRA and
JSON files under `reference/capture/`, and copy `reference.log` to
`reference/run.log`. The existing comparator's `compare(case, native_name,
tag="AGA_ORIGIN")` (or `ECS_ORIGIN`) can then replay the stored comparison.
Use `final-doubled.png` for the colour controls, `before-doubled.png` for ECS
blanking and `after.png` for AGA blanking. Known residuals return nonzero counts;
do not turn them into accepted-image baselines.

The old static consensus gate remains red on unrequalified native margin
coordinates. Its exact failure and the timed failures are retained in `logs/`.
The pending ECS stages, field-counter issue, vertical boundary and palette-XOR
residuals must not be described as fixed by the two completed corrections.

## Approved snapshot-54 correction

`stage54/` retains eight final native captures and 24 exact counter-origin
comparisons. The central and wrapping guests improve from 32,032 mismatches
per field to zero; all other ECS controls remain exact. Reference/input
hashes match `identities.json`; the native binary is unchanged across capture.

`stage54/validation.json` records 234 passing chip/machine tests, 44 query
tests, 55 snapshot tests, all fourteen in-flight restore boundaries, strict
Clippy and both six-case Test Kit lanes. `stage54/timed/` retains all ten
portable-corpus results: seven pass; ECSENA is one lores tick late, and two
ECS cases still fail the adjacent guest-field requirement. The static
cross-family registration and previous palette-XOR residuals remain open.

Replay by reconstructing each ECS case's existing `reference/capture/`,
`reference/run.log`, `inputs.json` and `probe.adf` as described above, then
copying its `stage54/<case>/stage54-doubled.png` into that case. Run
`compare_phase.compare(case, "stage54-doubled.png")`. All three `fields`
entries must have zero mismatches; `baseline` retains the older uncorrected
origin comparison and is not the phase-qualified result.

Run `python3.13 stage54/replay.py` from this directory to reconstruct all
archived inputs and rerun the comparisons without firmware or a live emulator.
It also requires the pre-fix central guest to fail at 32,032 samples in each
of three fields, proving the same comparison detects the missing stages.
