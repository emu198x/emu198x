//! Trace exceptions.
//!
//! The rules come from the MC68000 User's Manual, §6.3.8 "Tracing": if T1 is
//! set as an instruction begins, a trace exception follows it, stacking the
//! address of the next instruction. An illegal or privileged instruction is
//! not traced. An exception the instruction raises itself (TRAP) is processed
//! first, and trace comes before a pending interrupt. vAmiga's Moira core
//! follows the same rules.
//!
//! The traced instruction completes, including the prefetch that promotes the
//! next one, and trace then takes the next instruction's place. So the next
//! instruction is seen to start once before the trace handler, and again when
//! the handler returns. The MAME-generated SingleStepTests/m68000 corpus ends
//! its traced rows at that same point, before trace processing.

use std::collections::HashMap;

use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;

const CODE: u32 = 0x4000;
const STACK: u32 = 0x8000;
const NOP: u16 = 0x4E71;

/// Each vector points at its own handler of NOPs, so the handler reached
/// names the exception taken.
fn handler(vector: u32) -> u32 {
    0x10000 + vector * 0x10
}

struct Machine {
    cpu: Cpu68000,
    mem: HashMap<u32, u8>,
    clocks: u32,
}

impl Machine {
    /// A machine about to execute `code` at [`CODE`] with status `sr`.
    fn new(sr: u16, code: &[u16]) -> Self {
        let mut machine = Self {
            cpu: Cpu68000::new(),
            mem: HashMap::new(),
            clocks: 0,
        };
        for vector in 0..256 {
            let target = handler(vector);
            machine.write_long(vector * 4, target);
            for word in 0..8 {
                machine.write_word(target + word * 2, NOP);
            }
        }
        for (i, &word) in code.iter().enumerate() {
            machine.write_word(CODE + i as u32 * 2, word);
        }
        machine.cpu.regs.sr = sr;
        machine.cpu.regs.ssp = STACK;
        machine.cpu.regs.usp = 0x6000;
        machine.cpu.regs.pc = CODE + 4;
        let opcode = machine.read_word(CODE);
        let irc = machine.read_word(CODE + 2);
        machine.cpu.setup_prefetch(opcode, irc);
        machine
    }

    fn read_word(&self, addr: u32) -> u16 {
        let a = addr & 0xFF_FFFE;
        let byte = |a: u32| u16::from(*self.mem.get(&a).unwrap_or(&0));
        (byte(a) << 8) | byte(a + 1)
    }

    fn read_long(&self, addr: u32) -> u32 {
        (u32::from(self.read_word(addr)) << 16) | u32::from(self.read_word(addr + 2))
    }

    fn write_word(&mut self, addr: u32, val: u16) {
        let a = addr & 0xFF_FFFE;
        self.mem.insert(a, (val >> 8) as u8);
        self.mem.insert(a + 1, val as u8);
    }

    fn write_long(&mut self, addr: u32, val: u32) {
        self.write_word(addr, (val >> 16) as u16);
        self.write_word(addr + 2, val as u16);
    }

    fn tick(&mut self) {
        self.cpu.bus_status = BusStatus::Wait;
        if let State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = self.cpu.state
            && cycle_count >= 3
        {
            self.cpu.bus_status = if fc == FunctionCode::InterruptAck {
                // Autovector: the level's vector is 24 + level.
                BusStatus::Ready(24 + u16::from(interrupt_acknowledge_level(addr)))
            } else if is_read {
                let word = self.read_word(addr);
                BusStatus::Ready(if is_word {
                    word
                } else if addr & 1 == 0 {
                    word >> 8
                } else {
                    word & 0xFF
                })
            } else {
                let val = data.unwrap_or(0);
                if is_word {
                    self.write_word(addr, val);
                } else {
                    self.mem.insert(addr & 0xFF_FFFF, val as u8);
                }
                BusStatus::Ready(0)
            };
        }
        self.cpu.tick();
        self.clocks += 1;
    }

    /// The addresses of the next `count` instruction starts.
    fn starts(&mut self, count: usize) -> Vec<u32> {
        (0..count).map(|_| self.next_instruction()).collect()
    }

    /// Run to the start of the next instruction and return its address.
    fn next_instruction(&mut self) -> u32 {
        let start = self.cpu.instruction_starts;
        for _ in 0..1000 {
            self.tick();
            if self.cpu.instruction_starts > start {
                return self.cpu.instr_start_pc;
            }
        }
        panic!("no instruction started within 1000 clocks");
    }

    /// The SR and PC of the exception frame `depth` frames down the stack.
    fn frame(&self, depth: u32) -> (u16, u32) {
        let sp = self.cpu.regs.a(7) + depth * 6;
        (self.read_word(sp), self.read_long(sp + 2))
    }
}

#[test]
fn a_traced_instruction_is_followed_by_a_trace_exception() {
    let mut machine = Machine::new(0xA700, &[NOP, NOP]);
    assert_eq!(machine.starts(2), [CODE + 2, handler(9)]);
    assert_eq!(machine.frame(0), (0xA700, CODE + 2));
    assert_eq!(machine.cpu.regs.sr, 0x2700, "trace clears T1");
}

/// Trace processing takes 34 clocks (MC68000 User's Manual Table 8-14):
/// four idle, three stack writes, two vector reads and the handler's two
/// prefetches with two idle between. The traced NOP still completes its own
/// prefetch, so from the NOP to the handler takes 34 clocks more than from
/// an untraced NOP to the next instruction.
#[test]
fn trace_takes_34_clocks() {
    let mut untraced = Machine::new(0x2700, &[NOP, NOP]);
    assert_eq!(untraced.starts(1), [CODE + 2]);
    let mut traced = Machine::new(0xA700, &[NOP, NOP]);
    assert_eq!(traced.starts(2), [CODE + 2, handler(9)]);
    assert_eq!(traced.clocks - untraced.clocks, 34);
}

#[test]
fn the_instruction_that_sets_t1_is_not_traced() {
    // ORI #$8000,SR; NOP; NOP
    let mut machine = Machine::new(0x2700, &[0x007C, 0x8000, NOP, NOP]);
    assert_eq!(machine.starts(3), [CODE + 4, CODE + 6, handler(9)]);
    assert_eq!(machine.frame(0), (0xA700, CODE + 6));
}

#[test]
fn the_instruction_that_clears_t1_is_traced() {
    // ANDI #$7FFF,SR; NOP
    let mut machine = Machine::new(0xA700, &[0x027C, 0x7FFF, NOP]);
    assert_eq!(machine.starts(2), [CODE + 4, handler(9)]);
    assert_eq!(machine.frame(0), (0x2700, CODE + 4));
}

#[test]
fn an_illegal_instruction_is_not_traced() {
    let mut machine = Machine::new(0xA700, &[0x4AFC, NOP]);
    assert_eq!(machine.starts(2), [handler(4), handler(4) + 2]);
    assert_eq!(machine.frame(0), (0xA700, CODE));
}

#[test]
fn a_privileged_instruction_is_not_traced() {
    // MOVE #$2700,SR in user mode.
    let mut machine = Machine::new(0x8000, &[0x46FC, 0x2700, NOP]);
    assert_eq!(machine.starts(2), [handler(8), handler(8) + 2]);
    assert_eq!(machine.frame(0), (0x8000, CODE));
}

#[test]
fn a_traced_trap_enters_its_handler_through_the_trace_handler() {
    // TRAP #0
    let mut machine = Machine::new(0xA700, &[0x4E40, NOP]);
    assert_eq!(machine.starts(2), [handler(32), handler(9)]);
    assert_eq!(machine.frame(0), (0x2700, handler(32)));
    assert_eq!(machine.frame(1), (0xA700, CODE + 2));
}

#[test]
fn trace_comes_before_a_pending_interrupt() {
    let mut machine = Machine::new(0xA000, &[NOP, NOP]);
    machine.cpu.ipl = 3;
    assert_eq!(machine.starts(2), [CODE + 2, handler(27)]);
    assert_eq!(machine.frame(0), (0x2000, handler(9)));
    assert_eq!(machine.frame(1), (0xA000, CODE + 2));
}

#[test]
fn rte_into_traced_code_traces_the_next_instruction_not_itself() {
    let mut machine = Machine::new(0x2700, &[0x4E73]);
    for word in 0..4 {
        machine.write_word(0x5000 + word * 2, NOP);
    }
    machine.cpu.regs.ssp = STACK - 6;
    machine.write_word(STACK - 6, 0xA700);
    machine.write_long(STACK - 4, 0x5000);
    assert_eq!(machine.starts(3), [0x5000, 0x5002, handler(9)]);
    assert_eq!(machine.frame(0), (0xA700, 0x5002));
}

#[test]
fn stop_begun_with_t1_set_is_traced_at_once() {
    // STOP #$2700
    let mut machine = Machine::new(0xA700, &[0x4E72, 0x2700, NOP]);
    assert_eq!(machine.next_instruction(), handler(9));
    assert_eq!(machine.frame(0), (0x2700, CODE + 4));
}

#[test]
fn a_pending_trace_survives_a_snapshot() {
    let mut machine = Machine::new(0xA700, &[NOP, NOP]);
    assert_eq!(machine.next_instruction(), CODE + 2);
    // Mid-way through the prefetch that completes the traced NOP.
    machine.tick();
    let saved = serde_json::to_vec(&machine.cpu).expect("serialize mid-prefetch CPU");
    machine.cpu = serde_json::from_slice(&saved).expect("restore mid-prefetch CPU");
    assert_eq!(machine.next_instruction(), handler(9));
    assert_eq!(machine.frame(0), (0xA700, CODE + 2));
}
