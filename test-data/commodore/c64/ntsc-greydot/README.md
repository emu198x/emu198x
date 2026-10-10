# NTSC greydot launch phases

Native VICE x64sc 3.10 produces two stable phases for Daniel Kahlin's PAL
`greydot` test on the 65-cycle NTSC 6567R8 and 8562. Their first `$D021` store
is raster 110, cycle 15 or 16. All 408 stores in a frame are recorded here.

`cycle15-stores.bin` and `cycle16-stores.bin` contain 408 four-byte records:
little-endian u16 raster line, u8 cycle, u8 value. The traces are identical
across the two NTSC chips within each phase. The PNGs are the corresponding
384×247 native screenshots, with the reference window starting at raster 28
and wrapping after raster 262. Horizontal crop: 16 from our 416-pixel frame.

The snapshots use an external palette matching `mos-vic-ii/src/palette.rs`,
filter 0, and all five VICE colour controls set to neutral (1000). This
allows an exact digital colour-index comparison; it does not validate an
analogue display model. Opposite-phase images differ by 521 pixels on the
8562 and 49 on the 6567R8. The 521-pixel difference was reported as issue
1629 before the two launch phases were identified.

The program and ROMs remain external. The docs repository's
`plans/2026-10-10-c64-ntsc-greydot/capture.py` reproduces all fixtures and
retains their source and tool identities. It runs six launch delays on each
chip, inserting 0–5 NOPs before the first CLI via a monitor trampoline at
$0B00, and verifies complete, stable store traces. `sha256.json` identifies
the six committed fixtures. No emulator production timing changed.

Run `ntsc_greydot_matches_both_native_launch_phases` in the C64 runtime's
`vicii_testbench` integration test with `--ignored` and
`EMU198X_STRICT_FIXTURES=1`. It varies normal BASIC launch time over six
frames per chip, requires both native phases, compares all stores and
pixels, and checks that each opposite-phase image yields its known mismatch.
