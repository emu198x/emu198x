---
title: "Implement CMPM (Compare Memory) instruction"
date: 2026-02-03
category: implementation
tags:
  - 68000
  - cmpm
  - memory-compare
  - postincrement
  - string-operations
module: emu-68000
severity: medium
symptoms:
  - Memory comparison operations unavailable
  - String/buffer comparison not supported
  - Cannot emulate programs using CMPM for loops
---

# CMPM Memory Compare Implementation

CMPM compares two memory operands using postincrement addressing. It's commonly used for string and buffer comparisons in loops.

## Instruction Format

```
CMPM (Ay)+,(Ax)+
Opcode: 1011 Ax 1 ss 001 Ay
```

- **Ax**: Destination address register (bits 9-11)
- **ss**: Size (00=byte, 01=word, 10=long)
- **Ay**: Source address register (bits 0-2)

## Operation

1. Read source from (Ay)
2. Read destination from (Ax)
3. Compare: (Ax) - (Ay), set flags
4. Increment both Ay and Ax by operand size
5. No result stored (flags only)

## Implementation Approach

CMPM uses a dedicated micro-op with two phases:

```rust
fn exec_cmpm(&mut self, op: u16) {
    // Parse opcode
    let ay = (op & 7) as usize;
    let ax = ((op >> 9) & 7) as usize;

    // Set up state
    self.addr = self.regs.a(ay);   // Source address
    self.addr2 = self.regs.a(ax);  // Destination address
    self.data = ay as u32;         // Source register number
    self.data2 = ax as u32;        // Destination register number

    // Queue two-phase micro-op
    self.micro_ops.push(MicroOp::CmpmExecute);
    self.micro_ops.push(MicroOp::CmpmExecute);
}
```

The tick handler processes in phases:

```rust
fn tick_cmpm_execute<B: Bus>(&mut self, bus: &mut B) {
    match self.movem_long_phase {
        0 => {
            // Phase 0: Read source, increment Ay
            let src_val = self.read_by_size(bus, self.addr);
            self.store_temp(src_val);
            self.increment_a(ay);
            self.movem_long_phase = 1;
        }
        1 => {
            // Phase 1: Read dest, increment Ax, compare
            let dst_val = self.read_by_size(bus, self.addr2);
            self.increment_a(ax);
            let src_val = self.retrieve_temp();
            self.set_flags_cmp(src_val, dst_val, dst_val - src_val);
        }
    }
}
```

## A7 Stack Pointer Alignment

For byte operations, A7 increments by 2 (not 1) to maintain stack alignment:

```rust
let inc = match self.size {
    Size::Byte => if reg == 7 { 2 } else { 1 },
    Size::Word => 2,
    Size::Long => 4,
};
```

## Flags

CMPM sets flags exactly like CMP (subtract without storing):

| Flag | Condition |
|------|-----------|
| Z | Set if (Ax) == (Ay) |
| N | Set if result MSB is 1 |
| C | Set if borrow (src > dst) |
| V | Set on signed overflow |
| X | Not affected |

## Common Usage Pattern

CMPM is typically used in loops to compare buffers:

```asm
        LEA     buffer1,A0
        LEA     buffer2,A1
        MOVE.W  #length-1,D0
loop:   CMPM.B  (A0)+,(A1)+
        DBNE    D0,loop
        BEQ     equal
```

## Test Coverage

6 tests covering:
- Byte comparison (equal, not equal, negative result)
- Word comparison
- Long comparison
- A7 byte alignment (increments by 2)

## Related Documentation

- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md) - Similar flag patterns
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md) - Micro-op queue patterns
