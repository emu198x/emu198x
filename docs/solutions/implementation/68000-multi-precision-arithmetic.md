---
category: implementation
module: motorola-68000
tags:
  - 68000
  - multi-precision
  - extend-flag
  - addx
  - subx
status: durable
resolved: 2026-02-03
---

# 68000 multi-precision arithmetic

## Summary

`ADDX` and `SUBX` are the 68000's carry-chain instructions for arithmetic wider
than a single register. Their defining feature is the extend flag `X`, plus the
special zero-flag rule that lets a chain of operations report whether the whole
multi-word result is zero.

## Why It Matters

This is a durable pattern because it is easy to implement as "ADD with carry"
and still get the semantics wrong. The core traps are the distinction between
`X` and `C`, and the fact that `Z` is sticky across the chain.

## Approach

The core rule is:

- `ADDX`: `dst + src + X`
- `SUBX`: `dst - src - X`

The flag path then differs from ordinary `ADD` and `SUB`:

| Flag | Meaning in `ADDX`/`SUBX`                  |
| ---- | ----------------------------------------- |
| `X`  | carry or borrow for the next chained step |
| `C`  | mirrors `X` for these instructions        |
| `Z`  | clear on non-zero, unchanged on zero      |
| `N`  | follows result sign                       |
| `V`  | signed overflow                           |

That sticky-zero rule is what makes a high-word operation preserve the correct
"whole result is zero" meaning after a low-word operation has already run.

## Edge Cases

- `X` is not interchangeable with `C`; comparisons can change `C` without
  breaking the multi-precision carry chain.
- `SUBX` borrow detection must include the current extend flag.
- Memory predecrement forms still need the same carry-chain semantics as the
  register forms.
- The instruction is easy to test incorrectly if only one word of a larger
  result is examined.

## Regression Coverage

Coverage should include:

- `ADDX` with and without incoming `X`
- `SUBX` with borrow propagation
- sticky-zero behavior across chained operations
- representative long-width arithmetic and multi-word examples

## Related

- [68000 BCD Arithmetic](68000-bcd-arithmetic.md)
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
