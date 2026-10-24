# emu198x-yamaha-ym2612

The Yamaha YM2612 (OPN2) FM synthesis chip and its CMOS twin, the YM3438: the
Sega Mega Drive's sound chip.

Six channels of four-operator FM in all eight algorithms, with feedback,
detune and multiple. Envelopes with key scaling and SSG-EG. The global LFO,
both timers and their IRQ, CSM key-on, and channel 3's per-operator
frequencies. The DAC on channel 6, the busy flag, and the test registers.

## How it is modelled

The chip runs a 24-slot pipeline, one slot per internal clock, with an
internal clock every six input clocks. This crate steps that pipeline one
internal clock at a time. Register writes land on the slot the silicon would
land them on, and the `mol` and `mor` output pins change when the silicon's
do. `Variant::Ym2612` reproduces the NMOS chip's multiplexed 9-bit DAC with
its "ladder effect". `Variant::Ym3438` is the CMOS chip's clean output.

The model follows [Nuked-OPN2](https://github.com/nukeykt/Nuked-OPN2) by
Alexey Khokholov (Nuke.YKT), reverse-engineered from the YM3438 die. This
crate re-expresses that model in Rust; it does not bind to it. Nuked-OPN2 is
LGPL-2.1-or-later, which permits that under this crate's licence.
[ymfm](https://github.com/aaronsgiles/ymfm) by Aaron Giles is the second
reference.

## How it is tested

`tests/nuked_oracle.rs` runs twelve scripts on both variants. Each script
drives one area of the chip: algorithms, envelopes, SSG-EG, the LFO,
channel 3 and CSM, the timers and status, the DAC, detune and multiple,
key-on edge cases, and the test registers. The test demands the same output
pins as Nuked-OPN2, clock by clock, and the same value from every read.
`tests/oracle/regenerate.sh` rebuilds the expected output from Nuked-OPN2 at
a pinned commit. Nuked-OPN2 itself is not part of this repository.

## Using it

```rust
use emu198x_yamaha_ym2612::{Variant, Ym2612};

let mut chip = Ym2612::new(Variant::Ym2612);
chip.write(0, 0x28); // register $28, part I
chip.tick();         // one input clock: master clock / 7 on the Mega Drive
let samples = chip.take_samples(); // one [left, right] per 144 input clocks
```

The chip exposes its state as public fields and is stepped by the machine, so
it owns no bus and makes no callbacks, the same as every Emu198x chip.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulator cores. This crate is published so
siblings and outside projects can use the chip on its own. It is versioned
independently of the Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
