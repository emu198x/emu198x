---
title: "M68000 Multi-Precision Arithmetic with Extend Flag"
category: implementation
tags:
  - m68000
  - cpu-emulation
  - multi-precision
  - extend-flag
  - addx
  - subx
  - flag-calculation
  - rust
module: emu-68000
symptoms:
  - implementing multi-precision arithmetic (64-bit, 128-bit operations on 32-bit CPU)
  - ADDX/SUBX Z flag behavior differs from ADD/SUB
  - understanding X (extend) flag vs C (carry) flag
  - chaining arithmetic operations for large integers
severity: info
date: 2026-02-03
---

# M68000 Multi-Precision Arithmetic with Extend Flag

This documents the implementation of ADDX/SUBX instructions in the `emu-68000` crate, which enable multi-precision arithmetic on the 68000.

## Overview

The 68000's ADDX and SUBX instructions are specifically designed for multi-precision arithmetic - performing operations on numbers larger than 32 bits by chaining multiple operations. The key insight is the **X (Extend) flag**, which propagates carry/borrow between operations.

## The Extend Flag (X)

The X flag is bit 4 of the status register and behaves differently from the C (Carry) flag:

| Flag | Purpose | Set By |
|------|---------|--------|
| C | Carry/borrow for most operations | ADD, SUB, CMP, shifts, etc. |
| X | Carry for multi-precision chains | Only ADD, ADDX, SUB, SUBX, NEGX, shifts |

The crucial distinction: **C is set by comparisons and branches, X is not**. This allows you to compare intermediate results without destroying the carry chain.

## ADDX/SUBX vs ADD/SUB

### Operation

```rust
// ADD: result = dst + src
// ADDX: result = dst + src + X

let x = if self.regs.sr & X != 0 { 1u32 } else { 0 };
let result = dst.wrapping_add(src).wrapping_add(x);
```

### Critical Z Flag Difference

**ADD/SUB:** Z is set if result is zero, cleared if non-zero.

**ADDX/SUBX:** Z is **cleared if result is non-zero**, but **unchanged if result is zero**.

This behavior is essential for multi-precision arithmetic:

```rust
// ADDX Z flag behavior
if result_masked != 0 {
    sr &= !Z;  // Clear Z only if non-zero
}
// If zero, Z retains its previous value
```

Why? When adding two 64-bit numbers as two 32-bit operations:
1. Add low words first (ADDX with X initially 0)
2. Add high words (ADDX includes carry from low words)

If the low result is 0 but the high result is non-zero, the overall result is non-zero. By only *clearing* Z on non-zero results, the final Z flag correctly indicates whether the entire multi-precision result is zero.

## Implementation

### ADDX Register-to-Register

```rust
fn exec_addx(&mut self, op: u16) {
    let size = Size::from_bits(((op >> 6) & 3) as u8);
    let rx = (op & 7) as usize;        // Source register
    let ry = ((op >> 9) & 7) as usize; // Destination register
    let rm = op & 0x0008 != 0;         // Memory mode flag

    if rm {
        // Memory to memory: -(Ax),-(Ay) - pre-decrement
        // (implementation omitted for brevity)
    } else {
        // Register to register: Dx,Dy
        let src = self.read_data_reg(rx as u8, size);
        let dst = self.read_data_reg(ry as u8, size);
        let x = if self.regs.sr & X != 0 { 1u32 } else { 0 };

        let result = dst.wrapping_add(src).wrapping_add(x);
        self.write_data_reg(ry as u8, result, size);

        // Set flags (note Z flag special handling)
        self.set_flags_addx(src, dst, result, size);
    }
}
```

### SUBX Register-to-Register

```rust
fn exec_subx(&mut self, op: u16) {
    // Similar structure to ADDX
    let result = dst.wrapping_sub(src).wrapping_sub(x);

    // Borrow detection is more complex than carry
    let carry = (src_masked + x) > dst_masked
        || (src_masked == dst_masked && x != 0);
}
```

### Flag Calculation

```rust
fn set_flags_addx(&mut self, src: u32, dst: u32, result: u32, size: Size) {
    let (src_masked, dst_masked, result_masked, msb) = match size {
        Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80u32),
        Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
        Size::Long => (src, dst, result, 0x8000_0000),
    };

    let mut sr = self.regs.sr;

    // N: set if result is negative
    sr = Status::set_if(sr, N, result_masked & msb != 0);

    // Z: CRITICAL - cleared if non-zero, UNCHANGED if zero
    if result_masked != 0 {
        sr &= !Z;
    }

    // V: overflow (same signs in, different sign out)
    let overflow = (!(src_masked ^ dst_masked) & (src_masked ^ result_masked) & msb) != 0;
    sr = Status::set_if(sr, V, overflow);

    // C and X: carry out
    let carry = match size {
        Size::Byte => (u16::from(src as u8) + u16::from(dst as u8) + u16::from(x as u8)) > 0xFF,
        Size::Word => (u32::from(src as u16) + u32::from(dst as u16) + x) > 0xFFFF,
        Size::Long => src.checked_add(dst).and_then(|v| v.checked_add(x)).is_none(),
    };
    sr = Status::set_if(sr, C, carry);
    sr = Status::set_if(sr, X, carry);  // X copies C for ADDX/SUBX

    self.regs.sr = sr;
}
```

## Usage Example: 64-bit Addition

Adding two 64-bit numbers stored in D0:D1 and D2:D3:

```asm
    ; Add low longs (D1 + D3 -> D1)
    ADD.L   D3,D1       ; Sets C and X flags

    ; Add high longs with carry (D0 + D2 + X -> D0)
    ADDX.L  D2,D0       ; Includes carry from low addition

    ; Result: D0:D1 contains 64-bit sum
    ; Z flag is correct for entire 64-bit result
```

## Test Cases

```rust
#[test]
fn test_addx_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDX.L D0,D1 (opcode: 0xD380)
    load_words(&mut bus, 0x1000, &[0xD380]);
    cpu.regs.d[0] = 0x0000_0001;
    cpu.regs.d[1] = 0x0000_0002;
    cpu.regs.sr |= X; // Set extend flag

    run_instruction(&mut cpu, &mut bus);

    // D1 = D1 + D0 + X = 2 + 1 + 1 = 4
    assert_eq!(cpu.regs.d[1], 0x0000_0004);
}

#[test]
fn test_subx_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUBX.L D0,D1 (opcode: 0x9380)
    load_words(&mut bus, 0x1000, &[0x9380]);
    cpu.regs.d[0] = 0x0000_0001;
    cpu.regs.d[1] = 0x0000_0005;
    cpu.regs.sr |= X; // Set extend flag

    run_instruction(&mut cpu, &mut bus);

    // D1 = D1 - D0 - X = 5 - 1 - 1 = 3
    assert_eq!(cpu.regs.d[1], 0x0000_0003);
}
```

## Common Mistakes

### 1. Treating X Like C

```rust
// WRONG: Many instructions set C but not X
CMP.L   D0,D1    // Sets C, does NOT modify X
BCS     overflow // Uses C for comparison

// X flag preserved for next ADDX/SUBX in chain
```

### 2. Setting Z on Zero Result

```rust
// WRONG
sr = Status::set_if(sr, Z, result == 0);

// CORRECT for ADDX/SUBX
if result != 0 {
    sr &= !Z;
}
```

### 3. Forgetting SUBX Borrow Calculation

```rust
// WRONG: Simple subtraction
let carry = src > dst;

// CORRECT: Include X flag in borrow detection
let carry = (src + x) > dst || (src == dst && x != 0);
```

## Related Instructions

| Instruction | Uses X | Sets X | Notes |
|-------------|--------|--------|-------|
| ADD | No | Yes | X copies C |
| ADDX | Yes | Yes | Multi-precision add |
| SUB | No | Yes | X copies C |
| SUBX | Yes | Yes | Multi-precision subtract |
| NEGX | Yes | Yes | Multi-precision negate |
| ROXL/ROXR | Yes | Yes | Rotate through X |
| CMP | No | No | Doesn't affect X |
| ABCD/SBCD | Yes | Yes | BCD arithmetic |

## Related Documentation

- `docs/solutions/implementation/68000-micro-op-architecture.md` - Overall 68000 patterns
- M68000 Programmer's Reference Manual, Section 4 - Instruction Set
