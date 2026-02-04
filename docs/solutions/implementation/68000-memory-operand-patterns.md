---
title: "Implement memory operand support for bit, quick, and source operations"
date: 2026-02-04
category: implementation
tags:
  - 68000
  - memory-operands
  - bit-operations
  - micro-op
  - addq
  - subq
  - btst
  - bchg
  - bclr
  - bset
  - chk
  - mulu
  - muls
  - divu
  - divs
module: emu-68000
severity: medium
symptoms:
  - BTST/BCHG/BCLR/BSET memory operand not working
  - ADDQ/SUBQ to memory not modifying target
  - CHK memory operand not checking bounds
  - MULU/MULS/DIVU/DIVS memory source not loading operand
---

# Memory Operand Support: Bit, Quick, and Source Operations

This document covers the implementation of memory operand support for three categories of 68000 instructions that previously had stub implementations.

## Problem Summary

Several instruction categories had working register implementations but stubbed memory operand support:

1. **Bit operations** (BTST, BCHG, BCLR, BSET) with memory destinations
2. **Quick arithmetic** (ADDQ, SUBQ) with memory destinations
3. **Source operations** (CHK, MULU, MULS, DIVU, DIVS) with memory source

## Implementation Approach

### Pattern 1: BitMemOp - Bit Operations on Memory

New micro-op for BTST/BCHG/BCLR/BSET when the operand is a memory location.

**Key characteristics:**
- Always byte operations (bit number mod 8 for memory vs mod 32 for registers)
- BTST is read-only; BCHG/BCLR/BSET are read-modify-write
- Z flag set based on **original** bit value before modification

**MicroOp definition:**

```rust
/// Bit operation on memory byte (BTST/BCHG/BCLR/BSET).
///
/// Uses: `addr` for memory address, `data` for bit number (0-7),
/// `data2` for operation (0=BTST, 1=BCHG, 2=BCLR, 3=BSET).
/// BTST is read-only, others are read-modify-write.
/// Phase 0: Read byte. Phase 1 (for BCHG/BCLR/BSET): Write modified byte.
BitMemOp,
```

**Operation codes (data2):**

| Code | Operation | Description |
|------|-----------|-------------|
| 0 | BTST | Test bit only (read-only) |
| 1 | BCHG | Toggle bit (XOR with mask) |
| 2 | BCLR | Clear bit (AND with ~mask) |
| 3 | BSET | Set bit (OR with mask) |

**Instruction setup example:**

```rust
fn exec_btst_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
    let bit_num = self.regs.d[reg as usize];

    if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
        match addr_mode {
            AddrMode::DataReg(r) => {
                // Register: bit mod 32
                let bit = (bit_num % 32) as u8;
                // ... immediate execution
            }
            _ => {
                // Memory: bit mod 8
                self.size = Size::Byte;
                let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                self.addr = addr;
                self.data = bit_num & 7; // mod 8 for memory
                self.data2 = 0; // 0 = BTST
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::BitMemOp);
            }
        }
    }
}
```

### Pattern 2: AluMemRmw for ADDQ/SUBQ

Reuses existing `AluMemRmw` micro-op for quick arithmetic with memory destinations.

**Key insight:** ADDQ/SUBQ use an immediate value (1-8) instead of a register source. The `data` field holds the immediate value rather than reading from a register.

```rust
fn exec_addq(&mut self, size: Option<Size>, data: u8, mode: u8, ea_reg: u8) {
    let imm = if data == 0 { 8u32 } else { u32::from(data) };

    // ... register cases ...

    _ => {
        // Memory destination
        self.size = size;
        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
        self.addr = addr;
        self.data = imm; // Immediate value as source
        self.data2 = 0;  // 0 = ADD (1 = SUB for SUBQ)
        self.movem_long_phase = 0;
        self.micro_ops.push(MicroOp::AluMemRmw);
    }
}
```

### Pattern 3: AluMemSrc Extensions

Extended `AluMemSrc` micro-op with new operation codes for CHK and multiply/divide.

**New operation codes:**

| Code | Operation | Description |
|------|-----------|-------------|
| 9 | CHK | Bounds check, exception if out of range |
| 10 | MULU | Unsigned 16x16->32 multiply |
| 11 | MULS | Signed 16x16->32 multiply |
| 12 | DIVU | Unsigned 32/16 divide |
| 13 | DIVS | Signed 32/16 divide |

**CHK implementation:**

```rust
9 => {
    // CHK: check bounds, trigger exception if out of range
    let dn = self.regs.d[reg as usize] as i16;
    let upper_bound = src as i16;

    if dn < 0 {
        self.regs.sr |= N;
        self.exception(6); // CHK exception
    } else if dn > upper_bound {
        self.regs.sr &= !N;
        self.exception(6);
    }
}
```

**Multiply/divide implementations handle:**
- Division by zero exception (vector 5)
- Overflow detection (V flag set, result not stored)
- Proper flag setting (N, Z, V, C)
- Internal cycle timing (~70 for multiply, ~140-158 for divide)

## Testing Patterns

### The NOP Trick

When tests run a fixed number of ticks, the CPU may fetch and execute garbage after the test instruction completes. Add a NOP (0x4E71) after the instruction:

```rust
#[test]
fn test_btst_reg_memory() {
    // BTST D0, (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0x0110, 0x4E71]);
    bus.poke(0x2000, 0xFB); // Test data

    cpu.regs.d[0] = 2;      // Test bit 2
    cpu.regs.a[0] = 0x2000; // Address

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z set because bit 2 of 0xFB is clear
    assert!(cpu.regs.sr & emu_68000::Z != 0);
}
```

### Memory Result Verification

For byte operations, use `bus.poke()` to set up and `bus.peek()` to verify:

```rust
bus.poke(0x2000, 0x55);  // Setup: 0101_0101

// ... run instruction ...

assert_eq!(bus.peek(0x2000), 0x5D);  // Verify result
```

For word operations, reconstruct from two bytes:

```rust
let hi = bus.peek(0x2000);
let lo = bus.peek(0x2001);
let result = u16::from(hi) << 8 | u16::from(lo);
assert_eq!(result, 0x0013);
```

## Key Design Decisions

### Reusing movem_long_phase

The existing `movem_long_phase` field (originally for MOVEM) is reused to track read/write phases in BitMemOp. This avoids adding new fields and leverages the proven two-phase pattern.

### Extending vs Creating Micro-ops

- **BitMemOp**: New micro-op because bit operations have unique characteristics (byte-only, Z flag semantics)
- **ADDQ/SUBQ**: Reuse AluMemRmw because the pattern is identical to other RMW operations
- **CHK/MUL/DIV**: Extend AluMemSrc because they share the "read from memory, do something" pattern

### Operation Code Conventions

`data2` field encodes the operation type:
- AluMemRmw: 0-6 for binary/unary ALU ops
- AluMemSrc: 0-13 covering arithmetic, comparison, and special ops
- BitMemOp: 0-3 for the four bit operations

## Test Coverage

Tests added (total: 142 passing):
- `test_btst_reg_memory` - BTST with memory, Z flag on clear bit
- `test_bchg_reg_memory` - BCHG toggle and verify
- `test_bclr_reg_memory` - BCLR clear and verify
- `test_bset_reg_memory` - BSET set and verify
- `test_bset_reg_memory_bit_mod8` - Verify bit mod 8 for memory
- `test_addq_memory_word` - ADDQ word to memory
- `test_subq_memory_byte` - SUBQ byte from memory
- `test_mulu_memory` - MULU with memory source
- `test_divu_memory` - DIVU with memory source

## Related Documentation

- [68000 ALU Memory RMW](68000-alu-memory-rmw.md) - Core AluMemRmw pattern for ADD/SUB/AND/OR/EOR
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md) - Foundation for all micro-op patterns
- [68000 TAS and Shift Memory](68000-tas-shift-memory.md) - Similar RMW patterns for TAS and shifts
- [68000 Condition Code Instructions](68000-condition-code-instructions.md) - Scc and CHK instruction details
