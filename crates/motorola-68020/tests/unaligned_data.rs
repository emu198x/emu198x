//! MC68020 unaligned logical-data access regressions.
//!
//! Word and long data operands may begin at odd byte addresses on the
//! MC68020. Instruction fetches remain word-aligned and must still take
//! vector 3 when control flow selects an odd address.

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::{AddressErrorAccess, State};
use motorola_68020::Cpu68020;

const CODE_PC: u32 = 0x1000;
const ADDRESS_ERROR_HANDLER: u32 = 0x1800;
const VBR: u32 = 0x4000;
const ODD_DATA: u32 = 0x2001;
const ODD_CODE: u32 = 0x3001;
const INITIAL_ISP: u32 = 0x8000;
const NOP: u16 = 0x4E71;
const MOVE_W_A0_D0: u16 = 0x3010;
const MOVE_L_A0_D0: u16 = 0x2010;
const MOVE_W_D0_A0: u16 = 0x3080;
const MOVE_L_D0_A0: u16 = 0x2080;
const JMP_A0: u16 = 0x4ED0;

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

fn setup_cpu(opcode: u16, mem: &mut TestMem) -> Cpu68020 {
    mem.write_long(VBR + 3 * 4, ADDRESS_ERROR_HANDLER);
    mem.write_long(3 * 4, 0xDEAD_BEEF);
    mem.write_word(CODE_PC, opcode);
    mem.write_word(CODE_PC + 2, NOP);
    mem.write_word(CODE_PC + 4, NOP);
    mem.write_word(ADDRESS_ERROR_HANDLER, 0x60FE); // BRA.S *

    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2000;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.vbr = VBR;
    cpu.regs.pc = CODE_PC + 4;
    cpu.setup_prefetch(opcode, NOP);
    cpu
}

fn finish_data_instruction(cpu: &mut Cpu68020, mem: &mut TestMem) {
    let initial_instruction_count = cpu.instruction_starts;
    for _ in 0..2_000 {
        cpu.ipl = 0;
        service_bus(cpu, mem);
        cpu.tick();

        assert_ne!(
            cpu.instr_start_pc, ADDRESS_ERROR_HANDLER,
            "an unaligned data operand must not take vector 3"
        );
        if cpu.instruction_starts > initial_instruction_count {
            assert!(
                cpu.take_address_error_observation().is_none(),
                "an unaligned data operand must not report an address error"
            );
            return;
        }
    }

    panic!("data instruction did not complete");
}

#[test]
fn odd_move_word_read_assembles_big_endian_bytes_without_vector_three() {
    let mut mem = TestMem::new();
    mem.write_byte(ODD_DATA - 1, 0xA5);
    mem.write_byte(ODD_DATA, 0x12);
    mem.write_byte(ODD_DATA + 1, 0x34);
    mem.write_byte(ODD_DATA + 2, 0x5A);

    let mut cpu = setup_cpu(MOVE_W_A0_D0, &mut mem);
    cpu.regs.a[0] = ODD_DATA;
    cpu.regs.d[0] = 0xCAFE_BABE;
    cpu.regs.d[1] = 0x0BAD_F00D;

    finish_data_instruction(&mut cpu, &mut mem);

    assert_eq!(
        cpu.regs.d[0], 0xCAFE_1234,
        "MOVE.W must replace only the low destination word"
    );
    assert_eq!(cpu.regs.d[1], 0x0BAD_F00D);
    assert_eq!(
        [
            mem.read_byte(ODD_DATA - 1),
            mem.read_byte(ODD_DATA),
            mem.read_byte(ODD_DATA + 1),
            mem.read_byte(ODD_DATA + 2),
        ],
        [0xA5, 0x12, 0x34, 0x5A],
        "a word read must not alter its payload or guard bytes"
    );
}

#[test]
fn odd_move_long_read_assembles_big_endian_bytes_without_vector_three() {
    let mut mem = TestMem::new();
    mem.write_byte(ODD_DATA - 1, 0xA5);
    for (offset, value) in [0xDE, 0xAD, 0xBE, 0xEF].into_iter().enumerate() {
        mem.write_byte(
            ODD_DATA + u32::try_from(offset).expect("offset fits"),
            value,
        );
    }
    mem.write_byte(ODD_DATA + 4, 0x5A);

    let mut cpu = setup_cpu(MOVE_L_A0_D0, &mut mem);
    cpu.regs.a[0] = ODD_DATA;
    cpu.regs.d[0] = 0xCAFE_BABE;
    cpu.regs.d[1] = 0x0BAD_F00D;

    finish_data_instruction(&mut cpu, &mut mem);

    assert_eq!(cpu.regs.d[0], 0xDEAD_BEEF);
    assert_eq!(cpu.regs.d[1], 0x0BAD_F00D);
    assert_eq!(
        [
            mem.read_byte(ODD_DATA - 1),
            mem.read_byte(ODD_DATA),
            mem.read_byte(ODD_DATA + 1),
            mem.read_byte(ODD_DATA + 2),
            mem.read_byte(ODD_DATA + 3),
            mem.read_byte(ODD_DATA + 4),
        ],
        [0xA5, 0xDE, 0xAD, 0xBE, 0xEF, 0x5A],
        "a long read must not alter its payload or guard bytes"
    );
}

#[test]
fn odd_move_word_write_emits_big_endian_bytes_without_vector_three() {
    let mut mem = TestMem::new();
    mem.write_byte(ODD_DATA - 1, 0xA5);
    mem.write_byte(ODD_DATA, 0xCC);
    mem.write_byte(ODD_DATA + 1, 0xDD);
    mem.write_byte(ODD_DATA + 2, 0x5A);

    let mut cpu = setup_cpu(MOVE_W_D0_A0, &mut mem);
    cpu.regs.a[0] = ODD_DATA;
    cpu.regs.d[0] = 0xABCD_1234;
    cpu.regs.d[1] = 0x0BAD_F00D;

    finish_data_instruction(&mut cpu, &mut mem);

    assert_eq!(
        [
            mem.read_byte(ODD_DATA - 1),
            mem.read_byte(ODD_DATA),
            mem.read_byte(ODD_DATA + 1),
            mem.read_byte(ODD_DATA + 2),
        ],
        [0xA5, 0x12, 0x34, 0x5A],
        "MOVE.W must replace exactly two bytes and preserve both guards"
    );
    assert_eq!(cpu.regs.d[0], 0xABCD_1234);
    assert_eq!(cpu.regs.d[1], 0x0BAD_F00D);
}

#[test]
fn odd_move_long_write_emits_big_endian_bytes_without_vector_three() {
    let mut mem = TestMem::new();
    mem.write_byte(ODD_DATA - 1, 0xA5);
    for offset in 0..4 {
        mem.write_byte(ODD_DATA + offset, 0xCC);
    }
    mem.write_byte(ODD_DATA + 4, 0x5A);

    let mut cpu = setup_cpu(MOVE_L_D0_A0, &mut mem);
    cpu.regs.a[0] = ODD_DATA;
    cpu.regs.d[0] = 0xDEAD_BEEF;
    cpu.regs.d[1] = 0x0BAD_F00D;

    finish_data_instruction(&mut cpu, &mut mem);

    assert_eq!(
        [
            mem.read_byte(ODD_DATA - 1),
            mem.read_byte(ODD_DATA),
            mem.read_byte(ODD_DATA + 1),
            mem.read_byte(ODD_DATA + 2),
            mem.read_byte(ODD_DATA + 3),
            mem.read_byte(ODD_DATA + 4),
        ],
        [0xA5, 0xDE, 0xAD, 0xBE, 0xEF, 0x5A],
        "MOVE.L must replace exactly four bytes and preserve both guards"
    );
    assert_eq!(cpu.regs.d[0], 0xDEAD_BEEF);
    assert_eq!(cpu.regs.d[1], 0x0BAD_F00D);
}

#[test]
fn odd_jmp_target_still_takes_vector_three_on_instruction_fetch() {
    let mut mem = TestMem::new();
    let mut cpu = setup_cpu(JMP_A0, &mut mem);
    cpu.regs.a[0] = ODD_CODE;

    let mut reached_handler = false;
    for _ in 0..10_000 {
        cpu.ipl = 0;
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.instr_start_pc == ADDRESS_ERROR_HANDLER {
            reached_handler = true;
            break;
        }
    }
    assert!(
        reached_handler,
        "an odd instruction-fetch target must select vector 3"
    );

    let observation = cpu
        .take_address_error_observation()
        .expect("an odd instruction-fetch target must report an address error");
    assert_eq!(observation.requested_address, ODD_CODE);
    assert_eq!(observation.frame_fault_address, ODD_CODE);
    assert_eq!(observation.access, AddressErrorAccess::Read);
    assert_eq!(observation.function_code, FunctionCode::SupervisorProgram);
    assert_eq!(observation.frame_ir, JMP_A0);
    assert_eq!(
        observation.frame_pc, ODD_CODE,
        "the Format-$A PC must identify the rejected next instruction"
    );
    assert_eq!(
        mem.read_long(INITIAL_ISP - 32 + 2),
        ODD_CODE,
        "the independently decoded stack frame must contain the next PC"
    );
}
