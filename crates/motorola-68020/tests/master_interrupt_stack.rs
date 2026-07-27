//! MC68020 supervisor-stack and master-mode interrupt regressions.
//!
//! The MC68020 selects between the interrupt stack pointer (ISP) and
//! master stack pointer (MSP) with the status register's M bit. An
//! interrupt accepted while M is set saves the real Format-$0 frame on
//! the MSP, then clears M and saves a Format-$1 throwaway frame on the
//! ISP. `RTE` consumes both frames in the reverse order.

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const ENTRY_PC: u32 = 0x1000;
const INTERRUPTED_PC: u32 = ENTRY_PC + 4;
const HANDLER_PC: u32 = 0x1800;
const VBR: u32 = 0x2000;
const INITIAL_USP: u32 = 0x7000;
const INITIAL_ISP: u32 = 0x8000;
const INITIAL_MSP: u32 = 0x9000;
const INTERRUPT_LEVEL: u8 = 3;
const DEVICE_VECTOR: u8 = 64;
const FORMAT_ZERO_VECTOR_WORD: u16 = 0x0100;
const FORMAT_ONE_VECTOR_WORD: u16 = 0x1000 | FORMAT_ZERO_VECTOR_WORD;
const POISON_PC: u32 = 0x3000;
const USER_FRAME_SP: u32 = 0x7000;

#[derive(Clone)]
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

fn service_bus(
    cpu: &mut Cpu68020,
    mem: &mut TestMem,
    supply_interrupt_vector: bool,
    acknowledged: &mut bool,
) {
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
                assert!(
                    supply_interrupt_vector,
                    "the CPU must acknowledge the interrupt only once"
                );
                cpu.bus_status = BusStatus::Ready(u16::from(DEVICE_VECTOR));
                *acknowledged = true;
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

fn setup_interrupt_test(saved_sr: u16, handler_opcode: u16) -> (Cpu68020, TestMem) {
    let mut mem = TestMem::new();

    // Establish the requested supervisor mode, then wait in a stable
    // instruction-boundary loop until the interrupt is asserted.
    mem.write_word(ENTRY_PC, 0x46FC); // MOVE.W #saved_sr,SR
    mem.write_word(ENTRY_PC + 2, saved_sr);
    mem.write_word(INTERRUPTED_PC, 0x60FE); // BRA.S *

    mem.write_long(VBR + u32::from(DEVICE_VECTOR) * 4, HANDLER_PC);
    mem.write_word(HANDLER_PC, handler_opcode);
    mem.write_word(HANDLER_PC + 2, 0x60FE); // BRA.S * if handler falls through

    let mut cpu = Cpu68020::new();
    cpu.reset_to(INITIAL_ISP, ENTRY_PC);
    cpu.regs.usp = INITIAL_USP;
    cpu.regs.msp = INITIAL_MSP;
    cpu.regs.vbr = VBR;

    let mut ignored_acknowledge = false;
    for _ in 0..2_000 {
        cpu.ipl = 0;
        service_bus(&mut cpu, &mut mem, false, &mut ignored_acknowledge);
        cpu.tick();
        if cpu.ir == 0x60FE && cpu.regs.sr == saved_sr {
            return (cpu, mem);
        }
    }

    panic!("test program must reach its interruptible branch loop");
}

fn run_to_handler(cpu: &mut Cpu68020, mem: &mut TestMem) {
    let mut acknowledged = false;

    for _ in 0..10_000 {
        cpu.ipl = if acknowledged { 0 } else { INTERRUPT_LEVEL };
        service_bus(cpu, mem, !acknowledged, &mut acknowledged);
        cpu.tick();

        if cpu.instr_start_pc == HANDLER_PC {
            assert!(acknowledged, "the handler must be selected through IACK");
            return;
        }
    }

    panic!("interrupt must select handler ${HANDLER_PC:08X}");
}

fn assert_frame(mem: &TestMem, sp: u32, saved_sr: u16, format_vector: u16) {
    assert_eq!(
        mem.read_word(sp),
        saved_sr,
        "frame must preserve the pre-interrupt status register"
    );
    assert_eq!(
        mem.read_long(sp + 2),
        INTERRUPTED_PC,
        "frame must preserve the interrupted program counter"
    );
    assert_eq!(
        mem.read_word(sp + 6),
        format_vector,
        "frame must identify its format and acknowledged vector"
    );
}

#[test]
fn a7_routes_to_user_interrupt_or_master_stack_from_s_and_m() {
    let mut cpu = Cpu68020::new();
    cpu.regs.usp = INITIAL_USP;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.msp = INITIAL_MSP;

    cpu.regs.sr = 0x0000;
    assert_eq!(cpu.regs.a(7), INITIAL_USP, "user mode must select USP");
    cpu.regs.set_a(7, INITIAL_USP + 4);
    assert_eq!(cpu.regs.usp, INITIAL_USP + 4);

    cpu.regs.sr = 0x1000;
    assert_eq!(
        cpu.regs.a(7),
        INITIAL_USP + 4,
        "M has no stack-selection effect while S is clear"
    );

    cpu.regs.sr = 0x2000;
    assert_eq!(cpu.regs.a(7), INITIAL_ISP, "S=1 M=0 must select ISP");
    cpu.regs.set_active_sp(INITIAL_ISP - 4);
    assert_eq!(cpu.regs.ssp, INITIAL_ISP - 4);
    assert_eq!(cpu.regs.msp, INITIAL_MSP);

    cpu.regs.sr = 0x3000;
    assert_eq!(cpu.regs.a(7), INITIAL_MSP, "S=1 M=1 must select MSP");
    cpu.regs.set_active_sp(INITIAL_MSP - 4);
    assert_eq!(cpu.regs.msp, INITIAL_MSP - 4);
    assert_eq!(cpu.regs.ssp, INITIAL_ISP - 4);
}

#[test]
fn user_mode_interrupt_with_m_set_forces_s_only_in_the_throwaway_frame() {
    let saved_user_sr = 0x1000;
    let throwaway_sr = 0x3000;
    let (mut cpu, mut mem) = setup_interrupt_test(saved_user_sr, 0x60FE);

    assert_eq!(
        cpu.regs.active_sp(),
        INITIAL_USP,
        "S=0 must keep USP active even when M is set"
    );
    run_to_handler(&mut cpu, &mut mem);

    let master_frame_sp = INITIAL_MSP - 8;
    let throwaway_frame_sp = INITIAL_ISP - 8;
    assert_eq!(
        cpu.regs.usp, INITIAL_USP,
        "interrupt entry must preserve USP"
    );
    assert_eq!(cpu.regs.msp, master_frame_sp);
    assert_eq!(cpu.regs.ssp, throwaway_frame_sp);
    assert_eq!(
        cpu.regs.sr & 0x3000,
        0x2000,
        "the handler must run in interrupt mode"
    );

    assert_frame(
        &mem,
        master_frame_sp,
        saved_user_sr,
        FORMAT_ZERO_VECTOR_WORD,
    );
    assert_frame(
        &mem,
        throwaway_frame_sp,
        throwaway_sr,
        FORMAT_ONE_VECTOR_WORD,
    );
}

#[test]
fn interrupt_without_master_mode_pushes_one_format_zero_frame_on_isp() {
    let saved_sr = 0x2000;
    let (mut cpu, mut mem) = setup_interrupt_test(saved_sr, 0x60FE);

    run_to_handler(&mut cpu, &mut mem);

    let frame_sp = INITIAL_ISP - 8;
    assert_eq!(cpu.regs.ssp, frame_sp);
    assert_eq!(
        cpu.regs.msp, INITIAL_MSP,
        "an interrupt accepted with M clear must not touch MSP"
    );
    assert_eq!(
        cpu.regs.active_sp(),
        frame_sp,
        "the handler must run on ISP"
    );
    assert_eq!(cpu.regs.sr & 0x3000, 0x2000, "handler runs with S=1 M=0");
    assert_frame(&mem, frame_sp, saved_sr, FORMAT_ZERO_VECTOR_WORD);
}

#[test]
fn master_mode_interrupt_pushes_format_zero_on_msp_and_format_one_on_isp() {
    let saved_sr = 0x7000;
    let (mut cpu, mut mem) = setup_interrupt_test(saved_sr, 0x60FE);

    run_to_handler(&mut cpu, &mut mem);

    let master_frame_sp = INITIAL_MSP - 8;
    let throwaway_frame_sp = INITIAL_ISP - 8;
    assert_eq!(cpu.regs.msp, master_frame_sp);
    assert_eq!(cpu.regs.ssp, throwaway_frame_sp);
    assert_eq!(
        cpu.regs.active_sp(),
        throwaway_frame_sp,
        "the handler must run on ISP after interrupt entry clears M"
    );
    assert_eq!(cpu.regs.sr & 0x3000, 0x2000, "handler runs with S=1 M=0");
    assert_eq!(
        cpu.regs.sr & 0x4000,
        0,
        "interrupt entry must clear the set MC68020 T0 trace bit"
    );

    assert_frame(&mem, master_frame_sp, saved_sr, FORMAT_ZERO_VECTOR_WORD);
    assert_frame(&mem, throwaway_frame_sp, saved_sr, FORMAT_ONE_VECTOR_WORD);
}

#[test]
fn user_rte_discards_poisoned_format_one_pc_and_returns_through_master_frame() {
    let saved_user_sr = 0x1000;
    let (mut cpu, mut mem) = setup_interrupt_test(saved_user_sr, 0x4E73); // RTE

    run_to_handler(&mut cpu, &mut mem);

    let master_frame_sp = INITIAL_MSP - 8;
    let throwaway_frame_sp = INITIAL_ISP - 8;
    assert_eq!(
        mem.read_long(master_frame_sp + 2),
        INTERRUPTED_PC,
        "the real frame must retain the user return PC"
    );

    // Format-$1 is a throwaway frame: RTE must discard this PC after using
    // the saved S/M state to select MSP. Make it observably different from
    // the real frame so a one-frame return cannot pass accidentally.
    mem.write_long(throwaway_frame_sp + 2, POISON_PC);
    mem.write_word(POISON_PC, 0x60FE); // BRA.S * if the poison is used

    let mut ignored_acknowledge = false;
    for _ in 0..20_000 {
        cpu.ipl = 0;
        service_bus(&mut cpu, &mut mem, false, &mut ignored_acknowledge);
        cpu.tick();

        assert_ne!(
            cpu.instr_start_pc, POISON_PC,
            "RTE must not resume from the Format-$1 throwaway PC"
        );
        if cpu.instr_start_pc == INTERRUPTED_PC {
            assert_eq!(cpu.regs.pc, INTERRUPTED_PC + 2);
            assert_eq!(
                cpu.regs.sr, saved_user_sr,
                "the real MSP frame must restore S=0 M=1"
            );
            assert!(!cpu.regs.is_supervisor(), "RTE must return to user mode");
            assert_eq!(cpu.regs.usp, INITIAL_USP);
            assert_eq!(cpu.regs.ssp, INITIAL_ISP);
            assert_eq!(cpu.regs.msp, INITIAL_MSP);
            assert_eq!(
                cpu.regs.active_sp(),
                INITIAL_USP,
                "user mode must reactivate USP regardless of restored M"
            );
            return;
        }
    }

    panic!("RTE must discard Format-$1 and return through the user master frame");
}

#[test]
fn rte_discards_format_one_then_returns_through_master_format_zero_frame() {
    let saved_sr = 0x3000;
    let (mut cpu, mut mem) = setup_interrupt_test(saved_sr, 0x4E73); // RTE

    let mut acknowledged = false;
    let mut saw_handler = false;
    for _ in 0..20_000 {
        cpu.ipl = if acknowledged { 0 } else { INTERRUPT_LEVEL };
        service_bus(&mut cpu, &mut mem, !acknowledged, &mut acknowledged);
        cpu.tick();

        saw_handler |= cpu.instr_start_pc == HANDLER_PC;
        if saw_handler && cpu.instr_start_pc == INTERRUPTED_PC {
            assert!(acknowledged, "the interrupt must complete IACK");
            assert_eq!(cpu.interrupts_taken, 1, "deasserted IPL must not retrigger");
            assert_eq!(cpu.regs.pc, INTERRUPTED_PC + 2);
            assert_eq!(cpu.regs.sr, saved_sr);
            assert_eq!(cpu.regs.ssp, INITIAL_ISP);
            assert_eq!(cpu.regs.msp, INITIAL_MSP);
            assert_eq!(
                cpu.regs.active_sp(),
                INITIAL_MSP,
                "restored S=1 M=1 must make MSP active again"
            );
            return;
        }
    }

    panic!("RTE must consume the throwaway frame and return through the master frame");
}

#[test]
fn format_one_rte_can_restart_on_a_format_zero_frame_on_usp() {
    const RTE_PC: u32 = 0x1000;
    const RETURN_PC: u32 = 0x3000;
    const FORMAT_ONE_PC: u32 = 0xDEAD_BEEF;
    const FINAL_SR: u16 = 0x0015;

    let mut mem = TestMem::new();
    mem.write_word(RTE_PC, 0x4E73); // RTE
    mem.write_word(RETURN_PC, 0x60FE); // BRA.S *

    // The first frame is on ISP. Its user-mode SR makes USP active when
    // Format-$1 restarts RTE; its PC is deliberately not the return target.
    mem.write_word(INITIAL_ISP, 0x0000);
    mem.write_long(INITIAL_ISP + 2, FORMAT_ONE_PC);
    mem.write_word(INITIAL_ISP + 6, 0x1000);

    // The restarted RTE must consume this real frame from USP.
    mem.write_word(USER_FRAME_SP, FINAL_SR);
    mem.write_long(USER_FRAME_SP + 2, RETURN_PC);
    mem.write_word(USER_FRAME_SP + 6, 0x0000);

    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2000; // Supervisor interrupt mode: RTE starts on ISP.
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.usp = USER_FRAME_SP;
    cpu.regs.msp = INITIAL_MSP;
    cpu.regs.pc = RTE_PC + 4;
    cpu.setup_prefetch(0x4E73, 0x4E71);

    let mut acknowledged = false;
    for _ in 0..2_000 {
        cpu.ipl = 0;
        service_bus(&mut cpu, &mut mem, false, &mut acknowledged);
        cpu.tick();

        if cpu.instr_start_pc == RETURN_PC {
            assert_eq!(cpu.regs.pc, RETURN_PC + 2);
            assert_eq!(cpu.regs.sr, FINAL_SR);
            assert_eq!(
                cpu.regs.ssp,
                INITIAL_ISP + 8,
                "only the Format-$1 frame must be consumed from ISP"
            );
            assert_eq!(
                cpu.regs.usp,
                USER_FRAME_SP + 8,
                "the restarted Format-$0 frame must advance USP"
            );
            assert_eq!(
                cpu.regs.msp, INITIAL_MSP,
                "neither frame selects the master stack"
            );
            return;
        }
    }

    panic!("Format-$1 RTE must restart on and return through the USP frame");
}
