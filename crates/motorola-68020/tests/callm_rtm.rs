//! CALLM / RTM ($06C0-$06FF) — 68020 module call/return (#114).
//!
//! These are deliberately unimplemented: they take the illegal-
//! instruction exception (vector 4), matching WinUAE, Musashi, and the
//! 68030+ (which dropped the opcodes). See
//! knowledge/decisions/callm-rtm-illegal.md for the rationale. These
//! tests pin that behaviour so it can't silently change.

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

struct Outcome {
    next_instr_pc: u32,
    ssp: u32,
    vectored: bool,
}

/// Run one CALLM/RTM-range opcode (plus any extension words) and report
/// whether it vectored to the illegal-instruction handler.
fn run(opcode: u16, ext_words: &[u16]) -> Outcome {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    // Vector 4 (illegal instruction, offset 4*4 = $10) → handler.
    mem.write_long(4 * 4, HANDLER);
    mem.write_word(HANDLER, 0x4E71); // NOP

    mem.write_word(PC, opcode);
    for (i, w) in ext_words.iter().enumerate() {
        mem.write_word(PC + 2 + (i as u32) * 2, *w);
    }

    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.ssp = INITIAL_SSP;
    cpu.regs.set_active_sp(INITIAL_SSP);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(opcode, ext_words.first().copied().unwrap_or(0x4E71));

    let start = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start {
            return Outcome {
                next_instr_pc: cpu.instr_start_pc,
                ssp: cpu.regs.ssp,
                vectored: cpu.instr_start_pc == HANDLER,
            };
        }
    }
    panic!("instruction did not complete");
}

fn assert_illegal(opcode: u16, ext_words: &[u16]) {
    let r = run(opcode, ext_words);
    assert!(
        r.vectored,
        "opcode {opcode:#06x} must take the illegal-instruction exception"
    );
    assert!(
        r.ssp < INITIAL_SSP,
        "an exception frame must be pushed (SSP decreased)"
    );
    assert_eq!(r.next_instr_pc, HANDLER, "execution resumes in the handler");
}

#[test]
fn rtm_dn_is_illegal() {
    // RTM D0 ($06C0) .. RTM D7 ($06C7).
    assert_illegal(0x06C0, &[]);
    assert_illegal(0x06C7, &[]);
}

#[test]
fn rtm_an_is_illegal() {
    // RTM A0 ($06C8) .. RTM A7 ($06CF).
    assert_illegal(0x06C8, &[]);
    assert_illegal(0x06CF, &[]);
}

#[test]
fn callm_indirect_is_illegal() {
    // CALLM (A0) ($06D0) + argument-count extension word.
    assert_illegal(0x06D0, &[0x0004]);
}

#[test]
fn callm_d16_an_is_illegal() {
    // CALLM (d16,A0) ($06E8) + arg count + displacement.
    assert_illegal(0x06E8, &[0x0008, 0x0000]);
}

#[test]
fn callm_abs_long_is_illegal() {
    // CALLM (xxx).L ($06F9) + arg count + 32-bit address.
    assert_illegal(0x06F9, &[0x0010, 0x0000, 0x2000]);
}

#[test]
fn callm_pc_relative_is_illegal() {
    // CALLM (d16,PC) ($06FA) + arg count + displacement.
    assert_illegal(0x06FA, &[0x0002, 0x0000]);
}
