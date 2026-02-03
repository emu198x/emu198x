# 6502 Test Harness Trap Detection at Instruction Boundaries

---
title: "6502 decimal mode test harness fails to detect trap loops"
tags:
  - test-harness
  - 6502
  - trap-detection
  - decimal-mode
  - instruction-boundary
category: testing
module: emu-6502
symptoms:
  - "Test runs indefinitely past 50M+ instructions without completion"
  - "Trap detection never triggers despite PC looping at $024B-$024D"
  - "Same PC condition never satisfied during multi-byte instruction execution"
root_cause: "Trap detection checked PC equality on every tick instead of at instruction boundaries, missing self-jump traps because PC cycles through multiple values during JMP fetch"
date_solved: 2026-02-03
---

## Problem Description

The Klaus Dormann decimal mode test was running forever without detecting completion. The test trap is a `JMP $024B` instruction at address `$024B` (a self-jump infinite loop). Despite the CPU being stuck in this loop, the test harness never detected it.

**Symptoms observed:**
- Test exceeded 50M instruction limit without trapping
- Progress showed PC cycling through `$024B` → `$024C` → `$024D` → `$024B`
- The "same PC" detection never fired

## Root Cause Analysis

The bug was in the test harness, not the CPU implementation.

### The Wrong Approach

The broken implementation checked PC on every single clock tick:

```rust
loop {
    let pc = cpu.pc();

    if pc == prev_pc {
        same_pc_count += 1;
        if same_pc_count > 10 {
            // Trap detected
        }
    } else {
        same_pc_count = 0;
        prev_pc = pc;
        instructions += 1;
    }

    cpu.tick(&mut bus);  // Single tick, PC changes mid-instruction
}
```

**Why this fails:** The 6502 is a cycle-accurate processor where the Program Counter (PC) changes multiple times during a single instruction's execution. For a 3-byte instruction like `JMP $024B` (opcode `4C 4B 02`), the PC progresses through these values:

| Cycle | PC Value | Action |
|-------|----------|--------|
| 1 | `$024B` | Fetch opcode `4C` |
| 2 | `$024C` | Fetch low byte of address |
| 3 | `$024D` | Fetch high byte of address |
| After | `$024B` | Jump to target |

When checking on every tick, the harness sees the PC constantly changing. It never sees "same PC" because the PC is different on consecutive ticks, even though the CPU is stuck in an infinite loop.

## The Fix

Check PC only at instruction boundaries:

**File:** `crates/emu-6502/tests/dormann.rs`

```rust
loop {
    let start_pc = cpu.pc();

    // Check trap at instruction boundaries only
    if start_pc == prev_pc {
        same_pc_count += 1;
        if same_pc_count > 2 {
            // Trap detected - PC stayed same across complete instructions
            return check_result();
        }
    } else {
        same_pc_count = 0;
        prev_pc = start_pc;
    }

    // Execute complete instruction
    cpu.tick(&mut bus);
    cycles += 1;

    while !cpu.is_instruction_complete() {
        cpu.tick(&mut bus);
        cycles += 1;
    }

    instructions += 1;
}
```

### Why the Fix Works

1. **Captures PC before instruction starts:** `start_pc` records where the CPU is about to execute from.

2. **Runs the complete instruction:** The inner `while` loop continues ticking until `is_instruction_complete()` returns true.

3. **Compares instruction start addresses:** On the next iteration, `start_pc` will again be `$024B` if we're in a self-jump trap. Now `start_pc == prev_pc` is true.

4. **Uses a small threshold:** Seeing the same PC for 2-3 consecutive instructions reliably indicates a trap.

## Verification

**Test passed:**
- Klaus Dormann decimal mode test: **14,464,190 instructions**, **46,089,514 cycles**
- Error flag at `$0B` = `$00` (success)

## Prevention Strategies

### 1. API Design: Provide `is_instruction_complete()` Method

The 6502 already provides this:

```rust
/// Returns true if the previous instruction has completed and the CPU
/// is ready to fetch the next opcode.
#[must_use]
pub fn is_instruction_complete(&self) -> bool {
    self.state == State::FetchOpcode
}
```

### 2. State Validity Rules

| State Query | Valid When | Invalid When |
|-------------|------------|--------------|
| `pc` | `is_instruction_complete() == true` | Mid-instruction (points to operand bytes) |
| `registers` | `is_instruction_complete() == true` | May be mid-update during RMW ops |
| `flags` | `is_instruction_complete() == true` | May not reflect current operation |

### 3. Correct Code Patterns

**Trap detection:**
```rust
// CORRECT: Check at instruction boundaries
if cpu.is_instruction_complete() && cpu.pc() == prev_pc {
    // Trap detected
}
```

**Breakpoint detection:**
```rust
// CORRECT: Only fire at instruction start
if cpu.is_instruction_complete() && cpu.pc() == breakpoint_addr {
    // Breakpoint hit
}
```

**Step-by-instruction:**
```rust
// CORRECT: Run complete instruction
cpu.tick(&mut bus);
while !cpu.is_instruction_complete() {
    cpu.tick(&mut bus);
}
// NOW safe to inspect state
```

### 4. Test Cases to Catch This Bug

```rust
#[test]
fn trap_detection_requires_instruction_boundaries() {
    let mut cpu = Mos6502::new();
    let mut bus = SimpleBus::new();

    // JMP $0200 at $0200 (infinite loop)
    bus.load(0x0200, &[0x4C, 0x00, 0x02]);
    cpu.regs.pc = 0x0200;

    let mut prev_pc = 0xFFFF;
    let mut trap_detected = false;

    for _ in 0..50 {
        if cpu.is_instruction_complete() {
            let pc = cpu.pc();
            if pc == prev_pc {
                trap_detected = true;
                break;
            }
            prev_pc = pc;
        }
        cpu.tick(&mut bus);
    }

    assert!(trap_detected, "Should detect trap at $0200");
}
```

## Key Insight

**Trap detection is an instruction-level concept, not a cycle-level concept.**

A self-jump trap like `JMP *` (jump to self) will have the same starting PC for every instruction execution. By waiting for instruction completion before checking, we see the pattern: `$024B` → `$024B` → `$024B` → trap detected.

This aligns with the project's principle of **crystal-accurate timing** while enabling practical test harness functionality. The CPU ticks at cycle granularity (as required), but the test harness logic operates at instruction boundaries where trap detection makes semantic sense.

## Related Documentation

### Internal
- [6502 Decimal Test Setup](./6502-decimal-test-setup.md) - Test binary assembly and configuration
- [6502 BRK Stale Address Bug](../logic-errors/6502-brk-stale-addr-vector.md) - Related state management issue
- [docs/architecture.md](../../architecture.md) - Crystal-accurate timing model

### External
- [Klaus Dormann 6502 Test Suite](https://github.com/Klaus2m5/6502_65C02_functional_tests)
- [6502 Decimal Mode Tutorial](http://www.6502.org/tutorials/decimal_mode.html)
