# SN76489 tone intervals

`tone-intervals.csv` contains 3,069 unchanged full-XRoar results: three tone
channels and every nonzero ten-bit period. Each row records two successive
output-edge intervals in input clocks, after the first observed transition.
The permanent regression repeats each row for both existing chip variants.

XRoar commit: `b8491ab945d3e83fc1e98d62359813cf8a537d1d`.
The wrapper includes `src/sn76489.c` and links its real `src/filter.c`; only
allocation support is supplied locally. No tone/noise logic is copied or
replaced. The TI SN76489AN datasheet, PDF page 2, independently specifies
16 times the nonzero period between output toggles.

The fixture does not qualify power-on phase, noise clocks or period zero.
[Reproduction, hashes and baseline/corrected traces](https://github.com/emu198x/docs/blob/ea3a135/plans/2026-10-11-sn76489-tone-timing.md)
are retained with the correction plan. Fixture SHA-256:
`349dfecbfd932667dc4c3c47ed35601547dcb3156d49185159e012f694f5031b`.
