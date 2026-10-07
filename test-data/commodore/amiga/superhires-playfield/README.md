# AGA superhires playfield diagnostics

These are exploratory inputs, not an admitted hardware conformance corpus.
They reuse the CC0 sprite-horizontal-phase guest and bootloader, keeping its
SPHX ready record, quarter-position A5A5 sprite and per-field DMA pointer reset.
The original corpus and its fixed-lores validator remain independent.

Build with Python 3, `m68k-elf-as` and `m68k-elf-ld`:

```sh
python3 test-data/commodore/amiga/superhires-playfield/tools/build.py /tmp/amiga-shres
python3 test-data/commodore/amiga/superhires-playfield/tools/build.py /tmp/amiga-shres-pattern --word 0xa5a5
```

Each directory contains the exact guest sources, register/geometry inputs,
assembled payload, bootable ADF and a manifest with source/payload/ADF hashes.
`--word` accepts any 16-bit value; the default is the isolated MSB, `0x8000`.
Other row words are zero. All 256 rows have the same pattern. The guest keeps
its original word alignment: wide transfers may repeat selected words when
the bitplane is not aligned to their bus width. The manifest hashes identify
the exact emitted layout; an isolated source word need not appear only once
in a wide-fetch raster.

| Profile | BPLCON0 | FMODE | Words per row | Pattern word index |
|---|---|---|---|---|
| lores | 1000 | 0000 | 20 | 4 |
| hires | 9000 | 0000 | 40 | 8 |
| shres-fmode0 | 1040 | 0000 | 80 | 16 |
| shres-fmode1 | 1040 | 0001 | 80 | 16 |
| shres-fmode3 | 1040 | 0003 | 80 | 16 |

DDFSTRT/DDFSTOP are 0038/00D0, both modulos are zero. The pattern moves farther
into the row in faster modes so it remains visible. A lores-sized allocation
must not be reused after selecting superhires: these inputs fetch 80 words per
line, and the sample line at beam 132 would otherwise read unrelated chip RAM.

For Emu198x, boot `probe.adf` on A1200 PAL with Kickstart 3.1 and capture after
180 frames. Native sample line 132 is framebuffer row 214 at 1536×576. For
FS-UAE, use the SPHX-ready capture hook and the separate
[full-resolution host patch](../../../../tools/fs-uae-sprite-phase-capture/fs-uae-5.0.7-full-resolution-host.patch),
with its environment switch set. Capture adjacent guest fields 9, 10 and 11;
the same beam line is raw row 212 at 1512×576. Compare the retained sample
streams using the recorded framebuffer origins; do not search for alignment.

The observations and their limits are recorded in the primary
[video output phase reference](../../../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md).
DMA/serial/scroll regression checks live in
`crates/runtime-commodore-amiga/tests/superhires_dma.rs`.

The full-raster gate in `../wide-sprite-dma/tools/compare.py` now checks the
complete common raster at captured origins, including every 35 ns sample.
