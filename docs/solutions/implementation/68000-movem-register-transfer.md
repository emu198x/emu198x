---
title: "Implement cycle-accurate MOVEM instruction for 68000 CPU"
date: 2026-02-03
category: implementation
tags:
  - 68000
  - cpu
  - movem
  - microcode
  - cycle-accurate
  - multi-register
  - addressing-modes
  - predecrement
  - postincrement
module: emu-68000
severity: medium
symptoms:
  - MOVEM instruction not implemented
  - Unable to save/restore multiple registers
  - Stack frame operations incomplete
  - Subroutine prologue/epilogue patterns unsupported
---

# MOVEM Register Transfer Implementation

MOVEM (Move Multiple Registers) is one of the most complex 68000 instructions, transferring multiple registers to/from memory based on a 16-bit mask. This document covers the cycle-accurate implementation using self-chaining micro-ops.

## Problem

The previous MOVEM implementation was stubbed with simple `queue_internal()` calls that:

1. Did not perform actual memory transfers
2. Ignored the register mask
3. Did not handle predecrement mask reversal
4. Did not update address registers for pre/post modes

Without working MOVEM, common 68000 patterns like stack frame setup and register save/restore were broken.

## Solution

### Approach: Self-Chaining Micro-Ops

Unlike simpler instructions that queue all micro-ops upfront, MOVEM uses micro-ops that check the register mask and re-queue themselves for each register:

```rust
// From microcode.rs
MovemWrite,  // 4 cycles per word transfer
MovemRead,   // 4 cycles per word transfer
```

### CPU State for MOVEM

New state fields track execution:

```rust
// === MOVEM state ===
movem_predec: bool,      // Using predecrement mode
movem_postinc: bool,     // Using postincrement mode
movem_long_phase: u8,    // Long transfer: 0 = high word, 1 = low word
```

### Instruction Setup

The continuation function calculates the starting address and finds the first register:

```rust
fn exec_movem_to_mem_continuation(&mut self) {
    let mask = self.ext_words[0];
    let is_predec = matches!(addr_mode, AddrMode::AddrIndPreDec(_));

    // For predecrement, calculate starting address
    let (start_addr, ea_reg) = match addr_mode {
        AddrMode::AddrIndPreDec(r) => {
            let count = mask.count_ones();
            let dec_per_reg = if self.size == Size::Long { 4 } else { 2 };
            let start = self.regs.a(r as usize).wrapping_sub(count * dec_per_reg);
            (start, r)
        }
        // ... other modes
    };

    // Find first register based on mode
    let first_bit = if is_predec {
        self.find_first_movem_bit_down(mask)
    } else {
        self.find_first_movem_bit_up(mask)
    };

    if let Some(bit) = first_bit {
        self.data2 = bit as u32;
        self.micro_ops.push(MicroOp::MovemWrite);
    }
}
```

### Tick Handler with Self-Chaining

The tick function executes one 4-cycle memory transfer, then finds the next register:

```rust
fn tick_movem_write<B: Bus>(&mut self, bus: &mut B) {
    match self.cycle {
        0 | 1 | 2 => {}
        3 => {
            // Write current register value
            // ...

            // Find next register in mask
            let next_bit = if is_predec {
                self.find_next_movem_bit_down(mask, bit_idx)
            } else {
                self.find_next_movem_bit_up(mask, bit_idx)
            };

            if let Some(next) = next_bit {
                self.data2 = next as u32;
                self.cycle = 0;
                return;  // Stay on same micro-op
            }

            // All done - advance
            self.micro_ops.advance();
        }
    }
}
```

## Tricky Parts

### 1. Predecrement Mask Reversal

For `MOVEM.L D0/D1,-(A0)`, the register mask is interpreted in reverse:

| Mode | Bits 0-7 | Bits 8-15 |
|------|----------|-----------|
| Normal | D0-D7 | A0-A7 |
| Predecrement | A7-A0 | D7-D0 |

```rust
let value = if is_predec {
    if bit_idx < 8 {
        self.regs.a(7 - bit_idx)  // bit 0 = A7
    } else {
        self.regs.d[15 - bit_idx] // bit 8 = D7
    }
} else if bit_idx < 8 {
    self.regs.d[bit_idx]
} else {
    self.regs.a(bit_idx - 8)
};
```

### 2. Word Sign Extension for Address Registers

When loading word values into address registers, they must be sign-extended:

```rust
let word = self.read_word(bus, self.addr);
self.data = if bit_idx >= 8 {
    word as i16 as i32 as u32  // Sign extend for An
} else {
    u32::from(word)             // Zero extend for Dn
};
```

### 3. Address Register Update Timing

The address register is updated only after ALL transfers complete:

```rust
// After all writes done
if is_predec {
    let ea_reg = (self.addr2 & 7) as usize;
    let start_addr = self.regs.a(ea_reg);
    let count = mask.count_ones();
    let dec = if self.size == Size::Long { 4 } else { 2 };
    self.regs.set_a(ea_reg, start_addr.wrapping_sub(count * dec));
}
```

## Timing

| Operation | Cycles |
|-----------|--------|
| Word transfer | 4 |
| Long transfer | 8 (2 x 4) |
| Total | 8 + n*4 (word) or 12 + n*8 (long) |

## Test Coverage

9 tests cover:

- Word/long transfers in both directions
- Predecrement mode with reversed mask
- Postincrement mode with address update
- Word sign-extension for address registers
- Empty mask edge case
- All data registers (D0-D7)

## Files Changed

- `crates/emu-68000/src/cpu.rs` - Tick handlers and state fields
- `crates/emu-68000/src/cpu/execute.rs` - Decode and continuation functions
- `crates/emu-68000/src/microcode.rs` - MovemWrite/MovemRead micro-ops
- `crates/emu-68000/tests/instructions.rs` - Test suite

## Related Documentation

- [68000 Micro-Op Architecture](68000-micro-op-architecture.md) - General micro-op patterns
- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md) - Similar state tracking for ADDX/SUBX
