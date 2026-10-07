# Paula period reload probe

This probe isolates period-register interpretation from DMA supply and host
audio filtering. Eleven reload values and twelve mid-period writes are
measured by compiled, unmodified vAmiga methods and compared with actual
native DAC-latch transitions through the public Paula component interface.

The registered producer is vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. `reference.py` extracts only
`pokeAUDxPER` and `percntrld`; a scheduler recorder observes their requested
deadline. `reference.cpp.gz` retains the generated program, `source-hashes.json`
identifies the input methods' files, and `reference.csv` stores all 23 rows.
The source's `DMA_CYCLES` macro is eight master clocks. This is executable
component-source evidence, not a complete reference-machine trace.

```sh
python3.13 test-data/commodore/amiga/paula-audio/period-probe/reference.py \
  --source ../../emulators/amiga/vAmiga --output /tmp/paula-period-reference
cargo test --locked --release -p emu198x-commodore-paula-8364 \
  --test audio_period -- --nocapture
```

Run these commands from the emulator repo. Rebuilding the CSV reproduces the
retained bytes. The Rust tests require all eleven reload rows and all twelve
write rows; an empty or partial CSV cannot pass.

## Failure and correction

Before correction, all seven tested values below 124 (including zero) lasted
124 CCKs. Eight mid-period write cases consequently had the wrong next
deadline. The other eight rows were exact. `native-before.log.gz` preserves
the two failing regressions and all observations.

The existing period register remains 16 bits. Its effective reload and saved
remaining counter are now 32 bits: zero reloads 65,536, and other register
values reload unchanged. The current interval still finishes before a new
register value is used. No clock, DMA stage or filter changes are involved.
The user approved the public diagnostic field widening and Amiga snapshot
version 57, which rejects version 56 and earlier saves.

The native setup deliberately supplies startup and prefetched words. It
does not claim correctness under DMA starvation, attach modulation, manual
playback startup or interrupt timing. The existing three-case full-machine
waveform gate separately checks that ordinary period-512 playback retains its
registered routing, cadence and volume relationships.

Primary observations:
[`2026-paula-period-observations.md`](../../../../../../../reference/by-system/commodore-amiga/2026-paula-period-observations.md).

## Validation

All 23 rows agree after correction (`native-after.log.gz`). The retained logs
also record 104 Paula component tests, 56 runtime library tests, 60 snapshot
tests, 45 query tests and 39 board-level Paula tests passing. Snapshot replay
covers 27 checkpoints across OCS, ECS and AGA, including the 65,536 and 65,535
countdowns and actual sample transitions. Grouped and leaf queries agree on
the full 65,536 value.

The three-case full-machine audio gate passes with identical before/after
observations. Strict targeted Clippy (all targets), the release Amiga binary,
Rust formatting and the Python probe checks pass. `validation.json` identifies
the source baseline, result counts and retained artifact hashes.

`runtime-final.log.gz` preserves an initial validation failure: two older
snapshot tests still expected version 56. After correcting those assertions,
`snapshot-queries-final.log.gz` records the complete passing rerun.

## Licensing

The extracted vAmiga methods and their generated `reference.cpp.gz` remain
GPL-3.0-only, with Dirk W. Hoffmann's attribution retained. They are compiled
only as a separate diagnostic executable, never linked into Emu198x. The
original probe driver and observation data follow this corpus's CC0 dedication.
