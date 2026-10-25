# emu198x-yamaha-ym2610

The Yamaha YM2610 (OPNB): the SNK Neo Geo's sound chip.

Four channels of four-operator FM, an SSG with three square-wave channels,
noise and an envelope, six channels of fixed-rate ADPCM-A, one channel of
variable-rate ADPCM-B, and the OPN family's two timers and their IRQ.

## How it is modelled

The YM2610's FM block is the OPN2's pipeline with channels 1 and 4 left
unconnected, so this crate composes
[`emu198x-yamaha-ym2612`](../emu198x-yamaha-ym2612) in its YM3438 form
rather than carrying a second FM core. Writes that would key the missing
channels on, or enable a DAC the YM2610 lacks, never reach it. One FM core
means one oracle covers both chips, and a fix to the pipeline reaches both.

The SSG is [`emu198x-gi-ay-3-8910`](../emu198x-gi-ay-3-8910), clocked at a
quarter of the input clock so that it yields one sample per FM sample.

ADPCM-A and ADPCM-B are re-expressed from
[ymfm](https://github.com/aaronsgiles/ymfm) by Aaron Giles (BSD-3-Clause).
They run once per FM sample: ADPCM-B on every sample, ADPCM-A on every
third. They read sample ROM through a borrowed slice passed to each tick.
The V-ROM bus belongs to the YM2610 alone, so a pin-level request would
always be answered at once with the byte the slice gives.

Each sample carries the 16-bit FM and ADPCM mix the chip sends to its YM3016
DAC, and the SSG's analogue level, which the board mixes in.

## How it is tested

`tests/ymfm_oracle.rs` runs nine scripts through the ADPCM units: single and
overlapping ADPCM-A channels, the level and pan arithmetic, the 20-bit end
compare, and ADPCM-B's rates, levels, repeat, stop and reset. It demands
the output of ymfm's own engines sample for sample.
`tests/oracle/regenerate.sh` rebuilds the expected output from ymfm at a
pinned commit; ymfm itself is not part of this repository. The FM block is
covered by the YM2612 crate's Nuked-OPN2 oracle, and the unit tests cover
the routing, the missing channels and DAC, the status ports and the clocks.

## Using it

```rust
use emu198x_yamaha_ym2610::Ym2610;

let rom = vec![0u8; 0x10000]; // the V-ROM
let mut chip = Ym2610::new();
chip.write(0, 0x10);    // ADPCM-B control
chip.write(1, 0x80);    // start
chip.tick(&rom, &rom);  // one input clock: 8 MHz on the Neo Geo
let samples = chip.take_samples(); // one per 144 input clocks
```

The chip exposes its state as public fields and is stepped by the machine, so
it owns no bus and makes no callbacks, the same as every Emu198x chip.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulator cores. This crate is published so
siblings and outside projects can use the chip on its own. It is versioned
independently of the Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later. The ADPCM units follow ymfm, Copyright (c) 2021
Aaron Giles, BSD-3-Clause.
