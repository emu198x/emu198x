# emu198x-yamaha-ym2413

The Yamaha YM2413 (OPLL) FM synthesis chip and Konami's VRC7 sound core.

Nine channels of two-operator FM, or six channels plus five rhythm voices
(bass drum, snare, tom, top cymbal and high hat), with 15 instruments in ROM
and one user-defined instrument. Key scaling, the AM and vibrato LFOs,
modulator feedback, half-wave rectification and the sustain/release
envelope types. It is the sound of the Japanese Master System's FM unit,
MSX-MUSIC and the FM-PAC cartridge. `Variant::Vrc7` is the DS1001 core on
Konami's VRC7 Famicom mapper: six channels and its own instrument ROM.

## How it is modelled

The chip runs an 18-slot pipeline, one slot per internal clock, with an
internal clock every four input clocks. This crate steps that pipeline one
internal clock at a time, so register writes land on the slot the silicon
would land them on, and the melody (`mo`) and rhythm (`ro`) output pins
change when the silicon's do.

The model follows [Nuked-OPLL](https://github.com/nukeykt/Nuked-OPLL) by
Nuke.YKT, reverse-engineered from the YM2413 and VRC7 die shots. This crate
re-expresses that model in Rust; it does not bind to it. Nuked-OPLL is
GPL-2.0-or-later, the same licence as this crate.

## How it is tested

`tests/nuked_oracle.rs` runs scripts on both variants covering every ROM
instrument, the user instrument, rhythm mode, envelopes and their types,
key scaling, the LFOs, feedback, rectification, the test register and
register-write timing. It demands Nuked-OPLL's output pins clock for clock.
`tests/oracle/regenerate.sh` rebuilds the expected output from Nuked-OPLL at a
pinned commit, which is never committed.
