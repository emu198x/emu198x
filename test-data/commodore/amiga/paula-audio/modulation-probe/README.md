# Paula modulation and DMA transition probe

All 1,440 observations agree across 96 scenarios: four source channels, four
attachment modes, periods 1/8/124, and DMA grants on one- or five-CCK cadences.
The observations include target period/volume latches, delivered-word counts,
pending requests and request admission at startup and around each byte edge.

## Producer and boundary

`reference.py` extracts unmodified vAmiga transition, buffer-load, attachment
predicate and period/volume register methods from revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. The adapter supplies a clock recorder,
DMA-enabled input, the header's request/volume helpers, and deterministic DMA
words. The origin is the first real data word's `101 -> 010` transition.
IRQ and DAC-output callbacks are inert: neither is an observation of this
probe. Target channels expose their register latches; their own playback
engines are not run by the reference adapter.

Grants are deliberately synthetic, servicing only asserted requests on the
selected cadence. This isolates Paula's state transitions; it is not a
reference-machine trace, an Agnus scanline-arbitration test, physical-hardware
calibration, or proof of target DAC/PWM timing.

The original *Amiga Hardware Reference Manual*, third edition, pages 164–166,
independently specifies the request phases, holding/output-buffer boundary and
underflow repetition. UAE's `audio.cpp` (`loaddat` and states 2/3) corroborates
the attachment direction and data-latch source by source inspection.

```sh
python3.13 test-data/commodore/amiga/paula-audio/modulation-probe/reference.py \
  --source ../../emulators/amiga/vAmiga --output /tmp/paula-modulation-reference
cargo test --locked --release -p emu198x-commodore-paula-8364 \
  --test audio_modulation -- --nocapture
```

Run from the emulator repository. `reference.cpp.gz` retains the exact generated
program, and `source-hashes.json` identifies all three input source files.
The regression requires 96 scenarios and 1,440 observations; missing data
cannot pass.

## Faults and correction

The original immediate-grant probe failed 564 of 720 observations: 120 ordinary,
144 period-only, 165 volume-only and 135 combined. `initial-reference.*.gz`
and `native-before.log.gz` preserve that producer and failing comparison.

The correction delivers volume on startup and high-byte entry, period on
low-byte entry, and sources both from AUDxDAT. Ordinary/volume DMA requests
occur on high-byte entry; period requests occur on low-byte entry. Period-only
startup does not request another word before its first period transition.

The delayed-grant extension then exposed 36 observations where the original
buffer/request behaviour lost the byte phase or accumulated requests. The DMA
output buffer now repeats during underflow and AUDxDR stays a single pending
line. `native-delayed-before.log.gz` records that failure. A separate test
proved that words arriving at clocks 9, 12 or 15 must enter the high-byte output
at clock 16 (source period 8); `late-word-before.log.gz` records the stale-byte
failure before moving the buffer load to that edge.

The regression also compares the combined component tick with retained DMA
service for 72,000 ticks across all modes and channels. Snapshot replay and
the full-machine waveform gate are recorded separately in `validation.json`.
The saved schema remains version 57; no fields or clocks are added.

Manual playback startup/IRQ timing, live attachment-bit changes, target PWM
behaviour and physical analogue response remain outside these assertions.

## Licensing

Extracted vAmiga methods and generated C++ remain GPL-3.0-only, with Dirk W.
Hoffmann's attribution retained. They compile only into a separate diagnostic
executable, never into Emu198x. The original driver and observation data follow
the corpus's CC0 dedication.
