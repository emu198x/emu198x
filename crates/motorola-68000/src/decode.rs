//! Instruction decoding and follow-up state machine for the 68000.
//!
//! The 68000 executes instructions in multiple phases tracked by follow-up
//! tags. The first call to `decode_and_execute` decodes the opcode in IR
//! and sets up the initial follow-up tag. Subsequent calls route through
//! `continue_instruction` which advances through the tag state machine:
//!
//! ```text
//! FETCH_SRC_EA -> FETCH_SRC_DATA -> FETCH_DST_EA -> FETCH_DST_DATA
//!     -> EXECUTE -> WRITEBACK
//! ```
//!
//! Not every instruction uses all phases. MOVEQ has no follow-ups at all.
//! LEA skips data fetch. CMP skips writeback. The tag values let each
//! instruction define its own path through the pipeline.

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::bus::M68kBus;
use crate::cpu::{
    AluOp, BitOp, Cpu68000, State,
    TAG_ADDX_READ_DST, TAG_ADDX_READ_SRC, TAG_ADDX_WRITE,
    TAG_AE_FETCH_VECTOR, TAG_AE_FINISH, TAG_AE_PUSH_FAULT, TAG_AE_PUSH_INFO,
    TAG_AE_PUSH_IR, TAG_AE_PUSH_SR,
    TAG_BCC_EXECUTE, TAG_BSR_EXECUTE, TAG_DATA_DST_LONG,
    TAG_DATA_SRC_LONG, TAG_DBCC_EXECUTE, TAG_EA_DST_DISP, TAG_EA_DST_LONG,
    TAG_EA_DST_PCDISP, TAG_EA_SRC_DISP, TAG_EA_SRC_LONG, TAG_EA_SRC_PCDISP,
    TAG_EXC_FETCH_VECTOR, TAG_EXC_FINISH, TAG_EXC_STACK_PC_HI,
    TAG_EXC_STACK_PC_LO, TAG_EXC_STACK_SR, TAG_EXECUTE, TAG_FETCH_DST_DATA,
    TAG_FETCH_DST_EA, TAG_FETCH_SRC_DATA, TAG_FETCH_SRC_EA, TAG_JSR_EXECUTE,
    TAG_LINK_DISP, TAG_MOVEM_NEXT, TAG_MOVEM_RESOLVE_EA, TAG_MOVEM_STORE,
    TAG_RTE_READ_PC_HI, TAG_RTE_READ_PC_LO, TAG_RTE_READ_SR,
    TAG_RTR_READ_CCR, TAG_RTR_READ_PC_HI, TAG_RTR_READ_PC_LO,
    TAG_RTS_PC_HI, TAG_RTS_PC_LO,
    TAG_BCD_DST_READ, TAG_BCD_SRC_READ,
    TAG_CHK_EXECUTE, TAG_MULDIV_EXECUTE, TAG_MOVEP_TRANSFER,
    TAG_STOP_WAIT,
    TAG_UNLK_POP_HI, TAG_UNLK_POP_LO, TAG_WRITEBACK,
};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Decode the opcode in IR and begin execution.
    ///
    /// If `in_followup` is set, routes to `continue_instruction` instead.
    /// Otherwise, decodes the opcode and sets up the initial follow-up tag
    /// and micro-op sequence.
    pub fn decode_and_execute<B: M68kBus>(&mut self, bus: &mut B) {
        if self.in_followup {
            self.continue_instruction(bus);
            return;
        }

        let opcode = self.ir;

        // --- MOVE.b/w/l (0x1xxx, 0x2xxx, 0x3xxx) ---
        // Top 2 bits = 00, next 2 bits encode size (non-zero)
        if (opcode & 0xC000) == 0 && (opcode & 0x3000) != 0 {
            let size = match (opcode >> 12) & 3 {
                1 => Size::Byte,
                2 => Size::Long,
                3 => Size::Word,
                _ => unreachable!(),
            };
            let src_mode_bits = ((opcode >> 3) & 7) as u8;
            let src_reg = (opcode & 7) as u8;
            let dst_reg = ((opcode >> 9) & 7) as u8;
            let dst_mode_bits = ((opcode >> 6) & 7) as u8;

            self.size = size;
            self.src_mode = AddrMode::decode(src_mode_bits, src_reg);
            self.dst_mode = AddrMode::decode(dst_mode_bits, dst_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- CMPM (1011 Rx 1 ss 001 Ry) — must check before general EOR ---
        if (opcode & 0xF000) == 0xB000 {
            let opmode = (opcode >> 6) & 7;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;

            // CMPM: opmodes 4-6 with EA mode 001 (postincrement)
            if opmode >= 4 && opmode <= 6 && ea_mode_bits == 1 {
                let rx = ((opcode >> 9) & 7) as u8;
                let ry = (opcode & 7) as u8;
                let size = match opmode {
                    4 => Size::Byte,
                    5 => Size::Word,
                    6 => Size::Long,
                    _ => unreachable!(),
                };

                self.alu_op = AluOp::Cmp;
                self.size = size;
                self.src_mode = Some(AddrMode::AddrIndPostInc(ry));
                self.dst_mode = Some(AddrMode::AddrIndPostInc(rx));
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_SRC_EA;
                self.continue_instruction(bus);
                return;
            }
        }

        // --- ADDX/SUBX (register and memory modes) ---
        // ADDX: 1101 RRR 1 SS 00 M YYY, SUBX: 1001 RRR 1 SS 00 M YYY
        if matches!(opcode & 0xF000, 0xD000 | 0x9000) {
            let opmode = (opcode >> 6) & 7;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            // Opmodes 4-6 with ea_mode 0 (Dy,Dx) or 1 (-(Ay),-(Ax))
            if opmode >= 4 && opmode <= 6 && ea_mode_bits <= 1 {
                let is_add = (opcode & 0xF000) == 0xD000;
                let rx = ((opcode >> 9) & 7) as u8;
                let ry = (opcode & 7) as u8;
                let size = match opmode {
                    4 => Size::Byte,
                    5 => Size::Word,
                    6 => Size::Long,
                    _ => unreachable!(),
                };
                self.size = size;

                if ea_mode_bits == 0 {
                    // Register mode: Dy,Dx
                    let src = self.regs.d[ry as usize];
                    let dst = self.regs.d[rx as usize];
                    let (result, new_sr) = if is_add {
                        crate::alu::addx(src, dst, size, self.regs.sr)
                    } else {
                        crate::alu::subx(src, dst, size, self.regs.sr)
                    };
                    self.regs.sr = new_sr;
                    let reg = &mut self.regs.d[rx as usize];
                    *reg = match size {
                        Size::Byte => (*reg & 0xFFFF_FF00) | (result & 0xFF),
                        Size::Word => (*reg & 0xFFFF_0000) | (result & 0xFFFF),
                        Size::Long => result,
                    };
                    if size == Size::Long {
                        self.micro_ops.push(MicroOp::Internal(4));
                    }
                } else {
                    // Memory mode: -(Ay),-(Ax) — uses followup tags
                    self.ea_reg = ry;
                    self.src_val = rx as u32; // stash Ax index
                    self.alu_op = if is_add { AluOp::Add } else { AluOp::Sub };
                    self.in_followup = true;
                    // Pre-decrement source (Ay)
                    let dec = if ry == 7 && size == Size::Byte { 2 } else { size.bytes() };
                    self.addr = self.regs.a(ry as usize).wrapping_sub(dec);
                    self.regs.set_a(ry as usize, self.addr);
                    self.ae_undo_reg = Some((ry, dec, false, false));
                    self.micro_ops.push(MicroOp::Internal(2));
                    self.followup_tag = TAG_ADDX_READ_SRC;
                    self.queue_read_ops(size);
                    self.micro_ops.push(MicroOp::Execute);
                }
                return;
            }
        }

        // --- EXG (0xCxxx with specific opmodes) ---
        if (opcode & 0xF000) == 0xC000 {
            let opmode5 = ((opcode >> 3) & 0x1F) as u8;
            if (opcode & 0x0100) != 0 && matches!(opmode5, 0x08 | 0x09 | 0x11) {
                let rx = ((opcode >> 9) & 7) as usize;
                let ry = (opcode & 7) as usize;
                match opmode5 {
                    0x08 => {
                        // EXG Dx,Dy
                        let tmp = self.regs.d[rx];
                        self.regs.d[rx] = self.regs.d[ry];
                        self.regs.d[ry] = tmp;
                    }
                    0x09 => {
                        // EXG Ax,Ay
                        let tmp = self.regs.a(rx);
                        self.regs.set_a(rx, self.regs.a(ry));
                        self.regs.set_a(ry, tmp);
                    }
                    0x11 => {
                        // EXG Dx,Ay
                        let tmp = self.regs.d[rx];
                        self.regs.d[rx] = self.regs.a(ry);
                        self.regs.set_a(ry, tmp);
                    }
                    _ => {}
                }
                self.micro_ops.push(MicroOp::Internal(2));
                return;
            }
        }

        // --- ABCD (0xC100) / SBCD (0x8100) ---
        // ABCD: 1100 Rx 10000 M Ry, SBCD: 1000 Rx 10000 M Ry
        if matches!(opcode & 0xF000, 0xC000 | 0x8000) {
            let opmode = (opcode >> 6) & 7;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            if opmode == 4 && ea_mode_bits <= 1 {
                let is_add = (opcode & 0xF000) == 0xC000;
                let rx = ((opcode >> 9) & 7) as u8;
                let ry = (opcode & 7) as u8;

                if ea_mode_bits == 0 {
                    // Register mode: Dy,Dx (SBCD) or Dy,Dx (ABCD)
                    let src = (self.regs.d[ry as usize] & 0xFF) as u8;
                    let dst = (self.regs.d[rx as usize] & 0xFF) as u8;
                    let x = self.x_flag();
                    let (result, carry, overflow) = if is_add {
                        self.bcd_add(src, dst, x)
                    } else {
                        self.bcd_sub(dst, src, x)
                    };
                    self.regs.d[rx as usize] =
                        (self.regs.d[rx as usize] & 0xFFFF_FF00) | u32::from(result);
                    self.set_bcd_flags(result, carry, overflow);
                    self.micro_ops.push(MicroOp::Internal(2));
                } else {
                    // Memory mode: -(Ay),-(Ax)
                    let dec_src = if ry == 7 { 2u32 } else { 1 };
                    self.addr = self.regs.a(ry as usize).wrapping_sub(dec_src);
                    self.regs.set_a(ry as usize, self.addr);

                    // Stash state: is_add in movem_is_write, rx in ea_reg
                    self.movem_is_write = is_add;
                    self.ea_reg = rx;

                    self.micro_ops.push(MicroOp::Internal(2));
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.in_followup = true;
                    self.followup_tag = TAG_BCD_SRC_READ;
                    self.micro_ops.push(MicroOp::Execute);
                }
                return;
            }
        }

        // --- MULU/MULS (0xC0C0/0xC1C0) / DIVU/DIVS (0x80C0/0x81C0) ---
        if matches!(opcode & 0xF000, 0xC000 | 0x8000) {
            let opmode = (opcode >> 6) & 7;
            if opmode == 3 || opmode == 7 {
                self.size = Size::Word;
                let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                let ea_reg = (opcode & 7) as u8;
                self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_SRC_EA;
                self.continue_instruction(bus);
                return;
            }
        }

        // --- ADD/SUB/CMP/EOR/AND/OR (general register-EA form) ---
        // 0xBxxx: CMP (opmodes 0-2), EOR (opmodes 4-6), CMPA (3/7).
        if matches!(opcode & 0xF000, 0xD000 | 0x9000 | 0xB000 | 0xC000 | 0x8000) {
            let reg = ((opcode >> 9) & 7) as u8;
            let opmode = (opcode >> 6) & 7;

            let op = match opcode & 0xF000 {
                0xD000 => AluOp::Add,
                0x9000 => AluOp::Sub,
                0xB000 => if opmode >= 4 && opmode <= 6 { AluOp::Eor } else { AluOp::Cmp },
                0xC000 => AluOp::And,
                0x8000 => AluOp::Or,
                _ => unreachable!(),
            };

            // Handle all opmodes: 0-2 = EA→Dn, 4-6 = Dn→EA, 3/7 = ADDA/SUBA/CMPA
            if (opmode != 3 && opmode != 7) || matches!(opcode & 0xF000, 0xD000 | 0x9000 | 0xB000) {
                let size = match opmode {
                    0 | 4 => Size::Byte,
                    1 | 5 | 3 => Size::Word,
                    2 | 6 | 7 => Size::Long,
                    _ => unreachable!(),
                };
                let to_reg = opmode <= 2;
                let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                let ea_reg = (opcode & 7) as u8;
                let ea_mode = AddrMode::decode(ea_mode_bits, ea_reg).unwrap();

                // ADDA/SUBA/CMPA: source is EA, destination is An
                let is_addr = matches!(opmode, 3 | 7)
                    && matches!(opcode & 0xF000, 0xD000 | 0x9000 | 0xB000);

                self.alu_op = op;
                self.size = size;
                self.src_mode = if to_reg || is_addr {
                    Some(ea_mode)
                } else {
                    Some(AddrMode::DataReg(reg))
                };
                self.dst_mode = if to_reg {
                    Some(AddrMode::DataReg(reg))
                } else if is_addr {
                    Some(AddrMode::AddrReg(reg))
                } else {
                    Some(ea_mode)
                };
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_SRC_EA;
                self.continue_instruction(bus);
                return;
            }
        }

        // --- ADDQ/SUBQ (0x5xxx with size != 3) ---
        // Also handles DBcc (size == 3, EA mode == 001) and Scc (size == 3, other EA)
        if (opcode & 0xF000) == 0x5000 {
            let quick_val = {
                let d = (opcode >> 9) & 7;
                if d == 0 { 8 } else { d }
            } as u32;
            let is_sub = (opcode & 0x0100) != 0;
            let opmode = (opcode >> 6) & 3;

            if opmode == 3 {
                // DBcc or Scc
                if (opcode & 0x38) == 0x08 {
                    // DBcc: decrement Dn and branch if not -1.
                    // Save displacement from IRC now, before FetchIRC overwrites it.
                    self.ea_reg = (opcode & 7) as u8;
                    self.src_val = u32::from(self.irc);
                    self.in_followup = true;
                    self.followup_tag = TAG_DBCC_EXECUTE;
                    self.micro_ops.push(MicroOp::FetchIRC);
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                } else {
                    // Scc: set byte to 0xFF or 0x00 based on condition
                    let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                    let ea_reg = (opcode & 7) as u8;
                    self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                    self.size = Size::Byte;
                    self.in_followup = true;
                    self.followup_tag = TAG_FETCH_DST_EA;
                    self.continue_instruction(bus);
                    return;
                }
            }

            let size = match opmode {
                0 => Size::Byte,
                1 => Size::Word,
                2 => Size::Long,
                _ => unreachable!(),
            };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.alu_op = if is_sub { AluOp::Sub } else { AluOp::Add };
            self.size = size;
            self.src_mode = Some(AddrMode::Immediate);
            self.src_val = quick_val;
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_DST_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- ANDI/ORI/EORI to CCR/SR ---
        if matches!(opcode, 0x003C | 0x023C | 0x0A3C | 0x007C | 0x027C | 0x0A7C) {
            let is_sr = (opcode & 0x0040) != 0; // bit 6: 0=CCR, 1=SR
            if is_sr && self.check_supervisor() {
                return;
            }
            let imm = self.consume_irc() as u32;
            let op_type = (opcode >> 9) & 7;
            if is_sr {
                let sr32 = u32::from(self.regs.sr);
                let result = match op_type {
                    0 => sr32 | imm,         // ORI
                    1 => sr32 & imm,         // ANDI
                    5 => sr32 ^ imm,         // EORI
                    _ => sr32,
                };
                self.regs.sr = (result as u16) & crate::flags::SR_MASK;
            } else {
                let ccr = u32::from(self.regs.sr & 0xFF);
                let result = match op_type {
                    0 => ccr | (imm & 0x1F),   // ORI to CCR
                    1 => ccr & imm,             // ANDI to CCR
                    5 => ccr ^ (imm & 0x1F),   // EORI to CCR
                    _ => ccr,
                };
                self.regs.sr = (self.regs.sr & 0xFF00) | (result as u16 & 0xFF);
            }
            self.micro_ops.push(MicroOp::Internal(8));
            return;
        }

        // --- Bit operations (static/immediate form): BTST/BCHG/BCLR/BSET #n ---
        // 0000 1000 TT MMMRRR (bits 11-8 = 0x08)
        if (opcode & 0xFF00) == 0x0800 {
            let bit_type = ((opcode >> 6) & 3) as u8;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;
            let bit_num = (self.consume_irc() & 0xFF) as u32;

            if ea_mode_bits == 0 {
                // Register destination: long, bit mod 32
                let bit = bit_num & 31;
                let val = self.regs.d[ea_reg as usize];
                self.regs.sr = if val & (1 << bit) == 0 {
                    self.regs.sr | crate::flags::Z
                } else {
                    self.regs.sr & !crate::flags::Z
                };
                let result = match bit_type {
                    0 => val,                   // BTST
                    1 => val ^ (1 << bit),      // BCHG
                    2 => val & !(1 << bit),     // BCLR
                    3 => val | (1 << bit),      // BSET
                    _ => val,
                };
                if bit_type != 0 {
                    self.regs.d[ea_reg as usize] = result;
                }
                let extra = match bit_type {
                    0 => 2,
                    2 => if bit >= 16 { 6 } else { 4 },
                    _ => if bit >= 16 { 4 } else { 2 },
                };
                self.micro_ops.push(MicroOp::Internal(extra));
            } else {
                // Memory destination: byte, bit mod 8
                self.bit_op = match bit_type {
                    0 => BitOp::Btst,
                    1 => BitOp::Bchg,
                    2 => BitOp::Bclr,
                    3 => BitOp::Bset,
                    _ => BitOp::Btst,
                };
                self.src_val = bit_num;
                self.size = Size::Byte;
                self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_DST_EA;
                // Don't call continue_instruction() here — we already consumed
                // IRC for the bit number, and FetchIRC hasn't run yet.  Pushing
                // Execute lets the FetchIRC complete first so calc_ea_start()
                // in TAG_FETCH_DST_EA reads the correct extension word.
                self.micro_ops.push(MicroOp::Execute);
            }
            return;
        }

        // --- Bit operations (dynamic form): BTST/BCHG/BCLR/BSET Dn ---
        // 0000 RRR 1TT MMMRRR (bit 8 = 1, bits 11-9 = register)
        if (opcode & 0xF000) == 0x0000 && (opcode & 0x0100) != 0 {
            // Exclude MOVEP (ea_mode = 001)
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            if ea_mode_bits != 1 {
                let bit_type = ((opcode >> 6) & 3) as u8;
                let bit_reg = ((opcode >> 9) & 7) as usize;
                let ea_reg = (opcode & 7) as u8;
                let bit_num = self.regs.d[bit_reg];

                if ea_mode_bits == 0 {
                    // Register destination: long, bit mod 32
                    let bit = bit_num & 31;
                    let val = self.regs.d[ea_reg as usize];
                    self.regs.sr = if val & (1 << bit) == 0 {
                        self.regs.sr | crate::flags::Z
                    } else {
                        self.regs.sr & !crate::flags::Z
                    };
                    let result = match bit_type {
                        0 => val,
                        1 => val ^ (1 << bit),
                        2 => val & !(1 << bit),
                        3 => val | (1 << bit),
                        _ => val,
                    };
                    if bit_type != 0 {
                        self.regs.d[ea_reg as usize] = result;
                    }
                    let extra = match bit_type {
                        0 => 2,
                        2 => if bit >= 16 { 6 } else { 4 },
                        _ => if bit >= 16 { 4 } else { 2 },
                    };
                    self.micro_ops.push(MicroOp::Internal(extra));
                } else {
                    // Memory destination: byte, bit mod 8
                    self.bit_op = match bit_type {
                        0 => BitOp::Btst,
                        1 => BitOp::Bchg,
                        2 => BitOp::Bclr,
                        3 => BitOp::Bset,
                        _ => BitOp::Btst,
                    };
                    self.src_val = bit_num;
                    self.size = Size::Byte;
                    self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                    self.in_followup = true;
                    self.followup_tag = TAG_FETCH_DST_EA;
                    self.continue_instruction(bus);
                }
                return;
            }
        }

        // --- MOVEP (0x0108/0x0148/0x0188/0x01C8) ---
        // 0000 DDD 1 OO 001 AAA — bit 8 set, ea_mode=001
        // Falls through from dynamic bit ops which exclude ea_mode=001
        if (opcode & 0xF000) == 0x0000 && (opcode & 0x0100) != 0 {
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            if ea_mode_bits == 1 {
                let dn = ((opcode >> 9) & 7) as usize;
                let an = (opcode & 7) as usize;
                let opmode = ((opcode >> 6) & 3) as u8;
                let is_write = opmode & 2 != 0;
                let is_long = opmode & 1 != 0;
                let total_bytes: u8 = if is_long { 4 } else { 2 };

                // Consume displacement from IRC
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(an) as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = false;

                // Pack state: movem_idx = byte index, ea_reg = dn, movem_an_reg = total
                self.movem_idx = 0;
                self.ea_reg = dn as u8;
                self.movem_an_reg = total_bytes;
                self.movem_is_write = is_write;

                if is_write {
                    // Write first byte (MSB of Dn)
                    let shift = (u32::from(total_bytes) - 1) * 8;
                    self.data = (self.regs.d[dn] >> shift) & 0xFF;
                    self.micro_ops.push(MicroOp::WriteByte);
                } else {
                    self.micro_ops.push(MicroOp::ReadByte);
                }

                self.in_followup = true;
                self.followup_tag = TAG_MOVEP_TRANSFER;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
        }

        // --- ALU immediate (ORI, ANDI, SUBI, ADDI, EORI, CMPI) ---
        if matches!(
            opcode & 0xFF00,
            0x0000 | 0x0200 | 0x0400 | 0x0600 | 0x0A00 | 0x0C00
        ) {
            let op = match (opcode >> 9) & 7 {
                0 => AluOp::Or,
                1 => AluOp::And,
                2 => AluOp::Sub,
                3 => AluOp::Add,
                5 => AluOp::Eor,
                6 => AluOp::Cmp,
                _ => {
                    // Bit operations or other 0x0xxx — not handled here
                    self.begin_group1_exception(4, self.instr_start_pc);
                    return;
                }
            };
            let size = match (opcode >> 6) & 3 {
                0 => Size::Byte,
                1 => Size::Word,
                2 => Size::Long,
                _ => {
                    // Size 3 is invalid for ALU immediate ops → illegal instruction
                    eprintln!("ILLEGAL: ALU-imm size=3 opcode=${:04X} at PC=${:08X}", opcode, self.instr_start_pc);
                    self.begin_group1_exception(4, self.instr_start_pc);
                    return;
                }
            };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.alu_op = op;
            self.size = size;
            self.src_mode = Some(AddrMode::Immediate);
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_DATA;
            self.continue_instruction(bus);
            return;
        }

        // --- MOVEQ (0x7xxx with bit 8 = 0) ---
        if (opcode & 0xF100) == 0x7000 {
            let reg = ((opcode >> 9) & 7) as usize;
            let val = (opcode & 0xFF) as i8 as i32 as u32;
            self.regs.d[reg] = val;
            self.set_flags_move(val, Size::Long);
            return;
        }

        // --- Shifts and Rotates (0xExxx) ---
        if (opcode & 0xF000) == 0xE000 {
            let size_bits = ((opcode >> 6) & 3) as u8;

            if size_bits == 3 {
                // Memory shift: 1110 0TT D 11 MMMRRR — shift by 1, word size
                let shift_type = ((opcode >> 9) & 3) as u8;
                let direction = (opcode >> 8) & 1; // 0=right, 1=left
                let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                let ea_reg = (opcode & 7) as u8;

                self.size = Size::Word;
                self.src_val = (shift_type as u32) | ((direction as u32) << 4) | (1 << 8);
                self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_DST_EA;
                self.continue_instruction(bus);
            } else {
                // Register shift: 1110 CCC D SS I/R TT RRR
                let count_reg = ((opcode >> 9) & 7) as u8;
                let direction = (opcode >> 8) & 1;
                let size = match size_bits {
                    0 => Size::Byte,
                    1 => Size::Word,
                    2 => Size::Long,
                    _ => unreachable!(),
                };
                let ir_bit = (opcode >> 5) & 1;
                let shift_type = ((opcode >> 3) & 3) as u8;
                let reg = (opcode & 7) as u8;

                let count = if ir_bit == 0 {
                    // Immediate: 1-8 (0 encodes 8)
                    let c = count_reg as u32;
                    if c == 0 { 8 } else { c }
                } else {
                    // Register: mod 64
                    self.regs.d[count_reg as usize] & 63
                };

                self.perform_shift(reg, count, direction as u8, shift_type, size);

                // Timing: 6+2n (byte/word), 8+2n (long)
                let base = if size == Size::Long { 4u8 } else { 2u8 };
                let delay = base + (count as u8) * 2;
                self.micro_ops.push(MicroOp::Internal(delay));
            }
            return;
        }

        // --- BRA/Bcc/BSR (0x6xxx) ---
        if (opcode & 0xF000) == 0x6000 {
            let cond = ((opcode >> 8) & 0x0F) as u8;
            let disp8 = (opcode & 0xFF) as i8;
            self.in_followup = true;

            if cond == 1 {
                // BSR: push return PC, then branch
                self.followup_tag = TAG_BSR_EXECUTE;
                let return_pc = if disp8 == 0 {
                    self.instr_start_pc.wrapping_add(4)
                } else {
                    self.instr_start_pc.wrapping_add(2)
                };
                self.data = return_pc;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
            } else {
                self.followup_tag = TAG_BCC_EXECUTE;
            }

            if disp8 == 0 {
                // 16-bit displacement: read from IRC now, before FetchIRC
                // overwrites it. Queue FetchIRC to advance past the word.
                self.src_val = u32::from(self.irc);
                self.micro_ops.push(MicroOp::FetchIRC);
                // followup_tag stays as TAG_BCC_EXECUTE / TAG_BSR_EXECUTE
                // (no separate FETCH_DISP phase needed)
            } else {
                self.src_val = disp8 as i32 as u32;
            }

            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- JSR / JMP (0x4E80 / 0x4EC0) ---
        if (opcode & 0xFFC0) == 0x4E80 || (opcode & 0xFFC0) == 0x4EC0 {
            let is_jsr = (opcode & 0x40) == 0;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;

            if is_jsr {
                // Compute return PC and save for push AFTER EA resolution.
                // The real 68000 computes EA first, then pushes, then jumps.
                let ext = self.src_mode.unwrap().ext_word_count();
                let return_pc = self.instr_start_pc
                    .wrapping_add(2)
                    .wrapping_add(u32::from(ext) * 2);
                self.dst_val = return_pc;
            }

            self.continue_instruction(bus);
            return;
        }

        // --- LEA (0x41C0) ---
        if (opcode & 0xF1C0) == 0x41C0 {
            let reg = ((opcode >> 9) & 7) as u8;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            // Store destination A-register in dst_mode (not ea_reg, which
            // gets overwritten by calc_ea_start for displacement modes).
            self.dst_mode = Some(AddrMode::AddrReg(reg));
            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- SWAP (0x4840-0x4847) ---
        if (opcode & 0xFFF8) == 0x4840 {
            let r = (opcode & 7) as usize;
            let val = self.regs.d[r];
            let result = (val >> 16) | (val << 16);
            self.regs.d[r] = result;
            self.set_flags_move(result, Size::Long);
            return;
        }

        // --- PEA (0x4840 with ea_mode >= 2) ---
        if (opcode & 0xFFC0) == 0x4840 {
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            if self.calc_ea_start(self.src_mode.unwrap(), true) {
                self.followup_tag = TAG_FETCH_DST_EA;
            }
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- EXT (0x4880/0x48C0 with ea_mode = 000) ---
        if (opcode & 0xFFF8) == 0x4880 {
            // EXT.w: sign-extend byte → word in Dn
            let r = (opcode & 7) as usize;
            let val = self.regs.d[r];
            let ext = (val as u8 as i8 as i16) as u16;
            self.regs.d[r] = (val & 0xFFFF_0000) | u32::from(ext);
            self.set_flags_move(u32::from(ext), Size::Word);
            return;
        }
        if (opcode & 0xFFF8) == 0x48C0 {
            // EXT.l: sign-extend word → long in Dn
            let r = (opcode & 7) as usize;
            let val = self.regs.d[r];
            let ext = (val as u16 as i16 as i32) as u32;
            self.regs.d[r] = ext;
            self.set_flags_move(ext, Size::Long);
            return;
        }

        // --- MOVEM register→memory (0x4880/0x48C0 with ea_mode >= 2) ---
        if (opcode & 0xFF80) == 0x4880 {
            let size = if (opcode & 0x0040) != 0 { Size::Long } else { Size::Word };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;
            let mask = self.consume_irc();

            self.movem_mask = mask;
            self.movem_is_write = true;
            self.movem_idx = 0; // no deferred advance on first iteration
            self.size = size;

            let mode = AddrMode::decode(ea_mode_bits, ea_reg).unwrap();
            if let AddrMode::AddrIndPreDec(r) = mode {
                self.movem_an_reg = r;
                self.addr = self.regs.a(r as usize);
                self.in_followup = true;
                self.followup_tag = TAG_MOVEM_NEXT;
                self.micro_ops.push(MicroOp::Execute);
            } else {
                self.movem_an_reg = 0xFF;
                self.src_mode = Some(mode);
                self.in_followup = true;
                // Defer EA resolution: consume_irc() for the mask queued a FetchIRC
                // that must complete before calc_ea_start can read EA extension words.
                self.followup_tag = TAG_MOVEM_RESOLVE_EA;
                self.micro_ops.push(MicroOp::Execute);
            }
            return;
        }

        // --- MOVEM memory→register (0x4C80/0x4CC0) ---
        if (opcode & 0xFF80) == 0x4C80 {
            let size = if (opcode & 0x0040) != 0 { Size::Long } else { Size::Word };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;
            let mask = self.consume_irc();

            self.movem_mask = mask;
            self.movem_is_write = false;
            self.size = size;

            let mode = AddrMode::decode(ea_mode_bits, ea_reg).unwrap();
            if let AddrMode::AddrIndPostInc(r) = mode {
                self.movem_an_reg = r;
                self.addr = self.regs.a(r as usize);
                self.in_followup = true;
                self.followup_tag = TAG_MOVEM_NEXT;
                self.micro_ops.push(MicroOp::Execute);
            } else {
                self.movem_an_reg = 0xFF;
                self.src_mode = Some(mode);
                self.in_followup = true;
                // Defer EA resolution (same reason as register→memory above).
                self.followup_tag = TAG_MOVEM_RESOLVE_EA;
                self.micro_ops.push(MicroOp::Execute);
            }
            return;
        }

        // --- NBCD (0x4800) ---
        if (opcode & 0xFFC0) == 0x4800 {
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            if ea_mode_bits == 0 {
                // Register mode: NBCD Dn
                let val = (self.regs.d[ea_reg as usize] & 0xFF) as u8;
                let x = self.x_flag();
                let (result, carry, overflow) = self.nbcd_op(val, x);
                self.regs.d[ea_reg as usize] =
                    (self.regs.d[ea_reg as usize] & 0xFFFF_FF00) | u32::from(result);
                self.set_bcd_flags(result, carry, overflow);
                self.micro_ops.push(MicroOp::Internal(2));
                return;
            }

            // Memory: read-modify-write
            self.size = Size::Byte;
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_DST_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- NOT / NEG / NEGX + MOVE from SR / to CCR / to SR ---
        // These share the 0x40xx-0x46xx space, with size=11 being SR/CCR ops.
        // Bit 8 is always 0 for this group. CHK has bit 8=1 (bits 8-6 = 110),
        // so using 0xFF00 instead of 0xFE00 avoids conflicting with CHK.
        if matches!(opcode & 0xFF00, 0x4000 | 0x4200 | 0x4400 | 0x4600) {
            let sub_op = (opcode >> 9) & 7; // 0=NEGX/fromSR, 1=CLR, 2=NEG/toCCR, 3=NOT/toSR
            let size_bits = ((opcode >> 6) & 3) as u8;

            // Size=11 means SR/CCR operations (except CLR which doesn't have this)
            if size_bits == 3 {
                let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                let ea_reg = (opcode & 7) as u8;

                match sub_op {
                    0 => {
                        // MOVE from SR (0x40C0) — not privileged on 68000
                        self.data = u32::from(self.regs.sr);
                        self.size = Size::Word;
                        self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                        if ea_mode_bits == 0 {
                            // Dn: 6 cycles (Internal(2))
                            self.regs.d[ea_reg as usize] = (self.regs.d[ea_reg as usize] & 0xFFFF_0000) | (self.data & 0xFFFF);
                            self.micro_ops.push(MicroOp::Internal(2));
                        } else {
                            // Memory: dummy read then write
                            self.in_followup = true;
                            self.followup_tag = TAG_FETCH_DST_EA;
                            self.continue_instruction(bus);
                        }
                        return;
                    }
                    2 => {
                        // MOVE to CCR (0x44C0) — read source, apply to CCR
                        self.size = Size::Word;
                        self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                        self.dst_mode = None;
                        self.in_followup = true;
                        self.followup_tag = TAG_FETCH_SRC_EA;
                        self.continue_instruction(bus);
                        return;
                    }
                    3 => {
                        // MOVE to SR (0x46C0) — privileged
                        if self.check_supervisor() {
                            return;
                        }
                        self.size = Size::Word;
                        self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                        self.dst_mode = None;
                        self.in_followup = true;
                        self.followup_tag = TAG_FETCH_SRC_EA;
                        self.continue_instruction(bus);
                        return;
                    }
                    _ => {
                        // size=3 for CLR (sub_op=1) is invalid
                        self.begin_group1_exception(4, self.instr_start_pc);
                        return;
                    }
                }
            }

            // Standard NOT/NEG/NEGX (size 00/01/10)
            if sub_op == 0 || sub_op == 2 || sub_op == 3 {
                let size = match size_bits {
                    0 => Size::Byte,
                    1 => Size::Word,
                    2 => Size::Long,
                    _ => unreachable!(),
                };
                let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                let ea_reg = (opcode & 7) as u8;

                self.size = size;
                self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_DST_EA;
                self.continue_instruction(bus);
                return;
            }
        }

        // --- TAS (0x4AC0) --- must be before CLR/TST which matches 0x4A00
        if (opcode & 0xFFC0) == 0x4AC0 {
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            if ea_mode_bits == 0 {
                // TAS Dn: test and set bit 7
                let val = self.regs.d[ea_reg as usize] & 0xFF;
                self.set_flags_logic(val as u32, Size::Byte);
                self.regs.d[ea_reg as usize] =
                    (self.regs.d[ea_reg as usize] & 0xFFFF_FF00) | ((val | 0x80) as u32);
                return;
            }

            // Memory: read-modify-write
            self.size = Size::Byte;
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_DST_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- CLR / TST (0x4200 / 0x4A00) ---
        if (opcode & 0xFF00) == 0x4200 || (opcode & 0xFF00) == 0x4A00 {
            let size = match (opcode >> 6) & 3 {
                0 => Size::Byte,
                1 => Size::Word,
                2 => Size::Long,
                _ => {
                    self.begin_group1_exception(4, self.instr_start_pc);
                    return;
                }
            };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.size = size;
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_DST_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- CHK (0x4180) ---
        // CHK <ea>, Dn — check Dn.w against bounds [0, EA.w]
        // 0100 RRR 110 MMMRRR
        if (opcode & 0xF1C0) == 0x4180 {
            self.size = Size::Word;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;
            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- ILLEGAL (0x4AFC) ---
        if opcode == 0x4AFC {
            self.begin_group1_exception(4, self.instr_start_pc);
            return;
        }

        // --- Line A / Line F ---
        if (opcode & 0xF000) == 0xA000 {
            self.begin_group1_exception(10, self.instr_start_pc);
            return;
        }
        if (opcode & 0xF000) == 0xF000 {
            self.begin_group1_exception(11, self.instr_start_pc);
            return;
        }

        // --- RTS (0x4E75) ---
        if opcode == 0x4E75 {
            self.in_followup = true;
            self.followup_tag = TAG_RTS_PC_HI;
            self.micro_ops.push(MicroOp::PopLongHi);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- RTE (0x4E73) ---
        // RTE pops SR then PC from the supervisor stack. Restoring SR may
        // switch to user mode, so we can't use PopWord/PopLong (which use
        // active_sp). Instead, track SSP manually via self.addr and update
        // regs.ssp directly after each read.
        if opcode == 0x4E73 {
            if self.check_supervisor() {
                return;
            }
            self.addr = self.regs.ssp;
            self.in_followup = true;
            self.followup_tag = TAG_RTE_READ_SR;
            self.micro_ops.push(MicroOp::ReadWord);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- RTR (0x4E77) ---
        if opcode == 0x4E77 {
            // Read CCR word from stack
            self.in_followup = true;
            self.followup_tag = TAG_RTR_READ_CCR;
            self.micro_ops.push(MicroOp::PopWord);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- TRAP (0x4E40-0x4E4F) ---
        if (opcode & 0xFFF0) == 0x4E40 {
            let vector = (opcode & 0xF) as u8;
            let next_pc = self.instr_start_pc.wrapping_add(2);
            self.begin_group1_exception(32 + vector, next_pc);
            return;
        }

        // --- TRAPV (0x4E76) ---
        if opcode == 0x4E76 {
            if self.regs.sr & crate::flags::V != 0 {
                let next_pc = self.instr_start_pc.wrapping_add(2);
                self.begin_group1_exception(7, next_pc);
            }
            return;
        }

        // --- LINK (0x4E50-0x4E57) ---
        if (opcode & 0xFFF8) == 0x4E50 {
            let r = (opcode & 7) as u8;
            // Push An, then An = SP, then SP += displacement
            self.data = self.regs.a(r as usize);
            self.ea_reg = r;
            self.in_followup = true;
            self.followup_tag = TAG_LINK_DISP;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- UNLK (0x4E58-0x4E5F) ---
        if (opcode & 0xFFF8) == 0x4E58 {
            let r = (opcode & 7) as u8;
            // Save original SP for AE undo — if the popped An address is
            // odd, AE fires and A7 must revert to its pre-UNLK value.
            self.sp_undo = Some((self.regs.is_supervisor(), self.regs.active_sp()));
            // SP = An, pop An from stack
            self.regs.set_active_sp(self.regs.a(r as usize));
            self.ea_reg = r;
            self.in_followup = true;
            self.followup_tag = TAG_UNLK_POP_HI;
            self.micro_ops.push(MicroOp::PopLongHi);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- MOVE USP (0x4E60-0x4E6F) ---
        if (opcode & 0xFFF0) == 0x4E60 {
            if self.check_supervisor() {
                return;
            }
            let r = (opcode & 7) as usize;
            if (opcode & 0x08) == 0 {
                // MOVE An,USP — An → USP
                self.regs.usp = self.regs.a(r);
            } else {
                // MOVE USP,An — USP → An
                let val = self.regs.usp;
                self.regs.set_a(r, val);
            }
            return;
        }

        // --- STOP (0x4E72) ---
        // The real 68000 reads the immediate SR from IRC but does NOT
        // complete a pipeline refill before entering the stopped state.
        // We consume IRC for the immediate, then clear the FetchIRC it
        // queued and enter Stopped directly.
        if opcode == 0x4E72 {
            if self.check_supervisor() {
                return;
            }
            let new_sr = self.consume_irc();
            self.regs.sr = new_sr & crate::flags::SR_MASK;
            // Clear the FetchIRC that consume_irc queued — STOP doesn't
            // complete the pipeline refill.
            self.micro_ops.clear();
            // Fix irc_addr so that interrupt wake-up (which saves irc_addr
            // as the return PC) pushes the address of the next instruction
            // after STOP, not the stale address of the immediate word.
            self.irc_addr = self.next_fetch_addr;
            self.state = State::Stopped;
            return;
        }

        // --- Simple one-word instructions ---
        match opcode {
            0x4E71 => { /* NOP */ }
            0x4E70 => {
                // RESET (supervisor only)
                if self.check_supervisor() {
                    return;
                }
                self.micro_ops.push(MicroOp::AssertReset);
                self.micro_ops.push(MicroOp::Internal(124));
            }
            _ => {
                // Treat unknown opcodes as illegal for this CPU core model and
                // vector through the normal illegal-instruction exception.
                self.begin_group1_exception(4, self.instr_start_pc);
            }
        }
    }

    /// Continue a multi-phase instruction based on the current follow-up tag.
    ///
    /// Each tag represents a point in the instruction's execution pipeline.
    /// After completing its work, each handler sets the next tag and queues
    /// micro-ops needed to reach it.
    pub fn continue_instruction<B: M68kBus>(&mut self, _bus: &mut B) {
        match self.followup_tag {
            // --- Operand fetch pipeline ---

            TAG_FETCH_SRC_EA => {
                if self.calc_ea_start(self.src_mode.unwrap(), true) {
                    self.followup_tag = TAG_FETCH_SRC_DATA;
                }
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_FETCH_SRC_DATA => {
                let opcode = self.ir;

                // MOVEM: EA resolved, go to transfer loop
                if (opcode & 0xFF80) == 0x4880 || (opcode & 0xFF80) == 0x4C80 {
                    self.followup_tag = TAG_MOVEM_NEXT;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                // LEA/PEA: source "data" is the computed address
                if (opcode & 0xF1C0) == 0x41C0 || (opcode & 0xFFC0) == 0x4840 {
                    self.src_val = self.addr;
                    self.followup_tag = TAG_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                // JMP/JSR: source address is the jump target
                if (opcode & 0xFFC0) == 0x4EC0 || (opcode & 0xFFC0) == 0x4E80 {
                    self.followup_tag = TAG_JSR_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                // CHK: read source operand, then compare bounds.
                // CHK does NOT undo EA register modifications on address error.
                if (opcode & 0xF1C0) == 0x4180 {
                    self.ae_undo_reg = None;
                    match self.src_mode.unwrap() {
                        AddrMode::DataReg(r) => {
                            self.src_val = self.regs.d[r as usize] & 0xFFFF;
                            self.followup_tag = TAG_CHK_EXECUTE;
                            self.micro_ops.push(MicroOp::Execute);
                        }
                        AddrMode::Immediate => {
                            self.src_val = u32::from(self.consume_irc());
                            self.followup_tag = TAG_CHK_EXECUTE;
                            self.micro_ops.push(MicroOp::Execute);
                        }
                        _ => {
                            // Memory: queue word read, then compare
                            self.followup_tag = TAG_CHK_EXECUTE;
                            self.queue_read_ops(Size::Word);
                            self.micro_ops.push(MicroOp::Execute);
                        }
                    }
                    return;
                }

                // MULU/MULS/DIVU/DIVS: read source word, then execute
                if matches!(opcode & 0xF000, 0xC000 | 0x8000) {
                    let opmode = (opcode >> 6) & 7;
                    if opmode == 3 || opmode == 7 {
                        match self.src_mode.unwrap() {
                            AddrMode::DataReg(r) => {
                                self.src_val = self.regs.d[r as usize] & 0xFFFF;
                                self.followup_tag = TAG_MULDIV_EXECUTE;
                                self.micro_ops.push(MicroOp::Execute);
                            }
                            AddrMode::Immediate => {
                                self.src_val = u32::from(self.consume_irc());
                                self.followup_tag = TAG_MULDIV_EXECUTE;
                                self.micro_ops.push(MicroOp::Execute);
                            }
                            _ => {
                                self.followup_tag = TAG_MULDIV_EXECUTE;
                                self.queue_read_ops(Size::Word);
                                self.micro_ops.push(MicroOp::Execute);
                            }
                        }
                        return;
                    }
                }

                match self.src_mode.unwrap() {
                    AddrMode::DataReg(r) => {
                        self.src_val = self.regs.d[r as usize];
                        self.followup_tag = TAG_FETCH_DST_EA;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(r) => {
                        self.src_val = self.regs.a(r as usize);
                        self.followup_tag = TAG_FETCH_DST_EA;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        let v = self.consume_irc();
                        if self.size == Size::Long {
                            self.src_val = u32::from(v) << 16;
                            self.followup_tag = TAG_DATA_SRC_LONG;
                        } else {
                            self.src_val = u32::from(v);
                            self.followup_tag = TAG_FETCH_DST_EA;
                        }
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        // Memory source: queue read ops, data arrives via finish_bus_cycle
                        self.followup_tag = TAG_FETCH_DST_EA;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }

            TAG_FETCH_DST_EA => {
                // If source was a memory read, the result is in self.data.
                // Copy it to src_val now, before the dst read can overwrite data.
                if let Some(mode) = self.src_mode {
                    if !matches!(
                        mode,
                        AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
                    ) {
                        self.src_val = self.data;
                    }
                }

                // Source phase is complete — any source EA side effects (postinc,
                // predec) are committed. Clear ae_undo_reg so only destination
                // EA side effects can be rolled back on address errors.
                // Also clear program_space_access since the source read is done;
                // destination writes always use data space.
                self.ae_undo_reg = None;
                self.program_space_access = false;

                if let Some(m) = self.dst_mode {
                    if self.calc_ea_start(m, false) {
                        self.followup_tag = TAG_FETCH_DST_DATA;
                    }
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    self.followup_tag = TAG_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }

            TAG_FETCH_DST_DATA => {
                // MOVE and Scc are write-only: skip the destination read.
                // Read-modify-write instructions (ALU ops, CLR) need the read.
                let is_move = (self.ir & 0xC000) == 0 && (self.ir & 0x3000) != 0;
                let is_scc = (self.ir & 0xF0C0) == 0x50C0
                    && (self.ir & 0x0038) != 0x0008;
                if is_move || is_scc {
                    self.followup_tag = TAG_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                match self.dst_mode.unwrap() {
                    AddrMode::DataReg(r) => {
                        self.dst_val = self.regs.d[r as usize];
                        self.followup_tag = TAG_EXECUTE;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(r) => {
                        self.dst_val = self.regs.a(r as usize);
                        self.followup_tag = TAG_EXECUTE;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        // BTST Dn, #imm: read immediate value from IRC
                        self.dst_val = u32::from(self.consume_irc());
                        self.followup_tag = TAG_EXECUTE;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        self.followup_tag = TAG_EXECUTE;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }

            // --- Execute and writeback ---

            TAG_EXECUTE => {
                // If destination was a memory read, the result is in self.data.
                // Copy to dst_val before execute uses it. Skip for registers
                // (already loaded from register file) and Immediate (loaded
                // from IRC in TAG_FETCH_DST_DATA).
                if let Some(mode) = self.dst_mode {
                    if !matches!(
                        mode,
                        AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
                    ) {
                        self.dst_val = self.data;
                    }
                }

                self.perform_execute();
                self.followup_tag = TAG_WRITEBACK;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_WRITEBACK => {
                self.perform_writeback();
                self.in_followup = false;
            }

            // --- EA extension word handlers ---

            TAG_EA_SRC_LONG => {
                self.addr |= u32::from(self.consume_irc());
                self.followup_tag = TAG_FETCH_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_SRC_DISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.regs.a(self.ea_reg as usize).wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_SRC_PCDISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.ea_pc.wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_DST_LONG => {
                self.addr |= u32::from(self.consume_irc());
                self.followup_tag = TAG_FETCH_DST_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_DST_DISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.regs.a(self.ea_reg as usize).wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_DST_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_DST_PCDISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.ea_pc.wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_DST_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- Immediate long lo-word handlers ---

            TAG_DATA_SRC_LONG => {
                self.src_val |= u32::from(self.consume_irc());
                self.followup_tag = TAG_FETCH_DST_EA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_DATA_DST_LONG => {
                self.dst_val |= u32::from(self.consume_irc());
                self.followup_tag = TAG_EXECUTE;
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- Branch handlers ---

            TAG_BCC_EXECUTE => {
                let cond = ((self.ir >> 8) & 0x0F) as u8;
                if self.check_condition(cond) {
                    let disp = self.src_val as i16 as i32;
                    self.regs.pc = self.instr_start_pc.wrapping_add(2).wrapping_add(disp as u32);
                    self.next_fetch_addr = self.regs.pc;
                    self.micro_ops.clear();
                    self.micro_ops.push(MicroOp::FetchIRC);
                    self.micro_ops.push(MicroOp::PromoteIRC);
                }
                self.in_followup = false;
            }

            TAG_DBCC_EXECUTE => {
                let cond = ((self.ir >> 8) & 0x0F) as u8;
                let reg = self.ea_reg as usize;
                let disp = self.src_val as i16 as i32;

                if !self.check_condition(cond) {
                    let counter = (self.regs.d[reg] & 0xFFFF) as u16;
                    // Save original for undo on branch AE (odd target).
                    self.dbcc_dn_undo = Some((reg as u8, counter));
                    let decremented = counter.wrapping_sub(1);
                    self.regs.d[reg] =
                        (self.regs.d[reg] & 0xFFFF_0000) | u32::from(decremented);

                    if decremented != 0xFFFF {
                        // Branch taken: reload prefetch from branch target
                        self.regs.pc = self
                            .instr_start_pc
                            .wrapping_add(2)
                            .wrapping_add(disp as u32);
                        self.next_fetch_addr = self.regs.pc;
                        self.micro_ops.clear();
                        self.micro_ops.push(MicroOp::FetchIRC);
                        self.micro_ops.push(MicroOp::PromoteIRC);
                    }
                }
                self.in_followup = false;
            }

            // --- Subroutine handlers ---

            TAG_JSR_EXECUTE => {
                let is_jsr = (self.ir & 0x40) == 0;
                // Set PC to target and start the pipeline refill.
                self.regs.pc = self.addr;
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                // FetchIRC at target first — triggers AE if odd address.
                // For JSR, the push happens AFTER FetchIRC succeeds.
                // This matches the real 68000 where the stack is not
                // modified if the jump target is misaligned.
                self.micro_ops.push(MicroOp::FetchIRC);
                if is_jsr {
                    self.data = self.dst_val; // return PC saved at decode
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            TAG_BSR_EXECUTE => {
                let disp = self.src_val as i16 as i32;
                self.regs.pc = self.instr_start_pc.wrapping_add(2).wrapping_add(disp as u32);
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            TAG_RTS_PC_HI => {
                // PopLongHi stored hi word in self.data (already shifted << 16).
                // PopLongLo will combine the lo word into self.data.
                self.followup_tag = TAG_RTS_PC_LO;
                self.micro_ops.push(MicroOp::PopLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_RTS_PC_LO => {
                // self.data now has the complete 32-bit return address
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- Exception handlers ---

            TAG_EXC_STACK_PC_HI => {
                self.followup_tag = TAG_EXC_STACK_PC_LO;
                self.micro_ops.push(MicroOp::PushLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_STACK_PC_LO => {
                // Both interrupts and group 1/2 exceptions save the old SR
                // before entering supervisor mode. Push the saved value.
                self.data = u32::from(self.ae_saved_sr);
                self.followup_tag = TAG_EXC_STACK_SR;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_STACK_SR => {
                if let Some(vector) = self.exc_vector {
                    // Group 1/2 exception: skip InterruptAck, use known vector.
                    // Don't clear exc_vector yet — TAG_EXC_FINISH needs it to
                    // distinguish group 1/2 from interrupts (interrupt mask).
                    self.data = u32::from(vector);
                    self.followup_tag = TAG_EXC_FETCH_VECTOR;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    // Hardware interrupt: need InterruptAck to get vector
                    self.followup_tag = TAG_EXC_FETCH_VECTOR;
                    self.micro_ops.push(MicroOp::InterruptAck);
                    self.micro_ops.push(MicroOp::Execute);
                }
            }

            TAG_EXC_FETCH_VECTOR => {
                let vector = self.data as u8;
                self.addr = u32::from(vector) * 4;
                self.size = Size::Long;
                self.followup_tag = TAG_EXC_FINISH;
                self.queue_read_ops(Size::Long);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_FINISH => {
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                // For interrupts, set supervisor + clear trace + update mask.
                // For group 1/2, supervisor and trace were already set in
                // begin_group1_exception; don't change interrupt mask.
                if self.exc_vector.is_some() {
                    // Group 1/2: supervisor, trace, and mask already handled
                } else {
                    // Hardware interrupt: supervisor mode and trace were set in
                    // initiate_interrupt_exception. Update the interrupt mask
                    // to the level being acknowledged (happens after InterruptAck).
                    self.regs.sr =
                        (self.regs.sr & !0x0700) | (u16::from(self.target_ipl) << 8);
                }
                self.exc_vector = None;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- RTE handlers ---
            // RTE reads 6 bytes from SSP: SR(2) + PC(4). We track the SSP
            // position in self.addr and update regs.ssp directly because
            // restoring SR may switch to user mode (making active_sp = USP).
            TAG_RTE_READ_SR => {
                let new_sr = (self.data as u16) & crate::flags::SR_MASK;
                self.addr = self.addr.wrapping_add(2);
                self.regs.sr = new_sr;
                self.regs.ssp = self.addr;
                self.followup_tag = TAG_RTE_READ_PC_HI;
                self.micro_ops.push(MicroOp::ReadWord);
                self.micro_ops.push(MicroOp::Execute);
            }
            TAG_RTE_READ_PC_HI => {
                self.src_val = self.data; // Save PC hi word
                self.addr = self.addr.wrapping_add(2);
                self.regs.ssp = self.addr;
                self.followup_tag = TAG_RTE_READ_PC_LO;
                self.micro_ops.push(MicroOp::ReadWord);
                self.micro_ops.push(MicroOp::Execute);
            }
            TAG_RTE_READ_PC_LO => {
                let target = (self.src_val << 16) | (self.data & 0xFFFF);
                self.addr = self.addr.wrapping_add(2);
                self.regs.ssp = self.addr;
                self.regs.pc = target;
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- RTR handlers ---
            TAG_RTR_READ_CCR => {
                // data has the CCR word. Apply only bits 0-4.
                let ccr = self.data as u16;
                self.regs.sr = (self.regs.sr & 0xFF00) | (ccr & 0x001F);
                self.followup_tag = TAG_RTR_READ_PC_HI;
                self.micro_ops.push(MicroOp::PopLongHi);
                self.micro_ops.push(MicroOp::Execute);
            }
            TAG_RTR_READ_PC_HI => {
                self.followup_tag = TAG_RTR_READ_PC_LO;
                self.micro_ops.push(MicroOp::PopLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }
            TAG_RTR_READ_PC_LO => {
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- LINK handler ---
            TAG_LINK_DISP => {
                // Push done. An = current SP, then SP += sign-extended displacement
                let r = self.ea_reg as usize;
                let sp = self.regs.active_sp();
                self.regs.set_a(r, sp);
                let disp = self.consume_irc() as i16 as i32;
                self.regs.set_active_sp(sp.wrapping_add(disp as u32));
                self.in_followup = false;
            }

            // --- UNLK handlers ---
            TAG_UNLK_POP_HI => {
                self.followup_tag = TAG_UNLK_POP_LO;
                self.micro_ops.push(MicroOp::PopLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }
            TAG_UNLK_POP_LO => {
                let r = self.ea_reg as usize;
                self.regs.set_a(r, self.data);
                self.sp_undo = None; // Completed successfully, no undo needed
                self.in_followup = false;
            }

            // --- MOVEM: deferred EA resolution ---
            // Runs after FetchIRC from mask consumption has refilled IRC.
            // Now it's safe to call calc_ea_start which may consume IRC for
            // EA extension words (AbsShort, AbsLong hi, AddrIndIndex, PcIndex).
            TAG_MOVEM_RESOLVE_EA => {
                let mode = self.src_mode.unwrap();
                if self.calc_ea_start(mode, true) {
                    // EA fully resolved (AddrInd, AbsShort, AddrIndIndex, PcIndex)
                    self.followup_tag = TAG_MOVEM_NEXT;
                }
                // If false, calc_ea_start set a followup tag (e.g. TAG_EA_SRC_DISP,
                // TAG_EA_SRC_LONG) which will eventually reach TAG_FETCH_SRC_DATA
                // where the MOVEM redirect routes to TAG_MOVEM_NEXT.
                // Always push Execute to drive the next tag handler.
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- MOVEM transfer loop ---
            TAG_MOVEM_NEXT => {
                if self.movem_is_write {
                    // Register → memory.
                    // Apply deferred address advance from previous iteration.
                    // Write ops read self.addr at execution time, so we can't
                    // advance until after they've run. movem_idx=1 signals
                    // that the previous iteration queued writes at self.addr
                    // and we now need to step past them.
                    if self.movem_idx != 0 {
                        self.addr = self.addr.wrapping_add(self.size.bytes());
                        self.movem_idx = 0;
                    }

                    if self.movem_mask == 0 {
                        // All done. For predecrement, write the final address
                        // to An now (deferred from the loop to preserve An's
                        // original value for any in-mask An write).
                        if self.movem_an_reg != 0xFF {
                            self.regs.set_a(self.movem_an_reg as usize, self.addr);
                        }
                        self.in_followup = false;
                        return;
                    }
                    let idx = self.movem_mask.trailing_zeros() as u8;
                    self.movem_mask &= !(1u16 << idx);

                    // Get register value
                    let val = if self.movem_an_reg != 0xFF {
                        // Predecrement: reversed bit order
                        // Bits 0-7 = A7..A0, bits 8-15 = D7..D0
                        if idx < 8 {
                            self.regs.a((7 - idx) as usize)
                        } else {
                            self.regs.d[(15 - idx) as usize]
                        }
                    } else {
                        // Normal order: bits 0-7 = D0..D7, bits 8-15 = A0..A7
                        if idx < 8 {
                            self.regs.d[idx as usize]
                        } else {
                            self.regs.a((idx - 8) as usize)
                        }
                    };

                    if self.movem_an_reg != 0xFF {
                        // Predecrement: decrement address before write.
                        // Don't update An yet — if An is in the mask, the real
                        // 68000 writes An's original (un-decremented) value.
                        // An gets the final address only when mask is empty.
                        self.addr = self.addr.wrapping_sub(self.size.bytes());
                    }

                    self.data = val;
                    self.queue_write_ops(self.size);

                    if self.movem_an_reg == 0xFF {
                        // Defer address advance until next iteration (after
                        // write ops have executed and read self.addr).
                        self.movem_idx = 1;
                    }

                    self.followup_tag = TAG_MOVEM_NEXT;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    // Memory → register
                    if self.movem_mask == 0 {
                        // All done — extra read at final address (68000 quirk)
                        self.micro_ops.push(MicroOp::ReadWord);
                        if self.movem_an_reg != 0xFF {
                            self.regs.set_a(self.movem_an_reg as usize, self.addr);
                        }
                        self.in_followup = false;
                        return;
                    }
                    let idx = self.movem_mask.trailing_zeros() as u8;
                    self.movem_idx = idx;

                    self.queue_read_ops(self.size);
                    self.followup_tag = TAG_MOVEM_STORE;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }

            TAG_MOVEM_STORE => {
                let idx = self.movem_idx;
                self.movem_mask &= !(1u16 << idx);

                // Store to register (sign-extend word → long)
                let val = if self.size == Size::Word {
                    (self.data as u16 as i16 as i32) as u32
                } else {
                    self.data
                };

                // Normal order: bits 0-7 = D0..D7, bits 8-15 = A0..A7
                if idx < 8 {
                    self.regs.d[idx as usize] = val;
                } else {
                    self.regs.set_a((idx - 8) as usize, val);
                }

                self.addr = self.addr.wrapping_add(self.size.bytes());
                self.followup_tag = TAG_MOVEM_NEXT;
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- ADDX/SUBX memory mode handlers ---
            TAG_ADDX_READ_SRC => {
                // Source read done, result in self.data.
                // self.src_val holds the Ax (destination register) index from
                // decode — read it BEFORE overwriting with the source data.
                let rx = self.src_val as u8;
                self.dst_val = self.data; // save source data for TAG_ADDX_READ_DST

                // Source phase complete — clear ae_undo_reg (source predec committed).
                self.ae_undo_reg = None;

                // Pre-decrement destination (Ax)
                let dec = if rx == 7 && self.size == Size::Byte { 2 } else { self.size.bytes() };
                self.addr = self.regs.a(rx as usize).wrapping_sub(dec);
                self.regs.set_a(rx as usize, self.addr);
                self.ae_undo_reg = Some((rx, dec, false, true));

                self.followup_tag = TAG_ADDX_READ_DST;
                self.queue_read_ops(self.size);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_ADDX_READ_DST => {
                // Destination read done
                let src = self.dst_val; // source data saved earlier
                let dst = self.data;
                let is_add = self.alu_op == AluOp::Add;
                let (result, new_sr) = if is_add {
                    crate::alu::addx(src, dst, self.size, self.regs.sr)
                } else {
                    crate::alu::subx(src, dst, self.size, self.regs.sr)
                };
                self.regs.sr = new_sr;
                self.data = result;

                self.followup_tag = TAG_ADDX_WRITE;
                self.queue_write_ops(self.size);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_ADDX_WRITE => {
                // Write done
                self.in_followup = false;
            }

            // --- CHK bounds check ---
            // Source operand has been read. Compare Dn.w against [0, src.w].
            TAG_CHK_EXECUTE => {
                // For memory modes, the bus read result is in self.data.
                if let Some(mode) = self.src_mode {
                    if !matches!(mode, AddrMode::DataReg(_) | AddrMode::Immediate) {
                        self.src_val = self.data & 0xFFFF;
                    }
                }

                let dn_idx = ((self.ir >> 9) & 7) as usize;
                let dn_val = (self.regs.d[dn_idx] & 0xFFFF) as u16;
                let bound = (self.src_val & 0xFFFF) as u16;
                let dn_signed = dn_val as i16;
                let bound_signed = bound as i16;

                // Frame PC points past the entire CHK instruction (opcode + extension words).
                // At this point, irc_addr equals the address past the last consumed word.
                let frame_pc = self.irc_addr;

                // The real 68000 computes Dn.w - src.w internally. When Dn < 0
                // triggers the trap, 2 extra internal cycles fire if the
                // subtraction shows Dn <= src (signed, no overflow).
                let sub_result = dn_val.wrapping_sub(bound);
                let sub_n = sub_result & 0x8000 != 0;
                let sub_z = sub_result == 0;
                let sub_v =
                    ((dn_val ^ bound) & (dn_val ^ sub_result)) & 0x8000 != 0;

                if dn_signed < 0 {
                    // Dn < 0: set N, clear ZVC, preserve X, trap vector 6
                    let extra = if (sub_n || sub_z) && !sub_v { 2 } else { 0 };
                    self.regs.sr = (self.regs.sr & 0xFFF0) | 0x0008;
                    self.begin_group1_exception(6, frame_pc);
                    self.micro_ops.push_front(MicroOp::Internal(4 + extra));
                } else if dn_signed > bound_signed {
                    // Dn > upper bound: clear NZVC, preserve X, trap vector 6
                    self.regs.sr &= 0xFFF0;
                    self.begin_group1_exception(6, frame_pc);
                    self.micro_ops.push_front(MicroOp::Internal(4));
                } else {
                    // In bounds: clear NZVC, no trap
                    self.regs.sr &= 0xFFF0;
                    self.micro_ops.push(MicroOp::Internal(6));
                    self.in_followup = false;
                }
            }

            // --- Address error exception frame ---
            // Frame pushed in order: PC, SR, IR, fault addr, access info
            // Then read vector 3 (0x0C) and jump.

            TAG_AE_PUSH_SR => {
                // PC already pushed by begin_address_error.
                // Now push the saved SR.
                self.data = u32::from(self.ae_saved_sr);
                self.followup_tag = TAG_AE_PUSH_IR;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_PUSH_IR => {
                self.data = u32::from(self.ae_frame_ir);
                self.followup_tag = TAG_AE_PUSH_FAULT;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_PUSH_FAULT => {
                self.data = self.ae_fault_addr;
                self.followup_tag = TAG_AE_PUSH_INFO;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_PUSH_INFO => {
                self.data = u32::from(self.ae_access_info);
                self.followup_tag = TAG_AE_FETCH_VECTOR;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_FETCH_VECTOR => {
                // Vector 3 = address error, at memory address 0x0C
                self.addr = 3 * 4;
                self.followup_tag = TAG_AE_FINISH;
                self.queue_read_ops(Size::Long);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_FINISH => {
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                self.ae_in_progress = false;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- BCD -(An),-(An): source read complete ---
            TAG_BCD_SRC_READ => {
                // Source byte is in self.data from the ReadByte
                self.src_val = self.data & 0xFF;

                // Predec destination register
                let rx = self.ea_reg;
                let dec_dst = if rx == 7 { 2u32 } else { 1 };
                self.addr = self.regs.a(rx as usize).wrapping_sub(dec_dst);
                self.regs.set_a(rx as usize, self.addr);

                // Read destination byte
                self.micro_ops.push(MicroOp::ReadByte);
                self.followup_tag = TAG_BCD_DST_READ;
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- BCD -(An),-(An): dest read complete, compute and write ---
            TAG_BCD_DST_READ => {
                let is_add = self.movem_is_write;
                let src = self.src_val as u8;
                let dst = (self.data & 0xFF) as u8;
                let x = self.x_flag();

                let (result, carry, overflow) = if is_add {
                    self.bcd_add(src, dst, x)
                } else {
                    self.bcd_sub(dst, src, x)
                };

                self.set_bcd_flags(result, carry, overflow);
                self.data = u32::from(result);
                // Write result back to destination (addr already set to Ax)
                self.micro_ops.push(MicroOp::WriteByte);
                self.in_followup = false;
            }

            // --- MOVEP multi-byte transfer loop ---
            TAG_MOVEP_TRANSFER => {
                let dn = self.ea_reg as usize;
                let byte_idx = self.movem_idx;
                let total = self.movem_an_reg;
                let is_write = self.movem_is_write;

                if !is_write {
                    // Store the byte just read into the correct position in Dn
                    let shift = (u32::from(total) - 1 - u32::from(byte_idx)) * 8;
                    let mask = 0xFFu32 << shift;
                    self.regs.d[dn] = (self.regs.d[dn] & !mask)
                        | ((self.data & 0xFF) << shift);
                }

                let next_idx = byte_idx + 1;
                if next_idx >= total {
                    // All bytes transferred
                    self.in_followup = false;
                    return;
                }

                // Advance to next byte (skip one byte = +2 addresses)
                self.addr = self.addr.wrapping_add(2);
                self.movem_idx = next_idx;

                if is_write {
                    let shift = (u32::from(total) - 1 - u32::from(next_idx)) * 8;
                    self.data = (self.regs.d[dn] >> shift) & 0xFF;
                    self.micro_ops.push(MicroOp::WriteByte);
                } else {
                    self.micro_ops.push(MicroOp::ReadByte);
                }
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- MULU/MULS/DIVU/DIVS execution ---
            TAG_MULDIV_EXECUTE => {
                // For memory modes, bus read result is in self.data
                if let Some(mode) = self.src_mode {
                    if !matches!(mode, AddrMode::DataReg(_) | AddrMode::Immediate) {
                        self.src_val = self.data & 0xFFFF;
                    }
                }

                let opcode = self.ir;
                let dn = ((opcode >> 9) & 7) as usize;
                let src_word = (self.src_val & 0xFFFF) as u16;
                let top = opcode & 0xF000;
                let opmode = (opcode >> 6) & 7;

                match (top, opmode) {
                    (0xC000, 3) => {
                        // MULU: unsigned word multiply
                        let dst = (self.regs.d[dn] & 0xFFFF) as u16;
                        let result = u32::from(dst) * u32::from(src_word);
                        self.regs.d[dn] = result;

                        // Flags: N bit 31, Z if zero, V=0, C=0, X unchanged
                        let mut sr = self.regs.sr & !0x000F;
                        if result & 0x8000_0000 != 0 { sr |= 0x0008; }
                        if result == 0 { sr |= 0x0004; }
                        self.regs.sr = sr;

                        // Timing: 38 + 2 * (set bits in source word)
                        let total = 38 + 2 * src_word.count_ones();
                        let internal = total.saturating_sub(4) as u8;
                        self.micro_ops.push(MicroOp::Internal(internal));
                    }
                    (0xC000, 7) => {
                        // MULS: signed word multiply
                        let dst = self.regs.d[dn] as i16;
                        let src = src_word as i16;
                        let result = (i32::from(dst) * i32::from(src)) as u32;
                        self.regs.d[dn] = result;

                        let mut sr = self.regs.sr & !0x000F;
                        if result & 0x8000_0000 != 0 { sr |= 0x0008; }
                        if result == 0 { sr |= 0x0004; }
                        self.regs.sr = sr;

                        // Timing: Booth encoding transitions in source word
                        let v = u32::from(src_word);
                        let transitions = ((v ^ (v << 1)) & 0xFFFF).count_ones();
                        let total = 38 + 2 * transitions;
                        let internal = total.saturating_sub(4) as u8;
                        self.micro_ops.push(MicroOp::Internal(internal));
                    }
                    (0x8000, 3) => {
                        // DIVU: unsigned word divide
                        if src_word == 0 {
                            self.begin_group1_exception(5, self.irc_addr);
                            return;
                        }
                        let dividend = self.regs.d[dn];
                        let total_cycles = Self::divu_cycles(dividend, src_word);
                        let quotient = dividend / u32::from(src_word);
                        let remainder = dividend % u32::from(src_word);

                        if quotient > 0xFFFF {
                            // Overflow: V=1, N=1, C=0, Z=0, X unchanged
                            let mut sr = self.regs.sr & !0x000F;
                            sr |= 0x000A; // V + N
                            self.regs.sr = sr;
                        } else {
                            self.regs.d[dn] = (remainder << 16) | (quotient & 0xFFFF);
                            let mut sr = self.regs.sr & !0x000F;
                            if quotient & 0x8000 != 0 { sr |= 0x0008; }
                            if quotient & 0xFFFF == 0 { sr |= 0x0004; }
                            self.regs.sr = sr;
                        }
                        let internal = total_cycles.saturating_sub(4);
                        if internal > 0 {
                            self.micro_ops.push(MicroOp::Internal(internal));
                        }
                    }
                    (0x8000, 7) => {
                        // DIVS: signed word divide
                        if src_word == 0 {
                            self.begin_group1_exception(5, self.irc_addr);
                            return;
                        }
                        let dividend = self.regs.d[dn] as i32;
                        let divisor = src_word as i16;
                        let total_cycles = Self::divs_cycles(dividend, divisor);
                        let quotient = dividend / i32::from(divisor);
                        let remainder = dividend % i32::from(divisor);

                        if quotient > 32767 || quotient < -32768 {
                            // Overflow: V=1, N=1, C=0, Z=0, X unchanged
                            let mut sr = self.regs.sr & !0x000F;
                            sr |= 0x000A; // V + N
                            self.regs.sr = sr;
                        } else {
                            let q16 = quotient as u16;
                            let r16 = remainder as u16;
                            self.regs.d[dn] = (u32::from(r16) << 16) | u32::from(q16);
                            let mut sr = self.regs.sr & !0x000F;
                            if q16 & 0x8000 != 0 { sr |= 0x0008; }
                            if q16 == 0 { sr |= 0x0004; }
                            self.regs.sr = sr;
                        }
                        let internal = total_cycles.saturating_sub(4);
                        if internal > 0 {
                            self.micro_ops.push(MicroOp::Internal(internal));
                        }
                    }
                    _ => {}
                }
                self.in_followup = false;
            }

            // --- STOP: pipeline refill complete, enter stopped state ---
            TAG_STOP_WAIT => {
                self.state = State::Stopped;
                self.in_followup = false;
            }

            _ => {
                self.in_followup = false;
            }
        }
    }
}
