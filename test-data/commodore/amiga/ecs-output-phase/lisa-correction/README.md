# Lisa counter-phase correction

The approved correction removes an extra lores tick from window, bitplane,
sprite and Copper colour output. It also supplies the next counter to the
fixed blanking comparison. Timed registers, fractional edges, native geometry
and snapshot 53's saved layout remain unchanged.

`window-comparison.json` records the initial 51 exact window fields;
`window-final.json` repeats all seventeen guests with the final executable.
`controls-baseline.json` preserves the subsequent seven independent failing
controls. `controls-final.json` records all 21 corrected fields, including
coloured borders, four sprite fetch modes and two textured scroll modes.
Comparisons use all four 35 ns samples and exact RGB across the common raster.
Each case requires 600 complete, consistent reference counter-origin rows.
Source and binary identities remain in the parent validation record; the
reference producer and raw fields were not altered for the correction.

`guest-identities.json` binds the guest, native capture, producer configuration
and complete producer log. `traces/` preserves those logs. `hires-origin.log`
independently records counter 100 at buffer x18, retained padding 2 and LOL=0
for all 600 active observations in host-hires output. Its origin is counter
91, agreeing with the full-resolution producer's counter 100 at x36.

`test-kit-manifest-before.json` and `test-kit-assertions-before.json` retain the
prior Test Kit registration. The current registration changes only the source-
based consumer transform (raw x + 6 hires samples) and manifest hash binding.
All producer images, RGB hashes, exact assertions and dimensions are unchanged.
The native framebuffer has not been translated.

Rebuild the independent colour control from the existing window builder's
`legacy/` output:

```sh
python3.13 test-data/commodore/amiga/ecs-output-phase/tools/build_color_control.py /tmp/window/legacy /tmp/color-moves
```

The ADF SHA-256 is
`dfdaefa90e5f5f52f5ebf89344b39a25d5029fc51f54f8f43144e98504416075`.
`color-control-inputs.json` records the guest inputs. The four sprite controls
are the wide-sprite-DMA builder's FMODE 0/1/2/3, offset-zero guests. The scroll
controls are the midline-scroll builder's `lores-f3-changes` and
`shres-f0-changes` guests. Capture using the parent origin-instrumentation
patches and the existing SPHX full-resolution capture hook. Compare with
`compare_phase.py --trace-tag AGA_ORIGIN --native final.png` and a manifest
binding the ADF/native PNG hashes. Missing traces, incomplete fields and any
RGB difference fail the comparison.

These measurements establish agreement with the UAE software family in the
listed cases. They do not establish physical-hardware consensus or calibrate
ECS Copper colour timing and every programmable-blanking mode.
