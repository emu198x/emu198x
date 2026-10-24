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

## Discrete-TI zero-period transitions

`ti-zero-transitions.csv` records 60 unchanged full-XRoar cases: all three
tone channels, periods 5 and 15, and every internal-counter phase of each.
After synchronising on observed edges, each case writes zero, observes three
edges, then writes the original period and observes three more. The first
edge tests that writes retain the in-flight count; subsequent edges test the
reload. All 60 rows fail against the universal one-count clamp.

The same XRoar revision and real generator/filter sources listed above supply
the oracle. MAME independently maps discrete-TI zero to 1,024 and Sega zero to
the shortest interval. The TI datasheet does not explicitly specify zero;
these are implementation comparisons, not physical startup/write-latency
measurements. The existing Sega-zero and exhaustive nonzero controls remain.
Noise slaving is outside this fixture.

Fixture SHA-256: `7c698ad633c344cbfc412cc3a020cd02f4b78fb2298a3dc8d3e4cb7346be3c02`.

## Discrete-TI mode-3 noise transitions

`ti-noise-transitions.csv` contains 1,920 full-XRoar observations across 240
cases: periodic/white noise, starting periods 5/15, every internal-clock phase
of the complete tone cycle, and target periods 0/1/7. Each case synchronises on
an observed LFSR change, writes the target tone period and records eight further
shifts (interval, LFSR contents and tone output). Every baseline observation
fails at least one of those checks. The oracle uses the same unchanged XRoar
revision and generator/filter sources listed above.

The fixture tests discrete-TI mode 3 against actual tone edges; it makes no
claim about the independent fixed-noise clock, Sega slaving phase, physical
startup or sub-clock write latency. The TI datasheet page 3 names tone generator
3 output as the selected clock; the rising-edge convention is XRoar precedent.

Fixture SHA-256: `0834411d61d08b5e676883af9aaac1012d0276a5129a94bac53ca1a4a84a7a97`.
