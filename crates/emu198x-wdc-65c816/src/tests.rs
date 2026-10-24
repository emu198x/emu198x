//! What the single-instruction corpus cannot reach: reset, interrupts, the
//! WAI and STP states, and the RDY and RES inputs.

use super::*;
use crate::registers::{FLAG_C, FLAG_M, FLAG_X};

/// A flat 16 MB bus.
struct Bus {
    mem: Vec<u8>,
}

impl Bus {
    fn new() -> Self {
        Self {
            mem: vec![0; 1 << 24],
        }
    }

    fn load(&mut self, addr: u32, bytes: &[u8]) {
        let start = addr as usize;
        self.mem[start..start + bytes.len()].copy_from_slice(bytes);
    }

    fn vector(&mut self, at: u16, target: u16) {
        self.load(u32::from(at), &target.to_le_bytes());
    }

    /// Service the pins and tick once.
    fn cycle(&mut self, cpu: &mut Wdc65c816) {
        if cpu.bus_active() {
            if cpu.rw {
                cpu.data_in = self.mem[cpu.addr as usize];
            } else {
                self.mem[cpu.addr as usize] = cpu.data;
            }
        }
        cpu.tick();
    }

    /// Run until the next instruction boundary.
    fn instruction(&mut self, cpu: &mut Wdc65c816) -> u32 {
        let mut n = 0;
        loop {
            self.cycle(cpu);
            n += 1;
            if cpu.instruction_complete() || n > 100 {
                return n;
            }
        }
    }
}

/// A CPU out of reset at `$8000`.
fn booted(bus: &mut Bus) -> Wdc65c816 {
    bus.vector(0xFFFC, 0x8000);
    let mut cpu = Wdc65c816::new();
    let cycles = bus.instruction(&mut cpu);
    assert_eq!(cycles, 7, "reset takes seven cycles before the first fetch");
    cpu
}

#[test]
fn reset_enters_emulation_mode_and_reads_the_vector() {
    let mut bus = Bus::new();
    let mut cpu = Wdc65c816::new();
    cpu.regs.a = 0x1234;
    cpu.regs.x = 0xABCD;
    cpu.regs.d = 0x2000;
    cpu.regs.e = false;
    bus.vector(0xFFFC, 0x8123);
    cpu.reset();
    let mut vector_cycles = 0;
    while !cpu.instruction_complete() {
        if cpu.vpb {
            vector_cycles += 1;
            assert!(cpu.rw, "the vector is read");
        }
        bus.cycle(&mut cpu);
    }
    assert_eq!(vector_cycles, 2);
    assert_eq!(cpu.addr, 0x008123);
    assert!(cpu.e());
    assert!(cpu.m() && cpu.x());
    assert_eq!(cpu.regs.d, 0);
    assert_eq!(cpu.regs.x, 0x00CD, "X loses its high byte");
    assert_eq!(cpu.regs.a, 0x1234, "A is untouched");
    assert_eq!(cpu.regs.s >> 8, 0x01);
}

#[test]
fn holding_res_freezes_the_cpu_and_release_resets_it() {
    let mut bus = Bus::new();
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0xEA, 0xEA]);
    cpu.res = true;
    for _ in 0..5 {
        bus.cycle(&mut cpu);
    }
    assert_eq!(cpu.addr, 0x008000, "nothing happens while held");
    cpu.res = false;
    bus.cycle(&mut cpu);
    assert_eq!(cpu.total_cycles, 0, "release starts the reset sequence");
    assert_eq!(bus.instruction(&mut cpu), 7);
}

#[test]
fn rdy_low_holds_the_cycle() {
    let mut bus = Bus::new();
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0xA9, 0x42]); // LDA #$42
    bus.cycle(&mut cpu);
    let held = cpu.addr;
    cpu.rdy = false;
    for _ in 0..3 {
        bus.cycle(&mut cpu);
        assert_eq!(cpu.addr, held);
    }
    cpu.rdy = true;
    bus.instruction(&mut cpu);
    assert_eq!(cpu.regs.a & 0xFF, 0x42);
}

/// `CLC; XCE`, then the native-mode program.
fn native(bus: &mut Bus, program: &[u8]) -> Wdc65c816 {
    let mut cpu = booted(bus);
    bus.load(0x8000, &[0x18, 0xFB]);
    bus.load(0x8002, program);
    bus.instruction(&mut cpu);
    bus.instruction(&mut cpu);
    assert!(!cpu.e());
    cpu
}

#[test]
fn an_irq_is_taken_at_the_boundary_in_native_mode() {
    let mut bus = Bus::new();
    bus.vector(0xFFEE, 0x9000);
    // CLI; NOP; NOP
    let mut cpu = native(&mut bus, &[0x58, 0xEA, 0xEA]);
    cpu.regs.pbr = 0;
    bus.instruction(&mut cpu); // CLI
    cpu.irq = true;
    bus.instruction(&mut cpu); // NOP, sampling the IRQ
    let pushed_from = cpu.regs.s;
    let cycles = bus.instruction(&mut cpu);
    assert_eq!(
        cycles, 8,
        "native IRQ: two fetches, four pushes, two vector reads"
    );
    assert_eq!(cpu.addr, 0x009000);
    assert!(cpu.regs.flag(FLAG_I));
    // PBR, PCH, PCL, P.
    let s = u32::from(pushed_from);
    assert_eq!(bus.mem[s as usize], 0x00);
    assert_eq!(
        u16::from_le_bytes([bus.mem[s as usize - 2], bus.mem[s as usize - 1]]),
        0x8004
    );
    assert_eq!(cpu.regs.s, pushed_from - 4);
}

#[test]
fn a_masked_irq_waits_for_cli() {
    let mut bus = Bus::new();
    bus.vector(0xFFFE, 0x9000);
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0xEA, 0x58, 0xEA, 0xEA]);
    cpu.irq = true;
    bus.instruction(&mut cpu);
    assert_eq!(cpu.addr, 0x008001, "I is set out of reset");
    bus.instruction(&mut cpu); // CLI samples with I still set
    assert_eq!(cpu.addr, 0x008002, "so the instruction after CLI runs");
    bus.instruction(&mut cpu); // NOP, which samples with I clear
    assert_eq!(cpu.addr, 0x008003, "the interrupt sequence starts at PC");
    bus.instruction(&mut cpu);
    assert_eq!(cpu.addr, 0x009000);
}

#[test]
fn emulation_mode_irq_pushes_p_with_b_clear() {
    let mut bus = Bus::new();
    bus.vector(0xFFFE, 0x9000);
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0x58, 0xEA]);
    bus.instruction(&mut cpu);
    cpu.irq = true;
    bus.instruction(&mut cpu);
    let s = cpu.regs.s;
    let cycles = bus.instruction(&mut cpu);
    assert_eq!(cycles, 7, "no PBR push in emulation mode");
    let pushed_p = bus.mem[usize::from(s) - 2];
    assert_eq!(pushed_p & FLAG_X, 0, "B is clear for a hardware interrupt");
}

#[test]
fn nmi_is_edge_triggered() {
    let mut bus = Bus::new();
    bus.vector(0xFFEA, 0x9000);
    let mut cpu = native(&mut bus, &[0xEA; 8]);
    bus.load(0x9000, &[0xEA; 8]);
    cpu.nmi = true;
    bus.instruction(&mut cpu);
    bus.instruction(&mut cpu);
    assert_eq!(cpu.addr, 0x009000);
    // Still held: no second NMI.
    bus.instruction(&mut cpu);
    bus.instruction(&mut cpu);
    assert_eq!(cpu.addr, 0x009002);
}

#[test]
fn abort_completes_the_instruction_without_changing_registers() {
    let mut bus = Bus::new();
    bus.vector(0xFFE8, 0x9000);
    let mut cpu = native(&mut bus, &[0xA9, 0x55, 0xEA]);
    let before = cpu.regs.a;
    bus.cycle(&mut cpu); // opcode fetch of LDA
    cpu.abort = true;
    bus.cycle(&mut cpu);
    cpu.abort = false;
    while !cpu.instruction_complete() {
        bus.cycle(&mut cpu);
    }
    assert_eq!(cpu.regs.a, before, "the load is undone");
    bus.instruction(&mut cpu);
    assert_eq!(cpu.addr, 0x009000, "and the abort vector taken");
    // The return address is the aborted instruction.
    let s = usize::from(cpu.regs.s);
    assert_eq!(u16::from_le_bytes([bus.mem[s + 2], bus.mem[s + 3]]), 0x8002);
}

#[test]
fn wai_stops_until_an_interrupt_and_masked_irq_resumes() {
    let mut bus = Bus::new();
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0xCB, 0xEA]);
    bus.cycle(&mut cpu);
    bus.cycle(&mut cpu);
    bus.cycle(&mut cpu);
    assert!(cpu.waiting());
    assert!(!cpu.bus_active());
    for _ in 0..10 {
        bus.cycle(&mut cpu);
    }
    assert!(cpu.waiting());
    // I is set, so the IRQ only wakes it: execution resumes after WAI.
    cpu.irq = true;
    bus.cycle(&mut cpu);
    assert!(!cpu.waiting());
    assert_eq!(cpu.addr, 0x008001);
}

#[test]
fn wai_wakes_into_an_nmi() {
    let mut bus = Bus::new();
    bus.vector(0xFFFA, 0x9000);
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0xCB]);
    for _ in 0..4 {
        bus.cycle(&mut cpu);
    }
    assert!(cpu.waiting());
    cpu.nmi = true;
    bus.cycle(&mut cpu);
    bus.instruction(&mut cpu);
    assert_eq!(cpu.addr, 0x009000);
}

#[test]
fn stp_stops_until_reset() {
    let mut bus = Bus::new();
    let mut cpu = booted(&mut bus);
    bus.load(0x8000, &[0xDB]);
    for _ in 0..4 {
        bus.cycle(&mut cpu);
    }
    assert!(cpu.stopped());
    cpu.irq = true;
    cpu.nmi = true;
    for _ in 0..4 {
        bus.cycle(&mut cpu);
    }
    assert!(cpu.stopped(), "interrupts do not end STP");
    cpu.irq = false;
    cpu.nmi = false;
    cpu.res = true;
    bus.cycle(&mut cpu);
    cpu.res = false;
    bus.cycle(&mut cpu);
    assert!(!cpu.stopped());
}

#[test]
fn rep_and_sep_change_widths_and_emulation_forces_8_bit() {
    let mut bus = Bus::new();
    // REP #$30; LDA #$1234; SEP #$20; SEC; XCE
    let mut cpu = native(
        &mut bus,
        &[0xC2, 0x30, 0xA9, 0x34, 0x12, 0xE2, 0x20, 0x38, 0xFB],
    );
    bus.instruction(&mut cpu);
    assert!(!cpu.m() && !cpu.x());
    bus.instruction(&mut cpu);
    assert_eq!(cpu.regs.a, 0x1234);
    bus.instruction(&mut cpu);
    assert!(cpu.m());
    bus.instruction(&mut cpu);
    bus.instruction(&mut cpu);
    assert!(cpu.e());
    assert!(!cpu.regs.flag(FLAG_C), "C takes the old E");
    assert!(cpu.regs.flag(FLAG_M) && cpu.regs.flag(FLAG_X));
}

#[test]
fn the_cpu_round_trips_through_serde() {
    let mut bus = Bus::new();
    let mut cpu = native(&mut bus, &[0xA9, 0x12]);
    bus.cycle(&mut cpu);
    let json = serde_json::to_string(&cpu).expect("serialise");
    let mut copy: Wdc65c816 = serde_json::from_str(&json).expect("deserialise");
    bus.instruction(&mut cpu);
    bus.instruction(&mut copy);
    assert_eq!(cpu.regs, copy.regs);
    assert_eq!(cpu.addr, copy.addr);
}

#[test]
fn default_is_new() {
    let cpu = Wdc65c816::default();
    assert!(cpu.e());
    assert_eq!(Registers::default(), Registers::new());
}
