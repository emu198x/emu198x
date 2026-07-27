//! Pin-level interrupt-entry regressions for the Motorola 68010.
//!
//! These tests exercise the relationship between the interrupt
//! acknowledge response, the relocated vector-table lookup, and the
//! Format/Vector word saved in the short exception frame.

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::{State, TAG_EXC_IACK_COMPLETE};
use motorola_68000::microcode::MicroOp;
use motorola_68010::Cpu68010;

const ENTRY_PC: u32 = 0x0100;
const INTERRUPTED_PC: u32 = 0x0104;
const INITIAL_SSP: u32 = 0x8000;
const FRAME_SP: u32 = INITIAL_SSP - 8;
const VBR: u32 = 0x4000;
const INTERRUPT_LEVEL: u8 = 3;

#[derive(Clone, Copy)]
enum IackResponse {
    Ready(u16),
    Error,
}

#[derive(Clone)]
struct TestMem {
    bytes: Vec<u8>,
}

impl TestMem {
    fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }

    fn read_byte(&self, addr: u32) -> u8 {
        self.bytes.get(addr as usize).copied().unwrap_or(0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        (u16::from(self.read_byte(addr)) << 8) | u16::from(self.read_byte(addr + 1))
    }

    fn read_long(&self, addr: u32) -> u32 {
        (u32::from(self.read_word(addr)) << 16) | u32::from(self.read_word(addr + 2))
    }

    fn write_byte(&mut self, addr: u32, value: u8) {
        if let Some(byte) = self.bytes.get_mut(addr as usize) {
            *byte = value;
        }
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        self.write_byte(addr, (value >> 8) as u8);
        self.write_byte(addr + 1, value as u8);
    }

    fn write_long(&mut self, addr: u32, value: u32) {
        self.write_word(addr, (value >> 16) as u16);
        self.write_word(addr + 2, value as u16);
    }
}

fn service_bus(
    cpu: &mut Cpu68010,
    mem: &mut TestMem,
    iack_response: IackResponse,
    saw_iack: &mut bool,
) {
    if let State::BusCycle {
        addr,
        fc,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
    {
        if *cycle_count < 3 {
            cpu.bus_status = BusStatus::Wait;
            return;
        }

        if *fc == FunctionCode::InterruptAck {
            assert_eq!(
                *addr, 0x00FF_FFF7,
                "IACK must carry accepted level 3 on A3-A1"
            );
            assert_eq!(
                cpu.ipl, 0,
                "the live IPL request should be withdrawn before IACK"
            );
            *saw_iack = true;
            cpu.bus_status = match iack_response {
                IackResponse::Ready(vector) => BusStatus::Ready(vector),
                IackResponse::Error => BusStatus::Error,
            };
        } else if *is_read {
            let value = if *is_word {
                mem.read_word(*addr)
            } else {
                u16::from(mem.read_byte(*addr))
            };
            cpu.bus_status = BusStatus::Ready(value);
        } else {
            let value = data.unwrap_or(0);
            if *is_word {
                mem.write_word(*addr, value);
            } else {
                mem.write_byte(*addr, value as u8);
            }
            cpu.bus_status = BusStatus::Ready(0);
        }
    } else {
        cpu.bus_status = BusStatus::Wait;
    }
}

fn setup_interrupt_test(vector: u8, handler_pc: u32) -> (Cpu68010, TestMem) {
    let mut mem = TestMem::new(0x10000);

    mem.write_long(VBR + u32::from(vector) * 4, handler_pc);
    mem.write_word(handler_pc, 0x60FE); // BRA.S *

    mem.write_word(ENTRY_PC, 0x46FC); // MOVE.W #$2000,SR
    mem.write_word(ENTRY_PC + 2, 0x2000);
    mem.write_word(INTERRUPTED_PC, 0x60FE); // BRA.S *

    let mut cpu = Cpu68010::new();
    cpu.regs.vbr = VBR;
    cpu.reset_to(INITIAL_SSP, ENTRY_PC);

    let mut primed = false;
    let mut ignored_iack = false;
    for _ in 0..2_000 {
        cpu.ipl = 0;
        service_bus(
            &mut cpu,
            &mut mem,
            IackResponse::Ready(27),
            &mut ignored_iack,
        );
        cpu.tick();
        if cpu.regs.interrupt_mask() == 0 && cpu.ir == 0x60FE {
            primed = true;
            break;
        }
    }
    assert!(
        primed,
        "the test program must reach its interruptible branch loop"
    );

    (cpu, mem)
}

fn run_to_handler(
    cpu: &mut Cpu68010,
    mem: &mut TestMem,
    iack_response: IackResponse,
    handler_pc: u32,
) -> bool {
    let mut saw_iack = false;
    for _ in 0..10_000 {
        cpu.ipl = if cpu.target_ipl == 0 {
            INTERRUPT_LEVEL
        } else {
            0
        };
        service_bus(cpu, mem, iack_response, &mut saw_iack);
        cpu.tick();
        if cpu.instr_start_pc == handler_pc {
            return saw_iack;
        }
    }

    panic!("interrupt must select handler ${handler_pc:08X}");
}

fn assert_short_frame(cpu: &Cpu68010, mem: &TestMem, vector: u8) {
    assert_eq!(cpu.regs.active_sp(), FRAME_SP);
    assert_eq!(
        mem.read_word(FRAME_SP),
        0x2000,
        "the frame must retain the pre-interrupt SR"
    );
    assert_eq!(
        mem.read_long(FRAME_SP + 2),
        INTERRUPTED_PC,
        "the frame must retain the interrupted program counter"
    );
    assert_eq!(
        mem.read_word(FRAME_SP + 6),
        u16::from(vector) * 4,
        "the Format/Vector word must describe the acknowledged vector"
    );
}

fn assert_interrupt_entry(iack_response: IackResponse, vector: u8, handler_pc: u32) {
    let (mut cpu, mut mem) = setup_interrupt_test(vector, handler_pc);
    let saw_iack = run_to_handler(&mut cpu, &mut mem, iack_response, handler_pc);

    assert!(saw_iack, "the interrupt must perform an acknowledge cycle");
    assert_eq!(cpu.instr_start_pc, handler_pc);
    assert_eq!(
        cpu.regs.pc,
        handler_pc + 2,
        "PC must point past the promoted handler opcode"
    );
    assert_short_frame(&cpu, &mem, vector);
    assert_eq!(cpu.interrupts_taken, 1);
    assert_eq!(cpu.target_ipl, INTERRUPT_LEVEL);
    assert_eq!(
        cpu.regs.interrupt_mask(),
        INTERRUPT_LEVEL,
        "the active mask follows the accepted interrupt level, not the vector"
    );
}

#[test]
fn device_vector_64_selects_relocated_handler_and_is_saved_in_frame() {
    assert_interrupt_entry(IackResponse::Ready(0x40), 0x40, 0x2000);
}

#[test]
fn autovector_27_selects_relocated_handler_and_is_saved_in_frame() {
    assert_interrupt_entry(IackResponse::Ready(27), 27, 0x2200);
}

#[test]
fn iack_bus_error_selects_spurious_vector_and_saves_it_in_frame() {
    assert_interrupt_entry(IackResponse::Error, 24, 0x2400);
}

#[test]
fn device_vector_15_selects_uninitialized_interrupt_and_is_saved_in_frame() {
    assert_interrupt_entry(IackResponse::Ready(15), 15, 0x2600);
}

#[test]
fn serde_after_iack_preserves_the_pending_frame_and_handler_continuation() {
    let vector = 0x40;
    let handler_pc = 0x2800;
    let (mut cpu, mut mem) = setup_interrupt_test(vector, handler_pc);

    let mut completed_iack = false;
    for _ in 0..10_000 {
        cpu.ipl = if cpu.target_ipl == 0 {
            INTERRUPT_LEVEL
        } else {
            0
        };
        service_bus(
            &mut cpu,
            &mut mem,
            IackResponse::Ready(u16::from(vector)),
            &mut completed_iack,
        );
        cpu.tick();
        if completed_iack {
            break;
        }
    }

    assert!(
        completed_iack,
        "the test must complete interrupt acknowledge"
    );
    assert!(matches!(cpu.state, State::Idle));
    assert_eq!(cpu.followup_tag, TAG_EXC_IACK_COMPLETE);
    assert_eq!(cpu.micro_ops.front(), Some(MicroOp::Execute));
    assert_eq!(
        cpu.data,
        u32::from(vector),
        "the completed acknowledge response must be retained"
    );
    assert_eq!(
        cpu.regs.active_sp(),
        INITIAL_SSP,
        "the snapshot boundary must precede frame construction"
    );

    let encoded = rmp_serde::to_vec_named(&cpu).expect("serialize in-flight MC68010");
    let restored: Cpu68010 =
        rmp_serde::from_slice(&encoded).expect("deserialize in-flight MC68010");

    let mut uninterrupted = cpu;
    let mut uninterrupted_mem = mem.clone();
    let uninterrupted_iack = run_to_handler(
        &mut uninterrupted,
        &mut uninterrupted_mem,
        IackResponse::Ready(u16::from(vector)),
        handler_pc,
    );

    let mut resumed = restored;
    let mut resumed_mem = mem;
    let resumed_iack = run_to_handler(
        &mut resumed,
        &mut resumed_mem,
        IackResponse::Ready(u16::from(vector)),
        handler_pc,
    );

    assert!(
        !uninterrupted_iack && !resumed_iack,
        "continuation after completed IACK must not acknowledge a second time"
    );
    assert_eq!(uninterrupted.instr_start_pc, handler_pc);
    assert_eq!(resumed.instr_start_pc, handler_pc);
    assert_eq!(resumed.regs.pc, uninterrupted.regs.pc);
    assert_eq!(resumed.regs.sr, uninterrupted.regs.sr);
    assert_eq!(resumed.regs.active_sp(), uninterrupted.regs.active_sp());
    assert_short_frame(&uninterrupted, &uninterrupted_mem, vector);
    assert_short_frame(&resumed, &resumed_mem, vector);
}
