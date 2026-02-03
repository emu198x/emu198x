# 6502 BRK Instruction Reads Interrupt Vector from Stale Address

---
title: "6502 BRK instruction reads interrupt vector from stale address"
tags:
  - cpu
  - 6502
  - emulation
  - brk
  - interrupt
  - vector
  - addressing-mode
  - state-corruption
category: logic-errors
module: emu-6502
symptoms:
  - "Klaus Dormann 6502 functional test fails at $37C9 (BRK handler status check)"
  - "BRK instruction reads interrupt vector from wrong memory location (e.g., $0200 instead of $FFFE/$FFFF)"
  - "Interrupt handler jumps to incorrect address after BRK"
  - "Tests pass for other instructions but fail specifically on BRK/interrupt handling"
root_cause: "BRK instruction implementation reused self.addr field which contained stale values from previous instruction's addressing mode resolution"
date_solved: 2026-02-03
---

## Problem Description

The BRK instruction in the 6502 emulator was reading the interrupt vector from an incorrect address. Instead of always reading from `$FFFE/$FFFF` for software BRK interrupts, the CPU would sometimes jump to random addresses, causing test failures in the Klaus Dormann 6502 functional test suite.

**Symptoms observed:**
- BRK instruction would jump to incorrect addresses (e.g., $000B instead of $37AB)
- Klaus Dormann test fails at $37C9 with wrong status byte comparison
- Inconsistent behavior depending on what instructions preceded BRK

## Investigation Steps

1. **Added instruction tracing** to see execution flow before failure
2. **Examined BRK vector reads** - discovered BRK was reading from $0200 instead of $FFFE
3. **Added debug output** to BRK cycle 5:
   ```
   BRK: self.addr=$0200, using vector=$0200, reading $0B
   ```
4. **Identified root cause** - `self.addr` contained stale value from previous LDA $0200 instruction

## Root Cause Analysis

The bug stemmed from how the BRK/NMI/IRQ interrupt handling shared code:

**Design intent:**
- `begin_nmi()` sets `self.addr = 0xFFFA` and skips to cycle 2
- `begin_irq()` sets `self.addr = 0xFFFE` and skips to cycle 2
- Software BRK should use `self.addr = 0` (defaulting to `$FFFE`)

**The problem:**
Software BRK starts at cycle 1, which only runs for the BRK opcode ($00) fetched from the instruction stream. However, **cycle 1 never cleared `self.addr`**, leaving it with whatever value the previous instruction's addressing mode had set.

For example, if the previous instruction used absolute addressing (`LDA $1234`), `self.addr` would still contain `$1234`. When cycle 5 checked:

```rust
let vector = if self.addr != 0 { self.addr } else { 0xFFFE };
```

It would incorrectly use `$1234` as the vector address instead of `$FFFE`.

## The Fix

**File:** `crates/emu-6502/src/cpu.rs`

### Targeted Fix (in BRK cycle 1)

```rust
fn op_brk<B: Bus>(&mut self, bus: &mut B) {
    match self.cycle {
        1 => {
            // Padding byte (ignored but PC incremented)
            // For software BRK, clear addr so cycle 5 uses $FFFE
            // (begin_nmi/begin_irq skip to cycle 2 with addr already set)
            self.addr = 0;
            let _ = bus.read(self.regs.pc);
            self.regs.pc = self.regs.pc.wrapping_add(1);
            self.cycle = 2;
        }
        // ... cycles 2-6 unchanged
    }
}
```

### Systemic Fix (in finish())

To prevent similar bugs, also clear all temporary state when finishing any instruction:

```rust
fn finish(&mut self) {
    self.state = State::FetchOpcode;
    self.cycle = 0;

    // Clear temporary state to prevent inter-instruction leakage.
    // This is the primary defense against stale state bugs.
    self.addr = 0;
    self.data = 0;
    self.pointer = 0;
}
```

**Key insight:** Cycle 1 only executes for software BRK (opcode $00 fetched from the instruction stream). Hardware interrupts (`begin_nmi`/`begin_irq`) skip directly to cycle 2 with `self.addr` already set to the correct vector address.

## Verification

**Test passed:** Klaus Dormann 6502 functional test

**Results:**
- **30,646,177 instructions** executed successfully
- **96,241,373 cycles** total
- Test completed at success trap address `$3469`
- All 6502 instructions and addressing modes verified correct

## Prevention Strategies

### 1. Clear Temporary State in finish()

The primary defense - ensures no state leaks between instructions:

```rust
fn finish(&mut self) {
    self.state = State::FetchOpcode;
    self.cycle = 0;
    self.addr = 0;
    self.data = 0;
    self.pointer = 0;
}
```

### 2. Regression Tests

Added 10 tests that verify BRK uses correct vector after various addressing modes:

- `test_brk_after_absolute_addressing`
- `test_brk_after_absolute_x_addressing`
- `test_brk_after_absolute_y_addressing`
- `test_brk_after_indirect_indexed`
- `test_brk_after_indexed_indirect`
- `test_brk_after_rmw_absolute`
- `test_brk_after_jmp_indirect`
- `test_brk_after_page_crossing_read`
- `test_brk_after_sta_absolute`
- `test_brk_after_jsr_rts_sequence`

### 3. Code Patterns to Avoid

- **Don't overload variable meanings** - If a field is used for two purposes, make it two fields
- **Don't assume clean initial state** - Always initialize before use
- **Avoid implicit state communication** - Use explicit parameters or separate fields

## Related Documentation

- [docs/architecture.md](../../architecture.md) - Interrupt timing model
- [docs/milestones.md](../../milestones.md) - M3 verification criteria (Klaus Dormann test)
- [docs/cpu-state-management.md](../../cpu-state-management.md) - Full prevention strategies

## External References

- [Klaus Dormann 6502 Test Suite](https://github.com/Klaus2m5/6502_65C02_functional_tests)
- [6502 BRK Behavior](https://www.nesdev.org/wiki/CPU_interrupts) - NESdev Wiki
- [Visual 6502](http://visual6502.org) - Silicon-level analysis
