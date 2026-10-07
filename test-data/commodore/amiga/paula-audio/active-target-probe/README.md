# Modulation into running Paula channels

This probe detects a receiver reloading its old period when modulation arrives
on the same CCK. All four channels are playing, including receivers and the
intermediate channels in a chain. The correction applies each channel's
existing transition effects before advancing the next channel.

The matrix covers 88,704 channel observations in 1,296 scenarios: sources
0/1/2, source periods 2/8/124, receiver periods one lower/equal/one higher,
ordinary/volume/period/both attachment, single/chained attachment, manual/DMA
playback and modulation words 0/1/8. Channels start normally; each source then
receives its holding word. No further DMA grants occur. IRQs are observed
before acknowledgement on every CCK. Observations cover boundary and adjacent
clocks through four original periods; zero modulation checks the immediate
65,536 reload, not the whole subsequent long interval.

Pinned WinUAE `c32694e338fa5f34977f522eb4898adb069d2e73` and vAmiga
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` transition methods execute unchanged.
The adapter follows the inspected schedulers' ascending channel order.
WinUAE's early manual IRQ event is normalized to the next actual byte deadline.
Extracted sources preserve upstream copyrights and are not linked into Emu198x.
The source hashes include both inspected schedulers.

Columns:

`source,source_period,target_period,mode,chain,dma,word,clock,channel,state,period,counter,buffer,sample,volume,request,irq`

Modes are 0=ordinary, 1=volume, 16=period, 17=both. The references agree on
state, period, byte deadline, buffer, volume register, request and IRQ in every
row. Their sample columns differ in 28,190 rows: WinUAE supplies the raw DAC
sample, while vAmiga stores a volume-scaled sample (reported divided by 64)
and suppresses repeated byte edges. Do not treat the latter as an independent
raw-output oracle. The native regression compares raw samples with WinUAE.

Before correction, native differences in state/period/counter/buffer/sample/
volume/request/IRQ order were `[3876, 0, 4872, 424, 3866, 424, 0, 621]`,
including 2,584 samples on channels with attachment muting off (without
weighting by volume). All ordinary controls matched. The corrected
regression matches every observation. Empty and single-corrupted-deadline
fixtures must fail the shared checker.

The board regression checks period-eight source/receiver expiry with a new
period of one. All 36 whole/half-CCK restore checkpoints failed before the fix,
across sources 0/1/2 and OCS/ECS/AGA. It checks counter, raw sample and audible
stereo output after the edge, plus deterministic chip/IRQ/mixer replay and
final snapshot bytes. At CCK 10 the board fixture's manual receiver samples a
newly delivered IRQ and stops at 11, holding the low sample; the test asserts
this stop too. The final fixture fails against the old ordering and passes
with the correction. Snapshot version 60 is unchanged.

```sh
python3.13 test-data/commodore/amiga/paula-audio/active-target-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE --vamiga ../../emulators/amiga/vAmiga \
  --output /tmp/paula-active-target
cargo test --locked --release -p emu198x-commodore-paula-8364 --test active_target -- --nocapture
cargo test --locked --release -p runtime-commodore-amiga --test paula_active_target -- --nocapture
```

Primary evidence:
`reference/by-system/commodore-amiga/2026-paula-active-target-observations.md`
in the umbrella repository. This is reference-implementation evidence, not a
physical-chip capture. Complete DMAL, physical register latency, arbitrary
writes within a CCK, volume output-latch timing, PWM and analogue response
remain outside this comparison. Volume-register agreement does not establish
the instant when a changed gain reaches the DAC.
