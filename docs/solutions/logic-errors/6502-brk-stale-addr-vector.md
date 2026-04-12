---
category: logic-errors
module: mos-6502
tags:
  - 6502
  - brk
  - interrupt
  - state-corruption
status: archive-candidate
resolved: 2026-02-03
---

# 6502 BRK stale interrupt vector

## Summary

Software `BRK` was reusing stale temporary address state from the previous
instruction. Instead of reading the interrupt vector from `$FFFE/$FFFF`, it
could read from whatever address the prior addressing mode had left behind.

## Symptoms

- Klaus Dormann's functional test failed at the BRK handler status check
- `BRK` sometimes jumped to garbage addresses such as `$000B`
- behavior varied depending on which instruction ran immediately before `BRK`

## Root Cause

The interrupt path shared temporary state between hardware interrupts and
software `BRK`:

- `begin_nmi()` and `begin_irq()` correctly preloaded the vector address
- software `BRK` relied on the default path
- cycle 1 of `BRK` did not clear `self.addr`

That meant software `BRK` could inherit a stale absolute or indirect address
from the previous instruction. Later cycles then treated that stale value as the
interrupt vector base.

## Fix

The fix had two layers:

1. Clear `self.addr` in software `BRK` cycle 1 so the vector logic falls back
   to `$FFFE`.
2. Clear all temporary per-instruction state in `finish()` so values cannot leak
   between instructions.

```rust
fn finish(&mut self) {
    self.state = State::FetchOpcode;
    self.cycle = 0;
    self.addr = 0;
    self.data = 0;
    self.pointer = 0;
}
```

## Why It Can Recur

This note is still active because the failure mode is broader than `BRK`:

- temporary state can leak across instructions in cycle-accurate cores
- shared interrupt and opcode paths encourage implicit state coupling
- stale addressing data is easy to miss until a test crosses instruction
  boundaries in the wrong order

It is marked `archive-candidate` because the underlying lesson should
eventually live in a more general CPU-state hygiene note.

## Regression Coverage

Keep both of these checks:

- the full Klaus Dormann functional test run
- targeted regression tests that execute `BRK` after a range of addressing modes

The original fix added coverage after absolute, indexed, indirect, and
read-modify-write instruction paths.

## Related

- [6502 Illegal Opcode Implementation](../implementation/6502-illegal-opcodes.md)
- [6502 trap detection at instruction boundaries](../testing/6502-trap-detection-instruction-boundary.md)
- [docs/inventory.md](../../inventory.md)
