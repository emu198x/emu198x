//! Control flow and stack manipulation.
//!
//! - Relative jumps (`JR`, `JR cc`)
//! - Absolute jumps (`JP a16`, `JP cc, a16`, `JP (HL)`)
//! - Call / return (`CALL`, `CALL cc`, `RET`, `RET cc`, `RETI`)
//! - `RST n` — fixed-vector calls
//! - Stack `PUSH rr` / `POP rr`
//!
//! `RST`, `CALL`, and `RET` all manipulate the stack the same way as
//! `PUSH`/`POP`, so grouping them keeps the SP-sensitive code in one
//! file.

use crate::Sm83;

impl Sm83 {
    /// `JR r8` — unconditional relative jump. 3 m-cycles: fetch, read
    /// signed offset, internal compute.
    pub(super) fn op_jr(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                let offset = i16::from(self.z as i8);
                self.pc = (self.pc as i16).wrapping_add(offset) as u16;
                self.schedule_internal();
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JR cc, r8` — conditional relative jump. 2 m-cycles when the
    /// condition fails (offset read but not applied), 3 m-cycles when
    /// taken (offset applied via an internal compute cycle).
    pub(super) fn op_jr_cc(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                if self.condition_met() {
                    let offset = i16::from(self.z as i8);
                    self.pc = (self.pc as i16).wrapping_add(offset) as u16;
                    self.schedule_internal();
                    self.m_cycle = 3;
                } else {
                    self.finish_instruction();
                }
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JP a16` — 4 m-cycles (fetch, read lo, read hi, internal
    /// branch-cost cycle). The internal cycle is the documented extra
    /// tick that distinguishes JP from a simple register-pair load.
    pub(super) fn op_jp_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JP cc, a16` — 3 m-cycles when the condition fails (imm read
    /// but not applied); 4 m-cycles when taken (extra internal cycle
    /// matches unconditional JP).
    pub(super) fn op_jp_cc_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                if self.condition_met() {
                    self.pc = self.wz();
                    self.schedule_internal();
                    self.m_cycle = 4;
                } else {
                    self.finish_instruction();
                }
            }
            _ => self.finish_instruction(),
        }
    }

    /// `JP (HL)` — 1 m-cycle. Despite the mnemonic this is *not* an
    /// indirect jump; HL is loaded directly into PC without any bus
    /// dereference.
    pub(super) fn op_jp_hl(&mut self) {
        self.pc = self.hl();
        self.finish_instruction();
    }

    /// `CALL a16` ($CD) — 6 m-cycles. Fetches target, then uses an
    /// internal cycle to pre-decrement SP before pushing PC high /
    /// low, finally setting PC to the fetched target.
    pub(super) fn op_call_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.sp = self.sp.wrapping_sub(1);
                self.schedule_internal();
                self.m_cycle = 4;
            }
            4 => {
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 5;
            }
            5 => {
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.pc = self.wz();
                self.m_cycle = 6;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `CALL cc, a16` — 3 m-cycles when the condition fails, 6 when
    /// taken (matches the unconditional CALL path).
    pub(super) fn op_call_cc_a16(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                if self.condition_met() {
                    self.sp = self.sp.wrapping_sub(1);
                    self.schedule_internal();
                    self.m_cycle = 4;
                } else {
                    self.finish_instruction();
                }
            }
            4 => {
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 5;
            }
            5 => {
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.pc = self.wz();
                self.m_cycle = 6;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RET` — 4 m-cycles. Pops PC from the stack, then takes one
    /// internal cycle before the next opcode fetch.
    pub(super) fn op_ret(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RET cc` — 2 m-cycles when the condition fails (only the
    /// internal condition-eval cycle runs), 5 when taken (one extra
    /// condition-eval cycle on top of `RET`'s 4).
    pub(super) fn op_ret_cc(&mut self) {
        match self.m_cycle {
            1 => {
                // Internal cycle evaluates the condition. The pop
                // itself only starts if taken.
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                if self.condition_met() {
                    self.schedule_read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    self.m_cycle = 3;
                } else {
                    self.finish_instruction();
                }
            }
            3 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 4;
            }
            4 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.schedule_internal();
                self.m_cycle = 5;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RETI` — identical to `RET` with `IME` re-enabled *immediately*
    /// (no one-instruction delay, unlike `EI`).
    pub(super) fn op_reti(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 3;
            }
            3 => {
                self.w = self.data_in;
                self.pc = self.wz();
                self.ime = true;
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `RST n` — 4 m-cycles. Pushes PC, then jumps to a fixed `$00`,
    /// `$08`, …, `$38` vector derived from opcode bits 5-3.
    pub(super) fn op_rst(&mut self) {
        match self.m_cycle {
            1 => {
                self.sp = self.sp.wrapping_sub(1);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                let pc_hi = (self.pc >> 8) as u8;
                self.schedule_write(self.sp, pc_hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 3;
            }
            3 => {
                let pc_lo = (self.pc & 0xFF) as u8;
                self.schedule_write(self.sp, pc_lo);
                self.pc = u16::from(self.opcode & 0x38);
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `PUSH rr` — 4 m-cycles. The first is an internal SP-decrement
    /// delay before the two pushes land on the bus.
    pub(super) fn op_push_rr(&mut self) {
        match self.m_cycle {
            1 => {
                self.sp = self.sp.wrapping_sub(1);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            2 => {
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.read_reg16_af(pair);
                let hi = (value >> 8) as u8;
                self.w = (value & 0xFF) as u8; // stash the low byte for cycle 3
                self.schedule_write(self.sp, hi);
                self.sp = self.sp.wrapping_sub(1);
                self.m_cycle = 3;
            }
            3 => {
                let lo = self.w;
                self.schedule_write(self.sp, lo);
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `POP rr` — 3 m-cycles. The low nibble of F is masked off for
    /// `POP AF`, mirroring the hardwired zero on real silicon.
    pub(super) fn op_pop_rr(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.m_cycle = 3;
            }
            _ => {
                self.w = self.data_in;
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.wz();
                self.write_reg16_af(pair, value);
                self.finish_instruction();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::boot;
    use crate::FLAG_Z;

    // -- JR --------------------------------------------------------------

    #[test]
    fn jr_nz_takes_branch_when_z_clear() {
        let (mut cpu, mut bus) = boot(&[0x20, 0xFD]);
        cpu.f = 0;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 0xFFFF);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn jr_nz_skips_branch_when_z_set() {
        let (mut cpu, mut bus) = boot(&[0x20, 0xFD]);
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.pc, 2);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn jr_unconditional_applies_signed_offset() {
        let (mut cpu, mut bus) = boot(&[0x18, 0x05]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 7);
    }

    // -- JP --------------------------------------------------------------

    #[test]
    fn jp_a16_jumps_to_absolute_address() {
        let (mut cpu, mut bus) = boot(&[0xC3, 0x00, 0x01]);
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0100);
    }

    #[test]
    fn jp_z_taken_matches_unconditional_jp() {
        let (mut cpu, mut bus) = boot(&[0xCA, 0x00, 0x01]);
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0100);
    }

    #[test]
    fn jp_z_not_taken_is_three_ticks() {
        let (mut cpu, mut bus) = boot(&[0xCA, 0x00, 0x01]);
        cpu.f = 0;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 3);
    }

    #[test]
    fn jp_hl_loads_pc_from_hl_in_one_tick() {
        let (mut cpu, mut bus) = boot(&[0xE9]);
        cpu.set_hl(0x1234);
        bus.step(&mut cpu);
        assert_eq!(cpu.pc, 0x1234);
    }

    // -- CALL / RET / RETI / RST / PUSH / POP ----------------------------

    #[test]
    fn call_a16_pushes_return_address_and_jumps() {
        let (mut cpu, mut bus) = boot(&[0xCD, 0x95, 0x00]);
        cpu.sp = 0xFFFE;
        bus.run(&mut cpu, 6);
        assert_eq!(cpu.pc, 0x0095);
        assert_eq!(cpu.sp, 0xFFFC);
        assert_eq!(bus.ram[0xFFFD], 0x00); // return addr hi
        assert_eq!(bus.ram[0xFFFC], 0x03); // return addr lo
    }

    #[test]
    fn call_nz_taken_matches_unconditional_call() {
        let (mut cpu, mut bus) = boot(&[0xC4, 0x95, 0x00]);
        cpu.sp = 0xFFFE;
        cpu.f = 0;
        bus.run(&mut cpu, 6);
        assert_eq!(cpu.pc, 0x0095);
        assert_eq!(cpu.sp, 0xFFFC);
    }

    #[test]
    fn call_nz_skipped_keeps_pc_and_sp() {
        let (mut cpu, mut bus) = boot(&[0xC4, 0x95, 0x00]);
        cpu.sp = 0xFFFE;
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.pc, 3);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn ret_pops_return_address() {
        let (mut cpu, mut bus) = boot(&[0xC9]);
        bus.ram[0xFFFC] = 0x03;
        bus.ram[0xFFFD] = 0x00;
        cpu.sp = 0xFFFC;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0003);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn ret_cc_taken_takes_five_ticks() {
        let (mut cpu, mut bus) = boot(&[0xC0]); // RET NZ
        bus.ram[0xFFFC] = 0x03;
        bus.ram[0xFFFD] = 0x00;
        cpu.sp = 0xFFFC;
        cpu.f = 0;
        bus.run(&mut cpu, 5);
        assert_eq!(cpu.pc, 0x0003);
    }

    #[test]
    fn ret_cc_not_taken_takes_two_ticks() {
        let (mut cpu, mut bus) = boot(&[0xC0]); // RET NZ
        cpu.sp = 0xFFFC;
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.pc, 1);
        assert_eq!(cpu.sp, 0xFFFC);
    }

    #[test]
    fn reti_pops_and_sets_ime() {
        let (mut cpu, mut bus) = boot(&[0xD9]);
        bus.ram[0xFFFC] = 0x03;
        bus.ram[0xFFFD] = 0x00;
        cpu.sp = 0xFFFC;
        cpu.ime = false;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0003);
        assert!(cpu.ime);
    }

    #[test]
    fn rst_08_pushes_pc_and_jumps_to_vector() {
        let (mut cpu, mut bus) = boot(&[0xCF]); // RST $08
        cpu.sp = 0xFFFE;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.pc, 0x0008);
        assert_eq!(cpu.sp, 0xFFFC);
        assert_eq!(bus.ram[0xFFFC], 0x01); // PC was 1 when push happened
    }

    #[test]
    fn push_bc_and_pop_bc_round_trip() {
        let (mut cpu, mut bus) = boot(&[0xC5, 0xC1]);
        cpu.sp = 0xFFFE;
        cpu.b = 0x12;
        cpu.c = 0x34;
        bus.run(&mut cpu, 4); // PUSH
        assert_eq!(cpu.sp, 0xFFFC);

        cpu.b = 0;
        cpu.c = 0;
        bus.run(&mut cpu, 3); // POP
        assert_eq!(cpu.b, 0x12);
        assert_eq!(cpu.c, 0x34);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn pop_af_masks_low_nibble_of_f() {
        let (mut cpu, mut bus) = boot(&[0xF1]);
        bus.ram[0xFFFC] = 0xFF; // F
        bus.ram[0xFFFD] = 0x42; // A
        cpu.sp = 0xFFFC;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.f, 0xF0);
    }
}
