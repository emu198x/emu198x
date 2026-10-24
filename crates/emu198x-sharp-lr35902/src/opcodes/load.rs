//! `LD` / `LDH` family — every load and store the SM83 has.
//!
//! Covers register-pair immediates, register-indirect transfers (to
//! and from `(BC)`/`(DE)`/`(HL+)`/`(HL-)`), the 8-bit `LD r, r` /
//! `LD r, (HL)` / `LD (HL), r` group, immediate-byte loads, the
//! high-RAM `LDH` shortcuts, the `(C)` page accessor, the absolute
//! `(a16)` accessors, and the SP-flavoured loads (`LD (a16), SP`,
//! `LD HL, SP+r8`, `LD SP, HL`).

use crate::Sm83;
use crate::alu;

impl Sm83 {
    /// `LD rr, d16` — 3 m-cycles (fetch, read lo, read hi). `rr` is
    /// `BC`/`DE`/`HL`/`SP` per opcode bits 5-4.
    pub(super) fn op_ld_rr_d16(&mut self) {
        match self.m_cycle {
            1 => {
                // First post-fetch tick: schedule the low-byte read.
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
            _ => {
                self.w = self.data_in;
                let pair = (self.opcode >> 4) & 0b11;
                let value = self.wz();
                self.write_reg16_sp(pair, value);
                self.finish_instruction();
            }
        }
    }

    /// `LD (BC), A` / `LD (DE), A` — 2 m-cycles. Picks BC or DE from
    /// opcode bit 4.
    pub(super) fn op_ld_pair_a(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = if (self.opcode & 0x10) == 0 {
                    self.bc()
                } else {
                    self.de()
                };
                self.schedule_write(addr, self.a);
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD A, (BC)` / `LD A, (DE)` — 2 m-cycles.
    pub(super) fn op_ld_a_pair(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = if (self.opcode & 0x10) == 0 {
                    self.bc()
                } else {
                    self.de()
                };
                self.schedule_read(addr);
                self.m_cycle = 2;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `LD (HL+), A` ($22) / `LD (HL-), A` ($32) — 2 m-cycles. Writes
    /// `A` to `(HL)` and post-increments / decrements `HL`.
    pub(super) fn op_ld_hli_hld_a(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = self.hl();
                self.schedule_write(addr, self.a);
                let new_hl = if self.opcode == 0x22 {
                    addr.wrapping_add(1)
                } else {
                    addr.wrapping_sub(1)
                };
                self.set_hl(new_hl);
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD A, (HL+)` ($2A) / `LD A, (HL-)` ($3A) — 2 m-cycles.
    pub(super) fn op_ld_a_hli_hld(&mut self) {
        match self.m_cycle {
            1 => {
                let addr = self.hl();
                self.schedule_read(addr);
                let new_hl = if self.opcode == 0x2A {
                    addr.wrapping_add(1)
                } else {
                    addr.wrapping_sub(1)
                };
                self.set_hl(new_hl);
                self.m_cycle = 2;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `LD r, d8` — 2 m-cycles. Excludes LD (HL),d8 at $36 (3M; landed
    /// in step 4).
    pub(super) fn op_ld_r_d8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            _ => {
                let dst = (self.opcode >> 3) & 0b111;
                self.write_reg8(dst, self.data_in);
                self.finish_instruction();
            }
        }
    }

    /// `LD r, r` / `LD r, (HL)` / `LD (HL), r` — 1 m-cycle for the
    /// pure register transfer; 2 m-cycles when either operand is
    /// `(HL)`. `$76` (HALT) is excluded by the dispatch ranges.
    pub(super) fn op_ld_r_r(&mut self) {
        let dst = (self.opcode >> 3) & 0b111;
        let src = self.opcode & 0b111;

        match (self.m_cycle, dst, src) {
            // LD r, (HL) — first m-cycle: schedule read.
            (1, dst, 6) => {
                debug_assert!(dst != 6, "$76 is HALT, dispatch should have excluded it");
                self.schedule_read(self.hl());
                self.m_cycle = 2;
            }
            // LD r, (HL) — second m-cycle: latch result.
            (_, dst, 6) => {
                self.write_reg8(dst, self.data_in);
                self.finish_instruction();
            }
            // LD (HL), r — first m-cycle: schedule write.
            (1, 6, src) => {
                self.schedule_write(self.hl(), self.read_reg8(src));
                self.m_cycle = 2;
            }
            // LD (HL), r — second m-cycle.
            (_, 6, _) => self.finish_instruction(),
            // LD r, r — single m-cycle.
            (_, dst, src) => {
                let value = self.read_reg8(src);
                self.write_reg8(dst, value);
                self.finish_instruction();
            }
        }
    }

    /// `LD (HL), d8` — 3 m-cycles (fetch, read immediate, write to
    /// `(HL)`).
    pub(super) fn op_ld_hl_d8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_write(self.hl(), self.z);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LDH (a8), A` — 3 m-cycles. Stores `A` to `0xFF00 | a8`.
    pub(super) fn op_ldh_a8_a(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_write(0xFF00 | u16::from(self.z), self.a);
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LDH A, (a8)` — 3 m-cycles. Loads `A` from `0xFF00 | a8`.
    pub(super) fn op_ldh_a_a8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                self.schedule_read(0xFF00 | u16::from(self.z));
                self.m_cycle = 3;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `LD (C), A` ($E2) — 2 m-cycles. Stores `A` to `0xFF00 | C`.
    pub(super) fn op_ld_ffc_a(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_write(0xFF00 | u16::from(self.c), self.a);
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD A, (C)` ($F2) — 2 m-cycles. Loads `A` from `0xFF00 | C`.
    pub(super) fn op_ld_a_ffc(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(0xFF00 | u16::from(self.c));
                self.m_cycle = 2;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `LD (a16), A` ($EA) — 4 m-cycles.
    pub(super) fn op_ld_a16_a(&mut self) {
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
                self.schedule_write(self.wz(), self.a);
                self.m_cycle = 4;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD A, (a16)` ($FA) — 4 m-cycles.
    pub(super) fn op_ld_a_a16(&mut self) {
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
                self.schedule_read(self.wz());
                self.m_cycle = 4;
            }
            _ => {
                self.a = self.data_in;
                self.finish_instruction();
            }
        }
    }

    /// `LD (a16), SP` ($08) — 5 m-cycles. Stores SP low then SP high
    /// to `(a16)` and `(a16)+1`.
    pub(super) fn op_ld_a16_sp(&mut self) {
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
                let sp_lo = (self.sp & 0xFF) as u8;
                self.schedule_write(self.wz(), sp_lo);
                self.m_cycle = 4;
            }
            4 => {
                let sp_hi = (self.sp >> 8) as u8;
                let addr = self.wz().wrapping_add(1);
                self.schedule_write(addr, sp_hi);
                self.m_cycle = 5;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD HL, SP+r8` ($F8) — 3 m-cycles. Adds the signed offset to
    /// SP without modifying SP, stores into HL, derives flags from
    /// the low-byte unsigned arithmetic (Z=N=0).
    pub(super) fn op_ld_hl_sp_r8(&mut self) {
        match self.m_cycle {
            1 => {
                self.schedule_read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.m_cycle = 2;
            }
            2 => {
                self.z = self.data_in;
                let (result, flags) = alu::sp_add_offset(self.sp, self.z);
                self.set_hl(result);
                self.f = flags;
                self.schedule_internal();
                self.m_cycle = 3;
            }
            _ => self.finish_instruction(),
        }
    }

    /// `LD SP, HL` ($F9) — 2 m-cycles.
    pub(super) fn op_ld_sp_hl(&mut self) {
        match self.m_cycle {
            1 => {
                self.sp = self.hl();
                self.schedule_internal();
                self.m_cycle = 2;
            }
            _ => self.finish_instruction(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::boot;

    // -- LD rr, d16 ------------------------------------------------------

    #[test]
    fn ld_sp_d16_takes_three_ticks() {
        let (mut cpu, mut bus) = boot(&[0x31, 0xFE, 0xFF]);
        bus.step(&mut cpu);
        assert_eq!(cpu.sp, 0); // not set yet
        bus.step(&mut cpu);
        bus.step(&mut cpu);
        assert_eq!(cpu.sp, 0xFFFE);
        assert_eq!(cpu.pc, 3);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn ld_hl_d16_loads_h_and_l() {
        let (mut cpu, mut bus) = boot(&[0x21, 0xFF, 0x9F]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.h, 0x9F);
        assert_eq!(cpu.l, 0xFF);
    }

    #[test]
    fn ld_de_d16_loads_d_and_e() {
        let (mut cpu, mut bus) = boot(&[0x11, 0x04, 0x01]);
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.d, 0x01);
        assert_eq!(cpu.e, 0x04);
    }

    // -- LD r, d8 --------------------------------------------------------

    #[test]
    fn ld_a_d8_loads_immediate_into_a() {
        let (mut cpu, mut bus) = boot(&[0x3E, 0x42]);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn ld_b_d8_loads_immediate_into_b() {
        let (mut cpu, mut bus) = boot(&[0x06, 0x99]);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.b, 0x99);
    }

    // -- LD r, r and LD (HL),r / LD r,(HL) -------------------------------

    #[test]
    fn ld_b_a_copies_register_in_one_tick() {
        let (mut cpu, mut bus) = boot(&[0x47]);
        cpu.a = 0xAB;
        bus.step(&mut cpu);
        assert_eq!(cpu.b, 0xAB);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn ld_hl_indirect_a_writes_to_memory() {
        let (mut cpu, mut bus) = boot(&[0x77]);
        cpu.a = 0xFF;
        cpu.set_hl(0xC000);
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0xC000], 0xFF);
    }

    #[test]
    fn ld_a_hl_indirect_reads_from_memory() {
        let (mut cpu, mut bus) = boot(&[0x7E]);
        bus.ram[0x8000] = 0xBE;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0xBE);
    }

    // -- LD A,(BC/DE/HL+/HL-) and writes ---------------------------------

    #[test]
    fn ld_a_de_indirect_reads_from_de() {
        let (mut cpu, mut bus) = boot(&[0x1A]);
        bus.ram[0x0104] = 0xCE;
        cpu.set_de(0x0104);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0xCE);
    }

    #[test]
    fn ld_bc_indirect_a_writes_to_bc() {
        let (mut cpu, mut bus) = boot(&[0x02]);
        cpu.a = 0x42;
        cpu.set_bc(0xC000);
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0xC000], 0x42);
    }

    #[test]
    fn ld_hld_a_decrements_hl() {
        let (mut cpu, mut bus) = boot(&[0x32]);
        cpu.a = 0x42;
        cpu.set_hl(0x9FFF);
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0x9FFF], 0x42);
        assert_eq!(cpu.hl(), 0x9FFE);
    }

    #[test]
    fn ld_a_hli_increments_hl() {
        let (mut cpu, mut bus) = boot(&[0x2A]);
        bus.ram[0x8000] = 0x7F;
        cpu.set_hl(0x8000);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x7F);
        assert_eq!(cpu.hl(), 0x8001);
    }

    // -- Cross-instruction sanity ----------------------------------------

    #[test]
    fn small_program_writes_byte_via_hl() {
        // LD HL, $C000 ; LD A, $42 ; LD (HL), A ; NOP
        let (mut cpu, mut bus) = boot(&[
            0x21, 0x00, 0xC0, // LD HL, $C000  — 3 ticks
            0x3E, 0x42, // LD A, $42     — 2 ticks
            0x77, // LD (HL), A    — 2 ticks
            0x00, // NOP           — 1 tick
        ]);
        bus.run(&mut cpu, 3 + 2 + 2 + 1);
        assert_eq!(cpu.hl(), 0xC000);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(bus.ram[0xC000], 0x42);
        assert_eq!(cpu.pc, 7);
        assert!(!cpu.diag_unimplemented);
    }

    // -- LD (HL),d8 ------------------------------------------------------

    #[test]
    fn ld_hl_indirect_d8_writes_immediate_to_memory() {
        let (mut cpu, mut bus) = boot(&[0x36, 0x7F]);
        cpu.set_hl(0xC000);
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0xC000], 0x7F);
    }

    // -- LDH, (C), (a16) loads -------------------------------------------

    #[test]
    fn ldh_a8_a_writes_into_ff00_page() {
        let (mut cpu, mut bus) = boot(&[0xE0, 0x47]);
        cpu.a = 0xFC;
        bus.run(&mut cpu, 3);
        assert_eq!(bus.ram[0xFF47], 0xFC);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn ldh_a_a8_reads_from_ff00_page() {
        let (mut cpu, mut bus) = boot(&[0xF0, 0x44]);
        bus.ram[0xFF44] = 0x90;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.a, 0x90);
    }

    #[test]
    fn ld_ffc_a_writes_via_register_c() {
        let (mut cpu, mut bus) = boot(&[0xE2]);
        cpu.a = 0x80;
        cpu.c = 0x11;
        bus.run(&mut cpu, 2);
        assert_eq!(bus.ram[0xFF11], 0x80);
    }

    #[test]
    fn ld_a_ffc_reads_via_register_c() {
        let (mut cpu, mut bus) = boot(&[0xF2]);
        bus.ram[0xFF11] = 0x55;
        cpu.c = 0x11;
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.a, 0x55);
    }

    #[test]
    fn ld_a16_a_writes_to_absolute_address() {
        let (mut cpu, mut bus) = boot(&[0xEA, 0x34, 0x12]);
        cpu.a = 0x77;
        bus.run(&mut cpu, 4);
        assert_eq!(bus.ram[0x1234], 0x77);
    }

    #[test]
    fn ld_a_a16_reads_absolute_address() {
        let (mut cpu, mut bus) = boot(&[0xFA, 0x34, 0x12]);
        bus.ram[0x1234] = 0xBE;
        bus.run(&mut cpu, 4);
        assert_eq!(cpu.a, 0xBE);
    }

    #[test]
    fn ld_a16_sp_writes_both_bytes() {
        let (mut cpu, mut bus) = boot(&[0x08, 0x00, 0xC0]);
        cpu.sp = 0xABCD;
        bus.run(&mut cpu, 5);
        assert_eq!(bus.ram[0xC000], 0xCD);
        assert_eq!(bus.ram[0xC001], 0xAB);
    }

    // -- SP-flavoured loads ----------------------------------------------

    #[test]
    fn ld_hl_sp_r8_preserves_sp_and_sets_hl() {
        let (mut cpu, mut bus) = boot(&[0xF8, 0x02]); // LD HL, SP+2
        cpu.sp = 0xFFF0;
        bus.run(&mut cpu, 3);
        assert_eq!(cpu.hl(), 0xFFF2);
        assert_eq!(cpu.sp, 0xFFF0);
    }

    #[test]
    fn ld_sp_hl_copies_register_pair() {
        let (mut cpu, mut bus) = boot(&[0xF9]);
        cpu.set_hl(0x1234);
        bus.run(&mut cpu, 2);
        assert_eq!(cpu.sp, 0x1234);
    }
}
