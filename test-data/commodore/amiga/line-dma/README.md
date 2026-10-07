# Line-blitter stage evidence

The standard width-two line engine retains four CCK stages per pixel.
Optional B DMA adds a B read and a reserved bus stage, for six stages.
`crates/commodore-agnus-ocs/tests/line_dma.rs` observes the actual reads,
writes, result values, pointer movement and concurrent CPU availability.

Run the registered source-table adapter from the emulator root:

```sh
python3 test-data/commodore/amiga/line-dma/tools/reference_schedule.py \
  ../../emulators/amiga/vAmiga /private/tmp/emu198x-line-dma-probe/reference
```

The adapter extracts the original vAmiga microprogram table and compiles
it into a flag recorder. It checks all four programs are present and
terminate with the expected stage counts. The JSON report preserves the
source path and SHA-256. It establishes source-defined ordering, not a
full reference-emulator or original-silicon comparison.

The initial external probe in `/private/tmp/emu198x-line-dma-probe/`
exited 1: C read at CCK 1 and D write at CCK 2 in both B modes, with no
B read or pointer update. `red.log` retains that failure. `green.log`
records the corrected standard C read/write at CCK 2/4 and B-enabled
B/C/D transfers at 2/3/6.

Primary evidence and limits are recorded in
`reference/by-system/commodore-amiga/2026-line-blitter-stage-observations.md`
at the umbrella level. Nonstandard widths, live control/pointer writes,
exact startup relative to the reference engine and silicon revision
differences remain outside this probe.
