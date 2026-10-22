//! Every opcode word the 68000 does not implement must trap.
//!
//! `isa_conformance.rs` holds the disassembler's opcode table to the Isa198x
//! spec over all 65,536 words. This test holds the executor to the same table:
//! an opcode the disassembler renders as data must, executed in supervisor
//! mode, take the illegal-instruction exception (vector 4), or the line-A (10)
//! or line-F (11) exception for those groups, and stack its own address.
//!
//! An executor that runs such a word as a neighbouring instruction consumes
//! whatever follows as operands and carries on. `ILLEGAL` ($4AFC) run as
//! `TAS #imm` is the case that matters most: CPU-detection code points vector
//! 4 at the word after it, so the core lands mid-instruction instead.

use std::collections::HashMap;

use motorola_68000::Cpu68000;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68000::disasm::disassemble;

const CODE: u32 = 0x4000;
const STACK: u32 = 0x8000;

/// Each vector points at its own handler, so the handler reached names the
/// exception taken.
fn handler(vector: u32) -> u32 {
    0x10000 + vector * 0x10
}

struct Mem(HashMap<u32, u8>);

impl Mem {
    fn read_byte(&self, addr: u32) -> u8 {
        *self.0.get(&(addr & 0xFF_FFFF)).unwrap_or(&0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        let a = addr & 0xFF_FFFE;
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a + 1))
    }

    fn write_byte(&mut self, addr: u32, val: u8) {
        self.0.insert(addr & 0xFF_FFFF, val);
    }

    fn write_word(&mut self, addr: u32, val: u16) {
        let a = addr & 0xFF_FFFE;
        self.write_byte(a, (val >> 8) as u8);
        self.write_byte(a + 1, val as u8);
    }
}

fn service_bus(cpu: &mut Cpu68000, mem: &mut Mem) {
    cpu.bus_status = BusStatus::Wait;
    if let State::BusCycle {
        addr,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
        && *cycle_count >= 3
    {
        cpu.bus_status = if *is_read {
            BusStatus::Ready(if *is_word {
                mem.read_word(*addr)
            } else {
                u16::from(mem.read_byte(*addr))
            })
        } else {
            let val = data.unwrap_or(0);
            if *is_word {
                mem.write_word(*addr, val);
            } else {
                mem.write_byte(*addr, val as u8);
            }
            BusStatus::Ready(0)
        };
    }
}

/// Execute `opcode` once and return where the next instruction starts and
/// the PC on top of the stack.
fn execute(opcode: u16) -> (u32, u32) {
    let (next, stacked_pc, _) = run(0x2700, opcode, 0);
    (next, stacked_pc)
}

/// Execute `opcode` (with extension word `ext`) from status `sr` until the
/// next instruction starts. Returns where it starts, the PC on top of the
/// stack and the clocks taken.
fn run(sr: u16, opcode: u16, ext: u16) -> (u32, u32, u32) {
    let mut mem = Mem(HashMap::new());
    for vector in 0..256 {
        let target = handler(vector);
        mem.write_word(vector * 4, (target >> 16) as u16);
        mem.write_word(vector * 4 + 2, target as u16);
    }
    mem.write_word(CODE, opcode);
    mem.write_word(CODE + 2, ext);

    let mut cpu = Cpu68000::new();
    cpu.regs.sr = sr;
    cpu.regs.ssp = STACK;
    cpu.regs.usp = STACK - 0x1000;
    cpu.regs.pc = CODE + 4;
    cpu.setup_prefetch(opcode, ext);

    let start = cpu.instruction_starts;
    let mut clocks = 0;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        clocks += 1;
        if cpu.instruction_starts > start {
            break;
        }
    }
    let sp = cpu.regs.a(7);
    let stacked_pc = (u32::from(mem.read_word(sp + 2)) << 16) | u32::from(mem.read_word(sp + 4));
    (cpu.instr_start_pc, stacked_pc, clocks)
}

/// MC68000 User's Manual Table 8-14 gives illegal-instruction and
/// privilege-violation exceptions 34 clocks each: four idle, three stack
/// writes, two vector reads and the handler's two prefetches with two idle
/// between (as vAmiga's Moira sequences them). Both counts here stop before
/// the next instruction's last prefetch, so each exception takes 34 - 4
/// clocks more than a NOP (4 clocks, one prefetch).
#[test]
fn illegal_and_privilege_exceptions_take_34_clocks() {
    let (_, _, nop) = run(0x2700, 0x4E71, 0);
    let (next, _, illegal) = run(0x2700, 0x4AFC, 0);
    assert_eq!(next, handler(4));
    assert_eq!(illegal - nop, 30);
    // MOVE #$2700,SR in user mode.
    let (next, _, privilege) = run(0x0000, 0x46FC, 0x2700);
    assert_eq!(next, handler(8));
    assert_eq!(privilege - nop, 30);
}

/// The opcodes in `opcodes` the disassembler rejects that do not take the
/// expected exception when executed, one line each.
fn untrapped(opcodes: impl Iterator<Item = u16>) -> Vec<String> {
    // A panic is a failure to report, not a test abort, so keep its message
    // out of the output.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut failures = Vec::new();
    for opcode in opcodes {
        let bytes = opcode.to_be_bytes();
        let (text, _) = disassemble(0, |addr| bytes.get(addr as usize).copied().unwrap_or(0));
        let text = text.trim_start().to_ascii_lowercase();
        if !(text.starts_with("dc.") || text.starts_with("illegal")) {
            continue;
        }
        let vector = match opcode >> 12 {
            0xA => 10,
            0xF => 11,
            _ => 4,
        };
        let Ok((next, stacked_pc)) = std::panic::catch_unwind(|| execute(opcode)) else {
            failures.push(format!("${opcode:04X}: the core panicked"));
            continue;
        };
        if next != handler(vector) || stacked_pc != CODE {
            failures.push(format!(
                "${opcode:04X}: expected vector {vector}, next instruction at ${next:X}, \
                 stacked PC ${stacked_pc:X}"
            ));
        }
    }
    std::panic::set_hook(report);
    failures
}

/// ILLEGAL ($4AFC) shares TAS's opcode row, as do TAS's invalid
/// destinations: An, PC-relative and immediate.
#[test]
fn illegal_and_invalid_tas_destinations_trap() {
    let failures = untrapped(0x4AC0..=0x4AFF);
    assert!(
        failures.is_empty(),
        "{} opcodes did not trap:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
