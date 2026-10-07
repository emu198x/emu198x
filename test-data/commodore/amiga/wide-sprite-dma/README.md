# AGA wide sprite DMA diagnostics

These CC0 guests exercise sprite FMODE 0/1/2/3 at every even pointer offset
within an eight-byte block. They reuse the existing sprite-horizontal-phase
bootloader, per-field pointer reset and SPHX ready record. Each control word
occupies a full transfer with conspicuous padding. Every data lane differs,
and every displayed row has a different pattern. Unused sprites have equally
padded zero control words. These are software diagnostics, not an admitted
hardware conformance corpus.

Build with Python 3.13+, `m68k-elf-as` and `m68k-elf-ld`:

```sh
python3 test-data/commodore/amiga/wide-sprite-dma/tools/build.py /tmp/amiga-wide-sprites
```

Each case contains sources, inputs, assembled payload and a bootable ADF.
The manifest records source/payload/ADF hashes. Rebuilding reproduces all
sixteen ADFs byte-for-byte.

Boot each ADF on A1200 PAL with Kickstart 3.1. Capture Emu198x after 180 frames
with a headless script containing `run_frames` followed by `save_screenshot`;
retain the screenshot as the case's `after.png`. For the reference, use the
[full-resolution capture instructions](../../../../tools/fs-uae-sprite-phase-capture/FULL-RESOLUTION.md)
and the existing `config.uae.in` A1200 profile. Retain fields 9/10/11 and their
JSON metadata under the case's `reference/capture/`. Record the actual producer
binary, source revision, applied patches, ROM, ADF and configuration hashes.
Terminate the reference process after `CODEX_SPRITE_CAPTURE complete`.

FS-UAE 5.0.7 has a separate sprite FMODE=2 producer bug. Preserve its failing
captures before using the explicit one-line upstream WinUAE correction.
The capture instructions distinguish that producer; do not silently change
chipset logic or replace expected pixels to obtain a pass.

With the existing capture-tool Pillow environment:

```sh
python3 test-data/commodore/amiga/wide-sprite-dma/tools/compare.py /tmp/amiga-wide-sprites --output /tmp/amiga-wide-sprites/verification.json
```

The gate requires full native dimensions, actual recorded origins, the guest
identity, complete raw bytes and three adjacent guest field counters. It
compares every RGB pixel in the common 1512x574 raster, including all 35 ns
samples, border and blanking. Native origin (16,2) comes from the producer
beam origins, not a fitted alignment. Non-overlapping producer edges are
explicitly reported. A single mismatched pixel makes the command fail.
`--native before.png` applies the identical gate to a retained pre-fix capture.
An empty or incomplete capture cannot pass.

The same gate accepts the playfield builder's manifest, enabling whole-raster
checks of lores/hires/superhires playfields. Historical hires reference gates
retain their stated even-sample coverage.

The primary [observations and limitations](../../../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#wide-sprite-dma-and-full-raster-origins--2026-10-05)
record the memory-lane table, control stride, corrected vertical origin,
reference producer bug and full-raster results. Current artifacts live under
`/private/tmp/emu198x-wide-sprite-validation/`.
