---
title: "M68000 Cycle-Accurate Emulator Implementation Patterns"
category: implementation
tags:
  - m68000
  - cpu-emulation
  - cycle-accurate
  - micro-ops
  - instruction-decoding
  - rust
  - retro-computing
module: emu-68000
symptoms:
  - implementing cycle-accurate CPU emulation
  - handling multi-phase instruction execution
  - proper flag calculation for 68000 arithmetic operations
  - extension word decoding for complex addressing modes
  - sign extension behavior for word-to-long operations
severity: info
date: 2026-02-03
---

# M68000 Cycle-Accurate Emulator Implementation Patterns

This documents the key implementation patterns used in the `emu-68000` crate for cycle-accurate Motorola 68000 CPU emulation.

## Overview

The 68000 emulator uses a **micro-operation queue architecture** to achieve cycle-accurate execution. Each instruction is broken down into atomic operations (fetch, read, write, internal processing) that execute over fixed cycle counts, primarily 4 cycles for memory operations.

**Test Coverage:** 61 passing tests covering data movement, arithmetic, logic, shifts/rotates, branches, bit operations, word displacement branches, and privileged instructions.

## Pattern 1: Multi-Phase Instruction Execution

Complex instructions like MOVE with memory operands require multiple execution phases. The `InstrPhase` enum tracks progress:

```rust
/// Instruction execution phase for multi-step instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrPhase {
    /// Initial decode and setup.
    Initial,
    /// Fetched source extension words, need to calculate EA and read.
    SrcEACalc,
    /// Read source operand, now fetch dest extension words.
    SrcRead,
    /// Fetched dest extension words, need to calculate EA and write.
    DstEACalc,
    /// Final write to destination.
    DstWrite,
    /// Instruction complete.
    Complete,
}
```

**Usage in MOVE instruction:**

```rust
fn exec_move(&mut self, size: Size, src_mode: u8, src_reg: u8, dst_mode: u8, dst_reg: u8) {
    match self.instr_phase {
        InstrPhase::Initial => {
            // Setup: store modes, calculate extension word count
            self.src_mode = Some(src_mode);
            self.dst_mode = Some(dst_mode);

            // Queue fetching all extension words
            for _ in 0..src_ext {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.instr_phase = InstrPhase::SrcEACalc;
            self.micro_ops.push(MicroOp::Execute);
        }
        InstrPhase::SrcEACalc => {
            // Extension words fetched, calculate source EA and queue read
            let (addr, _is_reg) = self.calc_ea(src_mode, pc_at_ext);
            self.addr = addr;
            self.queue_read_ops(self.size);
            self.instr_phase = InstrPhase::SrcRead;
            self.micro_ops.push(MicroOp::Execute);
        }
        // ... continue through phases
    }
}
```

## Pattern 2: Micro-Op Queue Architecture

The queue holds 32 entries (vs Z80's 16) to accommodate longer 68000 instructions:

```rust
pub enum MicroOp {
    FetchOpcode,     // 4 cycles
    FetchExtWord,    // 4 cycles
    ReadByte,        // 4 cycles
    ReadWord,        // 4 cycles
    ReadLongHi,      // 4 cycles (high word)
    ReadLongLo,      // 4 cycles (low word)
    WriteByte,       // 4 cycles
    WriteWord,       // 4 cycles
    WriteLongHi,     // 4 cycles
    WriteLongLo,     // 4 cycles
    CalcEA,          // 0 cycles (instant)
    Execute,         // 0 cycles (instant)
    Internal,        // Variable cycles
    PushWord,        // 4 cycles
    PushLongHi,      // 4 cycles
    PushLongLo,      // 4 cycles
    PopWord,         // 4 cycles
    PopLongHi,       // 4 cycles
    PopLongLo,       // 4 cycles
    BeginException,  // 0 cycles (setup)
    ReadVector,      // 4 cycles
}

pub struct MicroOpQueue {
    ops: [MicroOp; 32],
    len: u8,
    pos: u8,
}
```

**Tick execution:**

```rust
fn tick_internal<B: Bus>(&mut self, bus: &mut B) {
    let Some(op) = self.micro_ops.current() else {
        self.queue_fetch();
        return;
    };

    match op {
        MicroOp::FetchOpcode => self.tick_fetch_opcode(bus),
        MicroOp::FetchExtWord => self.tick_fetch_ext_word(bus),
        MicroOp::ReadWord => self.tick_read_word(bus),
        MicroOp::Execute => {
            self.decode_and_execute();
            self.micro_ops.advance();
        }
        // ... all other micro-ops
    }
}
```

## Pattern 3: Extension Word Handling

Many addressing modes require 1-2 extension words after the opcode:

```rust
/// Extension words storage
ext_words: [u16; 4],
ext_count: u8,
ext_idx: u8,

/// Get next extension word and advance index.
fn next_ext_word(&mut self) -> u16 {
    let idx = self.ext_idx as usize;
    if idx < self.ext_count as usize {
        self.ext_idx += 1;
        self.ext_words[idx]
    } else {
        0
    }
}

/// Count extension words needed for an addressing mode.
fn ext_words_for_mode(&self, mode: AddrMode) -> u8 {
    match mode {
        AddrMode::DataReg(_) | AddrMode::AddrReg(_) => 0,
        AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => 0,
        AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_) => 1,
        AddrMode::AbsShort | AddrMode::PcDisp | AddrMode::PcIndex => 1,
        AddrMode::AbsLong => 2,
        AddrMode::Immediate => match self.size {
            Size::Byte | Size::Word => 1,
            Size::Long => 2,
        },
    }
}
```

## Pattern 4: Addressing Mode Calculation

```rust
pub enum AddrMode {
    DataReg(u8),          // Dn
    AddrReg(u8),          // An
    AddrInd(u8),          // (An)
    AddrIndPostInc(u8),   // (An)+
    AddrIndPreDec(u8),    // -(An)
    AddrIndDisp(u8),      // d16(An)
    AddrIndIndex(u8),     // d8(An,Xn)
    AbsShort,             // (xxx).W
    AbsLong,              // (xxx).L
    PcDisp,               // d16(PC)
    PcIndex,              // d8(PC,Xn)
    Immediate,            // #<data>
}

fn calc_ea(&mut self, mode: AddrMode, pc_at_ext: u32) -> (u32, bool) {
    match mode {
        AddrMode::DataReg(r) => (r as u32, true),  // is_reg = true
        AddrMode::AddrInd(r) => (self.regs.a(r as usize), false),
        AddrMode::AddrIndDisp(r) => {
            let disp = self.next_ext_word() as i16 as i32;
            let addr = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
            (addr, false)
        }
        AddrMode::AddrIndIndex(r) => {
            let ext = self.next_ext_word();
            let disp = (ext & 0xFF) as i8 as i32;
            let xn = ((ext >> 12) & 7) as usize;
            let is_addr = ext & 0x8000 != 0;
            let is_long = ext & 0x0800 != 0;
            let idx_val = if is_addr { self.regs.a(xn) } else { self.regs.d[xn] };
            let idx_val = if is_long { idx_val as i32 } else { idx_val as i16 as i32 };
            let addr = (self.regs.a(r as usize) as i32)
                .wrapping_add(disp)
                .wrapping_add(idx_val) as u32;
            (addr, false)
        }
        AddrMode::AbsShort => {
            let addr = self.next_ext_word() as i16 as i32 as u32;  // Sign extend
            (addr, false)
        }
        AddrMode::AbsLong => {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            ((u32::from(hi) << 16) | u32::from(lo), false)
        }
        // ... PC-relative, immediate, etc.
    }
}
```

## Pattern 5: Flag Calculation Helpers

Separate helpers for different operation types:

```rust
/// Set flags for MOVE-style operations (clears V and C, sets N and Z).
fn set_flags_move(&mut self, value: u32, size: Size) {
    self.regs.sr = Status::clear_vc(self.regs.sr);
    self.regs.sr = match size {
        Size::Byte => Status::update_nz_byte(self.regs.sr, value as u8),
        Size::Word => Status::update_nz_word(self.regs.sr, value as u16),
        Size::Long => Status::update_nz_long(self.regs.sr, value),
    };
}

/// Set flags for ADD operation.
fn set_flags_add(&mut self, src: u32, dst: u32, result: u32, size: Size) {
    let (src, dst, result, msb) = match size {
        Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
        Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
        Size::Long => (src, dst, result, 0x8000_0000),
    };

    let mut sr = self.regs.sr;
    sr = Status::set_if(sr, Z, result == 0);
    sr = Status::set_if(sr, N, result & msb != 0);

    // Carry: set if there was a carry out
    let carry = match size {
        Size::Byte => (u16::from(src as u8) + u16::from(dst as u8)) > 0xFF,
        Size::Word => (u32::from(src as u16) + u32::from(dst as u16)) > 0xFFFF,
        Size::Long => src.checked_add(dst).is_none(),
    };
    sr = Status::set_if(sr, C, carry);
    sr = Status::set_if(sr, X, carry);  // Extend copies Carry

    // Overflow: same sign inputs, different sign result
    let overflow = (!(src ^ dst) & (src ^ result) & msb) != 0;
    sr = Status::set_if(sr, V, overflow);

    self.regs.sr = sr;
}
```

## Pattern 6: Condition Code Evaluation

```rust
pub fn condition(sr: u16, cc: u8) -> bool {
    match cc & 0x0F {
        0x0 => true,                                    // T (true)
        0x1 => false,                                   // F (false)
        0x2 => (sr & C) == 0 && (sr & Z) == 0,          // HI (high)
        0x3 => (sr & C) != 0 || (sr & Z) != 0,          // LS (low or same)
        0x4 => (sr & C) == 0,                           // CC/HS (carry clear)
        0x5 => (sr & C) != 0,                           // CS/LO (carry set)
        0x6 => (sr & Z) == 0,                           // NE (not equal)
        0x7 => (sr & Z) != 0,                           // EQ (equal)
        0xC => { (sr & N) != 0 } == { (sr & V) != 0 },  // GE (N == V)
        0xD => { (sr & N) != 0 } != { (sr & V) != 0 },  // LT (N != V)
        0xE => (sr & Z) == 0 && { (sr & N) != 0 } == { (sr & V) != 0 }, // GT
        0xF => (sr & Z) != 0 || { (sr & N) != 0 } != { (sr & V) != 0 }, // LE
        // ... others
    }
}
```

## Pattern 7: Dual Stack Pointer Management

The 68000 has separate user and supervisor stack pointers:

```rust
pub struct Registers {
    pub d: [u32; 8],     // Data registers D0-D7
    pub a: [u32; 7],     // Address registers A0-A6
    pub usp: u32,        // User stack pointer
    pub ssp: u32,        // Supervisor stack pointer
    pub pc: u32,
    pub sr: u16,
}

/// Get address register by index (0-7).
/// A7 returns the active stack pointer based on supervisor mode.
pub fn a(&self, n: usize) -> u32 {
    if n < 7 {
        self.a[n]
    } else {
        self.active_sp()  // A7 is context-dependent
    }
}

pub const fn active_sp(&self) -> u32 {
    if self.is_supervisor() { self.ssp } else { self.usp }
}
```

## Pattern 8: Per-Cycle Memory Access Timing

Each memory access takes exactly 4 cycles:

```rust
fn tick_read_word<B: Bus>(&mut self, bus: &mut B) {
    match self.cycle {
        0 | 1 | 2 => {}  // Bus cycles 1-3: Address setup
        3 => {
            // Cycle 4: Read complete
            self.data = u32::from(self.read_word(bus, self.addr));
            self.cycle = 0;
            self.micro_ops.advance();
            return;
        }
        _ => unreachable!(),
    }
    self.cycle += 1;
}
```

## Key Gotchas

### Quick Immediate Encoding
ADDQ/SUBQ use a 3-bit immediate field where **0 encodes 8**, not 0:
```rust
let imm = if data == 0 { 8u32 } else { u32::from(data) };
```

### MOVEA.W Sign Extension
Word moves to address registers sign-extend to 32 bits:
```rust
// MOVEA.W with source = 0xFFFF becomes 0xFFFF_FFFF in An
```

### DBcc Always Has Word Displacement
Unlike Bcc which can use byte displacement, DBcc always has a word displacement following the opcode.

### Test Cycle Requirements
Complex instructions need many cycles to complete:
```rust
#[test]
fn test_move_long_immediate() {
    // MOVE.L #$12345678, D0 takes 12+ cycles
    for _ in 0..20 {
        cpu.tick(&mut bus);
    }
}
```

## Remaining Work

- Memory operand forms for arithmetic (read-modify-write ADD, SUB, etc.)
- MOVEM actual memory transfers (skeleton in place)
- NBCD, TAS (BCD operations)
- MOVEP (peripheral I/O)

## Related Documentation

- Z80 micro-op architecture: `crates/emu-z80/src/microcode.rs`
- 6502 cycle counter pattern: `crates/emu-6502/src/cpu.rs`
- Bus trait: `crates/emu-core/src/bus.rs`
