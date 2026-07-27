//! Save-state continuation regressions for MC68020 master-mode interrupts.
//!
//! The interrupt-entry and RTE sequencers carry stack-selection state that is
//! not reconstructible from the live SR alone. These tests snapshot at the two
//! boundaries where losing that state would redirect the continuation to the
//! wrong supervisor stack.

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::{State, TAG_RTE_READ_SR};
use motorola_68020::Cpu68020;

const ENTRY_PC: u32 = 0x1000;
const HANDLER_PC: u32 = 0x2000;
const INITIAL_ISP: u32 = 0x8000;
const INITIAL_MSP: u32 = 0x9000;
const FRAME_BYTES: u32 = 8;
const INTERRUPT_LEVEL: u8 = 3;
const INTERRUPT_VECTOR: u8 = 0x40;
const MASTER_SR: u16 = 0x3000;

#[derive(Clone, PartialEq, Eq)]
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

fn service_bus(cpu: &mut Cpu68020, mem: &mut TestMem) {
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
            cpu.bus_status = BusStatus::Ready(u16::from(INTERRUPT_VECTOR));
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

fn tick(cpu: &mut Cpu68020, mem: &mut TestMem, request_interrupt: bool) {
    cpu.ipl = if request_interrupt && cpu.target_ipl == 0 {
        INTERRUPT_LEVEL
    } else {
        0
    };
    service_bus(cpu, mem);
    cpu.tick();
}

fn setup_master_mode_loop() -> (Cpu68020, TestMem) {
    let mut mem = TestMem::new(0x10000);
    mem.write_long(u32::from(INTERRUPT_VECTOR) * 4, HANDLER_PC);
    mem.write_word(ENTRY_PC, 0x60FE); // BRA.S *
    mem.write_word(HANDLER_PC, 0x4E73); // RTE
    mem.write_word(HANDLER_PC + 2, 0x60FE); // BRA.S * if RTE fails

    let mut cpu = Cpu68020::new();
    cpu.reset_to(INITIAL_ISP, ENTRY_PC);

    let mut primed = false;
    for _ in 0..2_000 {
        tick(&mut cpu, &mut mem, false);
        if cpu.instr_start_pc == ENTRY_PC && cpu.ir == 0x60FE {
            primed = true;
            break;
        }
    }
    assert!(primed, "the branch loop must be ready for interruption");

    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.msp = INITIAL_MSP;
    cpu.regs.sr = MASTER_SR;
    assert_eq!(
        cpu.regs.active_sp(),
        INITIAL_MSP,
        "MC68020 supervisor master mode must select MSP"
    );

    (cpu, mem)
}

fn encoded(cpu: &Cpu68020) -> Vec<u8> {
    rmp_serde::to_vec_named(cpu).expect("serialize in-flight MC68020")
}

fn restored(cpu: &Cpu68020) -> Cpu68020 {
    rmp_serde::from_slice(&encoded(cpu)).expect("deserialize in-flight MC68020")
}

fn assert_equivalent_continuation(
    mut uninterrupted: Cpu68020,
    mut uninterrupted_mem: TestMem,
    mut resumed: Cpu68020,
    mut resumed_mem: TestMem,
    mut is_complete: impl FnMut(&Cpu68020) -> bool,
) {
    for tick_index in 0..20_000 {
        tick(&mut uninterrupted, &mut uninterrupted_mem, false);
        tick(&mut resumed, &mut resumed_mem, false);

        assert_eq!(
            encoded(&resumed),
            encoded(&uninterrupted),
            "CPU continuations diverged at tick {tick_index}"
        );
        assert!(
            resumed_mem == uninterrupted_mem,
            "memory continuations diverged at tick {tick_index}"
        );

        if is_complete(&uninterrupted) {
            assert!(
                is_complete(&resumed),
                "restored continuation did not reach the same terminal boundary"
            );
            return;
        }
    }

    panic!("continuations did not reach the expected terminal boundary");
}

#[test]
fn serde_mid_master_interrupt_entry_preserves_the_pending_isp_frame() {
    let (mut cpu, mut mem) = setup_master_mode_loop();
    let master_frame = INITIAL_MSP - FRAME_BYTES;

    let mut reached_snapshot_boundary = false;
    for _ in 0..20_000 {
        tick(&mut cpu, &mut mem, true);
        if cpu.regs.msp == master_frame
            && cpu.regs.ssp > INITIAL_ISP - FRAME_BYTES
            && cpu.target_ipl == INTERRUPT_LEVEL
        {
            reached_snapshot_boundary = true;
            break;
        }
    }
    assert!(
        reached_snapshot_boundary,
        "snapshot must land after the MSP frame and before the ISP frame completes"
    );

    let resumed = restored(&cpu);
    let resumed_mem = mem.clone();
    assert_equivalent_continuation(cpu, mem, resumed, resumed_mem, |candidate| {
        candidate.instr_start_pc == HANDLER_PC
    });
}

#[test]
fn serde_at_format_one_rte_restart_preserves_the_master_stack_selection() {
    let (mut cpu, mut mem) = setup_master_mode_loop();

    let mut reached_handler = false;
    for _ in 0..20_000 {
        tick(&mut cpu, &mut mem, true);
        if cpu.instr_start_pc == HANDLER_PC {
            reached_handler = true;
            break;
        }
    }
    assert!(reached_handler, "the interrupt handler must begin");

    let master_frame = INITIAL_MSP - FRAME_BYTES;
    let interrupted_pc = mem.read_long(master_frame + 2);
    let mut reached_snapshot_boundary = false;
    for _ in 0..20_000 {
        tick(&mut cpu, &mut mem, false);
        if cpu.followup_tag == TAG_RTE_READ_SR
            && cpu.regs.ssp == INITIAL_ISP
            && cpu.regs.msp == master_frame
            && cpu.regs.sr & 0x3000 == MASTER_SR
        {
            reached_snapshot_boundary = true;
            break;
        }
    }
    assert!(
        reached_snapshot_boundary,
        "snapshot must land when Format-$1 restarts RTE on the MSP frame"
    );

    let resumed = restored(&cpu);
    let resumed_mem = mem.clone();
    assert_equivalent_continuation(cpu, mem, resumed, resumed_mem, |candidate| {
        candidate.instr_start_pc == interrupted_pc
            && candidate.regs.sr == MASTER_SR
            && candidate.regs.msp == INITIAL_MSP
            && candidate.regs.ssp == INITIAL_ISP
    });
}
