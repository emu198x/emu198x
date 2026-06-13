//! F-line ($Fxxx) dispatch — 68020 (#112, step 1).
//!
//! The core now offers F-line opcodes to the variant decode hook before
//! falling back to the vector-11 F-line emulator trap. No FPU arm is
//! wired yet, so every F-line opcode is still *unclaimed* and must trap
//! vector 11 — exactly as before. These tests pin that fallback so it
//! survives the FPU work that builds on this route.

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

/// Run one opcode (plus extension words) and report whether it vectored
/// to the vector-11 (F-line emulator) handler.
fn vectors_to_fline_handler(opcode: u16, ext_words: &[u16]) -> bool {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    // Vector 11 (F-line, offset 11*4 = $2C) → handler.
    mem.write_long(11 * 4, HANDLER);
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
            return cpu.instr_start_pc == HANDLER;
        }
    }
    panic!("instruction did not complete");
}

#[test]
fn unclaimed_fpu_general_opcode_traps_vector_11() {
    // $F200 = cpID 1 (FPU), cpGEN. No FPU arm yet → vector 11.
    assert!(vectors_to_fline_handler(0xF200, &[0x0000]));
}

#[test]
fn unclaimed_fline_low_cpid_traps_vector_11() {
    // $F080 = cpID 0, cpGEN — never an FPU op → vector 11.
    assert!(vectors_to_fline_handler(0xF080, &[0x0000]));
}

#[test]
fn unclaimed_fline_max_opcode_traps_vector_11() {
    assert!(vectors_to_fline_handler(0xFFFF, &[]));
}

#[test]
fn fnop_traps_vector_11_until_fpu_wired() {
    // $F280 $0000 = FNOP (cpID 1, cpGEN, command word 0). Currently
    // unclaimed → vector 11; this test will flip to "executes" once the
    // FPU arm lands.
    assert!(vectors_to_fline_handler(0xF280, &[0x0000]));
}
