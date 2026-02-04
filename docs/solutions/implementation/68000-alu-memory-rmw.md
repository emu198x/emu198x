---
title: "Implement ALU memory read-modify-write operations"
date: 2026-02-03
category: implementation
tags:
  - 68000
  - add
  - sub
  - and
  - or
  - eor
  - memory-destination
  - read-modify-write
  - micro-op
module: emu-68000
severity: medium
symptoms:
  - ADD/SUB/AND/OR/EOR to memory operand not supported
  - Memory destination ALU operations fail
  - Programs using memory-based arithmetic fail
---

# ALU Memory Read-Modify-Write Implementation

ADD, SUB, AND, OR, and EOR instructions with memory destinations require atomic read-modify-write cycles. This document covers the implementation using a single generic `AluMemRmw` micro-op.

## Instruction Forms

All five operations support the form `OP Dn,<ea>` where the destination is memory:

| Instruction | Operation | Flags |
|-------------|-----------|-------|
| ADD Dn,<ea> | mem + Dn | X, N, Z, V, C |
| SUB Dn,<ea> | mem - Dn | X, N, Z, V, C |
| AND Dn,<ea> | mem & Dn | N, Z (V,C cleared) |
| OR Dn,<ea> | mem \| Dn | N, Z (V,C cleared) |
| EOR Dn,<ea> | mem ^ Dn | N, Z (V,C cleared) |

## Implementation Approach

### Single Generic Micro-Op

Rather than creating separate micro-ops for each ALU operation, we use a single `AluMemRmw` micro-op with an operation code:

```rust
// microcode.rs
pub enum MicroOp {
    /// ALU operation with memory destination (read-modify-write).
    ///
    /// Uses: `addr` for memory address, `data` for source value (from register),
    /// `data2` for operation (0=ADD, 1=SUB, 2=AND, 3=OR, 4=EOR).
    /// Size from `self.size`.
    AluMemRmw,
}
```

### Operation Codes

| Code | Operation |
|------|-----------|
| 0 | ADD |
| 1 | SUB |
| 2 | AND |
| 3 | OR |
| 4 | EOR |

### Exec Functions Pattern

Each exec function sets up state and queues the micro-op:

```rust
fn exec_add(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8, to_ea: bool) {
    // ...
    if to_ea {
        match addr_mode {
            AddrMode::DataReg(r) => {
                // Register destination: immediate execution
            }
            _ => {
                // Memory destination: read-modify-write
                self.size = size;
                let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                self.addr = addr;
                self.data = self.read_data_reg(reg, size);
                self.data2 = 0; // 0 = ADD
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
    }
}
```

### Tick Handler

The handler uses two phases via `movem_long_phase`:

```rust
fn tick_alu_mem_rmw<B: Bus>(&mut self, bus: &mut B) {
    match self.cycle {
        0 | 1 | 2 => {}
        3 => {
            match self.movem_long_phase {
                0 => {
                    // Phase 0: Read from memory
                    let mem_val = match self.size {
                        Size::Byte => u32::from(self.read_byte(bus, self.addr)),
                        Size::Word => u32::from(self.read_word(bus, self.addr)),
                        Size::Long => self.read_long(bus, self.addr),
                    };
                    // Store temporarily
                    self.ext_words[0] = mem_val as u16;
                    self.ext_words[1] = (mem_val >> 16) as u16;

                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }
                1 => {
                    // Phase 1: Perform operation and write back
                    let mem_val = u32::from(self.ext_words[0])
                        | (u32::from(self.ext_words[1]) << 16);
                    let src = self.data;

                    let result = match self.data2 {
                        0 => { /* ADD */ }
                        1 => { /* SUB */ }
                        2 => { /* AND */ }
                        3 => { /* OR */ }
                        4 => { /* EOR */ }
                        _ => mem_val,
                    };

                    // Write result back
                    match self.size { /* ... */ }

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

### Arithmetic Operations (ADD, SUB)

| Flag | Condition |
|------|-----------|
| N | Set if result is negative |
| Z | Set if result is zero |
| V | Set on signed overflow |
| C | Set on carry/borrow |
| X | Set same as C |

### Logical Operations (AND, OR, EOR)

| Flag | Condition |
|------|-----------|
| N | Set if result is negative |
| Z | Set if result is zero |
| V | Always cleared |
| C | Always cleared |
| X | Not affected |

## Test Coverage

6 tests covering:
- ADD.W memory word operation
- ADD.W with carry overflow
- SUB.B memory byte operation
- AND.W memory word operation
- OR.B memory byte operation
- EOR.W memory word operation

## Related Documentation

- [68000 TAS and Memory Shift Implementation](68000-tas-shift-memory.md) - Similar RMW pattern
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md) - Core architecture patterns
