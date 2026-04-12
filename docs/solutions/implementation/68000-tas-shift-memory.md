---
category: implementation
module: motorola-68000
tags:
  - 68000
  - tas
  - shift
  - rotate
  - read-modify-write
status: durable
resolved: 2026-02-03
---

# 68000 TAS and memory shift/rotate

## Summary

`TAS` and the memory forms of the shift and rotate instructions are all atomic
read-modify-write operations. The emulator handles them with single micro-ops
that run internal phases for read and write rather than pretending they are two
independent instructions.

## Why It Matters

These instructions are small but timing-sensitive. If the implementation splits
them into unrelated read and write operations, the model loses the real 68000
RMW shape and the flag behavior becomes harder to keep correct.

## Approach

Two dedicated micro-ops cover the memory-side behavior:

| Micro-op          | Used by                       | Shape                                 |
| ----------------- | ----------------------------- | ------------------------------------- |
| `TasExecute`      | `TAS <ea>`                    | read byte, set flags, write bit 7 set |
| `ShiftMemExecute` | memory shift and rotate forms | read word, shift once, write result   |

Both use an internal phase field to move from read to write while staying one
logical queued operation.

## Edge Cases

- `TAS` sets `N` and `Z` from the original byte, not the written-back value.
- Memory shift and rotate forms are word-only and always shift by exactly one.
- `X` tracks carry for shift families, but not for pure rotate families.
- `V` is cleared for these memory forms.

## Regression Coverage

Coverage should include:

- `TAS` on register and memory operands
- original-value flag behavior for `TAS`
- arithmetic, logical, and rotate memory forms
- carry and extend handling for shift families
- read-modify-write verification through the memory bus

## Related

- [68000 ALU memory read-modify-write](68000-alu-memory-rmw.md)
- [68000 Memory Operand Support](68000-memory-operand-patterns.md)
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
