//! 68000 instruction disassembler.
//!
//! Decodes a single instruction from memory and returns the mnemonic string
//! and its byte length. Covers the instruction groups that appear in ~95% of
//! real 68000 code.

/// Disassemble a single 68000 instruction at `addr`.
///
/// `read` fetches a byte from the given address (big-endian memory).
/// Returns `(mnemonic, byte_length)`.
pub fn disassemble(addr: u32, read: impl Fn(u32) -> u8) -> (String, u8) {
    let mut ctx = DisCtx::new(addr, &read);
    let opcode = ctx.read_word();
    let group = (opcode >> 12) & 0xF;

    let mnemonic = match group {
        0x0 => decode_group0(&mut ctx, opcode),
        0x1 => decode_move(&mut ctx, opcode, Size::Byte),
        0x2 => decode_move(&mut ctx, opcode, Size::Long),
        0x3 => decode_move(&mut ctx, opcode, Size::Word),
        0x4 => decode_group4(&mut ctx, opcode),
        0x5 => decode_group5(&mut ctx, opcode),
        0x6 => decode_group6(&mut ctx, opcode),
        0x7 => decode_moveq(opcode),
        0x8 => decode_group8(&mut ctx, opcode),
        0x9 => decode_addsub(&mut ctx, opcode, "sub"),
        0xB => decode_groupb(&mut ctx, opcode),
        0xC => decode_groupc(&mut ctx, opcode),
        0xD => decode_addsub(&mut ctx, opcode, "add"),
        0xE => decode_groupe(&mut ctx, opcode),
        _ => None,
    };

    match mnemonic {
        Some(s) => (s, ctx.bytes_read),
        None => (format!("dc.w ${:04X}", opcode), 2),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

struct DisCtx<'a> {
    addr: u32,
    base: u32,
    read: &'a dyn Fn(u32) -> u8,
    bytes_read: u8,
}

impl<'a> DisCtx<'a> {
    fn new(addr: u32, read: &'a dyn Fn(u32) -> u8) -> Self {
        Self {
            addr,
            base: addr,
            read,
            bytes_read: 0,
        }
    }

    fn read_word(&mut self) -> u16 {
        let hi = (self.read)(self.addr) as u16;
        let lo = (self.read)(self.addr.wrapping_add(1)) as u16;
        self.addr = self.addr.wrapping_add(2);
        self.bytes_read += 2;
        (hi << 8) | lo
    }

    fn read_long(&mut self) -> u32 {
        let hi = self.read_word() as u32;
        let lo = self.read_word() as u32;
        (hi << 16) | lo
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Size {
    Byte,
    Word,
    Long,
}

impl Size {
    fn suffix(self) -> &'static str {
        match self {
            Size::Byte => ".b",
            Size::Word => ".w",
            Size::Long => ".l",
        }
    }

    fn from_bits(bits: u16) -> Option<Self> {
        match bits & 3 {
            0 => Some(Size::Byte),
            1 => Some(Size::Word),
            2 => Some(Size::Long),
            _ => None,
        }
    }
}

fn format_ea(ctx: &mut DisCtx, mode: u16, reg: u16, size: Size) -> String {
    match mode {
        0 => format!("D{}", reg),
        1 => format!("A{}", reg),
        2 => format!("(A{})", reg),
        3 => format!("(A{})+", reg),
        4 => format!("-(A{})", reg),
        5 => {
            let disp = ctx.read_word() as i16;
            format!("({},A{})", disp, reg)
        }
        6 => {
            let ext = ctx.read_word();
            format_index_ext(ctx, ext, &format!("A{}", reg))
        }
        7 => match reg {
            0 => {
                let w = ctx.read_word();
                format!("(${:04X}).w", w)
            }
            1 => {
                let l = ctx.read_long();
                format!("(${:08X}).l", l)
            }
            2 => {
                let disp = ctx.read_word() as i16;
                let target = (ctx.base.wrapping_add(2) as i32).wrapping_add(disp as i32) as u32;
                format!("(${:08X},PC)", target)
            }
            3 => {
                let ext = ctx.read_word();
                format_index_ext(ctx, ext, "PC")
            }
            4 => {
                // Immediate
                match size {
                    Size::Byte => {
                        let w = ctx.read_word();
                        format!("#${:02X}", w & 0xFF)
                    }
                    Size::Word => {
                        let w = ctx.read_word();
                        format!("#${:04X}", w)
                    }
                    Size::Long => {
                        let l = ctx.read_long();
                        format!("#${:08X}", l)
                    }
                }
            }
            _ => "???".to_string(),
        },
        _ => "???".to_string(),
    }
}

/// Format an indexed addressing mode — both the brief extension word
/// (`(d8,An,Xn)`) and the 68020+ full format (base/outer displacement,
/// memory indirection, scaled index). Bit 8 selects the format. The
/// disassembler consumes any base/outer displacement words so the
/// reported instruction length stays correct.
fn format_index_ext(ctx: &mut DisCtx, ext: u16, base_reg: &str) -> String {
    let xn_type = if ext & 0x8000 != 0 { "A" } else { "D" };
    let xn_reg = (ext >> 12) & 7;
    let xn_size = if ext & 0x0800 != 0 { ".l" } else { ".w" };
    let scale = 1u16 << ((ext >> 9) & 0x3);
    let scale_str = if scale > 1 {
        format!("*{}", scale)
    } else {
        String::new()
    };

    if ext & 0x0100 == 0 {
        // Brief extension word: 8-bit displacement.
        let disp = (ext & 0xFF) as i8;
        return format!(
            "({},{},{}{}{}{})",
            disp, base_reg, xn_type, xn_reg, xn_size, scale_str
        );
    }

    // Full extension word (68020+).
    let bd: i64 = match ext & 0x0030 {
        0x20 => i64::from(ctx.read_word() as i16),
        0x30 => i64::from(ctx.read_long() as i32),
        _ => 0,
    };
    let indirect = ext & 0x0003 != 0;
    let od: i64 = if indirect {
        match ext & 0x0003 {
            0x2 => i64::from(ctx.read_word() as i16),
            0x3 => i64::from(ctx.read_long() as i32),
            _ => 0,
        }
    } else {
        0
    };

    let base_str = if ext & 0x0080 != 0 {
        String::new() // base suppressed
    } else {
        base_reg.to_string()
    };
    let index_str = if ext & 0x0040 != 0 {
        String::new() // index suppressed
    } else {
        format!("{}{}{}{}", xn_type, xn_reg, xn_size, scale_str)
    };
    let bd_str = if bd != 0 {
        bd.to_string()
    } else {
        String::new()
    };
    let od_str = if od != 0 {
        od.to_string()
    } else {
        String::new()
    };

    if !indirect {
        format!("({})", join_nonempty(&[bd_str, base_str, index_str]))
    } else if ext & 0x0004 == 0 {
        // Pre-indexed: index inside the indirection brackets.
        let inner = join_nonempty(&[bd_str, base_str, index_str]);
        format!("({})", join_nonempty(&[format!("[{inner}]"), od_str]))
    } else {
        // Post-indexed: index applied after the indirection.
        let inner = join_nonempty(&[bd_str, base_str]);
        format!(
            "({})",
            join_nonempty(&[format!("[{inner}]"), index_str, od_str])
        )
    }
}

/// Join the non-empty fragments of an addressing-mode operand with commas.
fn join_nonempty(parts: &[String]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

fn ea_mode_reg(opcode: u16) -> (u16, u16) {
    ((opcode >> 3) & 7, opcode & 7)
}

fn dest_mode_reg(opcode: u16) -> (u16, u16) {
    ((opcode >> 6) & 7, (opcode >> 9) & 7)
}

const CC_NAMES: [&str; 16] = [
    "t", "f", "hi", "ls", "cc", "cs", "ne", "eq", "vc", "vs", "pl", "mi", "ge", "lt", "gt", "le",
];

// ---------------------------------------------------------------------------
// Group decoders
// ---------------------------------------------------------------------------

fn decode_group0(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    // Bit operations with register
    if opcode & 0x0100 != 0 {
        let reg = (opcode >> 9) & 7;
        let op_name = match (opcode >> 6) & 3 {
            0 => "btst",
            1 => "bchg",
            2 => "bclr",
            3 => "bset",
            _ => unreachable!(),
        };
        let (mode, ea_reg) = ea_mode_reg(opcode);
        // MOVEP
        if mode == 1 {
            let disp = ctx.read_word() as i16;
            let sz = if (opcode >> 6) & 1 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            return if (opcode >> 7) & 1 != 0 {
                Some(format!(
                    "movep{} D{},({},A{})",
                    sz.suffix(),
                    reg,
                    disp,
                    ea_reg
                ))
            } else {
                Some(format!(
                    "movep{} ({},A{}),D{}",
                    sz.suffix(),
                    disp,
                    ea_reg,
                    reg
                ))
            };
        }
        let size = if mode == 0 { Size::Long } else { Size::Byte };
        let ea = format_ea(ctx, mode, ea_reg, size);
        return Some(format!("{} D{},{}", op_name, reg, ea));
    }

    // Immediate operations
    let (mode, reg) = ea_mode_reg(opcode);
    let op = (opcode >> 9) & 7;

    // Static bit operations (BTST/BCHG/BCLR/BSET #imm)
    if op == 4 {
        let bit_num = ctx.read_word() & 0xFF;
        let op_name = match (opcode >> 6) & 3 {
            0 => "btst",
            1 => "bchg",
            2 => "bclr",
            3 => "bset",
            _ => unreachable!(),
        };
        let size = if mode == 0 { Size::Long } else { Size::Byte };
        let ea = format_ea(ctx, mode, reg, size);
        return Some(format!("{} #{},$ea", op_name, bit_num).replace("$ea", &ea));
    }

    let size = Size::from_bits((opcode >> 6) & 3)?;
    let imm = read_imm(ctx, size);

    let op_name = match op {
        0 => "ori",
        1 => "andi",
        2 => "subi",
        3 => "addi",
        5 => "eori",
        6 => "cmpi",
        _ => return None,
    };

    // ORI/ANDI/EORI to CCR/SR
    if mode == 7 && reg == 4 {
        if size == Size::Byte {
            return Some(format!("{} #${},CCR", op_name, imm));
        } else if size == Size::Word {
            return Some(format!("{} #${},SR", op_name, imm));
        }
    }

    let ea = format_ea(ctx, mode, reg, size);
    Some(format!("{}{} #${},{}", op_name, size.suffix(), imm, ea))
}

fn read_imm(ctx: &mut DisCtx, size: Size) -> String {
    match size {
        Size::Byte => {
            let w = ctx.read_word();
            format!("{:02X}", w & 0xFF)
        }
        Size::Word => {
            let w = ctx.read_word();
            format!("{:04X}", w)
        }
        Size::Long => {
            let l = ctx.read_long();
            format!("{:08X}", l)
        }
    }
}

fn decode_move(ctx: &mut DisCtx, opcode: u16, size: Size) -> Option<String> {
    let (src_mode, src_reg) = ea_mode_reg(opcode);
    let (dst_mode_raw, dst_reg) = dest_mode_reg(opcode);
    // MOVE destination mode field is rearranged: bits 8-6 are mode, 11-9 are reg
    // but the mode encoding for dest uses a different mapping for modes >= 2
    // Actually the dest mode bits 8-6 map directly to normal EA modes

    // MOVEA
    if dst_mode_raw == 1 {
        let ea = format_ea(ctx, src_mode, src_reg, size);
        let move_size = if size == Size::Long { ".l" } else { ".w" };
        return Some(format!("movea{} {},A{}", move_size, ea, dst_reg));
    }

    let src = format_ea(ctx, src_mode, src_reg, size);
    let dst = format_ea(ctx, dst_mode_raw, dst_reg, size);
    Some(format!("move{} {},{}", size.suffix(), src, dst))
}

fn decode_group4(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    // NOP
    if opcode == 0x4E71 {
        return Some("nop".to_string());
    }
    // RTS
    if opcode == 0x4E75 {
        return Some("rts".to_string());
    }
    // RTE
    if opcode == 0x4E73 {
        return Some("rte".to_string());
    }
    // RTR
    if opcode == 0x4E77 {
        return Some("rtr".to_string());
    }
    // RESET
    if opcode == 0x4E70 {
        return Some("reset".to_string());
    }
    // ILLEGAL
    if opcode == 0x4AFC {
        return Some("illegal".to_string());
    }

    let (mode, reg) = ea_mode_reg(opcode);

    // TRAP
    if opcode & 0xFFF0 == 0x4E40 {
        return Some(format!("trap #{}", opcode & 0xF));
    }

    // LINK
    if opcode & 0xFFF8 == 0x4E50 {
        let disp = ctx.read_word() as i16;
        return Some(format!("link A{},#{}", reg, disp));
    }

    // UNLK
    if opcode & 0xFFF8 == 0x4E58 {
        return Some(format!("unlk A{}", reg));
    }

    // MOVE USP
    if opcode & 0xFFF0 == 0x4E60 {
        return if opcode & 8 != 0 {
            Some(format!("move USP,A{}", reg))
        } else {
            Some(format!("move A{},USP", reg))
        };
    }

    // SWAP
    if opcode & 0xFFF8 == 0x4840 {
        return Some(format!("swap D{}", reg));
    }

    // EXT
    if opcode & 0xFE38 == 0x4800 && (opcode >> 6) & 7 >= 2 {
        let dn = opcode & 7;
        return match (opcode >> 6) & 7 {
            2 => Some(format!("ext.w D{}", dn)),
            3 => Some(format!("ext.l D{}", dn)),
            7 => Some(format!("extb.l D{}", dn)),
            _ => None,
        };
    }

    // LEA
    if (opcode >> 6) & 7 == 7 {
        let an = (opcode >> 9) & 7;
        // LEA only: bits 15-12 = 0100, bits 8-6 = 111
        if opcode & 0xF1C0 == 0x41C0 {
            let ea = format_ea(ctx, mode, reg, Size::Long);
            return Some(format!("lea {},A{}", ea, an));
        }
    }

    // PEA
    if opcode & 0xFFC0 == 0x4840 && mode != 0 {
        let ea = format_ea(ctx, mode, reg, Size::Long);
        return Some(format!("pea {}", ea));
    }

    // JSR
    if opcode & 0xFFC0 == 0x4E80 {
        let ea = format_ea(ctx, mode, reg, Size::Long);
        return Some(format!("jsr {}", ea));
    }

    // JMP
    if opcode & 0xFFC0 == 0x4EC0 {
        let ea = format_ea(ctx, mode, reg, Size::Long);
        return Some(format!("jmp {}", ea));
    }

    // MOVEM
    if opcode & 0xFB80 == 0x4880 {
        let sz = if opcode & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mask = ctx.read_word();
        let ea = format_ea(ctx, mode, reg, sz);
        let dir = if opcode & 0x0400 != 0 {
            // Memory to register
            format!(
                "movem{} {},{}",
                sz.suffix(),
                ea,
                format_regmask(mask, false)
            )
        } else {
            // Register to memory — predecrement reverses mask
            let reversed = mode == 4;
            format!(
                "movem{} {},{}",
                sz.suffix(),
                format_regmask(mask, reversed),
                ea
            )
        };
        return Some(dir);
    }

    // CLR, NEG, NEGX, NOT, TST
    let op4 = (opcode >> 8) & 0xF;
    if matches!(op4, 0x2 | 0x0 | 0x4 | 0x6 | 0xA) {
        let size = Size::from_bits((opcode >> 6) & 3);
        if let Some(sz) = size {
            let name = match op4 {
                0x2 => "clr",
                0x0 => "negx",
                0x4 => "neg",
                0x6 => "not",
                0xA => "tst",
                _ => unreachable!(),
            };
            let ea = format_ea(ctx, mode, reg, sz);
            return Some(format!("{}{} {}", name, sz.suffix(), ea));
        }
    }

    None
}

fn decode_group5(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    let (mode, reg) = ea_mode_reg(opcode);
    let size_bits = (opcode >> 6) & 3;

    // DBcc — size field is 0b11 (bits 7-6) with the An mode (mode 1).
    if size_bits == 3 && mode == 1 {
        let cc = (opcode >> 8) & 0xF;
        let disp = ctx.read_word() as i16;
        let target = (ctx.base.wrapping_add(2) as i32).wrapping_add(disp as i32) as u32;
        return Some(format!(
            "db{} D{},${:08X}",
            CC_NAMES[cc as usize], reg, target
        ));
    }

    // Scc — size field 0b11, any mode except the An form claimed by DBcc above.
    if size_bits == 3 {
        let cc = (opcode >> 8) & 0xF;
        let ea = format_ea(ctx, mode, reg, Size::Byte);
        return Some(format!("s{} {}", CC_NAMES[cc as usize], ea));
    }

    // ADDQ / SUBQ
    let size = Size::from_bits(size_bits)?;
    let mut data = ((opcode >> 9) & 7) as u8;
    if data == 0 {
        data = 8;
    }
    let name = if opcode & 0x0100 != 0 { "subq" } else { "addq" };
    let ea = format_ea(ctx, mode, reg, size);
    Some(format!("{}{} #{},{}", name, size.suffix(), data, ea))
}

fn decode_group6(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    let cc = (opcode >> 8) & 0xF;
    let disp8 = (opcode & 0xFF) as i8;

    let (disp, suffix) = if disp8 == 0 {
        // Word displacement
        let d = ctx.read_word() as i16;
        (d as i32, ".w")
    } else if disp8 == -1 {
        // Long displacement (68020+)
        let d = ctx.read_long() as i32;
        (d, ".l")
    } else {
        (disp8 as i32, ".s")
    };

    let target = (ctx.base.wrapping_add(2) as i32).wrapping_add(disp) as u32;

    let name = match cc {
        0 => "bra",
        1 => "bsr",
        _ => {
            return Some(format!(
                "b{}{} ${:08X}",
                CC_NAMES[cc as usize], suffix, target
            ));
        }
    };
    Some(format!("{}{} ${:08X}", name, suffix, target))
}

fn decode_moveq(opcode: u16) -> Option<String> {
    if opcode & 0x0100 != 0 {
        return None;
    }
    let data = (opcode & 0xFF) as i8;
    let dn = (opcode >> 9) & 7;
    Some(format!("moveq #{},D{}", data, dn))
}

fn decode_group8(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    let dn = (opcode >> 9) & 7;
    let (mode, reg) = ea_mode_reg(opcode);
    let size_bits = (opcode >> 6) & 7;

    // SBCD
    if size_bits == 4 {
        return if mode == 0 {
            Some(format!("sbcd D{},D{}", reg, dn))
        } else {
            Some(format!("sbcd -(A{}),-(A{})", reg, dn))
        };
    }

    // DIVU
    if size_bits == 3 {
        let ea = format_ea(ctx, mode, reg, Size::Word);
        return Some(format!("divu.w {},D{}", ea, dn));
    }

    // DIVS
    if size_bits == 7 {
        let ea = format_ea(ctx, mode, reg, Size::Word);
        return Some(format!("divs.w {},D{}", ea, dn));
    }

    // OR
    let size = Size::from_bits(size_bits & 3)?;
    let ea = format_ea(ctx, mode, reg, size);
    if size_bits & 4 != 0 {
        // OR Dn,<ea>
        Some(format!("or{} D{},{}", size.suffix(), dn, ea))
    } else {
        // OR <ea>,Dn
        Some(format!("or{} {},D{}", size.suffix(), ea, dn))
    }
}

fn decode_addsub(ctx: &mut DisCtx, opcode: u16, base: &str) -> Option<String> {
    let dn = (opcode >> 9) & 7;
    let (mode, reg) = ea_mode_reg(opcode);
    let size_bits = (opcode >> 6) & 7;

    // ADDA / SUBA
    if size_bits == 3 || size_bits == 7 {
        let size = if size_bits == 3 {
            Size::Word
        } else {
            Size::Long
        };
        let ea = format_ea(ctx, mode, reg, size);
        return Some(format!("{}a{} {},A{}", base, size.suffix(), ea, dn));
    }

    // ADDX / SUBX
    if size_bits & 4 != 0 && (mode == 0 || mode == 1) {
        let size = Size::from_bits(size_bits & 3)?;
        let name = format!("{}x", base);
        return if mode == 0 {
            Some(format!("{}{} D{},D{}", name, size.suffix(), reg, dn))
        } else {
            Some(format!("{}{} -(A{}),-(A{})", name, size.suffix(), reg, dn))
        };
    }

    let size = Size::from_bits(size_bits & 3)?;
    let ea = format_ea(ctx, mode, reg, size);
    if size_bits & 4 != 0 {
        Some(format!("{}{} D{},{}", base, size.suffix(), dn, ea))
    } else {
        Some(format!("{}{} {},D{}", base, size.suffix(), ea, dn))
    }
}

fn decode_groupb(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    let dn = (opcode >> 9) & 7;
    let (mode, reg) = ea_mode_reg(opcode);
    let size_bits = (opcode >> 6) & 7;

    // CMPA
    if size_bits == 3 || size_bits == 7 {
        let size = if size_bits == 3 {
            Size::Word
        } else {
            Size::Long
        };
        let ea = format_ea(ctx, mode, reg, size);
        return Some(format!("cmpa{} {},A{}", size.suffix(), ea, dn));
    }

    // CMPM
    if size_bits & 4 != 0 && mode == 1 {
        let size = Size::from_bits(size_bits & 3)?;
        return Some(format!("cmpm{} (A{})+,(A{})+", size.suffix(), reg, dn));
    }

    // EOR
    if size_bits & 4 != 0 {
        let size = Size::from_bits(size_bits & 3)?;
        let ea = format_ea(ctx, mode, reg, size);
        return Some(format!("eor{} D{},{}", size.suffix(), dn, ea));
    }

    // CMP
    let size = Size::from_bits(size_bits & 3)?;
    let ea = format_ea(ctx, mode, reg, size);
    Some(format!("cmp{} {},D{}", size.suffix(), ea, dn))
}

fn decode_groupc(ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    let dn = (opcode >> 9) & 7;
    let (mode, reg) = ea_mode_reg(opcode);
    let size_bits = (opcode >> 6) & 7;

    // ABCD — opmode 0b100 with bits 7-4 == 0; bit 3 is the R/M flag (0 = data
    // registers, 1 = address-register predecrement). Every *other* opmode-0b100
    // word is AND.B Dn,<ea>, reached by the AND fall-through below — so this must
    // match the fixed ABCD bits, not the opmode alone.
    if opcode & 0x01F0 == 0x0100 {
        return if opcode & 0x0008 == 0 {
            Some(format!("abcd D{},D{}", reg, dn))
        } else {
            Some(format!("abcd -(A{}),-(A{})", reg, dn))
        };
    }

    // EXG — three fixed sub-encodings spanning opmodes 0b101 (Dn,Dn / An,An) and
    // 0b110 (Dn,An). Everything else in those opmodes is AND Dn,<ea> (below); the
    // old code claimed all of opmode 0b101 for EXG, losing AND.W Dn,<ea>.
    match opcode & 0x01F8 {
        0x0140 => return Some(format!("exg D{},D{}", dn, reg)),
        0x0148 => return Some(format!("exg A{},A{}", dn, reg)),
        0x0188 => return Some(format!("exg D{},A{}", dn, reg)),
        _ => {}
    }

    // MULU.W / MULS.W — <ea>,Dn
    if size_bits == 3 {
        let ea = format_ea(ctx, mode, reg, Size::Word);
        return Some(format!("mulu.w {},D{}", ea, dn));
    }
    if size_bits == 7 {
        let ea = format_ea(ctx, mode, reg, Size::Word);
        return Some(format!("muls.w {},D{}", ea, dn));
    }

    // AND — opmode 0b000/001/010 = <ea>,Dn; 0b100/101/110 = Dn,<ea>.
    let size = Size::from_bits(size_bits & 3)?;
    let ea = format_ea(ctx, mode, reg, size);
    if size_bits & 4 != 0 {
        Some(format!("and{} D{},{}", size.suffix(), dn, ea))
    } else {
        Some(format!("and{} {},D{}", size.suffix(), ea, dn))
    }
}

fn decode_groupe(_ctx: &mut DisCtx, opcode: u16) -> Option<String> {
    let (mode, reg) = ea_mode_reg(opcode);

    // Register shifts/rotates
    if (opcode >> 6) & 3 != 3 {
        let size = Size::from_bits((opcode >> 6) & 3)?;
        let dir = if opcode & 0x0100 != 0 { "l" } else { "r" };
        let base = match (opcode >> 3) & 3 {
            0 => "as",
            1 => "ls",
            2 => "rox",
            3 => "ro",
            _ => unreachable!(),
        };
        let count_or_reg = (opcode >> 9) & 7;
        if opcode & 0x0020 != 0 {
            // Register count
            return Some(format!(
                "{}{}{} D{},D{}",
                base,
                dir,
                size.suffix(),
                count_or_reg,
                reg
            ));
        } else {
            // Immediate count
            let count = if count_or_reg == 0 { 8 } else { count_or_reg };
            return Some(format!(
                "{}{}{} #{},D{}",
                base,
                dir,
                size.suffix(),
                count,
                reg
            ));
        }
    }

    // Memory shifts/rotates (size field == 3, word only)
    let dir = if opcode & 0x0100 != 0 { "l" } else { "r" };
    let base = match (opcode >> 9) & 3 {
        0 => "as",
        1 => "ls",
        2 => "rox",
        3 => "ro",
        _ => unreachable!(),
    };
    let ea = format_ea(_ctx, mode, reg, Size::Word);
    Some(format!("{}{}.w {}", base, dir, ea))
}

fn format_regmask(mask: u16, reversed: bool) -> String {
    let mut parts = Vec::new();
    for i in 0..16u16 {
        let bit = if reversed { 15 - i } else { i };
        if mask & (1 << bit) != 0 {
            if i < 8 {
                parts.push(format!("D{}", i));
            } else {
                parts.push(format!("A{}", i - 8));
            }
        }
    }
    if parts.is_empty() {
        "#0".to_string()
    } else {
        parts.join("/")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dis(bytes: &[u8]) -> (String, u8) {
        let data = bytes.to_vec();
        disassemble(0, move |addr| data.get(addr as usize).copied().unwrap_or(0))
    }

    #[test]
    fn test_nop() {
        let (s, len) = dis(&[0x4E, 0x71]);
        assert_eq!(s, "nop");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_rts() {
        let (s, len) = dis(&[0x4E, 0x75]);
        assert_eq!(s, "rts");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_move_l_d0_d1() {
        // MOVE.L D0,D1 = 0010_001_000_000_000 = $2200
        let (s, len) = dis(&[0x22, 0x00]);
        assert_eq!(s, "move.l D0,D1");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_lea_a0_a1() {
        // LEA (A0),A1 = 0100_001_111_010_000 = $43D0
        let (s, len) = dis(&[0x43, 0xD0]);
        assert_eq!(s, "lea (A0),A1");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_lea_full_format_scaled_index() {
        // LEA (A3,D0.w*2),A5 — opcode $4BF3, full-format ext $0310.
        // This is the Workbench 3.1 palette write; the brief decoder
        // mis-rendered it as "(16,A3,D0.w".
        let (s, len) = dis(&[0x4B, 0xF3, 0x03, 0x10]);
        assert_eq!(s, "lea (A3,D0.w*2),A5");
        assert_eq!(len, 4);
    }

    #[test]
    fn test_lea_full_format_memory_indirect() {
        // LEA ([16,A3,D0.w],4),A5 — pre-indexed memory indirect with
        // word base ($0010) and word outer ($0004) displacements.
        let (s, len) = dis(&[0x4B, 0xF3, 0x01, 0x22, 0x00, 0x10, 0x00, 0x04]);
        assert_eq!(s, "lea ([16,A3,D0.w],4),A5");
        assert_eq!(len, 8);
    }

    #[test]
    fn test_bra() {
        // BRA.S $+2+$10 = 0x6010, target = 0x12
        let (s, len) = dis(&[0x60, 0x10]);
        assert_eq!(s, "bra.s $00000012");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_bra_word() {
        // BRA.W with disp $0020 -> target = 2 + 0x20 = 0x22
        let (s, len) = dis(&[0x60, 0x00, 0x00, 0x20]);
        assert_eq!(s, "bra.w $00000022");
        assert_eq!(len, 4);
    }

    #[test]
    fn test_moveq() {
        // MOVEQ #42,D3 = 0111_011_0_00101010 = $762A
        let (s, len) = dis(&[0x76, 0x2A]);
        assert_eq!(s, "moveq #42,D3");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_moveq_negative() {
        // MOVEQ #-1,D0 = $70FF
        let (s, len) = dis(&[0x70, 0xFF]);
        assert_eq!(s, "moveq #-1,D0");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_jmp_an() {
        // JMP (A3) = 0100_1110_1101_0011 = $4ED3
        let (s, len) = dis(&[0x4E, 0xD3]);
        assert_eq!(s, "jmp (A3)");
        assert_eq!(len, 2);
    }

    // Group-5 (ADDQ/SUBQ/Scc/DBcc) regression cluster. DBcc has the size field
    // 0b11 (bits 7-6); the decoder tested 0b01, so real DBcc fell through to Scc
    // (DBF/DBRA — the canonical loop primitive — disassembled as `sf An`) and
    // ADDQ.W/SUBQ.W #n,An were mis-read as DBcc. Found by the isa-disasm
    // conformance spike, 2026-06-03.

    #[test]
    fn test_dbf_d0() {
        // DBF D0,$+2+$10 = $51C8, disp $0010 -> target $12. (DBRA is DBF.)
        let (s, len) = dis(&[0x51, 0xC8, 0x00, 0x10]);
        assert_eq!(s, "dbf D0,$00000012");
        assert_eq!(len, 4);
    }

    #[test]
    fn test_dbt_d7() {
        // DBT D7,$+2+$10 = $50CF (cc=T), disp $0010 -> target $12.
        let (s, len) = dis(&[0x50, 0xCF, 0x00, 0x10]);
        assert_eq!(s, "dbt D7,$00000012");
        assert_eq!(len, 4);
    }

    #[test]
    fn test_addq_w_an() {
        // ADDQ.W #8,A0 = $5048 (data 000 -> 8, size 0b01, mode 1). Was DBT.
        let (s, len) = dis(&[0x50, 0x48]);
        assert_eq!(s, "addq.w #8,A0");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_subq_w_an() {
        // SUBQ.W #1,A0 = $5348 (data 001, bit8 set -> subq, size 0b01, mode 1).
        let (s, len) = dis(&[0x53, 0x48]);
        assert_eq!(s, "subq.w #1,A0");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_sf_d0_still_scc() {
        // SF D0 = $51C0 (size 0b11, mode 0): a genuine Scc, must stay Scc.
        let (s, len) = dis(&[0x51, 0xC0]);
        assert_eq!(s, "sf D0");
        assert_eq!(len, 2);
    }

    // Group-C (AND/MULU/MULS/ABCD/EXG) regression cluster. The opmode field
    // overlaps three instruction families; the decoder claimed all of opmode
    // 0b100 for ABCD (losing AND.B Dn,<ea>) and all of opmode 0b101 for EXG
    // (losing AND.W Dn,<ea>), and mis-decoded EXG Dn,An as AND.L. Encodings
    // confirmed against vasm. Found by the isa-disasm conformance spike,
    // 2026-06-03.

    #[test]
    fn test_abcd() {
        // ABCD D1,D0 = $C101; ABCD -(A1),-(A0) = $C109.
        assert_eq!(dis(&[0xC1, 0x01]), ("abcd D1,D0".to_string(), 2));
        assert_eq!(dis(&[0xC1, 0x09]), ("abcd -(A1),-(A0)".to_string(), 2));
    }

    #[test]
    fn test_exg() {
        // EXG D0,D1 = $C141; EXG A0,A1 = $C149; EXG D0,A1 = $C189 (was and.l).
        assert_eq!(dis(&[0xC1, 0x41]), ("exg D0,D1".to_string(), 2));
        assert_eq!(dis(&[0xC1, 0x49]), ("exg A0,A1".to_string(), 2));
        assert_eq!(dis(&[0xC1, 0x89]), ("exg D0,A1".to_string(), 2));
    }

    #[test]
    fn test_and_dn_to_ea() {
        // AND Dn,<ea> (register-to-memory). and.b/and.l worked; and.w ($C150)
        // fell through to dc.w, and and.b ($C110) was mis-read as abcd.
        assert_eq!(dis(&[0xC1, 0x10]), ("and.b D0,(A0)".to_string(), 2));
        assert_eq!(dis(&[0xC1, 0x50]), ("and.w D0,(A0)".to_string(), 2));
        assert_eq!(dis(&[0xC1, 0x90]), ("and.l D0,(A0)".to_string(), 2));
    }

    #[test]
    fn test_and_ea_to_dn() {
        // AND <ea>,Dn — the other direction must still decode. $C240 = and.w d0,d1.
        assert_eq!(dis(&[0xC2, 0x40]), ("and.w D0,D1".to_string(), 2));
    }

    #[test]
    fn test_mul() {
        // MULU.W D1,D0 = $C0C1; MULS.W D1,D0 = $C1C1.
        assert_eq!(dis(&[0xC0, 0xC1]), ("mulu.w D1,D0".to_string(), 2));
        assert_eq!(dis(&[0xC1, 0xC1]), ("muls.w D1,D0".to_string(), 2));
    }

    #[test]
    fn test_unknown_opcode() {
        // Use an illegal pattern: $4AFB (near ILLEGAL=$4AFC)
        let (s, len) = dis(&[0x4A, 0xFB]);
        // Should decode or fall back to dc.w
        assert_eq!(len, 2, "unknown opcode should be 2 bytes");
        // Just verify it doesn't panic
        assert!(!s.is_empty());
    }

    #[test]
    fn test_addq_l() {
        // ADDQ.L #1,A7 = 0101_001_0_10_001_111 = $528F
        let (s, len) = dis(&[0x52, 0x8F]);
        assert_eq!(s, "addq.l #1,A7");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_subq_w() {
        // SUBQ.W #3,D2 = 0101_011_1_01_000_010 = $5742
        let (s, len) = dis(&[0x57, 0x42]);
        assert_eq!(s, "subq.w #3,D2");
        assert_eq!(len, 2);
    }

    #[test]
    fn test_move_w_imm() {
        // MOVE.W #$1234,D0 = 0011_000_000_111_100 = $303C, $1234
        let (s, len) = dis(&[0x30, 0x3C, 0x12, 0x34]);
        assert_eq!(s, "move.w #$1234,D0");
        assert_eq!(len, 4);
    }
}
