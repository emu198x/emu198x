//! Shared integration-test support for later-family interrupt inheritance.

use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;

const ENTRY_PC: u32 = 0x1000;
const DEVICE_HANDLER_PC: u32 = 0x1200;
const AUTOVECTOR_HANDLER_PC: u32 = 0x1400;
const INITIAL_SP: u32 = 0x4000;
const VBR: u32 = 0x2000;
const INTERRUPT_LEVEL: u8 = 3;
const DEVICE_VECTOR: u8 = 64;
const AUTOVECTOR: u8 = 24 + INTERRUPT_LEVEL;

struct TestMem {
    bytes: Vec<u8>,
}

impl TestMem {
    fn new() -> Self {
        Self {
            bytes: vec![0; 0x10_000],
        }
    }

    fn read_byte(&self, addr: u32) -> u8 {
        self.bytes.get(addr as usize).copied().unwrap_or(0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        (u16::from(self.read_byte(addr)) << 8) | u16::from(self.read_byte(addr.wrapping_add(1)))
    }

    fn write_byte(&mut self, addr: u32, value: u8) {
        if let Some(byte) = self.bytes.get_mut(addr as usize) {
            *byte = value;
        }
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        self.write_byte(addr, (value >> 8) as u8);
        self.write_byte(addr.wrapping_add(1), value as u8);
    }

    fn write_long(&mut self, addr: u32, value: u32) {
        self.write_word(addr, (value >> 16) as u16);
        self.write_word(addr.wrapping_add(2), value as u16);
    }
}

fn service_bus(cpu: &mut Cpu68000, mem: &mut TestMem, supplied_vector: &mut bool) {
    match &cpu.state {
        State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } if *cycle_count >= cpu.variant_min_bus_clocks => {
            if *fc == FunctionCode::InterruptAck {
                cpu.bus_status = BusStatus::Ready(u16::from(DEVICE_VECTOR));
                *supplied_vector = true;
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
        }
        _ => cpu.bus_status = BusStatus::Wait,
    }
}

/// Prove that a later-family wrapper inherits device-vectored interrupt
/// selection and records the selected vector in its Format-$0 frame.
pub fn assert_device_vector_is_fetched_and_stacked(cpu: &mut Cpu68000) {
    let mut mem = TestMem::new();

    // Lower the reset interrupt mask, then wait for the interrupt.
    mem.write_word(ENTRY_PC, 0x46FC); // MOVE.W #$2000,SR
    mem.write_word(ENTRY_PC + 2, 0x2000);
    mem.write_word(ENTRY_PC + 4, 0x60FE); // BRA.S *

    // The device vector and level-3 autovector deliberately select
    // different handlers. Reaching D0=$40 therefore proves that the
    // acknowledge response, rather than the asserted level, selected
    // the handler.
    mem.write_long(VBR + u32::from(DEVICE_VECTOR) * 4, DEVICE_HANDLER_PC);
    mem.write_word(DEVICE_HANDLER_PC, 0x7040); // MOVEQ #64,D0
    mem.write_word(DEVICE_HANDLER_PC + 2, 0x60FE); // BRA.S *

    mem.write_long(VBR + u32::from(AUTOVECTOR) * 4, AUTOVECTOR_HANDLER_PC);
    mem.write_word(AUTOVECTOR_HANDLER_PC, 0x701B); // MOVEQ #27,D0
    mem.write_word(AUTOVECTOR_HANDLER_PC + 2, 0x60FE); // BRA.S *

    cpu.reset_to(INITIAL_SP, ENTRY_PC);
    cpu.regs.vbr = VBR;

    let mut supplied_vector = false;
    for _ in 0..10_000 {
        cpu.ipl = INTERRUPT_LEVEL;
        service_bus(cpu, &mut mem, &mut supplied_vector);
        cpu.tick();

        if cpu.regs.d[0] == u32::from(DEVICE_VECTOR) {
            break;
        }
    }

    assert!(
        supplied_vector,
        "the test must complete IACK with device vector 64"
    );
    assert_eq!(
        cpu.regs.d[0],
        u32::from(DEVICE_VECTOR),
        "the vector fetched through the non-zero VBR must select the device handler"
    );
    assert_eq!(cpu.interrupts_taken, 1);
    assert_eq!(
        cpu.regs.active_sp(),
        INITIAL_SP - 8,
        "the interrupt must create one eight-byte Format-$0 frame"
    );
    assert_eq!(
        mem.read_word(cpu.regs.active_sp() + 6),
        u16::from(DEVICE_VECTOR) * 4,
        "the stacked vector offset must describe the vector returned by IACK"
    );
}
