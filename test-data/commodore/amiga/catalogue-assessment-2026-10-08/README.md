# Amiga catalogue requalification, 8 October 2026

The seven assessment mismatches are classified below. One real register
routing bug was fixed before accepting any replacement image: missing
BPL6PTL on every chipset, plus missing AGA BPL7/BPL8 pointer halves. The
corrected Banshee cold boot matches the repaired-snapshot diagnostic exactly.

Media, firmware, startup input, observation times, audio windows and the one
existing Workbench 1.3 ignore rectangle are unchanged. Only reviewed frame
and audio hashes change in the manifest. These remain deterministic software
compatibility checkpoints, not independent physical-hardware measurements.

## Revisions and method

- `1cead4e8`: previous catalogue baseline.
- `e773c9c1`: integrated CPU/chip timing and native AGA resolution changes.
- `65f29382`: subsequent Lisa/OCS blanking corrections, before the Paula work.
- `f41195ed`: pre-assessment current code, including Paula host bandwidth.
- `c9965d9b`: corrected machine register routing; no chip-stage or state change.

All seven entries were cold-booted at the first four revisions. The corrected
core then ran all ten entries, including the normal snapshot checks. The
before/intermediate result manifests retain successful capture exits and SHA-256
identities; `producers.json` identifies source commits and compiled binaries.
`input-files.sha256` binds the unchanged private corpus. The exact old
manifest is available at `1cead4e8` and the pre-update local manifest at
`f41195ed`.

`frames/` retains all 35 images used in this comparison. `measurements.json`
records dimensions, PNG/WAV SHA-256, stereo frame counts, nonzero sample
counts, peak, RMS and clipping counts. Audio is decoded as stereo signed
16-bit PCM at 48 kHz; RMS is sqrt(sum(sample²)/sample_count), with no alignment,
normalisation or resampling. Every capture contains samples; none clips at
either 16-bit endpoint. Disk/ROM data, raw guest memory and save states are
not redistributed.

The two `classification-shard-*.log` files intentionally retain the seven
old-baseline failures and ten snapshot passes. A clean production binary,
with the capture/trace instrumentation removed, runs the final manifest gate.
The final logs are recorded separately; old failing output is not relabelled.

## Classification

| Entry | Changed output and evidence | Decision |
|---|---|---|
| Workbench 1.3 | Exactly 9,184 pixels change from blue to black: x=0..7 and 760..767, y=0..573. All desktop pixels and audio agree. Capturing parent `c40c45de` and fix `97aa2afe` isolates the fixed OCS blanking latch. | Accept corrected edges; retain the original six-digit ignore rectangle. |
| Barbarian | The complete title/demo remains present, but combat pose and score differ. The new frame first appears at `e773c9c1` and is unchanged through the Paula work and pointer fix. This is guest progression after integrated bus/blitter/CPU timing changes, not a colour-only filter. Audio changes at integration and again during Paula corrections. | Accept the reviewed demo waypoint; do not claim exact reference combat-frame timing. |
| 1943 | Frame is byte-identical at every revision. Both old and new audio are entirely zero. The WAV changes from 95,846 to 95,845 stereo samples at `e773c9c1`. The runner captures a rounded count of complete emulated fields; the output sample phase changes its final sample count by one. | Accept the silent window; no lost or newly introduced sound. |
| Workbench 3.1 | Native Lisa width changes 768→1536, preserving four samples per lores tick. After doubling the old image and applying the independently established one-lores-tick (-4 native samples) phase change, central differences occupy only the pointer, x=164..207/y=38..59 (368 pixels). The remaining differences are blanking edges. Audio stays silent. | Accept native precision, corrected phase/blanking and separately clocked pointer. No new ignore region. |
| Banshee | The old baseline already contained duplicate text. Integrated DMA timing amplified the missing pointer writes into black streaks. Display RAM is unchanged; the Copper list programs all eight pointers, but machine dispatch dropped five register halves. Six CPU/Copper regressions fail before and pass after the correction. Cold boot and two-field restored diagnostic both produce `4ee6197e2dee4e3b`. | Fix the fault first, then accept the repaired POWERUPS page. |
| State of the Art | Integration changes the dancer/effect phase; OCS blanking changes the edges. Delivering BPL6PTL restores the EHB pattern. The noisy, fractured picture initially looked suspect, so an independent FS-UAE cold boot captured seven fields from 5500 to 6100. The same fractured red/yellow dancer and noise occur in the reference at field 6000. | Accept this effect as a compatibility waypoint. The exact pose differs; no RGB match or frame-phase accuracy is claimed. |
| Alien Syndrome NTSC | OPTIONS image is byte-identical throughout. Music remains active, with waveform changes at integrated timing and later Paula output fixes. | Accept the reviewed audio checkpoint under the existing amplitude, timing and bandwidth evidence. |

Workbench comparisons count exact RGB differences, not perceptual similarity.
The Workbench 3.1 phase is taken from the counter-traced Lisa correction; it
was not chosen by searching image offsets. The pointer observation is a
classification, not an added ignored region in the catalogue hash.

## Audio boundary

The measured music windows stay active in stereo without endpoint clipping:

| Entry | Previous PCM peak / RMS | Corrected PCM peak / RMS | Stereo samples |
|---|---:|---:|---:|
| Barbarian | 13,197 / 875.60 | 14,837 / 1,664.81 | 95,846→95,845 |
| Banshee | 25,965 / 5,085.48 | 26,517 / 4,999.55 | 95,846 |
| State of the Art | 28,472 / 5,898.31 | 29,145 / 5,552.13 | 95,846 |
| Alien Syndrome NTSC | 20,195 / 3,577.37 | 20,292 / 3,420.59 | 95,913 |

These measurements establish signal presence and bounds, not correct music
by themselves. The pre-Paula intermediate isolates later waveform changes
from guest/display progression. Existing reference-backed period, DMA,
interrupt and attachment probes explain the timing corrections. The later
`2d4f1867` accumulator preserves short signals, `461d92f4` removes an
unsupported cubic amplitude curve, and `f41195ed` rejects host-sampling
aliases. Their independent evidence lives in
[`paula-audio/`](../paula-audio/), notably the amplitude and bandwidth probes.
Physical PWM phase and analogue calibration remain outside this claim.

## Independent visual evidence

The Workbench changes use the retained counter-based diagnostic evidence in
[`ecs-output-phase/`](../ecs-output-phase/): `lisa-correction/`,
`ocs-right-edge/`, `top-field-blanking/` and `counter-reset/`. Those corpora
compare raw samples and retain failing baselines; this report does not turn
a title screenshot into a hardware oracle.

The State of the Art reference uses FS-UAE 5.0.7 with cycle-exact OCS A500,
512 KiB chip + 512 KiB slow RAM, the same Kickstart 1.3 and extracted ADF,
normal floppy pacing and write protection. `reference-sota/` retains the
configuration, seven completed raw-buffer PNGs, field/dimension metadata,
producer hashes, complete compressed log and observation-only source patch.
The hook copies the completed chipset buffer before frontend presentation;
it does not alter guest memory, CPU/chip timing, disk data or renderer logic.
Its core-field count is not assumed to be a synchronised guest event count.

## Reproduction

Set the existing catalogue media and firmware root variables to a corpus
matching `input-files.sha256`, then run the unmodified catalogue CLI:

```sh
cargo run --locked --release -p emu198x-catalogue --bin catalogue -- \
  run --manifest crates/emu198x-catalogue/manifest/amiga.toml
```

The same CLI's `capture --entry ID --save-screenshot PATH --save-audio PATH`
reproduces an individual observation. Add the manifest argument above.
Instrumented binary patches are retained only to reproduce the diagnostic
observations; production source contains none of those hooks.

## Local verification

The affected three machine crates plus runtime pass **633 tests**, with 122
explicitly ignored tests unchanged. This includes the real-ROM Workbench
goldens and live snapshot coverage. The catalogue library passes all 35
tests. Strict Clippy on the changed boards/runtime passes. Retained
reference replay reproduces the old failing signatures and exact corrected
results: 9 OCS edge fields, 90 counter-reset fields and 30 top-blank fields.

`python3.13 verify.py` checks all 35 retained image identities and dimensions,
the exact Workbench edge/pointer classifications, unchanged 1943/Alien
images, measured silent-window lengths and seven reference fields. It also checks
that only expected hashes changed in the two retained manifests and requires
ten distinct catalogue passes plus ten distinct snapshot passes in the
completed final logs. The
negative control changes a desktop pixel and updates its recorded file hash:
the semantic edge check still rejects it. Its failure is retained in
`negative-control.log`. Ruff passes.

The final clean production build passes **all ten catalogue entries and all
ten snapshot checks**, across OCS/ECS/AGA and PAL/NTSC. Both shards exit zero
with five entries and zero failures. `final-shard-1.log` and
`final-shard-2.log` retain the complete gate output. This closes the seven
assessment mismatches at the stated compatibility boundary. It does not
claim physical audio calibration or exact guest-frame synchronisation.
