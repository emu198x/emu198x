---
category: implementation
module: motorola-68000
tags:
  - 68000
  - cycle-accurate
  - micro-ops
  - architecture
status: durable
resolved: 2026-02-03
---

# 68000 micro-op architecture

## Summary

The 68000 core uses a micro-op queue and explicit execution phases to model
cycle-accurate behavior without turning every instruction into a hand-written
monolith. This note is the durable reference for how the CPU breaks work into
fetch, effective-address, read, write, internal, and exception steps.

## Why It Matters

Most of the later 68000 implementation notes only make sense if this structure
stays stable. When a new instruction is added, the first question is usually
"which existing micro-op pattern does it fit?" not "what bespoke state machine
should I invent?"

## Approach

The core architecture has a few recurring pieces:

### Execution phases

Complex instructions move through explicit phases such as initial decode, source
EA calculation, source read, destination EA calculation, and final write. That
is what makes instructions like `MOVE`, `MOVEM`, and multi-step memory
operations readable instead of implicit.

### Micro-op queue

The queue is the cycle-level scheduler. It holds fetches, reads, writes,
pushes, pops, and internal work items, and the tick loop advances one micro-op
at a time.

### Extension words

Addressing modes consume extension words through shared storage and indexing
helpers rather than open-coded PC arithmetic in every instruction.

### Effective-address calculation

Addressing-mode helpers return either a resolved address or a register target,
with sign extension and index handling centralized in one place.

### Flag helpers

MOVE-style, arithmetic, compare, and shift operations each have dedicated flag
paths instead of one catch-all flag updater.

### Special CPU state

Some behaviors need explicit state fields:

- instruction phase tracking
- extension-word storage and indexing
- dual stack-pointer handling
- per-instruction scratch data for multi-step memory operations

## Edge Cases

- Absolute short addressing sign-extends through 32-bit address calculation.
- PC-relative and indexed modes must consume extension words in the same order
  as the real CPU.
- Queue capacity matters because long instructions can stack many fetch and
  memory operations.
- Temporary state reuse is fine, but only if instruction completion reliably
  clears it.

## Regression Coverage

This architecture is protected indirectly by the broad 68000 instruction suite:

- data movement
- arithmetic and logic
- shifts and rotates
- branches and exceptions
- effective-address-heavy instructions

If a new instruction needs a totally novel queue pattern, it should justify why
the existing architecture is insufficient.

## Related

- [68000 Memory Operand Support](68000-memory-operand-patterns.md)
- [68000 MOVEM Register Transfer](68000-movem-register-transfer.md)
- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md)
