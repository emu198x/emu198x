---
category: implementation
module: motorola-68000
tags:
  - 68000
  - condition-codes
  - scc
  - chk
status: durable
resolved: 2026-02-03
---

# 68000 condition-code instructions

## Summary

This note covers the 68000 condition-code instructions `Scc` and `CHK`.
`Scc` turns the current status register into a byte-sized boolean result, while
`CHK` performs signed bounds checking and raises exception vector 6 when the
register is outside the allowed range.

## Why It Matters

These instructions expose the 68000's condition-code model directly. If the
shared condition helper is wrong, both boolean result instructions and
signed-comparison logic drift. `CHK` is especially useful because it mixes flag
state, signed interpretation, and exception behavior in one place.

## Approach

The implementation revolves around one shared condition evaluator over the
status register:

| Condition family | Meaning source      |
| ---------------- | ------------------- |
| unsigned         | `C` and `Z`         |
| signed           | `N xor V` plus `Z`  |
| direct flag      | `N`, `Z`, `V`, `C`  |
| constants        | always true / false |

`Scc` uses that helper to write either `$FF` or `$00`:

- register destinations update only the low byte of `Dn`
- memory destinations perform a byte write through normal EA handling
- register timing differs between true and false cases

`CHK` treats `Dn` as signed and compares it against zero and an upper bound:

- negative values set `N` and trap
- values above the upper bound clear `N` and trap
- values in range complete without exception

## Edge Cases

- Signed conditions rely on `N xor V`, not just `N`.
- `Scc` register destinations preserve the upper 24 bits of the data register.
- `CHK` is exception-oriented, so success timing and trap behavior need to stay
  separate.
- Memory and immediate upper-bound sources still need to pass through the same
  effective-address pipeline as the rest of the core.

## Regression Coverage

Useful coverage includes:

- `SEQ` true and false cases
- representative signed and unsigned condition checks
- `CHK` negative-value trap
- `CHK` upper-bound trap
- in-range `CHK` completion without exception

## Related

- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
- [68000 Group 0 immediate decode](../archive/68000-group0-immediate-decode.md)
