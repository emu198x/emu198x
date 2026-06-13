//! Bcc.L / BSR.L / BRA.L — 68020 32-bit displacement branches (#114).
//!
//! On the 68020 an 8-bit displacement field of `$FF` selects a 32-bit
//! displacement in the two following words. The displacement is relative
//! to instr_start + 2 (the address of the first displacement word). BSR.L
//! pushes the return address (past the 6-byte instruction). On the
//! 68000/68010 `$FF` is a normal 8-bit branch with displacement −1, so
//! this path is gated on the variant_long_branch flag. Semantics matched
//! to Musashi (`m68k_in.c` bcc/bsr/bra 32).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68k_common::flags::Z;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;

struct Mem {
    bytes: HashMap<u32, u8>,
}

impl Mem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }
    // Unprogrammed memory reads as NOP ($4E71) so any branch target and
    // fall-through path lands on a valid instruction.
    fn read_word(&self, a: u32) -> u16 {
        let a = a & 0x00FF_FFFF;
        match (self.bytes.get(&a), self.bytes.get(&(a.wrapping_add(1)))) {
            (None, None) => 0x4E71,
            (hi, lo) => (u16::from(*hi.unwrap_or(&0)) << 8) | u16::from(*lo.unwrap_or(&0)),
        }
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.bytes.insert(a & 0x00FF_FFFF, (v >> 8) as u8);
        self.bytes
            .insert((a.wrapping_add(1)) & 0x00FF_FFFF, v as u8);
    }
    fn read_long(&self, a: u32) -> u32 {
        (u32::from(self.read_word(a)) << 16) | u32::from(self.read_word(a.wrapping_add(2)))
    }
}

fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    let resp = if let State::BusCycle {
        addr,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
    {
        if *cycle_count >= 3 {
            if *is_read {
                let v = if *is_word {
                    mem.read_word(*addr)
                } else {
                    u16::from((mem.read_word(*addr) >> 8) as u8)
                };
                BusStatus::Ready(v)
            } else {
                let v = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, v);
                } else {
                    mem.bytes.insert(*addr & 0x00FF_FFFF, v as u8);
                }
                BusStatus::Ready(0)
            }
        } else {
            BusStatus::Wait
        }
    } else {
        BusStatus::Wait
    };
    cpu.bus_status = resp;
}

struct Out {
    next_pc: u32,
    ssp: u32,
    pushed: u32,
}

/// Run one long-branch instruction (opcode + 32-bit displacement) and
/// report where the next instruction starts, plus the stack pointer and
/// the long at the post-instruction stack top (for BSR return-address
/// checks).
fn run(opcode: u16, disp: u32, seed: impl FnOnce(&mut Cpu68020)) -> Out {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_word(PC, opcode);
    mem.write_word(PC + 2, (disp >> 16) as u16);
    mem.write_word(PC + 4, disp as u16);

    cpu.regs.sr |= 0x2000;
    cpu.regs.ssp = 0x0000_8000;
    cpu.regs.set_active_sp(0x0000_8000);
    seed(&mut cpu);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, (disp >> 16) as u16);

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            let ssp = cpu.regs.ssp;
            return Out {
                next_pc: cpu.instr_start_pc,
                ssp,
                pushed: mem.read_long(ssp),
            };
        }
    }
    panic!("instruction did not complete");
}

const BRA_L: u16 = 0x60FF;
const BSR_L: u16 = 0x61FF;
const BEQ_L: u16 = 0x67FF; // cond 0111 = EQ (branch if Z set)
const BNE_L: u16 = 0x66FF; // cond 0110 = NE (branch if Z clear)

#[test]
fn bra_l_positive_displacement_beyond_16_bit() {
    // disp = 0x0001_0000 (64 KiB, beyond the 16-bit positive range).
    // target = PC + 2 + disp = 0x1002 + 0x1_0000 = 0x1_1002.
    let r = run(BRA_L, 0x0001_0000, |_| {});
    assert_eq!(r.next_pc, 0x0001_1002, "BRA.L target = instr+2+disp");
}

#[test]
fn bra_l_negative_displacement() {
    // disp = -0x100 → target = 0x1002 - 0x100 = 0xF02.
    let r = run(BRA_L, (-0x100i32) as u32, |_| {});
    assert_eq!(r.next_pc, 0x0000_0F02);
}

#[test]
fn bra_l_full_32_bit_displacement() {
    // A displacement that only fits in 32 bits: 0x0080_0000 (8 MiB).
    // target = 0x1002 + 0x80_0000 = 0x80_1002.
    let r = run(BRA_L, 0x0080_0000, |_| {});
    assert_eq!(r.next_pc, 0x0080_1002);
}

#[test]
fn beq_l_taken_when_z_set() {
    let r = run(BEQ_L, 0x0001_0000, |cpu| cpu.regs.sr |= Z);
    assert_eq!(r.next_pc, 0x0001_1002, "Z set → BEQ.L taken");
}

#[test]
fn beq_l_not_taken_falls_through() {
    // Z clear → not taken; next instruction is past the 6-byte branch.
    let r = run(BEQ_L, 0x0001_0000, |cpu| cpu.regs.sr &= !Z);
    assert_eq!(r.next_pc, PC + 6, "Z clear → fall through to instr+6");
}

#[test]
fn bne_l_taken_when_z_clear() {
    let r = run(BNE_L, 0x0000_2000, |cpu| cpu.regs.sr &= !Z);
    assert_eq!(r.next_pc, 0x0000_3002, "Z clear → BNE.L taken");
}

#[test]
fn bne_l_not_taken_when_z_set() {
    let r = run(BNE_L, 0x0000_2000, |cpu| cpu.regs.sr |= Z);
    assert_eq!(r.next_pc, PC + 6, "Z set → BNE.L falls through");
}

#[test]
fn bsr_l_pushes_return_address_and_branches() {
    // BSR.L pushes the address past the 6-byte instruction (instr+6),
    // then branches to instr+2+disp.
    let r = run(BSR_L, 0x0001_0000, |_| {});
    assert_eq!(r.next_pc, 0x0001_1002, "BSR.L branches to target");
    assert_eq!(r.ssp, 0x0000_8000 - 4, "pushed a long → SP -= 4");
    assert_eq!(r.pushed, PC + 6, "return address = instr_start + 6");
}

#[test]
fn bsr_l_negative_displacement() {
    let r = run(BSR_L, (-0x200i32) as u32, |_| {});
    assert_eq!(r.next_pc, 0x1002 - 0x200);
    assert_eq!(r.pushed, PC + 6);
}
