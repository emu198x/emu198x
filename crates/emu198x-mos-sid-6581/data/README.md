# SID 6581 combined-waveform ROM tables

Raw `.dat` files sampled from real MOS 6581 SID chips, copied verbatim
from [reSID](https://github.com/VICE-Team/svn-mirror) / VICE 3.10
(`src/resid/wave6581_*.dat`).

The canonical compiled form used by the emulator lives in
`../src/combined_wave_tables.rs`, generated from the corresponding
`wave6581_*.h` files (also in reSID).

- `wave6581__ST.dat` — triangle + sawtooth combined
- `wave6581_P_T.dat` — pulse + triangle combined
- `wave6581_PS_.dat` — pulse + sawtooth combined
- `wave6581_PST.dat` — pulse + sawtooth + triangle combined

Each is 4096 bytes; each byte represents the 8-bit DAC output value
for one of the 4096 possible upper-12-bit accumulator positions.

## Licensing

reSID is © Dag Lem, distributed under GPL v2 or later. Emu198x adopted
GPL-2.0-or-later partly to allow direct reuse of this sampled data —
see the project-level `LICENSE` file.
