---
title: "M68000 Condition Code Instructions (Scc, CHK)"
category: implementation
tags:
  - m68000
  - cpu-emulation
  - condition-codes
  - scc
  - chk
  - flag-evaluation
  - rust
module: emu-68000
symptoms:
  - implementing Scc (Set on Condition) instruction
  - implementing CHK (Check Register Against Bounds)
  - evaluating 68000 condition codes from status register
  - understanding N/V flag interaction for signed comparisons
severity: info
date: 2026-02-03
---

# M68000 Condition Code Instructions

This documents the implementation of Scc (Set on Condition) and CHK (Check Register Against Bounds) instructions in the `emu-68000` crate.

## Scc - Set on Condition

### Purpose

Scc sets a byte to all ones ($FF) if a condition is true, or all zeros ($00) if false. It's the 68000's equivalent of a boolean result from a comparison.

### Condition Codes

The 68000 has 16 condition codes (0-15), evaluated from the status register flags:

```rust
pub fn condition(sr: u16, cc: u8) -> bool {
    match cc & 0x0F {
        0x0 => true,                                    // T (true/always)
        0x1 => false,                                   // F (false/never)
        0x2 => (sr & C) == 0 && (sr & Z) == 0,          // HI (higher)
        0x3 => (sr & C) != 0 || (sr & Z) != 0,          // LS (lower or same)
        0x4 => (sr & C) == 0,                           // CC/HS (carry clear)
        0x5 => (sr & C) != 0,                           // CS/LO (carry set)
        0x6 => (sr & Z) == 0,                           // NE (not equal)
        0x7 => (sr & Z) != 0,                           // EQ (equal)
        0x8 => (sr & V) == 0,                           // VC (overflow clear)
        0x9 => (sr & V) != 0,                           // VS (overflow set)
        0xA => (sr & N) == 0,                           // PL (plus/positive)
        0xB => (sr & N) != 0,                           // MI (minus/negative)
        0xC => { (sr & N) != 0 } == { (sr & V) != 0 },  // GE (greater or equal)
        0xD => { (sr & N) != 0 } != { (sr & V) != 0 },  // LT (less than)
        0xE => (sr & Z) == 0 && { (sr & N) != 0 } == { (sr & V) != 0 }, // GT (greater)
        0xF => (sr & Z) != 0 || { (sr & N) != 0 } != { (sr & V) != 0 }, // LE (less or equal)
        _ => unreachable!(),
    }
}
```

### Signed vs Unsigned Comparison

| Condition | Meaning | For Unsigned | For Signed |
|-----------|---------|--------------|------------|
| HI (2) | > | Yes | No |
| LS (3) | <= | Yes | No |
| CC (4) | >= | Yes | No |
| CS (5) | < | Yes | No |
| GE (12) | >= | No | Yes |
| LT (13) | < | No | Yes |
| GT (14) | > | No | Yes |
| LE (15) | <= | No | Yes |

Signed conditions use N⊕V (N XOR V) because:
- After SUB, if no overflow (V=0), N directly indicates sign of result
- If overflow (V=1), the sign is inverted from what N shows
- N⊕V corrects for this

### Implementation

```rust
fn exec_scc(&mut self, condition: u8, mode: u8, ea_reg: u8) {
    // Evaluate condition against current flags
    let value: u8 = if Status::condition(self.regs.sr, condition) {
        0xFF  // All ones if true
    } else {
        0x00  // All zeros if false
    };

    if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
        match addr_mode {
            AddrMode::DataReg(r) => {
                // Set low byte of register, preserve upper bytes
                self.regs.d[r as usize] =
                    (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(value);

                // Timing: 4 cycles if false, 6 if true
                self.queue_internal(if value == 0xFF { 6 } else { 4 });
            }
            _ => {
                // Memory destination
                self.addr = self.calc_ea_addr(addr_mode);
                self.data = u32::from(value);
                self.queue_write_ops(Size::Byte);
            }
        }
    }
}
```

### Opcode Encoding

```
Scc: 0101 cccc 11 mmm rrr
     │    │    │  │   └── EA register
     │    │    │  └────── EA mode
     │    │    └───────── Size = 11 (identifies Scc vs ADDQ/SUBQ)
     │    └────────────── Condition code (0-15)
     └─────────────────── Group 5 identifier
```

### Test Cases

```rust
#[test]
fn test_seq_true() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SEQ D0 (opcode: 0x57C0) - Set if equal
    load_words(&mut bus, 0x1000, &[0x57C0]);
    cpu.regs.sr |= Z; // Set Z flag so condition is true
    cpu.regs.d[0] = 0x0000_0000;

    run_instruction(&mut cpu, &mut bus);

    // Low byte should be 0xFF
    assert_eq!(cpu.regs.d[0] & 0xFF, 0xFF);
}

#[test]
fn test_seq_false() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SEQ D0 (opcode: 0x57C0)
    load_words(&mut bus, 0x1000, &[0x57C0]);
    cpu.regs.sr &= !Z; // Clear Z flag so condition is false
    cpu.regs.d[0] = 0x0000_00FF;

    run_instruction(&mut cpu, &mut bus);

    // Low byte should be 0x00
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x00);
}
```

## CHK - Check Register Against Bounds

### Purpose

CHK is an array bounds checking instruction. It compares a data register against 0 and an upper bound, triggering a CHK exception (vector 6) if out of bounds.

### Behavior

```
CHK <ea>,Dn

If Dn < 0:
    Set N flag
    Trigger CHK exception (vector 6)
Else if Dn > <ea>:
    Clear N flag
    Trigger CHK exception (vector 6)
Else:
    No exception, continue execution
```

### Implementation

```rust
fn exec_chk(&mut self, op: u16) {
    let reg = ((op >> 9) & 7) as usize;
    let mode = ((op >> 3) & 7) as u8;
    let ea_reg = (op & 7) as u8;

    // Get the data register value as signed word
    let dn = self.regs.d[reg] as i16;

    if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
        let upper_bound = match addr_mode {
            AddrMode::DataReg(r) => self.regs.d[r as usize] as i16,
            AddrMode::Immediate => {
                // Need to fetch immediate word
                self.queue_extension_fetch_and_continue();
                return;
            }
            _ => {
                // Memory source - fetch from EA
                self.queue_read_and_continue();
                return;
            }
        };

        // Check bounds
        if dn < 0 {
            // Negative value - set N flag and trigger exception
            self.regs.sr |= N;
            self.exception(6); // CHK exception
        } else if dn > upper_bound {
            // Above upper bound - clear N flag and trigger exception
            self.regs.sr &= !N;
            self.exception(6); // CHK exception
        } else {
            // Value is within bounds [0, upper_bound]
            self.queue_internal(10);
        }
    }
}
```

### Opcode Encoding

```
CHK: 0100 rrr 110 mmm eee
     │    │   │   │   └── EA register
     │    │   │   └────── EA mode
     │    │   └────────── Opcode identifier (110)
     │    └────────────── Data register (to check)
     └─────────────────── Group 4 identifier
```

### Why CHK Exists

CHK is a single-instruction array bounds check, more efficient than:

```asm
; Without CHK
    TST.W   D0          ; Check if negative
    BMI     bounds_err
    CMP.W   max_index,D0 ; Check upper bound
    BGT     bounds_err

; With CHK
    CHK     max_index,D0 ; Single instruction, auto-exception
```

The exception allows centralized bounds error handling via the CHK exception vector.

## Common Mistakes

### 1. Scc Only Sets a Byte

```rust
// WRONG: Setting the whole register
self.regs.d[r] = if condition { 0xFFFF_FFFF } else { 0 };

// CORRECT: Only the low byte
self.regs.d[r] = (self.regs.d[r] & 0xFFFF_FF00) | u32::from(value);
```

### 2. CHK Uses Word Size

CHK always operates on word-size values (16-bit signed), even on the 68020+ which has a long form.

### 3. N Flag in CHK Sets Direction

The N flag after a CHK exception tells the handler which bound was violated:
- N=1: Value was negative (below lower bound)
- N=0: Value exceeded upper bound

## Timing

| Instruction | Condition | Cycles |
|-------------|-----------|--------|
| Scc Dn | False | 4 |
| Scc Dn | True | 6 |
| Scc <ea> | - | 8+ (depends on EA) |
| CHK | No exception | 10 |
| CHK | Exception | 40+ (includes exception processing) |

## Related Documentation

- `docs/solutions/implementation/68000-micro-op-architecture.md` - Overall 68000 patterns
- `docs/solutions/implementation/68000-multi-precision-arithmetic.md` - X flag and arithmetic
- M68000 Programmer's Reference Manual, Section 4 - Instruction Set
