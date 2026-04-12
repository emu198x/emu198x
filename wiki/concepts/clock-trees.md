# Clock Trees

Every system starts with a master crystal oscillator. All other clocks derive from it by integer division. The emulator mirrors this: one master counter (`hc`), everything else derives.

## The principle

The master oscillator drives the loop. Not the CPU. Not the video chip. The crystal. If you find yourself writing `for _ in 0..tstates_per_frame`, stop — you're wrong.

See [ULA-drives model](../decisions/ula-drives-model.md) for how this works in practice.

## Verified clock trees

### ZX Spectrum

| Variant | Crystal (Hz) | CPU ÷ | CPU (Hz) | AY ÷ | AY (Hz) | T/line | Lines | T/frame |
|---------|-------------|-------|----------|------|---------|--------|-------|---------|
| 48K | 14,000,000 | 4 | 3,500,000 | — | — | 224 | 312 | 69,888 |
| 128K/+2 | 17,734,475 | 5 | 3,546,895 | 10 | 1,773,448 | 228 | 311 | 70,908 |
| +2A/+3 | 17,734,475 | 5 | 3,546,895 | 10 | 1,773,448 | 228 | 311 | 70,908 |
| TS2068 | 14,112,000 | 4 | 3,528,000 | — | — | — | — | — |
| Pentagon | 14,336,000 | 4 | 3,584,000 | — | — | — | — | — |

### Other systems (to be added)

Clock trees for C64, Amiga, NES, etc. will be added here as each system is implemented. The principle is the same — one crystal, integer divisors, no floating-point clock ratios.
