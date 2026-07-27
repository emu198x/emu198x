//! MC68020 dynamic bus-sizing integration regressions.
//!
//! These tests exercise complete logical long-word operands through the
//! processor's SIZ/DSACK state machine. Instruction fetches continue to use
//! the compatibility bus response, while data responders terminate each
//! physical phase as an 8-, 16-, or 32-bit port.

use motorola_68000::bus::{
    BusStatus, DataPortSize, TransferSize, dynamic_transfer_bytes, place_dynamic_read_data,
};
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const CODE_PC: u32 = 0x1000;
const DATA_BASE: u32 = 0x2000;
const INITIAL_ISP: u32 = 0x8000;
const MOVE_B_A0_D0: u16 = 0x1010;
const MOVE_L_A0_D0: u16 = 0x2010;
const MOVE_W_A0_D0: u16 = 0x3010;
const MOVE_B_D0_A0: u16 = 0x1080;
const MOVE_L_D0_A0: u16 = 0x2080;
const MOVE_W_D0_A0: u16 = 0x3080;
const NOP: u16 = 0x4E71;
const OPERAND: u32 = 0x1122_3344;

#[derive(Clone, PartialEq, Eq)]
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
        (u16::from(self.read_byte(addr)) << 8) | u16::from(self.read_byte(addr + 1))
    }

    fn read_packed(&self, addr: u32, count: u8) -> u32 {
        let mut value = 0;
        for offset in 0..count {
            value = (value << 8) | u32::from(self.read_byte(addr + u32::from(offset)));
        }
        value
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

    fn write_packed(&mut self, addr: u32, count: u8, value: u32) {
        for index in 0..count {
            let shift = u32::from(count - index - 1) * 8;
            self.write_byte(addr + u32::from(index), (value >> shift) as u8);
        }
    }

    fn set_operand(&mut self, addr: u32, operand: u32) {
        self.write_packed(addr, 4, operand);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Phase {
    address: u32,
    remaining: TransferSize,
    transferred: u8,
    port: DataPortSize,
    bus_data: u32,
}

type PhaseShape = (u32, TransferSize, u8, DataPortSize);

fn setup_cpu(opcode: u16, operand_address: u32, mem: &mut TestMem) -> Cpu68020 {
    mem.write_word(CODE_PC, opcode);
    mem.write_word(CODE_PC + 2, NOP);
    mem.write_word(CODE_PC + 4, NOP);
    mem.write_word(CODE_PC + 6, NOP);

    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2000;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.pc = CODE_PC + 4;
    cpu.regs.a[0] = operand_address;
    cpu.setup_prefetch(opcode, NOP);
    cpu
}

fn ready_cycle(cpu: &Cpu68020) -> Option<(u32, bool)> {
    let State::BusCycle {
        addr,
        is_read,
        cycle_count,
        ..
    } = &cpu.state
    else {
        return None;
    };

    (*cycle_count >= cpu.variant_min_bus_clocks).then_some((*addr, *is_read))
}

fn service_read(cpu: &mut Cpu68020, mem: &TestMem, port: DataPortSize, phases: &mut Vec<Phase>) {
    let Some((address, is_read)) = ready_cycle(cpu) else {
        cpu.bus_status = BusStatus::Wait;
        return;
    };

    if cpu.active_bus_transfer.is_none() {
        assert!(is_read, "only program reads use the compatibility path");
        cpu.bus_status = BusStatus::Ready(mem.read_word(address));
        return;
    }

    assert!(is_read, "read harness received a dynamic write cycle");
    let remaining = cpu.bus_transfer_size;
    let transferred = dynamic_transfer_bytes(remaining, address, port);
    let value = mem.read_packed(address, transferred);
    let bus_data = place_dynamic_read_data(value, transferred, address, port);
    phases.push(Phase {
        address,
        remaining,
        transferred,
        port,
        bus_data,
    });
    cpu.bus_status = BusStatus::ReadySized {
        data: bus_data,
        port,
    };
}

fn extract_port_data(bus_data: u32, transferred: u8, address: u32, port: DataPortSize) -> u32 {
    let start_lane = (address as u8) & (port.bytes() - 1);
    let mut value = 0;
    for index in 0..transferred {
        let source_lane = start_lane + index;
        let source_shift = 8 * (3 - source_lane);
        value = (value << 8) | ((bus_data >> source_shift) & 0xFF);
    }
    value
}

fn service_write(
    cpu: &mut Cpu68020,
    mem: &mut TestMem,
    port: DataPortSize,
    phases: &mut Vec<Phase>,
) {
    let Some((address, is_read)) = ready_cycle(cpu) else {
        cpu.bus_status = BusStatus::Wait;
        return;
    };

    if cpu.active_bus_transfer.is_none() {
        assert!(is_read, "only program reads use the compatibility path");
        cpu.bus_status = BusStatus::Ready(mem.read_word(address));
        return;
    }

    assert!(!is_read, "write harness received a dynamic read cycle");
    let remaining = cpu.bus_transfer_size;
    let transferred = dynamic_transfer_bytes(remaining, address, port);
    let bus_data = cpu.bus_data_out;
    let value = extract_port_data(bus_data, transferred, address, port);
    mem.write_packed(address, transferred, value);
    phases.push(Phase {
        address,
        remaining,
        transferred,
        port,
        bus_data,
    });
    cpu.bus_status = BusStatus::ReadySized { data: 0, port };
}

fn run_until_next_instruction(cpu: &mut Cpu68020, mut service_bus: impl FnMut(&mut Cpu68020)) {
    let initial_instruction_count = cpu.instruction_starts;
    for _ in 0..2_000 {
        cpu.ipl = 0;
        service_bus(cpu);
        cpu.tick();
        if cpu.instruction_starts > initial_instruction_count {
            return;
        }
    }

    panic!("data instruction did not complete");
}

fn expected_fixed_port_phases(port: DataPortSize, alignment: u32) -> Vec<PhaseShape> {
    use DataPortSize::{Byte as BytePort, Long as LongPort, Word as WordPort};
    use TransferSize::{Byte, Long, ThreeBytes, Word};

    let phases = match (port, alignment) {
        (LongPort, 0) => vec![(0, Long, 4)],
        (LongPort, 1) => vec![(0, Long, 3), (3, Byte, 1)],
        (LongPort, 2) => vec![(0, Long, 2), (2, Word, 2)],
        (LongPort, 3) => vec![(0, Long, 1), (1, ThreeBytes, 3)],
        (WordPort, 0 | 2) => vec![(0, Long, 2), (2, Word, 2)],
        (WordPort, 1 | 3) => vec![(0, Long, 1), (1, ThreeBytes, 2), (3, Byte, 1)],
        (BytePort, 0..=3) => vec![(0, Long, 1), (1, ThreeBytes, 1), (2, Word, 1), (3, Byte, 1)],
        _ => unreachable!("alignment is constrained to A1:A0"),
    };

    phases
        .into_iter()
        .map(|(offset, remaining, transferred)| (offset, remaining, transferred, port))
        .collect()
}

fn phase_shapes(phases: &[Phase], operand_address: u32) -> Vec<PhaseShape> {
    phases
        .iter()
        .map(|phase| {
            (
                phase.address - operand_address,
                phase.remaining,
                phase.transferred,
                phase.port,
            )
        })
        .collect()
}

#[test]
fn byte_operands_complete_through_ready_sized() {
    let operand_address = DATA_BASE + 2;
    let mut read_mem = TestMem::new();
    read_mem.write_byte(operand_address, 0x7E);
    let mut read_cpu = setup_cpu(MOVE_B_A0_D0, operand_address, &mut read_mem);
    read_cpu.regs.d[0] = 0xCAFE_BABE;
    let mut read_phases = Vec::new();

    run_until_next_instruction(&mut read_cpu, |cpu| {
        service_read(cpu, &read_mem, DataPortSize::Long, &mut read_phases);
    });

    assert_eq!(read_cpu.regs.d[0], 0xCAFE_BA7E);
    assert_eq!(
        phase_shapes(&read_phases, operand_address),
        vec![(0, TransferSize::Byte, 1, DataPortSize::Long)]
    );

    let mut write_mem = TestMem::new();
    write_mem.write_byte(operand_address - 1, 0xA5);
    write_mem.write_byte(operand_address, 0xCC);
    write_mem.write_byte(operand_address + 1, 0x5A);
    let mut write_cpu = setup_cpu(MOVE_B_D0_A0, operand_address, &mut write_mem);
    write_cpu.regs.d[0] = 0x1234_567E;
    let mut write_phases = Vec::new();

    run_until_next_instruction(&mut write_cpu, |cpu| {
        service_write(cpu, &mut write_mem, DataPortSize::Long, &mut write_phases);
    });

    assert_eq!(
        phase_shapes(&write_phases, operand_address),
        vec![(0, TransferSize::Byte, 1, DataPortSize::Long)]
    );
    assert_eq!(
        [
            write_mem.read_byte(operand_address - 1),
            write_mem.read_byte(operand_address),
            write_mem.read_byte(operand_address + 1),
        ],
        [0xA5, 0x7E, 0x5A]
    );
}

#[test]
fn odd_word_operands_complete_in_one_long_port_ready_sized_phase() {
    let operand_address = DATA_BASE + 1;
    let mut read_mem = TestMem::new();
    read_mem.write_word(operand_address, 0xBEEF);
    let mut read_cpu = setup_cpu(MOVE_W_A0_D0, operand_address, &mut read_mem);
    read_cpu.regs.d[0] = 0xCAFE_BABE;
    let mut read_phases = Vec::new();

    run_until_next_instruction(&mut read_cpu, |cpu| {
        service_read(cpu, &read_mem, DataPortSize::Long, &mut read_phases);
    });

    assert_eq!(read_cpu.regs.d[0], 0xCAFE_BEEF);
    assert_eq!(
        phase_shapes(&read_phases, operand_address),
        vec![(0, TransferSize::Word, 2, DataPortSize::Long)]
    );

    let mut write_mem = TestMem::new();
    write_mem.write_byte(operand_address - 1, 0xA5);
    write_mem.write_word(operand_address, 0xCCCC);
    write_mem.write_byte(operand_address + 2, 0x5A);
    let mut write_cpu = setup_cpu(MOVE_W_D0_A0, operand_address, &mut write_mem);
    write_cpu.regs.d[0] = 0x1234_BEEF;
    let mut write_phases = Vec::new();

    run_until_next_instruction(&mut write_cpu, |cpu| {
        service_write(cpu, &mut write_mem, DataPortSize::Long, &mut write_phases);
    });

    assert_eq!(
        phase_shapes(&write_phases, operand_address),
        vec![(0, TransferSize::Word, 2, DataPortSize::Long)]
    );
    assert_eq!(
        [
            write_mem.read_byte(operand_address - 1),
            write_mem.read_byte(operand_address),
            write_mem.read_byte(operand_address + 1),
            write_mem.read_byte(operand_address + 2),
        ],
        [0xA5, 0xBE, 0xEF, 0x5A]
    );
}

#[test]
fn long_reads_follow_the_exact_fixed_port_phase_matrix() {
    for port in [DataPortSize::Long, DataPortSize::Word, DataPortSize::Byte] {
        for alignment in 0..4 {
            let operand_address = DATA_BASE + alignment;
            let mut mem = TestMem::new();
            mem.set_operand(operand_address, OPERAND);
            let mut cpu = setup_cpu(MOVE_L_A0_D0, operand_address, &mut mem);
            cpu.regs.d[0] = 0xA5A5_5A5A;
            let mut phases = Vec::new();

            run_until_next_instruction(&mut cpu, |cpu| {
                service_read(cpu, &mem, port, &mut phases);
            });

            assert_eq!(
                cpu.regs.d[0], OPERAND,
                "read value differs for {port:?} port at A1:A0={alignment:02b}"
            );
            assert_eq!(
                phase_shapes(&phases, operand_address),
                expected_fixed_port_phases(port, alignment),
                "physical read phases differ for {port:?} port at A1:A0={alignment:02b}"
            );
        }
    }
}

fn expected_write_bus_image(remaining: TransferSize, address: u32) -> u32 {
    match remaining {
        TransferSize::Byte => 0x4444_4444,
        TransferSize::Word if address & 1 == 0 => 0x3344_3344,
        TransferSize::Word => 0x3333_4433,
        TransferSize::ThreeBytes => {
            [0x2233_4411, 0x2222_3344, 0x2233_2233, 0x2222_3322][(address & 3) as usize]
        }
        TransferSize::Long => {
            [0x1122_3344, 0x1111_2233, 0x1122_1122, 0x1111_2211][(address & 3) as usize]
        }
    }
}

#[test]
fn long_writes_duplicate_lanes_without_touching_guard_bytes() {
    for port in [DataPortSize::Long, DataPortSize::Word, DataPortSize::Byte] {
        for alignment in 0..4 {
            let operand_address = DATA_BASE + alignment;
            let mut mem = TestMem::new();
            mem.write_byte(operand_address - 1, 0xA5);
            mem.write_packed(operand_address, 4, 0xCCCC_CCCC);
            mem.write_byte(operand_address + 4, 0x5A);
            let mut cpu = setup_cpu(MOVE_L_D0_A0, operand_address, &mut mem);
            cpu.regs.d[0] = OPERAND;
            let mut phases = Vec::new();

            run_until_next_instruction(&mut cpu, |cpu| {
                service_write(cpu, &mut mem, port, &mut phases);
            });

            assert_eq!(
                phase_shapes(&phases, operand_address),
                expected_fixed_port_phases(port, alignment),
                "physical write phases differ for {port:?} port at A1:A0={alignment:02b}"
            );
            for phase in &phases {
                assert_eq!(
                    phase.bus_data,
                    expected_write_bus_image(phase.remaining, phase.address),
                    "D31-D0 write image differs for {port:?} port, SIZ={:?}, A1:A0={:02b}",
                    phase.remaining,
                    phase.address & 3
                );
            }
            assert_eq!(
                mem.read_packed(operand_address, 4),
                OPERAND,
                "stored value differs for {port:?} port at A1:A0={alignment:02b}"
            );
            assert_eq!(
                mem.read_byte(operand_address - 1),
                0xA5,
                "leading guard byte changed for {port:?} port at A1:A0={alignment:02b}"
            );
            assert_eq!(
                mem.read_byte(operand_address + 4),
                0x5A,
                "trailing guard byte changed for {port:?} port at A1:A0={alignment:02b}"
            );
        }
    }
}

fn encoded(cpu: &Cpu68020) -> Vec<u8> {
    rmp_serde::to_vec_named(cpu).expect("serialize in-flight MC68020 transfer")
}

#[test]
fn mixed_width_read_continues_identically_after_mid_transfer_serde() {
    let operand_address = DATA_BASE + 1;
    let mut mem = TestMem::new();
    mem.set_operand(operand_address, OPERAND);
    let mut uninterrupted = setup_cpu(MOVE_L_A0_D0, operand_address, &mut mem);
    uninterrupted.regs.d[0] = 0xA5A5_5A5A;
    let initial_instruction_count = uninterrupted.instruction_starts;
    let ports = [DataPortSize::Byte, DataPortSize::Long, DataPortSize::Word];
    let mut uninterrupted_phases = Vec::new();

    for _ in 0..2_000 {
        uninterrupted.ipl = 0;
        service_read(
            &mut uninterrupted,
            &mem,
            ports[uninterrupted_phases.len()],
            &mut uninterrupted_phases,
        );
        uninterrupted.tick();
        if uninterrupted_phases.len() == 1
            && uninterrupted
                .active_bus_transfer
                .is_some_and(|transfer| transfer.remaining == TransferSize::ThreeBytes)
        {
            break;
        }
    }

    assert_eq!(
        uninterrupted_phases.len(),
        1,
        "first byte phase must finish"
    );
    assert_eq!(uninterrupted.bus_transfer_size, TransferSize::ThreeBytes);
    assert_eq!(
        uninterrupted
            .active_bus_transfer
            .expect("logical transfer remains active")
            .read_data,
        0x11,
        "the accepted prefix must survive the snapshot"
    );
    let State::BusCycle {
        addr, cycle_count, ..
    } = &uninterrupted.state
    else {
        panic!("next physical phase must already be active");
    };
    assert_eq!(*addr, operand_address + 1);
    assert_eq!(*cycle_count, 0);

    let mut resumed: Cpu68020 =
        rmp_serde::from_slice(&encoded(&uninterrupted)).expect("restore in-flight MC68020");
    assert!(
        resumed.variant_dynamic_bus_sizing,
        "deserialization must reinstall the MC68020 bus capability"
    );
    assert_eq!(encoded(&resumed), encoded(&uninterrupted));
    let mut resumed_phases = uninterrupted_phases.clone();

    for tick_index in 0..2_000 {
        uninterrupted.ipl = 0;
        resumed.ipl = 0;
        let uninterrupted_port = ports
            .get(uninterrupted_phases.len())
            .copied()
            .unwrap_or(DataPortSize::Long);
        let resumed_port = ports
            .get(resumed_phases.len())
            .copied()
            .unwrap_or(DataPortSize::Long);
        service_read(
            &mut uninterrupted,
            &mem,
            uninterrupted_port,
            &mut uninterrupted_phases,
        );
        service_read(&mut resumed, &mem, resumed_port, &mut resumed_phases);
        uninterrupted.tick();
        resumed.tick();

        assert_eq!(
            encoded(&resumed),
            encoded(&uninterrupted),
            "CPU continuations diverged at tick {tick_index}"
        );
        assert_eq!(
            resumed_phases, uninterrupted_phases,
            "bus phases diverged at tick {tick_index}"
        );

        if uninterrupted.instruction_starts > initial_instruction_count {
            assert!(
                resumed.instruction_starts > initial_instruction_count,
                "restored processor did not reach the same instruction boundary"
            );
            break;
        }

        assert!(
            tick_index < 1_999,
            "restored data instruction did not complete"
        );
    }

    assert_eq!(uninterrupted.regs.d[0], OPERAND);
    assert_eq!(resumed.regs.d[0], OPERAND);
    assert_eq!(
        phase_shapes(&uninterrupted_phases, operand_address),
        vec![
            (0, TransferSize::Long, 1, DataPortSize::Byte),
            (1, TransferSize::ThreeBytes, 2, DataPortSize::Long),
            (3, TransferSize::Byte, 1, DataPortSize::Word),
        ]
    );
}
