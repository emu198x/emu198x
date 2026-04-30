//! 8-bit and 16-bit arithmetic, accumulator rotates, and the
//! flag-massaging instructions (DAA, CPL, SCF, CCF).
//!
//! Includes:
//!
//! - 8-bit `INC r` / `DEC r` and their `(HL)` variants
//! - 16-bit `INC rr` / `DEC rr`
//! - The eight-way ALU group (`$80..$BF`) and its immediate-byte
//!   variants
//! - `ADD HL, rr`
//! - `ADD SP, r8`
//! - The single-m-cycle accumulator rotates (`RLCA`/`RRCA`/`RLA`/`RRA`)
//!   — distinct from the CB rotates in that they always clear `Z`.
//! - `DAA`, `CPL`, `SCF`, `CCF`

use crate::Sm83;
use crate::alu::{self, AluOp};
use crate::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

impl Sm83 {
    /// `INC rr` — 2 m-cycles, no flags. The second m-cycle is internal
    /// (no bus op) — that's the cycle that costs a tick despite the
    /// 16-bit increment fitting the same bus width as 8-bit ops.
    pub(super) fn op_inc_rr(&mut self) {
        match self.m_cycle {
            1 => {
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.read_reg16_sp(pair).wrapping_add(1);
                self.write_reg16_sp(pair, value);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `DEC rr` — 2 m-cycles, no flags.
    pub(super) fn op_dec_rr(&mut self) {
        match self.m_cycle {
            1 => {
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.read_reg16_sp(pair).wrapping_sub(1);
                self.write_reg16_sp(pair, value);
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `INC r` — 1 m-cycle. C preserved, Z/N/H per [`Sm83::alu_inc8`].
    pub(super) fn op_inc_r(&mut self) {
        let reg = (self.opcode >> 3) & 0b111;
        let value = self.read_reg8(reg);
        let result = self.alu_inc8(value);
        self.write_reg8(reg, result);
        self.finish_instruction();
    }

    /// `DEC r` — 1 m-cycle. C preserved, Z/N/H per [`Sm83::alu_dec8`].
    pub(super) fn op_dec_r(&mut self) {
        let reg = (self.opcode >> 3) & 0b111;
        let value = self.read_reg8(reg);
        let result = self.alu_dec8(value);
        self.write_reg8(reg, result);
        self.finish_instruction();
    }

    /// `INC (HL)` — 3 m-cycles (read, modify, write).
    pub(super) fn op_inc_hl_indirect(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            2 => {
                let value = self.data_in;
                let result = self.alu_inc8(value);
                self.schedule_write(self.hl(), result);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `DEC (HL)` — 3 m-cycles (read, modify, write).
    pub(super) fn op_dec_hl_indirect(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            2 => {
                let value = self.data_in;
                let result = self.alu_dec8(value);
                self.schedule_write(self.hl(), result);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `ALU A, r` / `ALU A, (HL)` ($80..$BF) — 1 m-cycle for register
    /// operands; 2 m-cycles when the operand is `(HL)`.
    pub(super) fn op_alu_r(&mut self) {
        let src = self.opcode & 0b111;
        let op = AluOp::from_opcode_bits(self.opcode);

        match (self.m_cycle, src) {
            (1, 6) => {
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            (_, 6) => {
                let operand = self.data_in;
                self.alu(op, operand);
                self.finish_instruction();
            }
            (_, src) => {
                let operand = self.read_reg8(src);
                self.alu(op, operand);
                self.finish_instruction();
            }
        }
    }

    /// `ALU A, d8` — 2 m-cycles (fetch, immediate-byte read +
    /// in-place ALU).
    pub(super) fn op_alu_d8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            _ => {
                let op = AluOp::from_opcode_bits(self.opcode);
                let operand = self.data_in;
                self.alu(op, operand);
                self.finish_instruction();
            }
        }
    }

    /// `ADD HL, rr` — 2 m-cycles. Z preserved, N cleared, H = carry
    /// out of bit 11, C = carry out of bit 15.
    pub(super) fn op_add_hl_rr(&mut self) {
        match self.m_cycle {
            1 => {
                let pair = (self.opcode >> 4) & 0b11;
                let hl = u32::from(self.hl());
                let rr = u32::from(self.read_reg16_sp(pair));
                let result = hl + rr;

                let mut flags = self.f & FLAG_Z; // preserve Z
                if (hl & 0xFFF) + (rr & 0xFFF) > 0xFFF {
                    flags |= FLAG_H;
                }
                if result > 0xFFFF {
                    flags |= FLAG_C;
                }
                self.set_hl(result as u16);
                self.f = flags;
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `ADD SP, r8` — 4 m-cycles. Flags follow the same low-byte
    /// unsigned-arithmetic rules as `LD HL, SP+r8`; Z and N cleared.
    pub(super) fn op_add_sp_r8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                let (_, flags) = alu::sp_add_offset(self.sp, self.z);
                self.f = flags;
                self.schedule_internal();
                self.m_cycle = 3;
            }
            3 => {
                let (result, _) = alu::sp_add_offset(self.sp, self.z);
                self.sp = result;
                self.schedule_internal();
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    // -- Single-m-cycle accumulator rotates (distinct from CB rotates
    //    in that they always clear Z). ------------------------------------

    /// `RLCA` — rotate `A` left, copy bit 7 into `C` and bit 0.
    /// Z/N/H all cleared.
    pub(super) fn op_rlca(&mut self) {
        let bit7 = self.a >> 7;
        self.a = (self.a << 1) | bit7;
        self.f = if bit7 != 0 { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    /// `RRCA` — rotate `A` right, copy bit 0 into `C` and bit 7.
    pub(super) fn op_rrca(&mut self) {
        let bit0 = self.a & 1;
        self.a = (self.a >> 1) | (bit0 << 7);
        self.f = if bit0 != 0 { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    /// `RLA` — rotate `A` left through `C`.
    pub(super) fn op_rla(&mut self) {
        let carry_in = if (self.f & FLAG_C) != 0 { 1 } else { 0 };
        let carry_out = (self.a & 0x80) != 0;
        self.a = (self.a << 1) | carry_in;
        self.f = if carry_out { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    /// `RRA` — rotate `A` right through `C`.
    pub(super) fn op_rra(&mut self) {
        let carry_in: u8 = if (self.f & FLAG_C) != 0 { 0x80 } else { 0 };
        let carry_out = (self.a & 1) != 0;
        self.a = (self.a >> 1) | carry_in;
        self.f = if carry_out { FLAG_C } else { 0 };
        self.finish_instruction();
    }

    /// `DAA` — see [`Sm83::daa`].
    pub(super) fn op_daa(&mut self) {
        self.daa();
        self.finish_instruction();
    }

    /// `CPL` — complement `A`, set N and H, preserve Z and C.
    pub(super) fn op_cpl(&mut self) {
        self.a = !self.a;
        self.f = (self.f & (FLAG_Z | FLAG_C)) | FLAG_N | FLAG_H;
        self.finish_instruction();
    }

    /// `SCF` — set carry, clear N and H, preserve Z.
    pub(super) fn op_scf(&mut self) {
        self.f = (self.f & FLAG_Z) | FLAG_C;
        self.finish_instruction();
    }

    /// `CCF` — complement carry, clear N and H, preserve Z.
    pub(super) fn op_ccf(&mut self) {
        // Preserve Z and the existing C; clear N and H; flip C.
        self.f = (self.f & (FLAG_Z | FLAG_C)) ^ FLAG_C;
        self.finish_instruction();
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::boot;
    use crate::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

    // -- INC / DEC -------------------------------------------------------

    #[test]
    fn inc_c_sets_half_carry_on_nibble_overflow() {
        let (mut cpu, mut bus) = boot(&[0x0C]);
        cpu.c = 0x0F;
        bus.step(&mut cpu);
        assert_eq!(cpu.c, 0x10);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn dec_b_sets_half_carry_on_nibble_borrow() {
        let (mut cpu, mut bus) = boot(&[0x05]);
        cpu.b = 0x10;
        bus.step(&mut cpu);
        assert_eq!(cpu.b, 0x0F);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn dec_b_to_zero_sets_zero_flag() {
        let (mut cpu, mut bus) = boot(&[0x05]);
        cpu.b = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.b, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn inc_c_preserves_carry() {
        let (mut cpu, mut bus) = boot(&[0x0C]);
        cpu.f = FLAG_C;
        bus.step(&mut cpu);
        assert_ne!(cpu.f & FLAG_C, 0);
    }

    #[test]
    fn inc_hl_takes_two_ticks_and_no_flags() {
        let (mut cpu, mut bus) = boot(&[0x23]);
        cpu.set_hl(0x80FF);
        cpu.f = 0;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.hl(), 0x8100);
        assert_eq!(cpu.f, 0);
    }

    #[test]
    fn dec_bc_wraps() {
        let (mut cpu, mut bus) = boot(&[0x0B]);
        cpu.set_bc(0x0000);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.bc(), 0xFFFF);
    }

    // -- INC (HL) / DEC (HL) ---------------------------------------------

    #[test]
    fn inc_hl_indirect_updates_memory_and_flags() {
        let (mut cpu, mut bus) = boot(&[0x34]);
        bus.ram[0x8000] = 0x0F;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0x8000], 0x10);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn dec_hl_indirect_writes_back_and_flags_borrow() {
        let (mut cpu, mut bus) = boot(&[0x35]);
        bus.ram[0x8000] = 0x10;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0x8000], 0x0F);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_N, 0);
    }

    // -- ALU -------------------------------------------------------------

    #[test]
    fn xor_a_clears_a_and_sets_zero_flag() {
        let (mut cpu, mut bus) = boot(&[0xAF]);
        cpu.a = 0x42;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.f, FLAG_Z);
    }

    #[test]
    fn and_d8_sets_half_carry_flag() {
        let (mut cpu, mut bus) = boot(&[0xE6, 0x0F]);
        cpu.a = 0xF0;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
    }

    #[test]
    fn cp_d8_sets_flags_without_modifying_a() {
        let (mut cpu, mut bus) = boot(&[0xFE, 0x42]);
        cpu.a = 0x42;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x42);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn add_a_b_sets_carry_on_overflow() {
        let (mut cpu, mut bus) = boot(&[0x80]);
        cpu.a = 0xFF;
        cpu.b = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_ne!(cpu.f & FLAG_Z, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_C, 0);
    }

    #[test]
    fn add_a_hl_indirect_takes_two_ticks() {
        let (mut cpu, mut bus) = boot(&[0x86]);
        bus.ram[0x8000] = 0x05;
        cpu.set_hl(0x8000);
        cpu.a = 0x10;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x15);
    }

    #[test]
    fn sub_a_b_underflow_sets_carry() {
        let (mut cpu, mut bus) = boot(&[0x90]);
        cpu.a = 0x00;
        cpu.b = 0x01;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0xFF);
        assert_ne!(cpu.f & FLAG_N, 0);
        assert_ne!(cpu.f & FLAG_C, 0);
    }

    // -- 16-bit arithmetic -----------------------------------------------

    #[test]
    fn add_hl_rr_preserves_zero_flag() {
        let (mut cpu, mut bus) = boot(&[0x09]); // ADD HL, BC
        cpu.set_hl(0x0FFF);
        cpu.set_bc(0x0001);
        cpu.f = FLAG_Z;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.hl(), 0x1000);
        assert_ne!(cpu.f & FLAG_Z, 0); // preserved
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
    }

    #[test]
    fn add_sp_r8_applies_signed_offset() {
        let (mut cpu, mut bus) = boot(&[0xE8, 0xFE]); // ADD SP, -2
        cpu.sp = 0xFFF8;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.sp, 0xFFF6);
    }

    // -- Accumulator rotates + DAA / CPL / SCF / CCF ---------------------

    #[test]
    fn rla_rotates_through_carry() {
        let (mut cpu, mut bus) = boot(&[0x17]);
        cpu.a = 0x80;
        cpu.f = 0;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_ne!(cpu.f & FLAG_C, 0);
        assert_eq!(cpu.f & FLAG_Z, 0, "RLA never sets Z");
    }

    #[test]
    fn rla_carries_in_old_carry() {
        let (mut cpu, mut bus) = boot(&[0x17]);
        cpu.a = 0x00;
        cpu.f = FLAG_C;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.f & FLAG_C, 0);
    }

    #[test]
    fn cpl_complements_a_and_sets_n_h() {
        let (mut cpu, mut bus) = boot(&[0x2F]);
        cpu.a = 0xAA;
        cpu.f = FLAG_Z;
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x55);
        assert_ne!(cpu.f & FLAG_N, 0);
        assert_ne!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_Z, 0); // preserved
    }

    #[test]
    fn scf_sets_carry_and_clears_n_h() {
        let (mut cpu, mut bus) = boot(&[0x37]);
        cpu.f = FLAG_Z | FLAG_N | FLAG_H;
        bus.step(&mut cpu);
        assert_ne!(cpu.f & FLAG_C, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
        assert_eq!(cpu.f & FLAG_H, 0);
        assert_ne!(cpu.f & FLAG_Z, 0);
    }

    #[test]
    fn ccf_flips_carry_and_clears_n_h() {
        let (mut cpu, mut bus) = boot(&[0x3F]);
        cpu.f = FLAG_C | FLAG_N | FLAG_H;
        bus.step(&mut cpu);
        assert_eq!(cpu.f & FLAG_C, 0);
        assert_eq!(cpu.f & FLAG_N, 0);
        assert_eq!(cpu.f & FLAG_H, 0);
    }

    #[test]
    fn daa_adjusts_after_bcd_addition() {
        // A = 0x15 (BCD 15) + 0x27 (BCD 27) = 0x3C binary, DAA → 0x42.
        let (mut cpu, mut bus) = boot(&[0xC6, 0x27, 0x27]); // ADD A,$27 ; DAA
        cpu.a = 0x15;
        bus.run(&mut cpu, 2); // ADD
        assert_eq!(cpu.a, 0x3C);
        bus.step(&mut cpu); // DAA
        assert_eq!(cpu.a, 0x42);
    }
}
