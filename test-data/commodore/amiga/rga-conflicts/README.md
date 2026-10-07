# Display / refresh RGA conflicts

These 1,000 rows compile the unchanged registered `write_rga` and outgoing
refresh/bitplane/sprite service fragments from `custom.cpp` revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`. The builder rejects a different
source hash and checks a dropped-refresh negative control.

The 200 bitplane rows cover eight bitplane RGA registers and all five
fixed-register signals
($38, $3A, $3C, $3E, $1FE), and OCS/ECS 16-bit plus Alice 16/32/64-bit output.
Each begins after one ordinary refresh from the chip's reset pointer. Fixed
DMAL is admitted after the preceding `inc_cck`, before the next PT/MOD
sample. Bitplanes are initially unaddressed, so the combined type skips
BPLPT/MOD sampling and retains refresh's pointer.
`address` is the captured input pointer; `bitplane_after` is the separate
service result. Reads/payloads count stubbed transfer helpers, not physical CAS
edges. These are isolated source-fragment observations, not emitted-slot,
full-raster or silicon validation.

The 800 sprite rows cover all eight channels, both control/data identities,
both words, the same fixed signals and all supported widths. A suppressed
sprite strobe increments the live sprite pointer after refresh has advanced
it; a selected sprite data register retains the captured pointer without the
normal width increment. The tests compare these separate pointer effects,
actual memory reads and delivered payloads against the compiled source.

The common-chip test compares retained addresses, combined register selection,
refresh and display pointer results, normal data width, suppressed data
strobes, single service, both CPU phases, and postcard restoration.

Regenerate into a temporary directory, then compare all generated artifacts:

```sh
python3 build-reference.py --source /path/to/registered/custom.cpp --output /tmp/rga-reference
```

Primary provenance and boundaries are in the
[shared observations](../../../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md).

The paired A1200 PAL guests exercise hires four-plane DMA from DDF $18..$E0,
with BEAMCON0 $0020 (control) or $4020 (HARDDIS). They initialise all four
pointers and the low 1 KiB reached by DMA wrapping; leftover Kickstart RAM
is not a deterministic diagnostic source. The case identity records which
profile ran. Build them with the existing shared assembler/ADF helper:

```sh
python3 build-wrap.py /tmp/rga-wrap
```

Capture each reference guest's complete adjacent fields 9, 10 and 11 with
the full-resolution harness. The existing `wide-sprite-dma/tools/compare.py`
checks those ready records and compares fixed native (16,2) and reference
(0,0) origins. It rejects incomplete fields or any differing interior pixel.
The native ready counter must independently reach at least 9. Record
producer/input hashes before and after captures. The diagnostic applies to
the A1200 PAL software-reference profile; it is not hardware certification.

## Wrap-edge trace

`wrap-edge-observation.json` records the split of the 524-pixel HARDDIS residual:
32 missing native pixels from premature vertical-close DMA, and 492 differences
in reference host padding. It preserves source, executable, ROM, ADF and image
hashes, the final-line service trace and positive framebuffer-row coverage.
The strict comparison still reports 492 differences after the DMA correction.

To reproduce the read-only trace, first verify the registered `custom.cpp` and
`drawing.cpp` hashes against that record, then apply
`reference-wrap-edge-trace.patch` to a writable reference copy alongside the
existing full-resolution capture patches. Set `FSEMU_CODEX_RGA_TRACE=1` and
`FSEMU_CODEX_SCAN_TRACE=1` with the normal capture environment. Build the two
changed units using `CXXFLAGS=-O1`. Capture the controlled guests' fields 9–11
and require all six images to remain byte-identical to their uninstrumented
baselines. `SCAN2` rows identify actual buffer addresses; the `v` label alone
is not a framebuffer row. Require positive row coverage and match the logged
head/tail pixels to the corresponding raw field before interpreting a trace.

The reference's `get_line` fills the first four columns with black before
rendering. Those are recorded host padding, not evidence for adding blanking
to native Denise. Keep the current fixed origins and all pixel assertions;
changing the capture endpoint is separate work.

The same patch also logs `SIGNAL` records before clipping in the four hires
normal/odd-even, sprite/non-sprite emitters. Verify the registered generated
`linetoscr_aga_fm0.cpp` hash as well, and use `display_optimizations=none` for
that observation. At PAL `lol=0`, each record describes four composed samples
at `col + 8*pixtotal + 4*half`. Compare with the unchanged native origin (16,2),
duplicate non-interlaced rows, and check retained columns 4–11 against the final
reference bytes as coordinate anchors. Require positive coverage of every
padded mismatch. The recorded 6,840 control and 6,784 HARDDIS samples all match
native output; all 492 padded mismatches are covered. A one-pixel shifted
negative control produces 1,234 HARDDIS mismatches. This is separate evidence,
not permission to drop the padding columns from the existing image comparison.
