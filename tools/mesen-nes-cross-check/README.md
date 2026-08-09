# Mesen2 NES cross-check adapter

This directory answers a question the NES test corpus cannot answer on its own:
when a blargg ROM reports a failure as a table of measured numbers, what should
those numbers have been?

`sprdma_and_dmc_dma` is the case that motivated it. The ROM fails with `#01` and
prints sixteen clock counts — one per get/put alignment — followed by a CRC over
them. The counts are the whole diagnosis, but without the expected sixteen there
is no way to tell "one cycle out at half the alignments" from "the arbitration is
the wrong shape entirely". Editing DMA timing until a CRC matches is guesswork;
diffing against an emulator that already passes is measurement.

Mesen2 is the reference because it passes the ROM and because Emu198x's DMA
arbitration was modelled on its `NesCpu::ProcessPendingDma` in the first place,
so a divergence points at a specific arm rather than at "DMA is wrong".

## What it does

Runs one or more ROMs headless under Mesen2 and prints what each reports through
blargg's `$6000` protocol: the status byte and the zero-terminated ASCII report
at `$6004`. `MemoryType::NesMemory` is the CPU address space, so the report is
readable without knowing the mapper's layout.

Nothing in the vendored Mesen2 tree is modified. The harness drives the same C
API the Avalonia UI uses.

## Building

Mesen2 lives at `198x/emulators/nes/Mesen2` as a reference-only snapshot. Build
its core alone — the `core` target skips the .NET UI, which is not needed here
and pulls a much larger toolchain:

```sh
cd ../../../../emulators/nes/Mesen2
make core -j8            # needs SDL2; produces InteropDLL/obj.osx-arm64/MesenCore.dylib
```

Then build the adapter against it. The dylib's install name carries no path, so
it has to sit beside the binary rather than be found by rpath:

```sh
MESEN=../../../../emulators/nes/Mesen2
clang++ --std=c++17 -O1 -o mesen_probe main.cpp "$MESEN/InteropDLL/obj.osx-arm64/MesenCore.dylib"
cp "$MESEN/InteropDLL/obj.osx-arm64/MesenCore.dylib" .
```

Build products are deliberately not committed.

## Running

```sh
mkdir -p MesenHome
./mesen_probe "$PWD/MesenHome" path/to/rom.nes [more.nes...]
```

The home folder must be writable; Mesen2 puts its settings there.

## Recorded results

Expected values obtained this way are committed next to the corpus they explain,
not left in this directory — see
[`test-data/nintendo/nes/blargg-survey/sprdma-dmc-dma-expected.tsv`](../../test-data/nintendo/nes/blargg-survey/sprdma-dmc-dma-expected.tsv).
Re-derive them rather than trusting a stale copy if Mesen2's snapshot moves.
