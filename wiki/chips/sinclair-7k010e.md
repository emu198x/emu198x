# Sinclair 7K010E (128K / +2 ULA)

ULA used in the ZX Spectrum 128K and +2. Same contention model as the [Ferranti 6C001E](ferranti-6c001e.md) but different timing constants.

## Crate

`sinclair-ula-7k010e`

## Timing

| Parameter | Value |
|-----------|-------|
| Crystal | 17,734,475 Hz |
| CPU divisor | 5 |
| CPU clock | 3,546,895 Hz |
| AY divisor | 10 |
| AY clock | 1,773,448 Hz |
| Pixels per line | 456 |
| T-states per line | 228 |
| Lines per frame | 311 |
| T-states per frame | 70,908 |

## Contention

Pattern: `[6, 5, 4, 3, 2, 1, 0, 0]` (same as 48K), **phase 1** (different from 48K).
Contention start: T-state 14361.

- **Memory contention**: $4000-$7FFF always, $C000-$FFFF when odd bank (1, 3, 5, 7) paged
- **I/O contention**: yes, same 4 cases as 48K
- **Internal contention**: yes (IR on bus)
- **Floating bus**: returns ULA data bus during screen fetch, $FF during border
