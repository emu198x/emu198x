# VIC-II light-pen measurements

The four 1,280-byte files are native VICE x64sc 3.10 results from the staged
`split-tests/lightpen/lightpen.prg` (Daniel Kahlin's R04 measurement). Each
contains five 256-byte pages: D011, D012, LPX, LPY and interrupt flags. They
exclude the two-byte PRG load address. Model names identify PAL 6569/8565 and
NTSC 6567R8/8562R4.

The test varies the trigger delay through 256 clock positions, including the
last raster line and frame wrap. The C64C horizontal latch is one X unit
lower than its NMOS counterpart except where a held-low frame retrigger uses
the fixed retrigger coordinate. The frame retrigger also captures Y=0 and
raises the light-pen interrupt.

All 1,280 bytes per model agree with the physical references after applying
the testbench's documented pre-R03 preparation step. The original `.prg`
dumps require `makeref.c`, which their Makefile runs to produce the `.bin`
references embedded in the R04 guest. Compiling that converter changes only
LPX samples 254/255 (offsets 766/767) in these inputs, adding four X units to
each. Its complete outputs match these native files and the guest's embedded
references byte-for-byte. Original physical dump files remain immutable.

This is complete agreement for the measured schedule on these four reported
chip models, not a claim about all light-pen conditions or analogue latency.

Primary observations and attribution are recorded in the shared reference
library's `by-topic/vic-ii/2026-lightpen-measurement-observations.md`. The
physical-chip identities are as reported by the upstream README, including
its explicitly guessed PAL chip revisions. The staged upstream revision and
original dump dates are unresolved.

The docs repository's `plans/2026-10-10-c64-hmos-lightpen/capture.py` reproduces
the native files. Its evidence directory retains the exact program, ROM and
reference source identities, native logs and before/after measurements.
Program and ROM bytes remain external. `sha256.json` identifies these outputs.

Run `light_pen_measurement_matches_native_reference` in the C64 runtime's
`vicii_testbench` integration test with `--ignored` and
`EMU198X_STRICT_FIXTURES=1`. It requires the guest to write its completion port,
checks all five result pages and compares every byte with native VICE and the
prepared physical references. `light_pen_reference_normalization_matches_upstream`
checks the conversion against the independently generated captures and guest
references, and verifies that prepared dumps pass through unchanged.
