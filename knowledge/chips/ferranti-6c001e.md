# Ferranti 6C001E (48K ULA)

Custom ULA used in the ZX Spectrum 48K (Issue 2, Issue 3). Handles video generation, keyboard scanning, border colour, beeper, tape I/O, and memory contention. See [Spectrum overview](../systems/spectrum/overview.md).

## Crate

`ferranti-ula-6c001e`

## Timing

| Parameter | Value |
|-----------|-------|
| Crystal | 14,000,000 Hz |
| CPU divisor | 4 |
| CPU clock | 3,500,000 Hz |
| Pixels per line | 448 |
| T-states per line | 224 |
| Lines per frame | 312 |
| T-states per frame | 69,888 |

## Contention

Pattern: `[6, 5, 4, 3, 2, 1, 0, 0]` repeating every 8 T-states, phase 0.
Contention start: T-state 14335 (early) or 14336 (late — ULA drift).

- **Memory contention**: $4000-$7FFF only
- **I/O contention**: yes, 4 cases — see [contention](../systems/spectrum/contention.md)
- **Internal contention**: yes (IR register on bus during internal ops)
- **Floating bus**: returns ULA data bus during screen fetch, $FF during border

## Rendering

Outputs palette-indexed `u8` values to a framebuffer. RGBA conversion is a separate stage. See [half-cycle signals](../decisions/half-cycle-signals.md).

The ULA ticks every half-cycle and gates the CPU clock via `cpu_clock_active()`. Contention is implicit — when the ULA withholds the clock, the CPU freezes.
