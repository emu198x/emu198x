---
title: "68000 Group 0 instruction decode incorrectly routes CMPI and EORI to bit operations"
category: logic-errors
tags:
  - instruction-decode
  - 68000
  - bit-masking
  - opcode-routing
  - m68000
  - cpu-emulation
module: emu-68000
symptoms:
  - "CMPI instruction does not set condition flags (Z flag remains clear after comparing equal values)"
  - "EORI instruction does not modify destination register"
  - "Both instructions silently execute wrong handler with no error"
severity: critical
date: 2026-02-03
---

# 68000 Group 0 Instruction Decode Bug

## Problem Summary

CMPI and EORI instructions were being incorrectly dispatched to the bit operations handler instead of the immediate operations handler in the 68000 CPU emulator's Group 0 instruction decoder.

## Symptoms

- `test_cmpi_word_sets_zero` failed: Z flag not set after comparing equal values
- `test_eori_long` failed: destination register unchanged (original value instead of XOR'd result)
- Both instructions appeared to do nothing, with no error

## Root Cause

The original code checked bit 11 (`op & 0x0800`) to determine if an instruction was a bit operation. However, this check is too broad - it catches CMPI (bits 11-9 = `110`) and EORI (bits 11-9 = `101`) because both have bit 11 set.

### Opcode Encoding Analysis

| Bits 11-9 | Value | Operation |
|-----------|-------|-----------|
| 000 | 0 | ORI |
| 001 | 1 | ANDI |
| 010 | 2 | SUBI |
| 011 | 3 | ADDI |
| 100 | 4 | Static bit operations (BTST/BCHG/BCLR/BSET with immediate) |
| 101 | 5 | EORI ← bit 11 is SET |
| 110 | 6 | CMPI ← bit 11 is SET |
| 111 | 7 | (Illegal/Reserved) |

The problem: checking `op & 0x0800` (bit 11 alone) matches values 4, 5, 6, and 7 - not just value 4.

## Before (Broken)

```rust
fn decode_group_0(&mut self, op: u16) {
    if op & 0x0100 != 0 {
        // Bit operations with register
        // ...
    } else if op & 0x0800 != 0 {
        // Bit operations with immediate - THIS CATCHES CMPI/EORI INCORRECTLY!
        match (op >> 6) & 3 {
            0 => self.exec_btst_imm(mode, ea_reg),
            1 => self.exec_bchg_imm(mode, ea_reg),
            2 => self.exec_bclr_imm(mode, ea_reg),
            3 => self.exec_bset_imm(mode, ea_reg),
            _ => unreachable!(),
        }
    } else {
        // Immediate operations (ORI, ANDI, SUBI, ADDI, EORI, CMPI)
        match (op >> 9) & 7 {
            5 => self.exec_eori(size, mode, ea_reg),  // NEVER REACHED
            6 => self.exec_cmpi(size, mode, ea_reg),  // NEVER REACHED
            // ...
        }
    }
}
```

## After (Fixed)

```rust
fn decode_group_0(&mut self, op: u16) {
    if op & 0x0100 != 0 {
        // Bit operations with register (dynamic bit number in Dn)
        // ...
    } else {
        // Check bits 11-9 for operation type - dispatch ALL Group 0 ops here
        match (op >> 9) & 7 {
            0 => self.exec_ori(size, mode, ea_reg),
            1 => self.exec_andi(size, mode, ea_reg),
            2 => self.exec_subi(size, mode, ea_reg),
            3 => self.exec_addi(size, mode, ea_reg),
            4 => {
                // Static bit operations with immediate bit number
                match (op >> 6) & 3 {
                    0 => self.exec_btst_imm(mode, ea_reg),
                    1 => self.exec_bchg_imm(mode, ea_reg),
                    2 => self.exec_bclr_imm(mode, ea_reg),
                    3 => self.exec_bset_imm(mode, ea_reg),
                    _ => unreachable!(),
                }
            }
            5 => self.exec_eori(size, mode, ea_reg),  // NOW REACHABLE
            6 => self.exec_cmpi(size, mode, ea_reg),  // NOW REACHABLE
            7 => self.illegal_instruction(),
        }
    }
}
```

## Key Insight

The bits 11-9 field (`(op >> 9) & 7`) identifies **ALL** Group 0 operations. Value 4 specifically identifies static bit operations; all other values (0-3, 5-6) identify immediate arithmetic/logic operations.

Checking bit 11 alone was insufficient because it matched EORI (value 5 = `101` binary) and CMPI (value 6 = `110` binary).

The fix uses a proper dispatch on the full 3-bit field rather than a series of single-bit tests.

## Prevention Strategies

### 1. Decode from Most Specific to Least Specific

Structure decode logic to check the full bit field, not individual bits:

```rust
// BAD: Single bit check catches too much
if opcode & 0x0800 != 0 { /* bit operations */ }

// GOOD: Check full bit field
match (opcode >> 9) & 7 {
    4 => { /* bit operations */ }
    5 => { /* EORI */ }
    6 => { /* CMPI */ }
    // ...
}
```

### 2. Document Bit Field Boundaries

Add structured comments showing the exact bit layout:

```rust
/// Group 0 Instructions (bits 15-12 = 0b0000)
///
/// Bits 11-9 determine operation:
/// - 0: ORI, 1: ANDI, 2: SUBI, 3: ADDI
/// - 4: Static bit ops, 5: EORI, 6: CMPI, 7: illegal
```

### 3. Add Debug Assertions

Assert opcode falls in expected range:

```rust
fn exec_btst_imm(&mut self, mode: u8, ea_reg: u8) {
    debug_assert_eq!(
        (self.opcode >> 9) & 7, 4,
        "BTST_IMM called with wrong bits 11-9: {:#06x}", self.opcode
    );
    // ...
}
```

### 4. Test Each Sub-Category

For each decode branch, test at least one instruction:

```rust
#[test] fn decode_ori() { /* bits 11-9 = 0 */ }
#[test] fn decode_andi() { /* bits 11-9 = 1 */ }
#[test] fn decode_btst_static() { /* bits 11-9 = 4 */ }
#[test] fn decode_eori() { /* bits 11-9 = 5 */ }
#[test] fn decode_cmpi() { /* bits 11-9 = 6 */ }
```

### 5. Cross-Reference Against M68000 PRM

Keep the Programmer's Reference Manual opcode table accessible and verify decode logic against it when adding instructions.

## Test Cases

```rust
#[test]
fn test_cmpi_word_sets_zero() {
    // CMPI.W #$5678, D2 (opcode: 0x0C42)
    load_words(&mut bus, 0x1000, &[0x0C42, 0x5678]);
    cpu.regs.d[2] = 0x0000_5678;
    // After execution, Z flag should be set
    assert!(cpu.regs.sr & Z != 0);
}

#[test]
fn test_eori_long() {
    // EORI.L #$FFFFFFFF, D5 (opcode: 0x0A85)
    load_words(&mut bus, 0x1000, &[0x0A85, 0xFFFF, 0xFFFF]);
    cpu.regs.d[5] = 0x5555_AAAA;
    // After execution, D5 should be inverted
    assert_eq!(cpu.regs.d[5], 0xAAAA_5555);
}
```

## Related Documentation

- `docs/solutions/logic-errors/68000-jmp-jsr-decode-dispatch.md` - Similar decode bug in Group 4
- M68000 Programmer's Reference Manual, Section 8 - Opcode Maps
