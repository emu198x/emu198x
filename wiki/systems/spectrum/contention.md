# Spectrum Contention Model

The most complex timing aspect of the Spectrum. Three different ULA/gate array implementations with distinct contention behaviour. Getting this right was the driving reason for the [fresh start](../../decisions/fresh-start-rationale.md).

## How contention works

When the ULA needs to read VRAM, it withholds the CPU clock. The CPU freezes — no extra ticks, no catch-up. See [ULA-drives model](../../decisions/ula-drives-model.md).

## Memory contention

### 48K ([Ferranti 6C001E](../../chips/ferranti-6c001e.md))

- Pattern: `[6, 5, 4, 3, 2, 1, 0, 0]` repeating every 8 T-states
- Phase: 0
- Start: T-state 14335 or 14336 (early/late ULA drift)
- Contended range: $4000-$7FFF only

### 128K / +2 ([Sinclair 7K010E](../../chips/sinclair-7k010e.md))

- Pattern: `[6, 5, 4, 3, 2, 1, 0, 0]` (same as 48K)
- Phase: **1** (different from 48K)
- Start: T-state 14361
- Contended range: $4000-$7FFF always, $C000-$FFFF when odd bank (1, 3, 5, 7) paged

### +2A / +3 ([Amstrad 40077](../../chips/amstrad-40077.md))

- Pattern: `[1, 0, 7, 6, 5, 4, 3, 2]` (**completely different**)
- Phase: 0
- Start: T-state 14361
- Contended range: $4000-$7FFF always, $C000-$FFFF when banks 4-7 paged (NOT odd banks)

## I/O contention (48K / 128K only)

Per-T-state pattern, 4 cases:

| High byte contended? | A0 | Pattern |
|---------------------|-----|---------|
| No | 0 (even port) | N:1, C:3 |
| No | 1 (odd port) | N:4 |
| Yes | 0 (even port) | C:1, C:3 |
| Yes | 1 (odd port) | C:1, C:1, C:1, C:1 |

N = no contention delay. C = apply delay from the contention pattern.

**+2A/+3 has NO I/O contention** — the Amstrad gate array is MREQ-only.

## Internal operation contention

During internal CPU operations, the IR register is placed on the bus (MREQ not active).

- **48K / 128K**: contended if IR points to $4000-$7FFF (IR on bus, ULA sees address)
- **+2A / +3**: **no internal contention** (MREQ not active, gate array ignores address)

## Bus trait methods

The [Z80](../../chips/zilog-z80.md) bus interface exposes contention via these methods:

- `contend(addr)` — T1 of every memory M-cycle (MREQ active)
- `contend_no_mreq(addr)` — each T-state of internal ops (IR on bus, MREQ not active)
- `contend_io(port, t)` — each T-state of I/O M-cycle
- `m1_read(addr)` — T2 of M1 (opcode fetch, distinct from data read)
- `refresh(addr)` — T3 of M1 (IR on bus with RFSH, also contended)

## Sources

- https://sinclair.wiki.zxnet.co.uk/wiki/Contended_memory
- https://sinclair.wiki.zxnet.co.uk/wiki/Contended_I/O
- Chris Smith, *The ZX Spectrum ULA* (book)
- FUSE emulator contention tables (gold standard)
