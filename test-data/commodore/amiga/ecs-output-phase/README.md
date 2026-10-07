# ECS output-origin diagnostics

The version-53 ECS window/data edges agree with the reference in Denise counter
space. The old image comparison omitted four samples of reference output
padding. This corpus preserves that failure and measures the counter-to-buffer
mapping independently of image content.

See the [primary observations](../../../../../../reference/by-system/commodore-amiga/2026-ecs-output-phase-observations.md).
No production rendering or save-state schema changes are part of this result.

Apply `tools/fs-uae-counter-origin-trace.patch` to a writable copy of the
registered exploratory full-resolution producer, then build it using the
existing FS-UAE procedure. Set `FSEMU_CODEX_ECS_PHASE=1` during the usual SPHX
capture. Preserve its `reference/run.log` alongside the three raw fields.
The hook only reads state and logs observations.

The eleven inputs are the existing horizontal-window lores/hires guests.
`guest-manifest.json` pins the ADFs and native captures. Each native 70 ns
sample is repeated twice to compare at the reference's 35 ns sample spacing.
Use that manifest as `diagnostics.json` in the capture directory:

```sh
python3.13 test-data/commodore/amiga/ecs-output-phase/tools/compare_phase.py /tmp/ecs-output/guests --output /tmp/ecs-output/comparison.json
python3.13 -m unittest discover -s test-data/commodore/amiga/ecs-output-phase/tools -p 'test_*.py'
```

The comparator validates complete raw fields, guest identities and native/ADF
hashes through the existing whole-raster checks. It additionally requires a
complete, uniform counter-origin trace for every active row in all three guest
fields. Missing evidence is an error. Its separate counter-domain result does
not replace the admitted video gate or claim physical-beam calibration.

`ecs_output_phase_trace` is a read-only native example accepting `ocs`, `ecs` or `aga`,
a Kickstart path and an ADF. It checks SPHX readiness, records one complete
line and prints stored row edges. It uses the normal runtime clocks and chip
diagnostics; it does not alter execution.

Results: all 33 ECS fields and the three-field OCS control match in counter
space. Instrumentation leaves all 33 ECS raw fields byte-identical to the
original reference captures. The negative controls detect an injected pixel
error and reject absent origin evidence.

The AGA control uses the separate `fs-uae-aga-origin-control.patch` and
`FSEMU_CODEX_AGA_PHASE=1`. It establishes the same output origin. Both tested
AGA guests match the old mapping but fail the counter mapping: the native
window and data-end edges are one lores tick late. `aga-controls.json` and the
retained traces record this failing evidence. The approved correction is now
complete: [Lisa correction](lisa-correction/README.md) records all 72 exact
fields, independent sprite/COLOR controls, source-derived Test Kit mapping,
restore checks and boot requalification. Snapshot layout remains version 53.
