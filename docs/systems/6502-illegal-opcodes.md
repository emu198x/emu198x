# 6502 Illegal/Undocumented Opcodes

Reference documentation for the stable illegal opcodes implemented in the emu-6502 crate.

## Overview

The NMOS 6502 has 256 possible opcodes, but only ~151 are documented. The remaining opcodes produce consistent, predictable behavior due to how the CPU's decode ROM is structured. Many games and demos rely on these "illegal" opcodes for:

- **Code size reduction** - LAX combines LDA+TAX in one instruction
- **Speed optimization** - RMW illegals save cycles in tight loops
- **Copy protection** - Harder to disassemble/understand

## Implemented Opcodes

### Combined Read-Modify-Write Operations

These opcodes read memory, modify it, AND perform an ALU operation with the accumulator.

| Mnemonic | Operation | Flags | Addressing Modes |
|----------|-----------|-------|------------------|
| **SLO** | M = M << 1; A = A \| M | N, Z, C | zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y |
| **RLA** | M = ROL(M); A = A & M | N, Z, C | zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y |
| **SRE** | M = M >> 1; A = A ^ M | N, Z, C | zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y |
| **RRA** | M = ROR(M); A = ADC(A, M) | N, V, Z, C | zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y |
| **DCP** | M = M - 1; CMP(A, M) | N, Z, C | zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y |
| **ISC** | M = M + 1; A = SBC(A, M) | N, V, Z, C | zp, zp,X, abs, abs,X, abs,Y, (zp,X), (zp),Y |

### Load/Store Operations

| Mnemonic | Operation | Flags | Addressing Modes |
|----------|-----------|-------|------------------|
| **LAX** | A = M; X = M | N, Z | zp, zp,Y, abs, abs,Y, (zp,X), (zp),Y |
| **SAX** | M = A & X | None | zp, zp,Y, abs, (zp,X) |

### Immediate-Only Operations

| Mnemonic | Operation | Flags |
|----------|-----------|-------|
| **ANC** | A = A & imm; C = N | N, Z, C |
| **ALR** | A = (A & imm) >> 1 | N, Z, C |
| **ARR** | A = (A & imm) ROR 1; special flags | N, V, Z, C |
| **AXS** | X = (A & X) - imm (no borrow) | N, Z, C |

### Undocumented NOPs

| Bytes | Cycles | Opcodes |
|-------|--------|---------|
| 1 | 2 | $1A, $3A, $5A, $7A, $DA, $FA |
| 2 (imm) | 2 | $80, $82, $89, $C2, $E2 |
| 2 (zp) | 3 | $04, $44, $64 |
| 2 (zp,X) | 4 | $14, $34, $54, $74, $D4, $F4 |
| 3 (abs) | 4 | $0C |
| 3 (abs,X) | 4/5 | $1C, $3C, $5C, $7C, $DC, $FC |

### CPU Halt

| Mnemonic | Opcodes |
|----------|---------|
| **JAM/KIL** | $02, $12, $22, $32, $42, $52, $62, $72, $92, $B2, $D2, $F2 |

The CPU enters `State::Stopped` and will not execute further instructions until reset.

## Opcode Matrix

```
     0   1   2   3   4   5   6   7   8   9   A   B   C   D   E   F
0   BRK ORA JAM SLO NOP ORA ASL SLO PHP ORA ASL ANC NOP ORA ASL SLO
1   BPL ORA JAM SLO NOP ORA ASL SLO CLC ORA NOP SLO NOP ORA ASL SLO
2   JSR AND JAM RLA NOP AND ROL RLA PLP AND ROL ANC BIT AND ROL RLA
3   BMI AND JAM RLA NOP AND ROL RLA SEC AND NOP RLA NOP AND ROL RLA
4   RTI EOR JAM SRE NOP EOR LSR SRE PHA EOR LSR ALR JMP EOR LSR SRE
5   BVC EOR JAM SRE NOP EOR LSR SRE CLI EOR NOP SRE NOP EOR LSR SRE
6   RTS ADC JAM RRA NOP ADC ROR RRA PLA ADC ROR ARR JMP ADC ROR RRA
7   BVS ADC JAM RRA NOP ADC ROR RRA SEI ADC NOP RRA NOP ADC ROR RRA
8   NOP STA JAM SAX STY STA STX SAX DEY NOP TXA --- STY STA STX SAX
9   BCC STA JAM --- STY STA STX SAX TYA STA TXS --- --- STA --- ---
A   LDY LDA LDX LAX LDY LDA LDX LAX TAY LDA TAX --- LDY LDA LDX LAX
B   BCS LDA JAM LAX LDY LDA LDX LAX CLV LDA TSX --- LDY LDA LDX LAX
C   CPY CMP JAM DCP CPY CMP DEC DCP INY CMP DEX AXS CPY CMP DEC DCP
D   BNE CMP JAM DCP NOP CMP DEC DCP CLD CMP NOP DCP NOP CMP DEC DCP
E   CPX SBC JAM ISC CPX SBC INC ISC INX SBC NOP --- CPX SBC INC ISC
F   BEQ SBC JAM ISC NOP SBC INC ISC SED SBC NOP ISC NOP SBC INC ISC
```

Legend: Legal opcodes in normal text, **illegal implemented**, `---` = unstable/not implemented

## Implementation Details

### New Addressing Modes

Three RMW addressing modes were added specifically for illegal opcodes:

```rust
// Absolute,Y RMW (7 cycles) - no page-cross optimization for RMW
fn addr_aby_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8)

// Indexed indirect RMW (8 cycles)
fn addr_izx_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8)

// Indirect indexed RMW (8 cycles)
fn addr_izy_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8)
```

### Operation Functions

Combined operations reuse existing ALU logic:

```rust
fn do_slo(&mut self, val: u8) -> u8 {
    self.regs.p.set_if(C, val & 0x80 != 0);  // ASL sets carry
    let result = val << 1;
    self.regs.a |= result;                    // ORA with result
    self.regs.p.update_nz(self.regs.a);
    result  // Return shifted value for memory write
}

fn do_rra(&mut self, val: u8) -> u8 {
    let carry = if self.regs.p.is_set(C) { 0x80 } else { 0 };
    let new_carry = val & 0x01 != 0;
    let result = (val >> 1) | carry;          // ROR
    self.regs.p.set_if(C, new_carry);
    self.do_adc(result);                      // Reuse ADC (handles decimal mode)
    result
}
```

### ARR Flag Behavior

ARR has unusual flag behavior that differs from a simple AND+ROR:

```rust
fn do_arr(&mut self, val: u8) {
    self.regs.a &= val;
    let carry = if self.regs.p.is_set(C) { 0x80 } else { 0 };
    self.regs.a = (self.regs.a >> 1) | carry;
    self.regs.p.update_nz(self.regs.a);
    // C = bit 6 of result, V = bit 6 XOR bit 5
    self.regs.p.set_if(C, self.regs.a & 0x40 != 0);
    self.regs.p.set_if(V, (self.regs.a & 0x40 != 0) != (self.regs.a & 0x20 != 0));
}
```

## Cycle Counts

| Mode | Read | RMW |
|------|------|-----|
| Immediate | 2 | - |
| Zero Page | 3 | 5 |
| Zero Page,X/Y | 4 | 6 |
| Absolute | 4 | 6 |
| Absolute,X/Y | 4/5* | 7 |
| (Indirect,X) | 6 | 8 |
| (Indirect),Y | 5/6* | 8 |

*Add 1 cycle on page crossing for read operations. RMW operations always take the longer time.

## Not Implemented (Unstable)

These opcodes have unpredictable behavior varying by chip, temperature, or voltage:

| Opcode | Name | Issue |
|--------|------|-------|
| $AB | LAX imm | "Magic constant" varies ($00, $EE, $FF) |
| $8B | XAA/ANE | Same instability |
| $9F, $93 | AHX/SHA | Involves mysterious (H+1) term |
| $9E | SHX/SXA | Address high byte interaction |
| $9C | SHY/SYA | Address high byte interaction |
| $9B | TAS/SHS | Complex stack pointer interaction |
| $BB | LAS | Unstable in some contexts |

## Test Coverage

Unit tests in `crates/emu-6502/tests/instructions.rs`:

- `test_illegal_lax_zeropage`
- `test_illegal_sax_zeropage`
- `test_illegal_slo_zeropage`
- `test_illegal_rla_zeropage`
- `test_illegal_dcp_zeropage`
- `test_illegal_isc_zeropage`
- `test_illegal_anc_immediate`
- `test_illegal_alr_immediate`
- `test_illegal_axs_immediate`
- `test_illegal_nop_single_byte`
- `test_illegal_nop_two_byte`
- `test_illegal_nop_three_byte`
- `test_illegal_jam_halts_cpu`

## Games Using Illegal Opcodes

| Game | Platform | Opcodes |
|------|----------|---------|
| Arkanoid | NES | LAX |
| Beauty and the Beast | NES | DCP, ISC |
| Super Cars | NES | SLO |
| Puzznic | NES | LAX |
| Turbo Assembler | C64 | LAX, SAX |
| Gauntlet | C64 | LAX, SAX, DCP |

## References

- [NESdev Wiki - CPU unofficial opcodes](https://www.nesdev.org/wiki/CPU_unofficial_opcodes)
- [Masswerk - 6502 Illegal Opcodes Demystified](https://www.masswerk.at/nowgobang/2021/6502-illegal-opcodes)
- [Oxyron - 6502 Opcodes](https://www.oxyron.de/html/opcodes02.html)
- [Visual 6502](http://visual6502.org/) - Silicon-level verification
