//! Branch and jump instructions.
//!
//! Group 0x6: Bcc, BRA, BSR
//! Group 0x5 (SS=11): Scc, DBcc

use crate::alu::Size;
use crate::cpu::Cpu68000;
use crate::flags::Status;
use crate::microcode::MicroOp;

impl Cpu68000 {
    // ================================================================
    // Bcc / BRA / BSR  (group 0x6)
    // ================================================================
    //
    // Encoding: 0110 CCCC DDDDDDDD
    //   CCCC = condition (0000=BRA, 0001=BSR, others=Bcc)
    //   DDDDDDDD = 8-bit displacement (0 = 16-bit displacement in ext word)
    //
    // Timing:
    //   Bcc taken (8-bit disp):   10 cycles
    //   Bcc taken (16-bit disp):  10 cycles
    //   Bcc not taken (8-bit):     8 cycles
    //   Bcc not taken (16-bit):   12 cycles
    //   BRA (8-bit disp):         10 cycles
    //   BRA (16-bit disp):        10 cycles
    //   BSR (8-bit disp):         18 cycles
    //   BSR (16-bit disp):        18 cycles

    pub(crate) fn exec_branch(&mut self) {
        let op = self.ir;
        let cond = ((op >> 8) & 0xF) as u8;
        let disp8 = (op & 0xFF) as i8;

        // Handle followup for BSR stack write
        if self.in_followup {
            match self.followup_tag {
                80 => { self.bsr_push_pc(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        match cond {
            0 => {
                // BRA: always branch
                // For 16-bit displacement, use consume_irc_deferred to avoid a
                // wasted FetchIRC at the old PC (branch invalidates the pipeline).
                let disp = if disp8 == 0 {
                    let d16 = self.consume_irc_deferred() as i16;
                    i32::from(d16)
                } else {
                    i32::from(disp8)
                };
                let target = (self.instr_start_pc.wrapping_add(2) as i32)
                    .wrapping_add(disp) as u32;
                self.regs.pc = target;
                self.irc_addr = target;
                self.micro_ops.push(MicroOp::Internal(2));
                self.refill_prefetch_branch();
            }
            1 => {
                // BSR: branch to subroutine
                let disp = if disp8 == 0 {
                    let d16 = self.consume_irc_deferred() as i16;
                    i32::from(d16)
                } else {
                    i32::from(disp8)
                };
                let target = (self.instr_start_pc.wrapping_add(2) as i32)
                    .wrapping_add(disp) as u32;

                // Push return address (PC after this instruction)
                // For 8-bit: return = instr_start_pc + 2
                // For 16-bit: return = instr_start_pc + 4
                let return_pc = if disp8 == 0 {
                    self.instr_start_pc.wrapping_add(4)
                } else {
                    self.instr_start_pc.wrapping_add(2)
                };

                self.data = return_pc;
                self.data2 = target as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                // Push return PC (high word first, then low word)
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                self.in_followup = true;
                self.followup_tag = 80;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => {
                // Bcc: conditional branch
                let cc = Status::condition(self.regs.sr, cond);
                if cc {
                    // Branch taken — use consume_irc_deferred for 16-bit
                    let disp = if disp8 == 0 {
                        let d16 = self.consume_irc_deferred() as i16;
                        i32::from(d16)
                    } else {
                        i32::from(disp8)
                    };
                    let target = (self.instr_start_pc.wrapping_add(2) as i32)
                        .wrapping_add(disp) as u32;
                    self.regs.pc = target;
                    self.irc_addr = target;
                    self.micro_ops.push(MicroOp::Internal(2));
                    self.refill_prefetch_branch();
                } else {
                    // Branch not taken — sequential execution, need consume_irc
                    // (FetchIRC refill IS needed since we continue at old PC)
                    if disp8 == 0 {
                        // 16-bit displacement: consume and discard
                        let _disp = self.consume_irc();
                        // 12 cycles: FetchIRC(4) + Internal(4) + FetchIRC(4) for next
                        self.micro_ops.push(MicroOp::Internal(4));
                    } else {
                        // 8-bit displacement: 8 cycles
                        // Internal(4) then FetchIRC(4) for next
                        self.micro_ops.push(MicroOp::Internal(4));
                    }
                }
            }
        }
    }

    /// Tag 80: BSR push complete, now jump to target.
    fn bsr_push_pc(&mut self) {
        let target = self.data2;
        self.regs.pc = target;
        self.irc_addr = target;
        self.in_followup = false;
        self.followup_tag = 0;
        self.refill_prefetch_branch();
    }

    // refill_prefetch_branch() is in cpu.rs (shared by branches.rs, misc.rs)

    // ================================================================
    // Scc / DBcc  (group 0x5, SS=11)
    // ================================================================
    //
    // Scc: 0101 CCCC 11 MMMRRR
    //   If cc true: set byte at EA to $FF
    //   If cc false: set byte at EA to $00
    //   Timing: Dn true=6, Dn false=4, memory=8+EA
    //
    // DBcc: 0101 CCCC 11 001 RRR (EA mode=001)
    //   If cc false: Dn-1; if Dn != -1, branch (disp in ext word)
    //   Timing: cc true=12, cc false branch=10, cc false expire=14
    //
    // Followup tags:
    //   81 = Scc AbsLong ext2
    //   82 = Scc RMW write (read done, now write Scc value)

    pub(crate) fn exec_scc_dbcc(&mut self) {
        let op = self.ir;
        let cond = ((op >> 8) & 0xF) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if self.in_followup {
            match self.followup_tag {
                81 => { self.scc_abslong_ext2(); return; }
                82 => { self.scc_rmw_write(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        if ea_mode == 1 {
            // DBcc
            self.exec_dbcc(cond, ea_reg);
        } else {
            // Scc
            self.exec_scc(cond, ea_mode, ea_reg);
        }
    }

    fn exec_scc(&mut self, cond: u8, ea_mode: u8, ea_reg: u8) {
        use crate::addressing::AddrMode;

        let cc = Status::condition(self.regs.sr, cond);
        let value: u32 = if cc { 0xFF } else { 0x00 };

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;

        match ea {
            AddrMode::DataReg(r) => {
                self.write_data_reg(r, value, Size::Byte);
                if cc {
                    // True: 6 cycles (Internal(2) extra)
                    self.micro_ops.push(MicroOp::Internal(2));
                }
                // False: 4 cycles (just next FetchIRC)
            }
            // Memory modes: read-modify-write. The 68000 reads from the EA
            // first (value discarded), then writes. This matches CLR/TAS behavior.
            // Save the Scc value in data2, queue ReadByte, then followup tag 82
            // restores data and queues WriteByte.
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.data2 = value;
                self.scc_queue_rmw();
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                let inc = if r == 7 { 2 } else { 1 };
                self.regs.set_a(r as usize, a.wrapping_add(inc));
                self.addr = a;
                self.data2 = value;
                self.scc_queue_rmw();
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if r == 7 { 2 } else { 1 };
                let a = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.data2 = value;
                self.micro_ops.push(MicroOp::Internal(2));
                self.scc_queue_rmw();
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.data2 = value;
                self.scc_queue_rmw();
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.data2 = value;
                self.micro_ops.push(MicroOp::Internal(2));
                self.scc_queue_rmw();
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.data2 = value;
                self.scc_queue_rmw();
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.addr2 = value; // stash Scc value in addr2 (data2 used for address)
                self.in_followup = true;
                self.followup_tag = 81;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Queue ReadByte + Execute(tag 82) for Scc RMW.
    /// Scc value must be in data2 and EA address in addr.
    fn scc_queue_rmw(&mut self) {
        self.micro_ops.push(MicroOp::ReadByte);
        self.in_followup = true;
        self.followup_tag = 82;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 81: Scc AbsLong second address word.
    fn scc_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.data2 = self.addr2; // restore Scc value from addr2 stash
        // Now queue RMW: ReadByte, then tag 82 writes
        self.followup_tag = 82;
        self.micro_ops.push(MicroOp::ReadByte);
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 82: Scc RMW write — read is done, now write the Scc value.
    fn scc_rmw_write(&mut self) {
        self.data = self.data2;
        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(Size::Byte);
    }

    fn exec_dbcc(&mut self, cond: u8, reg: u8) {
        let cc = Status::condition(self.regs.sr, cond);

        if cc {
            // Condition true: don't decrement, don't branch
            // Consume displacement word and discard
            let _disp = self.consume_irc();
            // 12 cycles: FetchIRC(4) + Internal(4) + next FetchIRC(4)
            self.micro_ops.push(MicroOp::Internal(4));
        } else {
            // Condition false: decrement Dn.w
            let val = (self.regs.d[reg as usize] & 0xFFFF) as u16;
            let new_val = val.wrapping_sub(1);
            // Save original for AE undo — if the branch target is odd,
            // the real 68000 undoes the Dn decrement.
            self.dbcc_dn_undo = Some((reg, val));
            // Write back only the low word
            self.regs.d[reg as usize] =
                (self.regs.d[reg as usize] & 0xFFFF0000) | u32::from(new_val);

            if new_val == 0xFFFF {
                // Counter expired (-1): don't branch
                // Consume displacement word and discard
                let _disp = self.consume_irc();
                // 14 cycles: FetchIRC(4) + Internal(6) + next FetchIRC(4)
                self.micro_ops.push(MicroOp::Internal(6));
            } else {
                // Branch: read displacement from IRC (deferred — branch
                // invalidates pipeline, FetchIRC would be wasted)
                let disp = self.consume_irc_deferred() as i16;
                let target = (self.instr_start_pc.wrapping_add(2) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.regs.pc = target;
                self.irc_addr = target;
                // 10 cycles: Internal(2) + FetchIRC(4) at target + FetchIRC(4) for next
                self.micro_ops.push(MicroOp::Internal(2));
                self.refill_prefetch_branch();
            }
        }
    }
}
