# emu198x-sony-s-dsp

The Sony S-DSP, the Super Nintendo's sound generator: eight voices playing
BRR samples with Gaussian interpolation, ADSR and GAIN envelopes, noise,
pitch modulation, and an echo unit with an eight-tap FIR filter, mixed to
32 kHz stereo.

The host calls `tick()` once per SPC700 cycle; 32 ticks make a sample, and
each tick does the work the hardware does on that step, so register writes
land when the hardware would see them. The DSP owns the 64 KB audio RAM; the
host routes the SPC700's accesses to it.

Checked step for step against [ares](https://github.com/ares-emulator/ares)'s
DSP, whose logic it follows (ISC licence, © ares team, Near et al): a harness
in `tests/oracle/` compiles ares's own source against scripted cases.
