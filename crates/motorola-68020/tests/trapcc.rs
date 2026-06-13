//! TRAPcc ($50F8/$50FA/$50FC + cc) — 68020 conditional trap (#114).
//!
//! No Tom Harte / m68k-generated fixtures cover TRAPcc, so these are
//! hand-authored against M68000PRM § 6.2.40:
//!
//! - The condition field (bits 11-8) selects when the trap is taken.
//! - The reg field (bits 2-0) selects the operand size: 2 = word,
//!   3 = long, 4 = none. The operand is *not* consumed as data; the
//!   instruction only steps the prefetch past it.
//! - Taken → vector 7 (same as TRAPV), saved PC points past the whole
//!   instruction (opcode + operand).
//! - Not taken → execution continues at the next instruction.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;
const HANDLER: u32 = 0x0000_3000;
const INITIAL_SSP: u32 = 0x0000_8000;

struct Mem {
    bytes: HashMap<u32, u8>,
}

impl Mem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }
    fn read_byte(&self, a: u32) -> u8 {
        *self.bytes.get(&(a & 0x00FF_FFFF)).unwrap_or(&0)
    }
    fn read_word(&self, a: u32) -> u16 {
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a.wrapping_add(1)))
    }
    fn write_byte(&mut self, a: u32, v: u8) {
        self.bytes.insert(a & 0x00FF_FFFF, v);
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.write_byte(a, (v >> 8) as u8);
        self.write_byte(a.wrapping_add(1), v as u8);
    }
    fn write_long(&mut self, a: u32, v: u32) {
        self.write_word(a, (v >> 16) as u16);
        self.write_word(a.wrapping_add(2), v as u16);
    }
}

fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    if let State::BusCycle {
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
                    u16::from(mem.read_byte(*addr))
                };
                cpu.bus_status = BusStatus::Ready(v);
            } else {
                let v = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, v);
                } else {
                    mem.write_byte(*addr, v as u8);
                }
                cpu.bus_status = BusStatus::Ready(0);
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }
    } else {
        cpu.bus_status = BusStatus::Wait;
    }
}

/// Run a TRAPcc to the start of the next instruction and report what
/// happened. `operand_words` are the extension words after the opcode;
/// `sr_flags` seeds the condition flags (low byte of SR).
struct Outcome {
    /// instr_start_pc once the following instruction promotes.
    next_instr_pc: u32,
    /// Active SSP after the run (decreases iff a frame was pushed).
    ssp: u32,
    /// True if execution vectored to the trap handler.
    trapped: bool,
}

fn run_trapcc(opcode: u16, operand_words: &[u16], sr_flags: u16) -> Outcome {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    // Vector 7 (offset 7*4 = $1C) → handler; NOP at handler + next-instr.
    mem.write_long(7 * 4, HANDLER);
    mem.write_word(HANDLER, 0x4E71); // NOP

    mem.write_word(PC, opcode);
    for (i, w) in operand_words.iter().enumerate() {
        mem.write_word(PC + 2 + (i as u32) * 2, *w);
    }
    // NOP at the fall-through point and a couple beyond, so the
    // not-taken path promotes a clean next instruction.
    let after = PC + 2 + (operand_words.len() as u32) * 2;
    for k in 0..3 {
        mem.write_word(after + (k as u32) * 2, 0x4E71);
    }

    cpu.regs.sr = 0x2000 | (sr_flags & 0x1F); // supervisor + CCR flags
    cpu.regs.ssp = INITIAL_SSP;
    cpu.regs.set_active_sp(INITIAL_SSP);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, operand_words.first().copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            let trapped = cpu.instr_start_pc == HANDLER;
            return Outcome {
                next_instr_pc: cpu.instr_start_pc,
                ssp: cpu.regs.active_sp(),
                trapped,
            };
        }
    }
    panic!("TRAPcc did not reach the next instruction");
}

// cc field values (bits 11-8).
const CC_T: u16 = 0x0; // always
const CC_F: u16 = 0x1; // never
const CC_NE: u16 = 0x6;
const CC_EQ: u16 = 0x7;
const Z_FLAG: u16 = 0x04;

#[test]
fn trapt_no_operand_takes_vector_7() {
    let op = 0x50F8 | (CC_T << 8) | 4; // TRAPT, no operand
    let r = run_trapcc(op, &[], 0);
    assert!(r.trapped, "TRAPT must vector to the handler");
    assert!(r.ssp < INITIAL_SSP, "an exception frame must be pushed");
}

#[test]
fn trapf_no_operand_falls_through() {
    let op = 0x50F8 | (CC_F << 8) | 4; // TRAPF, no operand
    let r = run_trapcc(op, &[], 0);
    assert!(!r.trapped, "TRAPF never traps");
    assert_eq!(r.next_instr_pc, PC + 2, "1-word instruction → next at +2");
    assert_eq!(r.ssp, INITIAL_SSP, "no frame pushed");
}

#[test]
fn trapf_word_operand_steps_past_one_word() {
    let op = 0x50F8 | (CC_F << 8) | 2; // TRAPF.W
    let r = run_trapcc(op, &[0xDEAD], 0);
    assert!(!r.trapped);
    assert_eq!(r.next_instr_pc, PC + 4, "opcode + 1 word → next at +4");
    assert_eq!(r.ssp, INITIAL_SSP);
}

#[test]
fn trapf_long_operand_steps_past_two_words() {
    let op = 0x50F8 | (CC_F << 8) | 3; // TRAPF.L
    let r = run_trapcc(op, &[0xDEAD, 0xBEEF], 0);
    assert!(!r.trapped);
    assert_eq!(r.next_instr_pc, PC + 6, "opcode + 2 words → next at +6");
    assert_eq!(r.ssp, INITIAL_SSP);
}

#[test]
fn trapne_respects_the_z_flag() {
    let op = 0x50F8 | (CC_NE << 8) | 4; // TRAPNE, no operand

    // Z clear → NE true → trap taken.
    let taken = run_trapcc(op, &[], 0);
    assert!(taken.trapped, "TRAPNE with Z=0 must trap");

    // Z set → NE false → fall through.
    let not = run_trapcc(op, &[], Z_FLAG);
    assert!(!not.trapped, "TRAPNE with Z=1 must not trap");
    assert_eq!(not.next_instr_pc, PC + 2);
}

#[test]
fn trapeq_word_operand_traps_with_pc_past_operand() {
    // TRAPEQ.W with Z set → taken; the word operand must still be
    // skipped (the stacked PC points past it, but here we just confirm
    // the trap fires and a frame is pushed).
    let op = 0x50F8 | (CC_EQ << 8) | 2;
    let r = run_trapcc(op, &[0x1234], Z_FLAG);
    assert!(r.trapped, "TRAPEQ with Z=1 must trap");
    assert!(r.ssp < INITIAL_SSP);
}

#[test]
fn trapcc_reg_5_is_illegal() {
    // Mode 111 / reg 5 is not a TRAPcc form → ILLEGAL (vector 4).
    // Vector 4 ($10) → a distinct handler so we can tell it apart.
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    mem.write_long(4 * 4, 0x0000_4000); // vector 4 → $4000
    mem.write_word(0x0000_4000, 0x4E71);
    let op = 0x50F8 | (CC_T << 8) | 5; // reg 5 — undefined
    mem.write_word(PC, op);
    for k in 0..3 {
        mem.write_word(PC + 2 + (k as u32) * 2, 0x4E71);
    }
    cpu.regs.sr = 0x2000;
    cpu.regs.ssp = INITIAL_SSP;
    cpu.regs.set_active_sp(INITIAL_SSP);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(op, 0x4E71);

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            assert_eq!(
                cpu.instr_start_pc, 0x0000_4000,
                "reg 5 must take the ILLEGAL vector (4)"
            );
            return;
        }
    }
    panic!("did not complete");
}
