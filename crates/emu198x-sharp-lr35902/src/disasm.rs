//! Sharp LR35902 (Game Boy "SM83") disassembler.
//!
//! The SM83 is Z80-derived but distinct: no IX/IY or shadow registers, no
//! `ED`/`DD`/`FD` prefixes (only `CB`), four jump conditions instead of eight,
//! and a handful of unique opcodes (`LDH`, `LD (C),A`, `ADD SP,r8`,
//! `LD HL,SP+r8`, the `HL+`/`HL-` loads, `STOP`, `SWAP`). Eleven opcodes are
//! unmapped on the SM83. Mnemonic style matches the workspace Z80
//! disassembler (`ADD A,B`, `SUB B`, `RST $08`, `JP (HL)`).

/// Disassemble one instruction at `addr`.
///
/// `read` returns the byte at the given address with no side effects.
/// Returns `(mnemonic, byte_length)`.
pub fn disassemble(addr: u16, read: impl Fn(u16) -> u8) -> (String, u8) {
    let mut d = Decoder::new(addr, &read);
    let opcode = d.next();
    let s = if opcode == 0xCB {
        d.disasm_cb()
    } else {
        d.disasm_unprefixed(opcode)
    };
    (s, d.len())
}

struct Decoder<'a, F: Fn(u16) -> u8> {
    addr: u16,
    offset: u8,
    read: &'a F,
}

impl<F: Fn(u16) -> u8> Decoder<'_, F> {
    fn new(addr: u16, read: &F) -> Decoder<'_, F> {
        Decoder {
            addr,
            offset: 0,
            read,
        }
    }

    fn next(&mut self) -> u8 {
        let b = (self.read)(self.addr.wrapping_add(u16::from(self.offset)));
        self.offset += 1;
        b
    }

    fn len(&self) -> u8 {
        self.offset
    }

    fn imm8(&mut self) -> String {
        format!("${:02X}", self.next())
    }

    fn imm16(&mut self) -> String {
        let lo = self.next();
        let hi = self.next();
        format!("${:04X}", u16::from(lo) | (u16::from(hi) << 8))
    }

    /// Signed 8-bit immediate as `+d` / `-d`, used by `ADD SP,r8` and
    /// `LD HL,SP+r8`.
    fn rel8(&mut self) -> String {
        let d = self.next() as i8;
        if d < 0 {
            format!("-${:02X}", d.unsigned_abs())
        } else {
            format!("+${d:02X}")
        }
    }

    /// PC-relative branch target (`JR`), resolved to an absolute address.
    fn rel_target(&mut self) -> String {
        let d = self.next() as i8;
        let target = self
            .addr
            .wrapping_add(u16::from(self.offset))
            .wrapping_add(d as u16);
        format!("${target:04X}")
    }

    fn disasm_unprefixed(&mut self, opcode: u8) -> String {
        match opcode {
            // 0x40-0x7F: LD r,r' (0x76 is HALT, not LD (HL),(HL)).
            0x76 => "HALT".into(),
            0x40..=0x7F => {
                format!("LD {},{}", r8((opcode >> 3) & 7), r8(opcode & 7))
            }
            // 0x80-0xBF: 8-bit ALU on A.
            0x80..=0xBF => {
                let a = alu((opcode >> 3) & 7);
                let r = r8(opcode & 7);
                if a.ends_with(',') {
                    format!("{a}{r}")
                } else {
                    format!("{a} {r}")
                }
            }
            _ => self.disasm_misc(opcode),
        }
    }

    /// The irregular blocks: `0x00-0x3F` and `0xC0-0xFF`.
    #[allow(clippy::too_many_lines)]
    fn disasm_misc(&mut self, opcode: u8) -> String {
        match opcode {
            0x00 => "NOP".into(),
            0x10 => {
                // STOP is encoded as `10 00`; consume the padding byte so the
                // debugger advances to the next instruction correctly.
                let _padding = self.next();
                "STOP".into()
            }
            0x76 => "HALT".into(),
            0xF3 => "DI".into(),
            0xFB => "EI".into(),
            0x07 => "RLCA".into(),
            0x0F => "RRCA".into(),
            0x17 => "RLA".into(),
            0x1F => "RRA".into(),
            0x27 => "DAA".into(),
            0x2F => "CPL".into(),
            0x37 => "SCF".into(),
            0x3F => "CCF".into(),
            0xC9 => "RET".into(),
            0xD9 => "RETI".into(),
            0xE9 => "JP (HL)".into(),
            0xF9 => "LD SP,HL".into(),

            // 16-bit immediate loads / SP store.
            0x01 | 0x11 | 0x21 | 0x31 => format!("LD {},{}", r16(opcode >> 4), self.imm16()),
            0x08 => format!("LD ({}),SP", self.imm16()),

            // INC/DEC rr.
            0x03 | 0x13 | 0x23 | 0x33 => format!("INC {}", r16(opcode >> 4)),
            0x0B | 0x1B | 0x2B | 0x3B => format!("DEC {}", r16(opcode >> 4)),
            // ADD HL,rr.
            0x09 | 0x19 | 0x29 | 0x39 => format!("ADD HL,{}", r16(opcode >> 4)),

            // INC/DEC r (and (HL)).
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                format!("INC {}", r8((opcode >> 3) & 7))
            }
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                format!("DEC {}", r8((opcode >> 3) & 7))
            }
            // LD r,d8.
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
                format!("LD {},{}", r8((opcode >> 3) & 7), self.imm8())
            }

            // Indirect A loads/stores via rr and HL+/HL-.
            0x02 => "LD (BC),A".into(),
            0x12 => "LD (DE),A".into(),
            0x22 => "LD (HL+),A".into(),
            0x32 => "LD (HL-),A".into(),
            0x0A => "LD A,(BC)".into(),
            0x1A => "LD A,(DE)".into(),
            0x2A => "LD A,(HL+)".into(),
            0x3A => "LD A,(HL-)".into(),

            // Relative jumps.
            0x18 => format!("JR {}", self.rel_target()),
            0x20 | 0x28 | 0x30 | 0x38 => {
                format!("JR {},{}", cc((opcode >> 3) & 3), self.rel_target())
            }

            // Stack push/pop.
            0xC1 | 0xD1 | 0xE1 | 0xF1 => format!("POP {}", r16stk(opcode >> 4)),
            0xC5 | 0xD5 | 0xE5 | 0xF5 => format!("PUSH {}", r16stk(opcode >> 4)),

            // Absolute jumps / calls / returns (conditional + unconditional).
            0xC3 => format!("JP {}", self.imm16()),
            0xC2 | 0xCA | 0xD2 | 0xDA => format!("JP {},{}", cc((opcode >> 3) & 3), self.imm16()),
            0xCD => format!("CALL {}", self.imm16()),
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                format!("CALL {},{}", cc((opcode >> 3) & 3), self.imm16())
            }
            0xC0 | 0xC8 | 0xD0 | 0xD8 => format!("RET {}", cc((opcode >> 3) & 3)),

            // 8-bit ALU on A with immediate.
            0xC6 => format!("ADD A,{}", self.imm8()),
            0xCE => format!("ADC A,{}", self.imm8()),
            0xD6 => format!("SUB {}", self.imm8()),
            0xDE => format!("SBC A,{}", self.imm8()),
            0xE6 => format!("AND {}", self.imm8()),
            0xEE => format!("XOR {}", self.imm8()),
            0xF6 => format!("OR {}", self.imm8()),
            0xFE => format!("CP {}", self.imm8()),

            // High-page and (C) accesses.
            0xE0 => format!("LDH ({}),A", self.imm8()),
            0xF0 => format!("LDH A,({})", self.imm8()),
            0xE2 => "LD (C),A".into(),
            0xF2 => "LD A,(C)".into(),
            0xEA => format!("LD ({}),A", self.imm16()),
            0xFA => format!("LD A,({})", self.imm16()),

            // SP arithmetic.
            0xE8 => format!("ADD SP,{}", self.rel8()),
            0xF8 => format!("LD HL,SP{}", self.rel8()),

            // Restarts.
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                format!("RST ${:02X}", opcode & 0x38)
            }

            // Unmapped on the SM83.
            0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
                format!("DB ${opcode:02X}")
            }

            _ => format!("DB ${opcode:02X}"),
        }
    }

    /// `CB`-prefixed: rotates/shifts (`x = 0`) and `BIT`/`RES`/`SET`.
    fn disasm_cb(&mut self) -> String {
        let opcode = self.next();
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        match x {
            0 => format!("{} {}", rot(y), r8(z)),
            1 => format!("BIT {y},{}", r8(z)),
            2 => format!("RES {y},{}", r8(z)),
            3 => format!("SET {y},{}", r8(z)),
            _ => unreachable!("x is two bits"),
        }
    }
}

fn r8(n: u8) -> &'static str {
    match n & 7 {
        0 => "B",
        1 => "C",
        2 => "D",
        3 => "E",
        4 => "H",
        5 => "L",
        6 => "(HL)",
        7 => "A",
        _ => "?",
    }
}

/// 16-bit register pair selected by an opcode's bits 4-5 (`SP` in slot 3).
fn r16(n: u8) -> &'static str {
    match n & 3 {
        0 => "BC",
        1 => "DE",
        2 => "HL",
        3 => "SP",
        _ => "?",
    }
}

/// 16-bit register pair for `PUSH`/`POP` (`AF` in slot 3).
fn r16stk(n: u8) -> &'static str {
    match n & 3 {
        0 => "BC",
        1 => "DE",
        2 => "HL",
        3 => "AF",
        _ => "?",
    }
}

/// SM83 branch condition (only four, unlike the Z80's eight).
fn cc(n: u8) -> &'static str {
    match n & 3 {
        0 => "NZ",
        1 => "Z",
        2 => "NC",
        3 => "C",
        _ => "?",
    }
}

fn alu(n: u8) -> &'static str {
    match n & 7 {
        0 => "ADD A,",
        1 => "ADC A,",
        2 => "SUB",
        3 => "SBC A,",
        4 => "AND",
        5 => "XOR",
        6 => "OR",
        7 => "CP",
        _ => "?",
    }
}

/// `CB` rotate/shift class — note `SWAP` in slot 6 (the SM83 replaces the
/// Z80's undocumented `SLL`).
fn rot(n: u8) -> &'static str {
    match n & 7 {
        0 => "RLC",
        1 => "RRC",
        2 => "RL",
        3 => "RR",
        4 => "SLA",
        5 => "SRA",
        6 => "SWAP",
        7 => "SRL",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::disassemble;

    /// Disassemble a byte slice at address 0.
    fn dis(bytes: &[u8]) -> (String, u8) {
        disassemble(0, |a| bytes.get(a as usize).copied().unwrap_or(0))
    }

    #[test]
    fn simple_and_immediate_forms() {
        assert_eq!(dis(&[0x00]), ("NOP".into(), 1));
        assert_eq!(dis(&[0x3E, 0x42]), ("LD A,$42".into(), 2));
        assert_eq!(dis(&[0x01, 0x34, 0x12]), ("LD BC,$1234".into(), 3));
        assert_eq!(dis(&[0xC6, 0x10]), ("ADD A,$10".into(), 2));
        assert_eq!(dis(&[0xD6, 0x10]), ("SUB $10".into(), 2));
    }

    #[test]
    fn register_blocks() {
        assert_eq!(dis(&[0x47]), ("LD B,A".into(), 1)); // 0x40-0x7F block
        assert_eq!(dis(&[0x76]), ("HALT".into(), 1)); // hole in the LD block
        assert_eq!(dis(&[0x80]), ("ADD A,B".into(), 1));
        assert_eq!(dis(&[0x90]), ("SUB B".into(), 1));
        assert_eq!(dis(&[0xBE]), ("CP (HL)".into(), 1));
    }

    #[test]
    fn sm83_unique_opcodes() {
        assert_eq!(dis(&[0x22]), ("LD (HL+),A".into(), 1));
        assert_eq!(dis(&[0x3A]), ("LD A,(HL-)".into(), 1));
        assert_eq!(dis(&[0xE0, 0x44]), ("LDH ($44),A".into(), 2));
        assert_eq!(dis(&[0xF0, 0x44]), ("LDH A,($44)".into(), 2));
        assert_eq!(dis(&[0xE2]), ("LD (C),A".into(), 1));
        assert_eq!(dis(&[0xE8, 0xFE]), ("ADD SP,-$02".into(), 2));
        assert_eq!(dis(&[0xF8, 0x08]), ("LD HL,SP+$08".into(), 2));
        assert_eq!(dis(&[0x08, 0x00, 0xC0]), ("LD ($C000),SP".into(), 3));
        assert_eq!(dis(&[0xEA, 0x00, 0xC0]), ("LD ($C000),A".into(), 3));
    }

    #[test]
    fn jumps_calls_and_restarts() {
        // JR forward 2 from address 0: target = 0 + 2 (len) + 2 = $0004.
        assert_eq!(dis(&[0x18, 0x02]), ("JR $0004".into(), 2));
        assert_eq!(dis(&[0x20, 0x02]), ("JR NZ,$0004".into(), 2));
        assert_eq!(dis(&[0xC3, 0x00, 0x40]), ("JP $4000".into(), 3));
        assert_eq!(dis(&[0xCA, 0x00, 0x40]), ("JP Z,$4000".into(), 3));
        assert_eq!(dis(&[0xCD, 0x00, 0x40]), ("CALL $4000".into(), 3));
        assert_eq!(dis(&[0xE9]), ("JP (HL)".into(), 1));
        assert_eq!(dis(&[0xDF]), ("RST $18".into(), 1));
        assert_eq!(dis(&[0xFF]), ("RST $38".into(), 1));
        assert_eq!(dis(&[0xD9]), ("RETI".into(), 1));
    }

    #[test]
    fn cb_prefixed() {
        assert_eq!(dis(&[0xCB, 0x00]), ("RLC B".into(), 2));
        assert_eq!(dis(&[0xCB, 0x36]), ("SWAP (HL)".into(), 2)); // SM83 SWAP slot
        assert_eq!(dis(&[0xCB, 0x47]), ("BIT 0,A".into(), 2));
        assert_eq!(dis(&[0xCB, 0x7E]), ("BIT 7,(HL)".into(), 2));
        assert_eq!(dis(&[0xCB, 0x86]), ("RES 0,(HL)".into(), 2));
        assert_eq!(dis(&[0xCB, 0xFF]), ("SET 7,A".into(), 2));
    }

    #[test]
    fn unmapped_opcodes_render_as_data() {
        assert_eq!(dis(&[0xD3]), ("DB $D3".into(), 1));
        assert_eq!(dis(&[0xED]), ("DB $ED".into(), 1));
    }
}
