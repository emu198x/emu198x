//! Save-state continuation regressions for MC68020 Format-$A frames.
//!
//! Format-$A entry and RTE use serialized step counters because neither
//! continuation can be reconstructed from the visible stack pointer alone.
//! These tests snapshot in the middle of each sequence and compare restored
//! and uninterrupted execution after every tick.

use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::{State, TAG_AE_FMT_A_STEP, TAG_RTE_READ_FMTA_STEP};
use motorola_68020::Cpu68020;

const FAULT_PC: u32 = 0x1000;
const RTE_PC: u32 = 0x1200;
const HANDLER_PC: u32 = 0x1800;
const RETURN_PC: u32 = 0x3000;
const INITIAL_ISP: u32 = 0x8000;
const ODD_INSTRUCTION_ADDRESS: u32 = 0x5001;
const FORMAT_A_BYTES: u32 = 32;
const JMP_A0: u16 = 0x4ED0;
const RTE: u16 = 0x4E73;
const NOP: u16 = 0x4E71;

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

fn tick(cpu: &mut Cpu68020, mem: &mut TestMem) {
    cpu.ipl = 0;
    service_bus(cpu, mem);
    cpu.tick();
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
        tick(&mut uninterrupted, &mut uninterrupted_mem);
        tick(&mut resumed, &mut resumed_mem);

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
fn serde_mid_format_a_entry_preserves_the_remaining_field_sequence() {
    let mut mem = TestMem::new();
    mem.write_long(3 * 4, HANDLER_PC);
    mem.write_word(HANDLER_PC, 0x60FE); // BRA.S *
    mem.write_word(FAULT_PC, JMP_A0);
    mem.write_word(FAULT_PC + 2, NOP);
    mem.write_word(FAULT_PC + 4, NOP);

    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2000;
    cpu.regs.ssp = INITIAL_ISP;
    cpu.regs.a[0] = ODD_INSTRUCTION_ADDRESS;
    cpu.regs.pc = FAULT_PC + 4;
    cpu.setup_prefetch(JMP_A0, NOP);

    let mut reached_snapshot_boundary = false;
    for _ in 0..20_000 {
        tick(&mut cpu, &mut mem);
        if cpu.followup_tag == TAG_AE_FMT_A_STEP && cpu.regs.ssp == INITIAL_ISP - 16 {
            reached_snapshot_boundary = true;
            break;
        }
    }
    assert!(
        reached_snapshot_boundary,
        "snapshot must land halfway through the 13-field Format-$A push"
    );

    let resumed = restored(&cpu);
    let resumed_mem = mem.clone();
    assert_equivalent_continuation(cpu, mem, resumed, resumed_mem, |candidate| {
        candidate.instr_start_pc == HANDLER_PC && candidate.regs.ssp == INITIAL_ISP - FORMAT_A_BYTES
    });
}

#[test]
fn serde_mid_format_a_rte_tail_preserves_the_remaining_word_count() {
    let frame_sp = INITIAL_ISP - FORMAT_A_BYTES;
    let saved_sr = 0x2015;
    let mut mem = TestMem::new();
    mem.write_word(RTE_PC, RTE);
    mem.write_word(RTE_PC + 2, NOP);
    mem.write_word(RETURN_PC, 0x60FE); // BRA.S *

    mem.write_word(frame_sp, saved_sr);
    mem.write_long(frame_sp + 2, RETURN_PC);
    mem.write_word(frame_sp + 6, 0xA00C);
    for tail_index in 0..12 {
        mem.write_word(
            frame_sp + 8 + tail_index * 2,
            0xA100 | u16::try_from(tail_index).expect("tail index fits in u16"),
        );
    }

    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2000;
    cpu.regs.ssp = frame_sp;
    cpu.regs.pc = RTE_PC + 4;
    cpu.setup_prefetch(RTE, NOP);

    let mid_tail_sp = frame_sp + 8 + 5 * 2;
    let mut reached_snapshot_boundary = false;
    for _ in 0..20_000 {
        tick(&mut cpu, &mut mem);
        if cpu.followup_tag == TAG_RTE_READ_FMTA_STEP && cpu.regs.ssp == mid_tail_sp {
            reached_snapshot_boundary = true;
            break;
        }
    }
    assert!(
        reached_snapshot_boundary,
        "snapshot must land after five of the twelve Format-$A tail words"
    );

    let resumed = restored(&cpu);
    let resumed_mem = mem.clone();
    assert_equivalent_continuation(cpu, mem, resumed, resumed_mem, |candidate| {
        candidate.instr_start_pc == RETURN_PC
            && candidate.regs.sr == saved_sr
            && candidate.regs.ssp == INITIAL_ISP
    });
}
