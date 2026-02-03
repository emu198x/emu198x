---
title: "Implement TAS and memory shift/rotate instructions"
date: 2026-02-03
category: implementation
tags:
  - 68000
  - tas
  - test-and-set
  - shift
  - rotate
  - asl
  - asr
  - lsl
  - lsr
  - rol
  - ror
  - roxl
  - roxr
  - read-modify-write
  - micro-op
  - atomic
module: emu-68000
severity: medium
symptoms:
  - TAS instruction not supported
  - Memory shift/rotate operations not supported
  - Programs using TAS for synchronization fail
  - Single-bit memory shift patterns unavailable
---

# TAS and Memory Shift/Rotate Implementation

TAS (Test And Set) and memory shift/rotate instructions perform atomic read-modify-write cycles. This document covers the implementation using single micro-ops with internal phase tracking.

## Instruction Specifications

### TAS (Test And Set)

```
TAS <ea>
Opcode: 0100 1010 11 mode reg
```

| Aspect | Value |
|--------|-------|
| Size | Byte only |
| Operation | Test operand, set N/Z flags, then set bit 7 |
| Flags | N, Z based on **original** value; V, C cleared |

### Memory Shift/Rotate

```
ASL/ASR/LSL/LSR/ROL/ROR <ea>
Opcode: 1110 kind dr 11 mode reg
```

| Field | Bits | Values |
|-------|------|--------|
| kind | 9-10 | 00=AS, 01=LS, 10=ROX, 11=RO |
| dr | 8 | 0=right, 1=left |
| mode | 3-5 | Addressing mode |
| reg | 0-2 | Register number |

| Aspect | Value |
|--------|-------|
| Size | Word only (always) |
| Shift count | Always 1 |
| Flags | N, Z from result; C = last bit out; X = C for shifts only |

## Implementation Approach

### Single Micro-Op with Internal Phases

Both TAS and memory shifts use a single micro-op that handles read and write phases internally. This differs from CMPM which pushes two micro-ops.

**Why single micro-op?**
1. Atomicity - RMW operations must be indivisible
2. State coherence - `movem_long_phase` tracks progress
3. Timing accuracy - models the 68000's atomic RMW bus cycle

### Micro-Op Definitions

```rust
// microcode.rs
pub enum MicroOp {
    /// TAS: Test and Set byte.
    /// Phase 0: Read byte, set flags. Phase 1: Write byte with bit 7 set.
    TasExecute,

    /// Memory shift/rotate: read word, shift by 1, write back.
    /// Uses: `addr`, `data` (kind), `data2` (direction).
    /// Phase 0: Read word. Phase 1: Shift and write.
    ShiftMemExecute,
}
```

### Exec Functions

```rust
// execute.rs
fn exec_tas(&mut self, mode: u8, ea_reg: u8) {
    if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
        match addr_mode {
            AddrMode::DataReg(r) => {
                // Register: immediate execution
                let value = (self.regs.d[r as usize] & 0xFF) as u8;
                self.set_flags_move(u32::from(value), Size::Byte);
                self.regs.d[r as usize] =
                    (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(value | 0x80);
                self.queue_internal(4);
            }
            _ => {
                // Memory: queue read-modify-write
                let (addr, _) = self.calc_ea(addr_mode, self.regs.pc);
                self.addr = addr;
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::TasExecute);  // Single push
            }
        }
    }
}

fn exec_shift_mem(&mut self, kind: u8, direction: bool, mode: u8, ea_reg: u8) {
    if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
        match addr_mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) => {
                self.illegal_instruction();  // Memory only
            }
            _ => {
                let (addr, _) = self.calc_ea(addr_mode, self.regs.pc);
                self.addr = addr;
                self.data = u32::from(kind);
                self.data2 = if direction { 1 } else { 0 };
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::ShiftMemExecute);  // Single push
            }
        }
    }
}
```

### Tick Handlers

```rust
// cpu.rs
fn tick_tas_execute<B: Bus>(&mut self, bus: &mut B) {
    match self.cycle {
        0 | 1 | 2 => {}
        3 => {
            match self.movem_long_phase {
                0 => {
                    // Phase 0: Read byte, set flags from ORIGINAL value
                    let value = self.read_byte(bus, self.addr);
                    self.data = u32::from(value);
                    self.set_flags_move(u32::from(value), Size::Byte);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }
                1 => {
                    // Phase 1: Write byte with bit 7 set
                    let value = (self.data as u8) | 0x80;
                    self.write_byte(bus, self.addr, value);
                    self.movem_long_phase = 0;
                    self.cycle = 0;
                    self.micro_ops.advance();
                    return;
                }
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
    self.cycle += 1;
}

fn tick_shift_mem_execute<B: Bus>(&mut self, bus: &mut B) {
    match self.cycle {
        0 | 1 | 2 => {}
        3 => {
            match self.movem_long_phase {
                0 => {
                    // Phase 0: Read word
                    let value = self.read_word(bus, self.addr);
                    self.ext_words[0] = value;
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }
                1 => {
                    // Phase 1: Shift by 1 and write back
                    let value = u32::from(self.ext_words[0]);
                    let kind = self.data as u8;
                    let direction = self.data2 != 0;
                    let (result, carry) = self.shift_word_by_one(value, kind, direction);

                    self.write_word(bus, self.addr, result as u16);
                    self.set_flags_move(result, Size::Word);
                    self.regs.sr = Status::set_if(self.regs.sr, C, carry);
                    if kind < 2 {  // X only for shifts, not rotates
                        self.regs.sr = Status::set_if(self.regs.sr, X, carry);
                    }
                    self.regs.sr &= !V;

                    self.movem_long_phase = 0;
                    self.cycle = 0;
                    self.micro_ops.advance();
                    return;
                }
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
    self.cycle += 1;
}
```

## Flag Handling

### TAS Flags

TAS sets flags based on the **original** value (before setting bit 7):

| Flag | Condition |
|------|-----------|
| N | Set if original byte has bit 7 set |
| Z | Set if original byte is zero |
| V | Always cleared |
| C | Always cleared |
| X | Not affected |

### Shift vs Rotate Flags

| Flag | Shifts (AS, LS) | Rotates (RO, ROX) |
|------|-----------------|-------------------|
| N | Set if MSB of result is 1 | Same |
| Z | Set if result is zero | Same |
| V | Cleared | Cleared |
| C | Last bit shifted out | Last bit rotated out |
| X | Set same as C | **Not affected** |

## Shift Operations

```rust
fn shift_word_by_one(&self, value: u32, kind: u8, left: bool) -> (u32, bool) {
    let mask = 0xFFFF_u32;
    let msb = 0x8000_u32;

    match (kind, left) {
        // ASL/LSL - Left shift (carry = old MSB)
        (0 | 1, true) => {
            let carry = (value & msb) != 0;
            ((value << 1) & mask, carry)
        }
        // ASR - Right shift with sign extend
        (0, false) => {
            let carry = (value & 1) != 0;
            let sign = value & msb;
            (((value >> 1) | sign) & mask, carry)
        }
        // LSR - Right shift zero extend
        (1, false) => {
            let carry = (value & 1) != 0;
            ((value >> 1) & mask, carry)
        }
        // ROXL - Rotate left through X
        (2, true) => {
            let x_in = u32::from(self.regs.sr & X != 0);
            let carry = (value & msb) != 0;
            (((value << 1) | x_in) & mask, carry)
        }
        // ROXR - Rotate right through X
        (2, false) => {
            let x_in = if self.regs.sr & X != 0 { msb } else { 0 };
            let carry = (value & 1) != 0;
            (((value >> 1) | x_in) & mask, carry)
        }
        // ROL - Rotate left
        (3, true) => {
            let carry = (value & msb) != 0;
            (((value << 1) | (value >> 15)) & mask, carry)
        }
        // ROR - Rotate right
        (3, false) => {
            let carry = (value & 1) != 0;
            (((value >> 1) | (value << 15)) & mask, carry)
        }
        _ => (value, false),
    }
}
```

## Testing Guidance

### Cycle Count Precision

A critical lesson from this implementation: **running too many cycles corrupts flags**.

When tests execute excess cycles, subsequent instructions (like NOPs) execute and may modify flags. The solution:

1. Add NOP (0x4E71) after target instruction
2. Use conservative but bounded cycle counts
3. Calculate expected cycles: fetch (4) + read (4) + write (4) = 12 minimum

```rust
// CORRECT: NOP padding, bounded cycles
load_words(&mut bus, 0x1000, &[0x4AD0, 0x4E71]);  // TAS (A0), NOP
for _ in 0..20 {
    cpu.tick(&mut bus);
}
```

### Test Coverage

10 tests covering:
- TAS data register (positive, zero, negative values)
- TAS memory operand
- ASL, ASR, LSL, LSR, ROL, ROR memory operations
- Flag verification for each operation

## Related Documentation

- [68000 Micro-Op Architecture](68000-micro-op-architecture.md) - Core architecture patterns
- [68000 CMPM Implementation](68000-cmpm-memory-compare.md) - Similar two-phase pattern
- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md) - X flag patterns
