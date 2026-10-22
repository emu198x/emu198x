//! Every opcode word on the 68020, whatever follows it.
//!
//! The 68020 shares the 68000 core's decoder and adds its own through the
//! variant hook, so an encoding neither implements must still end in an
//! exception rather than a panic. Mode 7 with register 5-7 names no
//! addressing mode on any 680x0: in an integer instruction it takes the
//! illegal-instruction exception, and in a coprocessor ($Fxxx) instruction
//! the line-F exception, as WinUAE's op_illg does.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const CODE: u32 = 0x4000;
const STACK: u32 = 0x8000;

/// Each vector points at its own handler, so the handler reached names the
/// exception taken.
fn handler(vector: u32) -> u32 {
    0x10000 + vector * 0x10
}

/// Execute `opcode` with extension word `ext` once and return where the
/// next instruction starts.
fn execute(opcode: u16, ext: u16) -> u32 {
    let mut mem: HashMap<u32, u8> = HashMap::new();
    let write_word = |mem: &mut HashMap<u32, u8>, addr: u32, val: u16| {
        mem.insert(addr, (val >> 8) as u8);
        mem.insert(addr + 1, val as u8);
    };
    for vector in 0..256 {
        let target = handler(vector);
        write_word(&mut mem, vector * 4, (target >> 16) as u16);
        write_word(&mut mem, vector * 4 + 2, target as u16);
    }
    write_word(&mut mem, CODE, opcode);
    write_word(&mut mem, CODE + 2, ext);

    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2700;
    cpu.regs.ssp = STACK;
    cpu.regs.pc = CODE + 4;
    cpu.setup_prefetch(opcode, ext);

    let start = cpu.instruction_starts;
    for _ in 0..2000 {
        cpu.bus_status = BusStatus::Wait;
        if let State::BusCycle {
            addr,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = cpu.state
            && cycle_count >= 3
        {
            let word = |mem: &HashMap<u32, u8>, a: u32| {
                (u16::from(*mem.get(&a).unwrap_or(&0)) << 8)
                    | u16::from(*mem.get(&(a + 1)).unwrap_or(&0))
            };
            let aligned = addr & 0xFF_FFFE;
            cpu.bus_status = if is_read {
                let value = word(&mem, aligned);
                BusStatus::Ready(if is_word {
                    value
                } else if addr & 1 == 0 {
                    value >> 8
                } else {
                    value & 0xFF
                })
            } else {
                let value = data.unwrap_or(0);
                if is_word {
                    write_word(&mut mem, aligned, value);
                } else {
                    mem.insert(addr & 0xFF_FFFF, value as u8);
                }
                BusStatus::Ready(0)
            };
        }
        cpu.tick();
        if cpu.instruction_starts > start {
            break;
        }
    }
    cpu.instr_start_pc
}

#[test]
fn no_opcode_word_panics() {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked: Vec<String> = (0..=u16::MAX)
        .filter(|&opcode| std::panic::catch_unwind(|| execute(opcode, 0)).is_err())
        .map(|opcode| format!("${opcode:04X}"))
        .collect();
    std::panic::set_hook(report);
    assert!(
        panicked.is_empty(),
        "{} opcode words panicked: {}",
        panicked.len(),
        panicked.join(" ")
    );
}

#[test]
fn undefined_effective_addresses_take_the_exception_for_their_line() {
    // MULU.L <mode 7, reg 5>,D0: an integer instruction the 68020 adds.
    assert_eq!(execute(0x4C3D, 0x0000), handler(4));
    // FMOVE.L <mode 7, reg 5>,FP0: a coprocessor general instruction.
    assert_eq!(execute(0xF23D, 0x4000), handler(11));
}
