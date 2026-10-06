# SID combined-waveform OSC3 samples

Raw `.dat` files of OSC3 readings sampled from real SID chips, copied
byte for byte from reSID as vendored with VICE 3.10 (`src/resid/`,
mirrored at <https://github.com/VICE-Team/svn-mirror>).

| File | Model | Combination (control bits 6-4) |
|---|---|---|
| `wave6581__ST.dat` | 6581 | triangle + sawtooth (`$3`) |
| `wave6581_P_T.dat` | 6581 | pulse + triangle (`$5`) |
| `wave6581_PS_.dat` | 6581 | pulse + sawtooth (`$6`) |
| `wave6581_PST.dat` | 6581 | pulse + sawtooth + triangle (`$7`) |
| `wave8580__ST.dat` | 8580 | triangle + sawtooth (`$3`) |
| `wave8580_P_T.dat` | 8580 | pulse + triangle (`$5`) |
| `wave8580_PS_.dat` | 8580 | pulse + sawtooth (`$6`) |
| `wave8580_PST.dat` | 8580 | pulse + sawtooth + triangle (`$7`) |

Each file is 4096 bytes, one 8-bit OSC3 reading for each of the 4096
upper-12-bit accumulator positions. reSID took every sample with
FREQ = `$1000`, which steps the upper 12 bits by one each cycle, and
the pulse held on (reSID `wave.h`, "Combined waveforms"). The emulator
places each byte in the top eight bits of the 12-bit waveform DAC
input (`<< 4`), as reSID's `samp2src.pl` does.

## Which chips

reSID does not say which chip each table came from. Its `THANKS` file
credits Tibor Biczo, Andreas Boose and André Fachat with "combined
waveform samples for 6581 R1, R3, R4, and 8580 R5 SID chips", and its
`NEWS` adds 8580 combined waveforms in reSID 0.5. So the 8580 tables
come from an 8580 R5; the 6581 tables come from one or more of the R1,
R3 and R4 revisions. reSIDfp replaces sampled tables with a parametric
fit to newer samplings (6581 R2/R3, 8580 R5), which this crate does not
use.

## How the crate uses them

`../src/combined_wave_tables.rs` holds the 6581 tables as Rust literals
and reads the 8580 tables from these files at compile time. A unit test
in `../src/voice.rs` checks every entry of both sets against these
files.

## Licensing

reSID is © Dag Lem, distributed under GPL v2 or later. Emu198x adopted
GPL-2.0-or-later partly to allow direct reuse of this sampled data —
see the project-level `LICENSE` file.
