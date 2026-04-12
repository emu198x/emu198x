---
category: implementation
module: motorola-68000
tags:
  - 68000
  - read-modify-write
  - micro-op
  - memory-destination
status: durable
resolved: 2026-02-03
---

# ALU memory read-modify-write

## Summary

Memory-destination `ADD`, `SUB`, `AND`, `OR`, and `EOR` all follow the same
68000 pattern: read the effective address, apply the operation with a register
source, then write the result back with correct timing and flags. The emulator
handles that with one reusable `AluMemRmw` micro-op instead of separate
instruction-specific handlers.

## Why It Matters

This pattern is worth keeping because the 68000 treats memory-destination ALU
operations as true read-modify-write sequences. If each instruction grows its
own bespoke path, timing, flag handling, and write-back behavior drift apart.

## Approach

The shared micro-op uses existing temporary fields rather than introducing a
separate state machine per instruction:

- `self.addr` holds the destination effective address
- `self.data` holds the source register value
- `self.data2` selects the ALU operation
- `self.size` carries byte, word, or long width
- `movem_long_phase` is reused as a two-phase sub-state

| Code | Operation |
| ---- | --------- |
| 0    | ADD       |
| 1    | SUB       |
| 2    | AND       |
| 3    | OR        |
| 4    | EOR       |

Each exec function resolves the effective address, stores the source register
value, sets the operation code, and queues `MicroOp::AluMemRmw`.

```rust
self.size = size;
self.addr = addr;
self.data = self.read_data_reg(reg, size);
self.data2 = 0; // ADD
self.movem_long_phase = 0;
self.micro_ops.push(MicroOp::AluMemRmw);
```

The tick handler then runs in two phases:

1. Read the existing memory value and stash it in scratch storage.
2. Compute the result, update flags, and write the result back.

That keeps the memory-destination path consistent across all five operations
while leaving register-destination variants on their fast direct path.

## Edge Cases

- Arithmetic operations update `X`, `N`, `Z`, `V`, and `C`.
- Logical operations update `N` and `Z`, clear `V` and `C`, and leave `X`
  untouched.
- Byte, word, and long variants must preserve the correct bus-width behavior.
- Register destinations should not go through the read-modify-write micro-op.

## Regression Coverage

Current targeted tests cover:

- `ADD.W` memory destination
- `ADD.W` with carry and overflow behavior
- `SUB.B` memory destination
- `AND.W` memory destination
- `OR.B` memory destination
- `EOR.W` memory destination

## Related

- [68000 Memory Operand Support](68000-memory-operand-patterns.md)
- [68000 TAS and Memory Shift Implementation](68000-tas-shift-memory.md)
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md)
