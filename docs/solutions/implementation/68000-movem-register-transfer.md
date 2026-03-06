---
category: implementation
module: motorola-68000
tags:
  - 68000
  - movem
  - micro-op
  - addressing-modes
status: durable
resolved: 2026-02-03
---

# 68000 MOVEM register transfer

## Summary

`MOVEM` moves multiple registers to or from memory using a bitmask. The 68000
core implements it with self-chaining micro-ops that process one register at a
time while preserving mode-specific ordering and address-register update rules.

## Why It Matters

`MOVEM` is one of the easiest places to accidentally fake behavior with a bulk
copy helper. That would miss predecrement mask reversal, address update timing,
and the sign-extension rule for word loads into address registers.

## Approach

The instruction is modeled as iterative queue work, not a one-shot transfer:

1. decode the register mask and addressing mode
2. compute the starting address
3. find the first register selected by the mask
4. queue a read or write micro-op
5. after each transfer, locate the next register and continue until done

That lets the same instruction body cover:

- register-to-memory
- memory-to-register
- word and long sizes
- predecrement and postincrement behavior

## Edge Cases

- Predecrement mode reverses the logical meaning of the mask bits.
- Word loads into address registers must be sign-extended.
- Address registers should update after the full transfer, not after each
  logical register in the high-level model.
- Empty masks need defined behavior rather than accidental queue underflow.

## Regression Coverage

Coverage should include:

- word and long transfers in both directions
- predecrement mask reversal
- postincrement address updates
- address-register sign extension on word loads
- empty-mask behavior

## Related

- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md)
