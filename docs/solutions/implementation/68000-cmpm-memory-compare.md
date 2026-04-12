---
category: implementation
module: motorola-68000
tags:
  - 68000
  - cmpm
  - memory-compare
  - postincrement
status: durable
resolved: 2026-02-03
---

# 68000 CMPM memory compare

## Summary

`CMPM` compares two memory operands using postincrement addressing on both
address registers. The emulator implements it as a small two-phase micro-op that
reads the source, reads the destination, updates both address registers, and
sets compare flags without storing a result.

## Why It Matters

`CMPM` is the 68000's built-in buffer and string comparison primitive. It looks
simple at the mnemonic level, but it combines dual-address traversal, compare
flags, and address-register update rules in one instruction.

## Approach

The instruction is modeled as a dedicated compare micro-op with two phases:

1. read from `(Ay)+` and advance `Ay`
2. read from `(Ax)+`, advance `Ax`, then set flags as `dst - src`

That keeps the compare semantics aligned with the ordinary `CMP` helpers while
still respecting the instruction's memory traversal behavior.

## Edge Cases

- `CMPM` sets flags like `CMP`: `Z`, `N`, `V`, `C`, but does not store a value.
- Both address registers must advance by operand size after their respective
  reads.
- For byte operations, `A7` still follows the stack-alignment rule and advances
  by 2.
- The source and destination addresses are distinct; the order matters because
  flags reflect `destination - source`.

## Regression Coverage

Coverage should include byte, word, and long compares, unequal and equal cases,
negative results, and the special `A7` byte-increment rule.

## Related

- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md)
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
