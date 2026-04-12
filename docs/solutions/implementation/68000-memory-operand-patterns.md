---
category: implementation
module: motorola-68000
tags:
  - 68000
  - memory-operands
  - bit-operations
  - quick-arithmetic
status: durable
resolved: 2026-02-04
---

# 68000 memory operand support patterns

## Summary

This note captures the reusable patterns for instructions that already worked on
register operands but needed real memory support. The three main groups are
bit-manipulation on memory, quick arithmetic with memory destinations, and
instructions that read a source operand from memory before doing internal work.

## Why It Matters

This is the kind of emulator work that becomes repetitive and buggy if each
instruction grows a one-off memory path. The value here is identifying which
existing micro-op can be reused, and where a new one is justified.

## Approach

The normalization falls into three patterns:

| Pattern     | Used by                        | Core idea                         |
| ----------- | ------------------------------ | --------------------------------- |
| `BitMemOp`  | `BTST`, `BCHG`, `BCLR`, `BSET` | byte read or byte RMW on memory   |
| `AluMemRmw` | `ADDQ`, `SUBQ` to memory       | reuse the generic memory ALU path |
| `AluMemSrc` | `CHK`, `MUL*`, `DIV*`          | read source, then do internal op  |

The main decision rule is simple:

- if the instruction mutates memory, prefer an RMW-shaped micro-op
- if it only needs a source value from memory, extend the source-read path
- if memory semantics differ materially from register semantics, make the split
  explicit instead of hiding it in one exec function

## Edge Cases

- Memory bit operations use bit number mod 8, while register forms use mod 32.
- `BTST` on memory is read-only; `BCHG`, `BCLR`, and `BSET` are true RMW paths.
- `ADDQ` and `SUBQ` use an immediate value as the logical source even when the
  destination is memory.
- `CHK`, `DIVU`, `DIVS`, `MULU`, and `MULS` inherit divide-by-zero, overflow,
  and exception behavior after the source read.
- In fixed-tick tests, a trailing `NOP` after the instruction under test keeps
  post-instruction garbage fetches from polluting the assertion.

## Regression Coverage

Coverage should include:

- bit test and bit update on memory bytes
- quick add and subtract against memory destinations
- memory-source `CHK`, multiply, and divide paths
- byte and word result verification through bus reads
- negative and exceptional cases for divide and bounds checking

## Related

- [68000 ALU memory read-modify-write](68000-alu-memory-rmw.md)
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
- [68000 TAS and Memory Shift Implementation](68000-tas-shift-memory.md)
