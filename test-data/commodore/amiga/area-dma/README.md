# Area-blitter DMA schedules

The live D-only probe originally wrote six successive words one CCK apart.
The hardware manual's area timing and the registered vAmiga active programs
require a two-CCK period. Adding delay to a remaining-count diagnostic would
leave the fetch/result/write ordering wrong.

`reference-programs.tsv` records all 32 channel/fill main programs by compiling
vAmiga's unchanged active table in a flag recorder. Regenerate independently:

```sh
python3 test-data/commodore/amiga/area-dma/tools/reference_schedule.py \
  ../../emulators/amiga/vAmiga /private/tmp/amiga-area-reference
cmp test-data/commodore/amiga/area-dma/reference-programs.tsv \
  /private/tmp/amiga-area-reference/reference-programs.tsv
cargo test --release -p commodore-agnus-ocs --test area_dma
```

The Rust test compares actual requests, transfers and CPU bus availability
against every recorded program over four words. The first D cell is locked
and bus-free; later writes consume the previous held result. Additional tests
check masked/shifted overlapping memory and denied idle-cell admission.
The existing blitter tests cover minterms, fill, row modulos, BZERO and the
separate revision-dependent final-D completion observers. Runtime tests
snapshot both unprimed and primed stages, including fill idles and the drain.

This adapter executes source-defined schedules, not a full vAmiga machine or
silicon. The paired full-raster FS-UAE guests in `../copper-blitter-wait/`
provide an independent complete-machine check of the D-only completion path.
The all-disabled program verifies its free-cell schedule, not a new reference
claim about its data/BZERO semantics. Exact propagation of mid-blit register
writes remains bounded by the existing decision.
