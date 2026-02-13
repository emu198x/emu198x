//! Shift and rotate instructions (group 0xE).
//!
//! Register variant: 1110 CCC D SS I TT RRR (SS != 11)
//!   CCC = count/register, D = direction (0=right, 1=left)
//!   SS = size (00=byte, 01=word, 10=long)
//!   I = type (0=count in register, 1=immediate count)
//!   TT = kind (00=AS, 01=LS, 10=ROX, 11=RO)
//!   RRR = data register
//!
//! Memory variant: 1110 0TT D 11 MMMRRR (SS=11)
//!   TT = kind, D = direction, shift by 1, word size, RMW
//!
//! Register timing: 6+2n (byte/word), 8+2n (long) where n = shift count
//! Memory timing: 8+EA (read-modify-write)
//!
//! Followup tags:
//!   110 = memory shift RMW: read complete, compute + writeback
//!   111 = memory shift AbsLong ext2

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::cpu::Cpu68000;
use crate::flags::{Status, C, V, X};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Decode and dispatch group 0xE (shifts/rotates).
    pub(crate) fn exec_shift_rotate(&mut self) {
        let op = self.ir;

        // Handle followups
        if self.in_followup {
            match self.followup_tag {
                110 => { self.shift_mem_rmw(); return; }
                111 => { self.shift_mem_abslong_ext2(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        let size_bits = ((op >> 6) & 3) as u8;

        if size_bits == 3 {
            // Memory shift: 1110 0TT D 11 MMMRRR
            self.exec_shift_memory();
        } else {
            // Register shift: 1110 CCC D SS I TT RRR
            self.exec_shift_register();
        }
    }

    /// Execute register shift/rotate.
    fn exec_shift_register(&mut self) {
        let op = self.ir;
        let count_or_reg = ((op >> 9) & 7) as u8;
        let direction = op & 0x0100 != 0; // true = left
        let size_bits = ((op >> 6) & 3) as u8;
        let immediate = op & 0x0020 == 0; // 0 = immediate count
        let kind = ((op >> 3) & 3) as u8; // 00=AS, 01=LS, 10=ROX, 11=RO
        let reg = (op & 7) as u8;

        let size = match Size::from_bits(size_bits) {
            Some(s) => s,
            None => { self.illegal_instruction(); return; }
        };

        let count = if immediate {
            if count_or_reg == 0 { 8u32 } else { count_or_reg as u32 }
        } else {
            self.regs.d[count_or_reg as usize] % 64
        };

        let value = self.read_data_reg(reg, size);
        let (result, carry) = self.shift_alu(kind, direction, value, count, size);

        self.write_data_reg(reg, result, size);
        self.set_shift_flags(kind, direction, value, result, carry, count, size);

        // Timing: 6+2n (byte/word), 8+2n (long) total.
        // Subtract 4 for start_next FetchIRC.
        let base_internal = if size == Size::Long { 4u32 } else { 2u32 };
        let internal = base_internal + 2 * count;
        if internal > 0 {
            self.micro_ops.push(MicroOp::Internal(internal as u8));
        }
    }

    /// Execute memory shift/rotate (word, shift by 1).
    fn exec_shift_memory(&mut self) {
        let op = self.ir;
        let kind = ((op >> 9) & 3) as u8;
        let direction = op & 0x0100 != 0;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = Size::Word;
        // Stash shift parameters: kind in bits 0-1, direction in bit 2
        self.addr2 = u32::from(kind) | if direction { 4 } else { 0 };

        match ea {
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 110;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                self.regs.set_a(r as usize, a.wrapping_add(2));
                self.addr = a;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 110;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let a = self.regs.a(r as usize).wrapping_sub(2);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 110;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 110;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 110;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 110;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 111;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 111: Memory shift AbsLong second address word.
    fn shift_mem_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(Size::Word);
        self.followup_tag = 110;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 110: Memory shift read complete, compute + writeback.
    fn shift_mem_rmw(&mut self) {
        let kind = (self.addr2 & 3) as u8;
        let direction = self.addr2 & 4 != 0;
        let value = self.data & 0xFFFF;

        let (result, carry) = self.shift_alu(kind, direction, value, 1, Size::Word);
        self.set_shift_flags(kind, direction, value, result, carry, 1, Size::Word);

        self.data = result;
        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(Size::Word);
    }

    /// Core shift/rotate ALU. Returns (result, carry_out).
    fn shift_alu(
        &self,
        kind: u8,
        direction: bool,
        value: u32,
        count: u32,
        size: Size,
    ) -> (u32, bool) {
        let (mask, msb_bit) = match size {
            Size::Byte => (0xFFu32, 0x80u32),
            Size::Word => (0xFFFF, 0x8000),
            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
        };
        let bits = match size {
            Size::Byte => 8u32,
            Size::Word => 16,
            Size::Long => 32,
        };

        match (kind, direction) {
            // ASL
            (0, true) => {
                if count == 0 {
                    (value, false)
                } else if count >= bits {
                    let c = if count == bits { value & 1 != 0 } else { false };
                    (0, c)
                } else {
                    let shifted = (value << count) & mask;
                    let c = (value >> (bits - count)) & 1 != 0;
                    (shifted, c)
                }
            }
            // ASR
            (0, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let sign_bit = value & msb_bit != 0;
                    if count >= bits {
                        let result = if sign_bit { mask } else { 0 };
                        (result, sign_bit)
                    } else {
                        let mut result = value;
                        for _ in 0..count {
                            result = (result >> 1) | if sign_bit { msb_bit } else { 0 };
                        }
                        let c = (value >> (count - 1)) & 1 != 0;
                        (result & mask, c)
                    }
                }
            }
            // LSL
            (1, true) => {
                if count == 0 {
                    (value, false)
                } else if count >= bits {
                    let c = if count == bits { value & 1 != 0 } else { false };
                    (0, c)
                } else {
                    let shifted = (value << count) & mask;
                    let c = (value >> (bits - count)) & 1 != 0;
                    (shifted, c)
                }
            }
            // LSR
            (1, false) => {
                if count == 0 {
                    (value, false)
                } else if count >= bits {
                    let c = if count == bits {
                        (value >> (bits - 1)) & 1 != 0
                    } else {
                        false
                    };
                    (0, c)
                } else {
                    let shifted = (value >> count) & mask;
                    let c = (value >> (count - 1)) & 1 != 0;
                    (shifted, c)
                }
            }
            // ROXL
            (2, true) => {
                if count == 0 {
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let total_bits = bits + 1;
                    let eff = count % total_bits;
                    if eff == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        let rotated = ((extended << eff) | (extended >> (total_bits - eff)))
                            & ((1u64 << total_bits) - 1);
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROXR
            (2, false) => {
                if count == 0 {
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let total_bits = bits + 1;
                    let eff = count % total_bits;
                    if eff == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        let rotated = ((extended >> eff) | (extended << (total_bits - eff)))
                            & ((1u64 << total_bits) - 1);
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROL
            (3, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let eff = count % bits;
                    if eff == 0 {
                        (value, value & 1 != 0)
                    } else {
                        let rotated =
                            ((value << eff) | (value >> (bits - eff))) & mask;
                        let c = rotated & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            // ROR
            (3, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let eff = count % bits;
                    if eff == 0 {
                        (value, value & msb_bit != 0)
                    } else {
                        let rotated =
                            ((value >> eff) | (value << (bits - eff))) & mask;
                        let c = (value >> (eff - 1)) & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            _ => (value, false),
        }
    }

    /// Set flags after a shift/rotate operation.
    fn set_shift_flags(
        &mut self,
        kind: u8,
        direction: bool,
        original: u32,
        result: u32,
        carry: bool,
        count: u32,
        size: Size,
    ) {
        // N and Z based on result
        self.set_flags_move(result, size);

        // C flag
        if count > 0 {
            self.regs.sr = Status::set_if(self.regs.sr, C, carry);
            // X flag: set for AS, LS, ROX; NOT set for RO
            if kind < 3 {
                self.regs.sr = Status::set_if(self.regs.sr, X, carry);
            }
        } else if kind == 2 {
            // ROX with count=0: C = X
            let x = self.regs.sr & X != 0;
            self.regs.sr = Status::set_if(self.regs.sr, C, x);
        } else {
            // Count=0: C cleared
            self.regs.sr &= !C;
        }

        // V flag: only ASL sets V (if MSB changed during any shift step)
        if kind == 0 && direction {
            if count == 0 {
                self.regs.sr &= !V;
            } else {
                let bits = match size {
                    Size::Byte => 8u32,
                    Size::Word => 16,
                    Size::Long => 32,
                };
                let mask = size.mask();
                let v = if count >= bits {
                    (original & mask) != 0
                } else {
                    let check_bits = count + 1;
                    let check_mask = if check_bits >= bits {
                        mask
                    } else {
                        ((1u32 << check_bits) - 1) << (bits - check_bits)
                    };
                    let top_bits = original & check_mask;
                    top_bits != 0 && top_bits != check_mask
                };
                self.regs.sr = Status::set_if(self.regs.sr, V, v);
            }
        } else {
            self.regs.sr &= !V;
        }
    }
}
