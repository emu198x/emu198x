# 6502 Illegal Opcode Implementation for Game Compatibility

---
title: "6502 illegal opcode implementation for game compatibility"
tags:
  - 6502
  - illegal-opcodes
  - undocumented
  - nes
  - c64
  - emulation
category: implementation
module: emu-6502
date_solved: 2026-02-03
---

## Overview

Complete implementation of stable 6502 illegal/undocumented opcodes required for NES and C64 game compatibility. This completes M3 milestone requirements.

## Implementation Summary

### Categories Implemented

#### 1. Combined RMW Operations (7 addressing modes each)

These opcodes combine a read-modify-write operation with an accumulator operation:

| Opcode | Name | Operation | Description |
|--------|------|-----------|-------------|
| SLO | ASL + ORA | M = M << 1; A = A \| M | Shift Left then OR |
| RLA | ROL + AND | M = ROL(M); A = A & M | Rotate Left then AND |
| SRE | LSR + EOR | M = M >> 1; A = A ^ M | Shift Right then XOR |
| RRA | ROR + ADC | M = ROR(M); A = A + M + C | Rotate Right then ADC |
| DCP | DEC + CMP | M = M - 1; compare A,M | Decrement then Compare |
| ISC | INC + SBC | M = M + 1; A = A - M - !C | Increment then Subtract |

**Addressing modes:** zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y

**Opcode mapping:**

```
SLO: $07 (zp), $17 (zp,X), $0F (abs), $1F (abs,X), $1B (abs,Y), $03 (izx), $13 (izy)
RLA: $27 (zp), $37 (zp,X), $2F (abs), $3F (abs,X), $3B (abs,Y), $23 (izx), $33 (izy)
SRE: $47 (zp), $57 (zp,X), $4F (abs), $5F (abs,X), $5B (abs,Y), $43 (izx), $53 (izy)
RRA: $67 (zp), $77 (zp,X), $6F (abs), $7F (abs,X), $7B (abs,Y), $63 (izx), $73 (izy)
DCP: $C7 (zp), $D7 (zp,X), $CF (abs), $DF (abs,X), $DB (abs,Y), $C3 (izx), $D3 (izy)
ISC: $E7 (zp), $F7 (zp,X), $EF (abs), $FF (abs,X), $FB (abs,Y), $E3 (izx), $F3 (izy)
```

#### 2. Load/Store Operations

| Opcode | Name | Operation | Addressing Modes |
|--------|------|-----------|------------------|
| LAX | LDA + LDX | A = X = M | zp, zp,Y, abs, abs,Y, (zp,X), (zp),Y |
| SAX | Store A AND X | M = A & X | zp, zp,Y, abs, (zp,X) |

**Opcode mapping:**

```
LAX: $A7 (zp), $B7 (zp,Y), $AF (abs), $BF (abs,Y), $A3 (izx), $B3 (izy)
SAX: $87 (zp), $97 (zp,Y), $8F (abs), $83 (izx)
```

#### 3. Immediate Operations

| Opcode | Name | Operation | Description |
|--------|------|-----------|-------------|
| ANC | AND + copy N to C | A = A & imm; C = N | $0B, $2B |
| ALR | AND + LSR | A = (A & imm) >> 1 | $4B |
| ARR | AND + ROR (weird flags) | A = ROR(A & imm) | $6B |
| AXS | (A AND X) - imm | X = (A & X) - imm | $CB |

**ARR flag behavior:** C = bit 6 of result, V = bit 6 XOR bit 5

#### 4. Undocumented NOPs

| Type | Bytes | Cycles | Opcodes |
|------|-------|--------|---------|
| Single-byte | 1 | 2 | $1A, $3A, $5A, $7A, $DA, $FA |
| Immediate | 2 | 2 | $80, $82, $89, $C2, $E2 |
| Zero page | 2 | 3 | $04, $44, $64 |
| Zero page,X | 2 | 4 | $14, $34, $54, $74, $D4, $F4 |
| Absolute | 3 | 4 | $0C |
| Absolute,X | 3 | 4/5 | $1C, $3C, $5C, $7C, $DC, $FC |

#### 5. JAM/KIL Halt Instructions

Opcodes that halt the CPU (requires reset to recover):

```
$02, $12, $22, $32, $42, $52, $62, $72, $92, $B2, $D2, $F2
```

## Technical Implementation Details

### New RMW Addressing Modes

Three new addressing mode handlers were added for illegal RMW opcodes that don't exist in the official instruction set:

#### addr_aby_rmw (Absolute,Y RMW - 7 cycles)

```rust
fn addr_aby_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
    match self.cycle {
        1 => { /* fetch low address byte */ }
        2 => { /* fetch high byte, add Y (may cross page) */ }
        3 => { /* dummy read, fix page crossing */ }
        4 => { /* read value from effective address */ }
        5 => { /* write original value back (RMW pattern) */ }
        6 => { /* write modified value */ }
    }
}
```

#### addr_izx_rmw (Indexed Indirect RMW - 8 cycles)

```rust
fn addr_izx_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
    match self.cycle {
        1 => { /* fetch pointer */ }
        2 => { /* dummy read, add X */ }
        3 => { /* fetch low byte of address */ }
        4 => { /* fetch high byte of address */ }
        5 => { /* read value */ }
        6 => { /* write original value */ }
        7 => { /* write modified value */ }
    }
}
```

#### addr_izy_rmw (Indirect Indexed RMW - 8 cycles)

```rust
fn addr_izy_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
    match self.cycle {
        1 => { /* fetch pointer */ }
        2 => { /* fetch low byte from pointer */ }
        3 => { /* fetch high byte, add Y */ }
        4 => { /* dummy read, fix page crossing */ }
        5 => { /* read value */ }
        6 => { /* write original value */ }
        7 => { /* write modified value */ }
    }
}
```

### Operation Implementations

Example of a combined RMW operation (SLO):

```rust
/// SLO - Shift Left then OR with accumulator
fn do_slo(&mut self, val: u8) -> u8 {
    self.regs.p.set_if(C, val & 0x80 != 0);
    let result = val << 1;
    self.regs.a |= result;
    self.regs.p.update_nz(self.regs.a);
    result
}
```

Example of weird flag behavior (ARR):

```rust
/// ARR - AND then ROR with weird flags
fn do_arr(&mut self, val: u8) {
    self.regs.a &= val;
    let carry = if self.regs.p.is_set(C) { 0x80 } else { 0 };
    self.regs.a = (self.regs.a >> 1) | carry;
    self.regs.p.update_nz(self.regs.a);
    // Weird flag behavior: C = bit 6, V = bit 6 XOR bit 5
    self.regs.p.set_if(C, self.regs.a & 0x40 != 0);
    self.regs.p.set_if(V, (self.regs.a & 0x40 != 0) != (self.regs.a & 0x20 != 0));
}
```

## Unit Tests

13 unit tests were added for illegal opcodes:

| Test | Opcode(s) Tested |
|------|------------------|
| `test_illegal_lax_zeropage` | LAX $A7 |
| `test_illegal_sax_zeropage` | SAX $87 |
| `test_illegal_slo_zeropage` | SLO $07 |
| `test_illegal_rla_zeropage` | RLA $27 |
| `test_illegal_dcp_zeropage` | DCP $C7 |
| `test_illegal_isc_zeropage` | ISC $E7 |
| `test_illegal_anc_immediate` | ANC $0B |
| `test_illegal_alr_immediate` | ALR $4B |
| `test_illegal_axs_immediate` | AXS $CB |
| `test_illegal_nop_single_byte` | NOP $1A |
| `test_illegal_nop_two_byte` | NOP $80 |
| `test_illegal_nop_three_byte` | NOP $0C |
| `test_illegal_jam_halts_cpu` | JAM $02 |

## Verification

### Test Results

```
Unit tests: 5 passed
Instruction tests: 27 passed (including 13 illegal opcode tests)
Dormann functional: PASS (30.6M instructions, 96.2M cycles)
Dormann decimal: PASS (14.5M instructions, 46M cycles)
```

All Dormann tests continue to pass, confirming no regressions in documented instruction behavior.

### Game Compatibility Impact

These illegal opcodes are used by:

**NES:**
- Many games use illegal opcodes for copy protection
- SLO, RLA, SRE, RRA commonly used for optimized RMW operations
- LAX/SAX for compact code

**C64:**
- Demos extensively use illegal opcodes
- Many crackers used illegal NOPs
- Games like "Turbo Outrun" use illegal opcodes

## Files Modified

**File:** `/Users/stevehill/Projects/Emu198x/crates/emu-6502/src/cpu.rs`

- Added opcode dispatch for all illegal opcodes (lines 601-733)
- Added `addr_aby_rmw`, `addr_izx_rmw`, `addr_izy_rmw` addressing modes (lines 1353-1472)
- Added operation implementations: `do_lax`, `get_sax`, `do_slo`, `do_rla`, `do_sre`, `do_rra`, `do_dcp`, `do_isc`, `do_anc`, `do_alr`, `do_arr`, `do_axs` (lines 1667-1767)

**File:** `/Users/stevehill/Projects/Emu198x/crates/emu-6502/tests/instructions.rs`

- Added 13 illegal opcode tests (lines 521-809)

## Opcodes NOT Implemented

The following unstable/unreliable illegal opcodes are NOT implemented:

| Opcode | Name | Reason |
|--------|------|--------|
| $AB | LAX immediate | Unstable (AND with magic constant varies by chip) |
| $8B | XAA | Highly unstable |
| $93, $9F | SHA | Unstable store operations |
| $9E | SHX | Unstable store operations |
| $9C | SHY | Unstable store operations |
| $9B | TAS | Unstable |
| $BB | LAS | Rarely used |

These opcodes behave inconsistently across different 6502 chips and are not used by games that need to be compatible with all hardware.

## References

- [NESdev Wiki - CPU unofficial opcodes](https://www.nesdev.org/wiki/CPU_unofficial_opcodes)
- [6502 Illegal Opcodes](http://www.oxyron.de/html/opcodes02.html)
- [Extra Instructions of the 65xx Series CPU](http://www.ffd2.com/fridge/docs/6502-NMOS.extra.opcodes)
- [Visual 6502](http://visual6502.org) - Silicon-level verification

## Related Documentation

- [6502 BRK stale address bug](../logic-errors/6502-brk-stale-addr-vector.md)
- [6502 decimal test setup](../testing/6502-decimal-test-setup.md)
- [docs/milestones.md](../../milestones.md) - M3 verification criteria
