//! The pin contract and a few instructions run as a host would run them.

use super::*;

/// Run the CPU against 64 KB of RAM for `ticks` cycles, recording each bus
/// cycle.
fn run(cpu: &mut Spc700, ram: &mut [u8; 0x10000], ticks: usize) -> Vec<BusCycle> {
    let mut seen = Vec::new();
    for _ in 0..ticks {
        cpu.tick();
        let cycle = cpu.bus_cycle();
        match cycle {
            BusCycle::Read(addr) => cpu.data_in = ram[usize::from(addr)],
            BusCycle::Write(addr, data) => ram[usize::from(addr)] = data,
            BusCycle::Idle => {}
        }
        seen.push(cycle);
    }
    seen
}

fn program(bytes: &[u8]) -> (Spc700, Box<[u8; 0x10000]>) {
    let mut ram = Box::new([0u8; 0x10000]);
    ram[0x0200..0x0200 + bytes.len()].copy_from_slice(bytes);
    let mut cpu = Spc700::new();
    cpu.reset_to(0x0200);
    (cpu, ram)
}

#[test]
fn power_on_matches_the_s_smp() {
    let cpu = Spc700::default();
    assert_eq!(cpu.regs.sp, 0xEF);
    assert_eq!(cpu.regs.p.bits(), 0x02);
    assert!(cpu.at_instruction_boundary());
}

#[test]
fn flags_pack_and_unpack() {
    for bits in 0..=255u8 {
        assert_eq!(Flags::from_bits(bits).bits(), bits);
    }
}

#[test]
fn a_store_drives_the_write_on_its_own_cycle() {
    // MOV A,#$42 ; MOV $10,A
    let (mut cpu, mut ram) = program(&[0xE8, 0x42, 0xC4, 0x10]);
    let cycles = run(&mut cpu, &mut ram, 6);
    assert_eq!(
        cycles,
        [
            BusCycle::Read(0x0200),
            BusCycle::Read(0x0201),
            BusCycle::Read(0x0202),
            BusCycle::Read(0x0203),
            BusCycle::Read(0x0010),
            BusCycle::Write(0x0010, 0x42),
        ]
    );
    assert_eq!(ram[0x10], 0x42);
}

#[test]
fn sync_marks_only_opcode_fetches() {
    // NOP ; NOP
    let (mut cpu, mut ram) = program(&[0x00, 0x00]);
    let mut syncs = Vec::new();
    for _ in 0..4 {
        cpu.tick();
        syncs.push(cpu.sync);
        if let BusCycle::Read(addr) = cpu.bus_cycle() {
            cpu.data_in = ram[usize::from(addr)];
        }
    }
    assert_eq!(syncs, [true, false, true, false]);
    let _ = &mut ram;
}

#[test]
fn the_direct_page_flag_moves_direct_accesses_to_page_one() {
    // SETP ; MOV $10,A
    let (mut cpu, mut ram) = program(&[0x40, 0xC4, 0x10]);
    cpu.regs.a = 0x99;
    run(&mut cpu, &mut ram, 6);
    assert_eq!(ram[0x0110], 0x99);
    assert_eq!(ram[0x0010], 0x00);
}

#[test]
fn multiply_and_divide_take_their_idle_cycles() {
    // MOV Y,#$12 ; MOV A,#$34 ; MUL YA
    let (mut cpu, mut ram) = program(&[0x8D, 0x12, 0xE8, 0x34, 0xCF]);
    let cycles = run(&mut cpu, &mut ram, 4 + 9 + 1);
    let idles = cycles.iter().filter(|c| **c == BusCycle::Idle).count();
    assert_eq!(idles, 7);
    assert_eq!(cpu.regs.ya(), 0x12 * 0x34);

    // MOV X,#$10 ; DIV YA,X with YA = $03E8
    let (mut cpu, mut ram) = program(&[0xCD, 0x10, 0x9E]);
    cpu.regs.set_ya(1000);
    run(&mut cpu, &mut ram, 2 + 12 + 1);
    assert_eq!((cpu.regs.a, cpu.regs.y), (62, 8));
}

#[test]
fn sleep_parks_the_cpu_reading_pc_and_idling() {
    let (mut cpu, mut ram) = program(&[0xEF]);
    let cycles = run(&mut cpu, &mut ram, 5);
    assert_eq!(
        cycles,
        [
            BusCycle::Read(0x0200),
            BusCycle::Read(0x0201),
            BusCycle::Idle,
            BusCycle::Read(0x0201),
            BusCycle::Idle,
        ]
    );
    assert_eq!(cpu.regs.halt, Halt::Sleep);
    assert!(!cpu.at_instruction_boundary());
    cpu.reset_to(0x0200);
    assert_eq!(cpu.regs.halt, Halt::None);
}

#[test]
fn a_call_and_return_round_trip_the_stack() {
    // CALL $0300 ... at $0300: RET
    let (mut cpu, mut ram) = program(&[0x3F, 0x00, 0x03]);
    ram[0x0300] = 0x6F;
    run(&mut cpu, &mut ram, 8 + 5 + 1);
    // The next opcode fetch is on the pins, but PC moves only when that
    // instruction completes.
    assert_eq!(cpu.regs.pc, 0x0203, "back after the CALL");
    assert_eq!(cpu.regs.sp, 0xEF);
}

#[test]
fn a_snapshot_mid_instruction_resumes_identically() {
    // MOV X,#$10 ; DIV YA,X
    let (mut cpu, mut ram) = program(&[0xCD, 0x10, 0x9E]);
    cpu.regs.set_ya(1000);
    run(&mut cpu, &mut ram, 6);
    let json = serde_json::to_string(&cpu).expect("serialise");
    let mut copy: Spc700 = serde_json::from_str(&json).expect("deserialise");
    let mut ram_copy = ram.clone();
    let a = run(&mut cpu, &mut ram, 10);
    let b = run(&mut copy, &mut ram_copy, 10);
    assert_eq!(a, b);
    assert_eq!(cpu.regs, copy.regs);
}
