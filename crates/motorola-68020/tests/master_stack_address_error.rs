//! MC68020 master-stack alignment and Format-$A regressions.
//!
//! Data operands may begin at odd addresses on the MC68020, but instruction
//! words remain aligned. These tests keep that distinction explicit while
//! pinning exception-frame and RTE stack-bank behaviour.

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::{AddressErrorAccess, State};
use motorola_68020::Cpu68020;

const PC: u32 = 0x1000;
const HANDLER_PC: u32 = 0x1800;
const VBR: u32 = 0x2000;
const INITIAL_USP: u32 = 0x7000;
const INITIAL_ISP: u32 = 0x8000;
const INITIAL_MSP: u32 = 0x9000;
const ODD_DATA_ADDRESS: u32 = 0x5001;
const ODD_INSTRUCTION_PC: u32 = PC + 3;
const BRA_TO_ODD: u16 = 0x6001;
const UNLK_A0: u16 = 0x4E58;
const FORMAT_A_BYTES: u32 = 32;
const RETURN_PC: u32 = 0x3000;

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

    fn read_long(&self, addr: u32) -> u32 {
        (u32::from(self.read_word(addr)) << 16) | u32::from(self.read_word(addr.wrapping_add(2)))
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

fn service_bus(cpu: &mut Cpu68020, mem: &mut TestMem) {
    match &cpu.state {
        State::BusCycle {
            addr,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } if *cycle_count >= cpu.variant_min_bus_clocks => {
            if *is_read {
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

#[test]
fn odd_instruction_prefetch_builds_full_format_a_on_msp_and_preserves_isp() {
    let mut mem = TestMem::new();
    mem.write_long(VBR + 3 * 4, HANDLER_PC);
    mem.write_long(3 * 4, 0xDEAD_BEEF);
    mem.write_word(HANDLER_PC, 0x60FE); // BRA.S *
    mem.write_word(PC, BRA_TO_ODD);
    mem.write_word(PC + 2, 0x4E71); // NOP in IRC
    mem.write_word(PC + 4, 0x4E71); // next prefetch

    // Start the complete target frame extent non-zero so the two highest
    // zero-valued internal words cannot pass merely because RAM was clear.
    let frame_start = (INITIAL_MSP - FORMAT_A_BYTES) as usize;
    mem.bytes[frame_start..INITIAL_MSP as usize].fill(0x5A);

    // A distinct non-zero guard makes an accidental Format-$A write through
    // ISP visible even if the final ISP pointer were repaired later.
    let isp_guard_start = (INITIAL_ISP - 32) as usize;
    let isp_guard_end = INITIAL_ISP as usize;
    for (index, byte) in (0u8..32).zip(mem.bytes[isp_guard_start..isp_guard_end].iter_mut()) {
        *byte = 0x80 | index;
    }
    let expected_isp_guard = mem.bytes[isp_guard_start..isp_guard_end].to_vec();

    let mut cpu = Cpu68020::new();
    cpu.regs.usp = INITIAL_USP;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.msp = INITIAL_MSP;
    cpu.regs.sr = 0x3000;
    cpu.regs.vbr = VBR;
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(BRA_TO_ODD, 0x4E71);

    let mut observation = None;
    let mut reached_handler = false;
    for _ in 0..10_000 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        observation = observation.or_else(|| cpu.take_address_error_observation());
        if cpu.instr_start_pc == HANDLER_PC {
            reached_handler = true;
            break;
        }
    }
    assert!(reached_handler, "address error must select vector 3");

    let observation = observation.expect("odd instruction prefetch must report an address error");
    assert_eq!(observation.requested_address, ODD_INSTRUCTION_PC);
    assert_eq!(observation.frame_fault_address, ODD_INSTRUCTION_PC);
    assert_eq!(observation.access, AddressErrorAccess::Read);
    assert_eq!(observation.function_code, FunctionCode::SupervisorProgram);
    assert_eq!(observation.saved_sr, 0x3000);
    assert_eq!(observation.frame_ir, BRA_TO_ODD);

    let frame_sp = INITIAL_MSP - FORMAT_A_BYTES;
    assert_eq!(
        cpu.regs.msp, frame_sp,
        "the full Format-$A frame must occupy 32 bytes on MSP"
    );
    assert_eq!(
        cpu.regs.ssp, INITIAL_ISP,
        "master-mode frame construction must leave ISP untouched"
    );
    assert_eq!(cpu.regs.usp, INITIAL_USP);
    assert_eq!(
        cpu.regs.active_sp(),
        frame_sp,
        "the vector-3 handler must remain in supervisor master mode"
    );
    assert_eq!(
        &mem.bytes[isp_guard_start..isp_guard_end],
        expected_isp_guard.as_slice(),
        "Format-$A construction must not write through ISP"
    );

    assert_eq!(mem.read_word(frame_sp), 0x3000);
    assert_eq!(
        observation.frame_pc, ODD_INSTRUCTION_PC,
        "the short-frame PC must identify the rejected next instruction"
    );
    assert_eq!(mem.read_long(frame_sp + 2), ODD_INSTRUCTION_PC);
    assert_eq!(
        mem.read_word(frame_sp + 6),
        0xA00C,
        "vector 3 must use a Format-$A frame"
    );
    assert_eq!(
        mem.read_word(frame_sp + 12),
        BRA_TO_ODD,
        "pipe-stage C must identify the branch with the odd target"
    );
    assert_eq!(
        mem.read_long(frame_sp + 16),
        ODD_INSTRUCTION_PC,
        "Format-$A must retain the rejected odd instruction address"
    );
    assert_eq!(
        mem.read_word(frame_sp + 0x1C),
        0,
        "Format-$A internal word at offset $1C must occupy the frame"
    );
    assert_eq!(
        mem.read_word(frame_sp + 0x1E),
        0,
        "Format-$A internal word at offset $1E must occupy the frame"
    );
}

#[test]
fn odd_unlk_long_pop_succeeds_on_msp_without_address_error() {
    const POPPED_A0: u32 = 0x1234_5678;

    let mut mem = TestMem::new();
    mem.write_word(PC, UNLK_A0);
    mem.write_word(PC + 2, 0x4E71); // NOP in IRC
    mem.write_word(PC + 4, 0x4E71); // next prefetch
    mem.write_long(ODD_DATA_ADDRESS, POPPED_A0);

    let mut cpu = Cpu68020::new();
    cpu.regs.usp = INITIAL_USP;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.msp = INITIAL_MSP;
    cpu.regs.sr = 0x3000;
    cpu.regs.a[0] = ODD_DATA_ADDRESS;
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(UNLK_A0, 0x4E71);

    let start_count = cpu.instruction_starts;
    let mut saw_high_word = false;
    let mut saw_low_word = false;
    let mut completed = false;
    for _ in 0..2_000 {
        if let State::BusCycle { addr, is_read, .. } = &cpu.state
            && *is_read
        {
            saw_high_word |= *addr == ODD_DATA_ADDRESS;
            saw_low_word |= *addr == ODD_DATA_ADDRESS + 2;
        }

        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instruction_starts > start_count {
            completed = true;
            break;
        }
    }

    assert!(completed, "odd-address UNLK must complete normally");
    assert!(
        saw_high_word && saw_low_word,
        "UNLK must read both words of the odd-address long operand"
    );
    assert_eq!(
        cpu.take_address_error_observation(),
        None,
        "odd MC68020 data operands must not raise address error"
    );
    assert_eq!(cpu.regs.a[0], POPPED_A0);
    assert_eq!(
        cpu.regs.msp,
        ODD_DATA_ADDRESS + 4,
        "UNLK must leave MSP immediately above the popped long"
    );
    assert_eq!(cpu.regs.ssp, INITIAL_ISP);
    assert_eq!(cpu.regs.usp, INITIAL_USP);
    assert_eq!(cpu.regs.active_sp(), ODD_DATA_ADDRESS + 4);
}

#[test]
fn rte_consumes_full_format_a_frame_from_msp() {
    const SAVED_SR: u16 = 0x3015;

    let mut mem = TestMem::new();
    let frame_sp = INITIAL_MSP - FORMAT_A_BYTES;
    mem.write_word(PC, 0x4E73); // RTE
    mem.write_word(PC + 2, 0x4E71); // NOP in IRC
    mem.write_word(RETURN_PC, 0x60FE); // BRA.S *

    mem.write_word(frame_sp, SAVED_SR);
    mem.write_long(frame_sp + 2, RETURN_PC);
    mem.write_word(frame_sp + 6, 0xA00C);
    for (index, offset) in (0u16..12).zip((8u32..32).step_by(2)) {
        mem.write_word(offset + frame_sp, 0xA100 | index);
    }

    let mut cpu = Cpu68020::new();
    cpu.regs.usp = INITIAL_USP;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.msp = frame_sp;
    cpu.regs.sr = 0x3000;
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(0x4E73, 0x4E71);

    for _ in 0..10_000 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instr_start_pc == RETURN_PC {
            assert_eq!(cpu.regs.pc, RETURN_PC + 2);
            assert_eq!(cpu.regs.sr, SAVED_SR);
            assert_eq!(
                cpu.regs.msp, INITIAL_MSP,
                "RTE must advance MSP across all 16 Format-$A words"
            );
            assert_eq!(cpu.regs.ssp, INITIAL_ISP);
            assert_eq!(cpu.regs.usp, INITIAL_USP);
            assert_eq!(
                cpu.regs.active_sp(),
                INITIAL_MSP,
                "restored S=1 M=1 must leave MSP active"
            );
            return;
        }
    }

    panic!("RTE must restore SR/PC after consuming a complete Format-$A frame");
}
