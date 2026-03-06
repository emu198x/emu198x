---
category: implementation
module: motorola-68000
tags:
  - 68000
  - bcd
  - arithmetic
  - packed-decimal
status: durable
resolved: 2026-02-03
---

# 68000 BCD arithmetic

## Summary

This note covers the 68000 packed-BCD instructions `ABCD`, `SBCD`, and `NBCD`.
They are the decimal-arithmetic counterpart to the binary arithmetic core and
matter because they rely on nibble correction, extend-flag chaining, and
non-standard zero-flag behavior.

## Why It Matters

BCD instructions are small but easy to get subtly wrong. They are built for
multi-byte decimal math, so the correctness bar is not just "one byte adds
properly" but "carry and zero propagate across a chain the way real 68000 code
expects."

## Approach

The implementation keeps the decimal adjustment logic explicit:

| Instruction | Operation       | Notes                           |
| ----------- | --------------- | ------------------------------- |
| `ABCD`      | `dst + src + X` | Decimal add with extend         |
| `SBCD`      | `dst - src - X` | Decimal subtract with extend    |
| `NBCD`      | `0 - ea - X`    | Ten's-complement style negation |

The arithmetic runs per nibble, not as a plain binary add/subtract:

- low nibble is processed first
- invalid decimal values are corrected
- carry or borrow propagates into the high nibble
- the final result is repacked into BCD form

That same structure makes the multi-byte register path work naturally with the
extend flag.

## Edge Cases

- `Z` is sticky for BCD chains: clear it on non-zero results, leave it alone on
  zero results.
- `C` and `X` mirror each other for decimal carry and borrow.
- `N` and `V` are not the main semantic signal here, so consistency matters more
  than pretending they work like binary arithmetic.
- Register mode is implemented; the memory predecrement forms remain the
  follow-on work item.

## Regression Coverage

Current coverage includes decimal add, subtract, negation, carry and borrow
generation, low-nibble correction, extend propagation, and the sticky-zero
behavior that makes multi-byte decimal chains work.

## Related

- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md)
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
