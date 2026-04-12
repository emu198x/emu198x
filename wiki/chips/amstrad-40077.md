# Amstrad 40077 (+2A / +3 Gate Array)

Gate array used in the ZX Spectrum +2A, +2B, and +3. Replaces the ULA with significantly different contention behaviour.

## Crate

`amstrad-ula-40077`

## Timing

| Parameter | Value |
|-----------|-------|
| Crystal | 17,734,475 Hz |
| CPU divisor | 5 |
| CPU clock | 3,546,895 Hz |
| T-states per line | 228 |
| Lines per frame | 311 |
| T-states per frame | 70,908 |

Same crystal and frame timing as [Sinclair 7K010E](sinclair-7k010e.md), but contention is fundamentally different.

## Contention

Pattern: `[1, 0, 7, 6, 5, 4, 3, 2]` — **different from 48K/128K**.
Contention start: T-state 14361.

- **Memory contention**: $4000-$7FFF always, $C000-$FFFF when banks 4-7 paged (**not** odd banks — different from 128K)
- **I/O contention**: **none** (MREQ-only gate array)
- **Internal contention**: **none** (MREQ not active during internal ops)
- **Floating bus**: always $FF (Amstrad killed floating bus)

This is the most significant behavioural difference between Spectrum variants. Software relying on floating bus or I/O contention timing breaks on +2A/+3.
