//! Z80 disassembler.
//!
//! Handles all prefix groups: unprefixed, CB, ED, DD/FD (IX/IY), and
//! DD CB/FD CB (IX/IY bit ops with displacement).

/// Disassemble one instruction at the given address.
///
/// `read` returns the byte at the given address (no side effects).
/// Returns `(mnemonic_string, byte_length)`.
pub fn disassemble(addr: u16, read: impl Fn(u16) -> u8) -> (String, u8) {
    let mut d = Decoder::new(addr, &read);
    let opcode = d.next();
    let s = match opcode {
        0xCB => d.disasm_cb(),
        0xED => d.disasm_ed(),
        0xDD => d.disasm_indexed("IX"),
        0xFD => d.disasm_indexed("IY"),
        _ => d.disasm_unprefixed(opcode),
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
        let v = self.next();
        format!("${v:02X}")
    }

    fn imm16(&mut self) -> String {
        let lo = self.next();
        let hi = self.next();
        let v = u16::from(lo) | (u16::from(hi) << 8);
        format!("${v:04X}")
    }

    fn rel_target(&mut self) -> String {
        let d = self.next() as i8;
        let target = self
            .addr
            .wrapping_add(u16::from(self.offset))
            .wrapping_add(d as u16);
        format!("${target:04X}")
    }
}

// ---------------------------------------------------------------------------
// Helper tables
// ---------------------------------------------------------------------------

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

fn r16(n: u8) -> &'static str {
    match n & 3 {
        0 => "BC",
        1 => "DE",
        2 => "HL",
        3 => "SP",
        _ => "?",
    }
}

fn r16af(n: u8) -> &'static str {
    match n & 3 {
        0 => "BC",
        1 => "DE",
        2 => "HL",
        3 => "AF",
        _ => "?",
    }
}

fn cc(n: u8) -> &'static str {
    match n & 7 {
        0 => "NZ",
        1 => "Z",
        2 => "NC",
        3 => "C",
        4 => "PO",
        5 => "PE",
        6 => "P",
        7 => "M",
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

fn rot(n: u8) -> &'static str {
    match n & 7 {
        0 => "RLC",
        1 => "RRC",
        2 => "RL",
        3 => "RR",
        4 => "SLA",
        5 => "SRA",
        6 => "SLL",
        7 => "SRL",
        _ => "?",
    }
}

// ---------------------------------------------------------------------------
// Unprefixed opcodes
// ---------------------------------------------------------------------------

impl<F: Fn(u16) -> u8> Decoder<'_, F> {
    fn disasm_unprefixed(&mut self, opcode: u8) -> String {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let p = y >> 1;
        let q = y & 1;

        match (x, z) {
            (0, 0) => match y {
                0 => "NOP".into(),
                1 => "EX AF,AF'".into(),
                2 => {
                    let t = self.rel_target();
                    format!("DJNZ {t}")
                }
                3 => {
                    let t = self.rel_target();
                    format!("JR {t}")
                }
                _ => {
                    let t = self.rel_target();
                    format!("JR {},{t}", cc(y - 4))
                }
            },
            (0, 1) if q == 0 => {
                let v = self.imm16();
                format!("LD {},{v}", r16(p))
            }
            (0, 1) => format!("ADD HL,{}", r16(p)),
            (0, 2) => match (p, q) {
                (0, 0) => "LD (BC),A".into(),
                (1, 0) => "LD (DE),A".into(),
                (2, 0) => {
                    let v = self.imm16();
                    format!("LD ({v}),HL")
                }
                (3, 0) => {
                    let v = self.imm16();
                    format!("LD ({v}),A")
                }
                (0, 1) => "LD A,(BC)".into(),
                (1, 1) => "LD A,(DE)".into(),
                (2, 1) => {
                    let v = self.imm16();
                    format!("LD HL,({v})")
                }
                (3, 1) => {
                    let v = self.imm16();
                    format!("LD A,({v})")
                }
                _ => "???".into(),
            },
            (0, 3) if q == 0 => format!("INC {}", r16(p)),
            (0, 3) => format!("DEC {}", r16(p)),
            (0, 4) => format!("INC {}", r8(y)),
            (0, 5) => format!("DEC {}", r8(y)),
            (0, 6) => {
                let v = self.imm8();
                format!("LD {},{v}", r8(y))
            }
            (0, 7) => match y {
                0 => "RLCA".into(),
                1 => "RRCA".into(),
                2 => "RLA".into(),
                3 => "RRA".into(),
                4 => "DAA".into(),
                5 => "CPL".into(),
                6 => "SCF".into(),
                7 => "CCF".into(),
                _ => "???".into(),
            },
            (1, _) if y == 6 && z == 6 => "HALT".into(),
            (1, _) => format!("LD {},{}", r8(y), r8(z)),
            (2, _) => {
                let a = alu(y);
                let r = r8(z);
                if a.ends_with(',') {
                    format!("{a}{r}")
                } else {
                    format!("{a} {r}")
                }
            }
            (3, 0) => format!("RET {}", cc(y)),
            (3, 1) if q == 0 => format!("POP {}", r16af(p)),
            (3, 1) => match p {
                0 => "RET".into(),
                1 => "EXX".into(),
                2 => "JP (HL)".into(),
                3 => "LD SP,HL".into(),
                _ => "???".into(),
            },
            (3, 2) => {
                let v = self.imm16();
                format!("JP {},{v}", cc(y))
            }
            (3, 3) => match y {
                0 => {
                    let v = self.imm16();
                    format!("JP {v}")
                }
                2 => {
                    let v = self.imm8();
                    format!("OUT ({v}),A")
                }
                3 => {
                    let v = self.imm8();
                    format!("IN A,({v})")
                }
                4 => "EX (SP),HL".into(),
                5 => "EX DE,HL".into(),
                6 => "DI".into(),
                7 => "EI".into(),
                _ => "???".into(),
            },
            (3, 4) => {
                let v = self.imm16();
                format!("CALL {},{v}", cc(y))
            }
            (3, 5) if q == 0 => format!("PUSH {}", r16af(p)),
            (3, 5) => {
                let v = self.imm16();
                format!("CALL {v}")
            }
            (3, 6) => {
                let v = self.imm8();
                let a = alu(y);
                if a.ends_with(',') {
                    format!("{a}{v}")
                } else {
                    format!("{a} {v}")
                }
            }
            (3, 7) => format!("RST ${:02X}", y * 8),
            _ => "???".into(),
        }
    }

    // CB prefix (bit operations)
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
            _ => "???".into(),
        }
    }

    // ED prefix (extended instructions)
    fn disasm_ed(&mut self) -> String {
        let opcode = self.next();

        match opcode {
            0x40 => "IN B,(C)".into(),
            0x41 => "OUT (C),B".into(),
            0x42 => "SBC HL,BC".into(),
            0x43 => {
                let v = self.imm16();
                format!("LD ({v}),BC")
            }
            0x44 => "NEG".into(),
            0x45 => "RETN".into(),
            0x46 => "IM 0".into(),
            0x47 => "LD I,A".into(),
            0x48 => "IN C,(C)".into(),
            0x49 => "OUT (C),C".into(),
            0x4A => "ADC HL,BC".into(),
            0x4B => {
                let v = self.imm16();
                format!("LD BC,({v})")
            }
            0x4D => "RETI".into(),
            0x4F => "LD R,A".into(),
            0x50 => "IN D,(C)".into(),
            0x51 => "OUT (C),D".into(),
            0x52 => "SBC HL,DE".into(),
            0x53 => {
                let v = self.imm16();
                format!("LD ({v}),DE")
            }
            0x56 => "IM 1".into(),
            0x57 => "LD A,I".into(),
            0x58 => "IN E,(C)".into(),
            0x59 => "OUT (C),E".into(),
            0x5A => "ADC HL,DE".into(),
            0x5B => {
                let v = self.imm16();
                format!("LD DE,({v})")
            }
            0x5E => "IM 2".into(),
            0x5F => "LD A,R".into(),
            0x60 => "IN H,(C)".into(),
            0x61 => "OUT (C),H".into(),
            0x62 => "SBC HL,HL".into(),
            0x63 => {
                let v = self.imm16();
                format!("LD ({v}),HL")
            }
            0x67 => "RRD".into(),
            0x68 => "IN L,(C)".into(),
            0x69 => "OUT (C),L".into(),
            0x6A => "ADC HL,HL".into(),
            0x6B => {
                let v = self.imm16();
                format!("LD HL,({v})")
            }
            0x6F => "RLD".into(),
            0x72 => "SBC HL,SP".into(),
            0x73 => {
                let v = self.imm16();
                format!("LD ({v}),SP")
            }
            0x78 => "IN A,(C)".into(),
            0x79 => "OUT (C),A".into(),
            0x7A => "ADC HL,SP".into(),
            0x7B => {
                let v = self.imm16();
                format!("LD SP,({v})")
            }
            0xA0 => "LDI".into(),
            0xA1 => "CPI".into(),
            0xA2 => "INI".into(),
            0xA3 => "OUTI".into(),
            0xA8 => "LDD".into(),
            0xA9 => "CPD".into(),
            0xAA => "IND".into(),
            0xAB => "OUTD".into(),
            0xB0 => "LDIR".into(),
            0xB1 => "CPIR".into(),
            0xB2 => "INIR".into(),
            0xB3 => "OTIR".into(),
            0xB8 => "LDDR".into(),
            0xB9 => "CPDR".into(),
            0xBA => "INDR".into(),
            0xBB => "OTDR".into(),
            _ => format!("DB $ED,${opcode:02X}"),
        }
    }

    // DD/FD prefix (IX/IY indexed instructions)
    fn disasm_indexed(&mut self, reg: &str) -> String {
        let opcode = self.next();

        if opcode == 0xCB {
            // DDCB/FDCB: displacement before opcode
            let d = self.next() as i8;
            let op = self.next();
            let disp = format_disp(reg, d);
            let x = op >> 6;
            let y = (op >> 3) & 7;
            return match x {
                0 => format!("{} {disp}", rot(y)),
                1 => format!("BIT {y},{disp}"),
                2 => format!("RES {y},{disp}"),
                3 => format!("SET {y},{disp}"),
                _ => "???".into(),
            };
        }

        match opcode {
            0x09 | 0x19 | 0x29 | 0x39 => {
                let p = (opcode >> 4) & 3;
                let src = match p {
                    0 => "BC",
                    1 => "DE",
                    2 => reg,
                    3 => "SP",
                    _ => "?",
                };
                format!("ADD {reg},{src}")
            }
            0x21 => {
                let v = self.imm16();
                format!("LD {reg},{v}")
            }
            0x22 => {
                let v = self.imm16();
                format!("LD ({v}),{reg}")
            }
            0x23 => format!("INC {reg}"),
            0x2A => {
                let v = self.imm16();
                format!("LD {reg},({v})")
            }
            0x2B => format!("DEC {reg}"),
            0x34 => {
                let d = self.next() as i8;
                format!("INC {}", format_disp(reg, d))
            }
            0x35 => {
                let d = self.next() as i8;
                format!("DEC {}", format_disp(reg, d))
            }
            0x36 => {
                let d = self.next() as i8;
                let n = self.next();
                format!("LD {},${n:02X}", format_disp(reg, d))
            }
            0xE1 => format!("POP {reg}"),
            0xE3 => format!("EX (SP),{reg}"),
            0xE5 => format!("PUSH {reg}"),
            0xE9 => format!("JP ({reg})"),
            0xF9 => format!("LD SP,{reg}"),
            _ => {
                let x = opcode >> 6;
                let y = (opcode >> 3) & 7;
                let z = opcode & 7;
                match x {
                    1 => {
                        // LD r,r' with IX/IY substitution
                        let dst = self.ix_r8(reg, y);
                        let src = self.ix_r8(reg, z);
                        format!("LD {dst},{src}")
                    }
                    2 => {
                        // ALU ops
                        let operand = self.ix_r8(reg, z);
                        let a = alu(y);
                        if a.ends_with(',') {
                            format!("{a}{operand}")
                        } else {
                            format!("{a} {operand}")
                        }
                    }
                    0 if z == 4 => {
                        let o = self.ix_r8(reg, y);
                        format!("INC {o}")
                    }
                    0 if z == 5 => {
                        let o = self.ix_r8(reg, y);
                        format!("DEC {o}")
                    }
                    0 if z == 6 => {
                        let o = self.ix_r8(reg, y);
                        let v = self.imm8();
                        format!("LD {o},{v}")
                    }
                    _ => format!(
                        "DB ${:02X},${opcode:02X}",
                        if reg == "IX" { 0xDD } else { 0xFD }
                    ),
                }
            }
        }
    }

    /// Get register name with IX/IY substitution. Consumes displacement byte for (IX+d).
    fn ix_r8(&mut self, reg: &str, n: u8) -> String {
        match n & 7 {
            0 => "B".into(),
            1 => "C".into(),
            2 => "D".into(),
            3 => "E".into(),
            4 => format!("{reg}H"),
            5 => format!("{reg}L"),
            6 => {
                let d = self.next() as i8;
                format_disp(reg, d)
            }
            7 => "A".into(),
            _ => "?".into(),
        }
    }
}

fn format_disp(reg: &str, d: i8) -> String {
    if d >= 0 {
        format!("({reg}+${d:02X})")
    } else {
        format!("({reg}-${:02X})", -i16::from(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_from(data: &[u8]) -> impl Fn(u16) -> u8 + '_ {
        move |addr| *data.get(addr as usize).unwrap_or(&0)
    }

    #[test]
    fn nop() {
        let (s, len) = disassemble(0, read_from(&[0x00]));
        assert_eq!(s, "NOP");
        assert_eq!(len, 1);
    }

    #[test]
    fn ld_a_immediate() {
        let (s, len) = disassemble(0, read_from(&[0x3E, 0x42]));
        assert_eq!(s, "LD A,$42");
        assert_eq!(len, 2);
    }

    #[test]
    fn jp_absolute() {
        let (s, len) = disassemble(0, read_from(&[0xC3, 0x00, 0x80]));
        assert_eq!(s, "JP $8000");
        assert_eq!(len, 3);
    }

    #[test]
    fn ldir() {
        let (s, len) = disassemble(0, read_from(&[0xED, 0xB0]));
        assert_eq!(s, "LDIR");
        assert_eq!(len, 2);
    }

    #[test]
    fn bit_cb() {
        let (s, len) = disassemble(0, read_from(&[0xCB, 0x46]));
        assert_eq!(s, "BIT 0,(HL)");
        assert_eq!(len, 2);
    }

    #[test]
    fn ld_ix_imm() {
        let (s, len) = disassemble(0, read_from(&[0xDD, 0x21, 0x34, 0x12]));
        assert_eq!(s, "LD IX,$1234");
        assert_eq!(len, 4);
    }

    #[test]
    fn jr_relative() {
        let mut mem = vec![0u8; 0x8002];
        mem[0x8000] = 0x18; // JR
        mem[0x8001] = 0x05; // +5
        let (s, len) = disassemble(0x8000, read_from(&mem));
        assert_eq!(s, "JR $8007");
        assert_eq!(len, 2);
    }

    #[test]
    fn halt() {
        let (s, len) = disassemble(0, read_from(&[0x76]));
        assert_eq!(s, "HALT");
        assert_eq!(len, 1);
    }

    #[test]
    fn call() {
        let (s, len) = disassemble(0, read_from(&[0xCD, 0x56, 0x34]));
        assert_eq!(s, "CALL $3456");
        assert_eq!(len, 3);
    }
}
