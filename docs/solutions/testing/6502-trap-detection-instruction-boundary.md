---
category: testing
module: mos-6502
tags:
  - 6502
  - test-harness
  - trap-detection
  - instruction-boundary
status: durable
resolved: 2026-02-03
---

# 6502 trap detection at instruction boundaries

## Summary

Trap detection for a cycle-accurate 6502 harness must sample `pc` at
instruction boundaries, not on every tick. That rule fixed the Klaus Dormann
decimal-mode test harness, which was missing a self-jump trap even though the
CPU was behaving correctly.

## Problem

The Dormann test ends in a self-jump trap at `JMP $024B`. A tick-by-tick
harness never saw the same `pc` twice in a row because the 6502 walks operand
bytes during instruction fetch:

| Cycle | PC value | Action                     |
| ----- | -------- | -------------------------- |
| 1     | `$024B`  | Fetch opcode `4C`          |
| 2     | `$024C`  | Fetch low byte of address  |
| 3     | `$024D`  | Fetch high byte of address |
| next  | `$024B`  | Jump to target             |

That means "same PC on consecutive ticks" is the wrong trap heuristic for a
cycle-level CPU model.

## Setup

This workflow assumes:

- a cycle-accurate 6502 core
- an `is_instruction_complete()` query
- a harness that can tick until the current instruction finishes
- a known trap address or self-jump condition to watch for

## Workflow

1. Record `start_pc` only when the CPU is ready to start an instruction.
2. Compare that start address with the previous instruction's start address.
3. Execute the full instruction before sampling `pc` again.

```rust
let start_pc = cpu.pc();

if start_pc == prev_pc {
    same_pc_count += 1;
} else {
    same_pc_count = 0;
    prev_pc = start_pc;
}

cpu.tick(&mut bus);
while !cpu.is_instruction_complete() {
    cpu.tick(&mut bus);
}
```

In practice, a small threshold like two or three repeated instruction starts is
enough to identify a trap loop.

## Failure Signals

If the harness is still wrong, the usual symptoms are:

- tests running far past the expected instruction budget
- `pc` appearing to cycle through operand bytes instead of stabilizing
- a known self-jump trap never being reported

## Regression Coverage

Keep both of these in place:

- the Klaus Dormann decimal-mode test run
- a targeted unit test that proves self-jump detection only works when sampled
  at instruction boundaries

## Related

- [6502 Decimal Test Setup](6502-decimal-test-setup.md)
- [6502 BRK stale interrupt vector](../logic-errors/6502-brk-stale-addr-vector.md)
- [docs/inventory.md](../../inventory.md)
