use common_sinclair_zx_spectrum::{driver::SpectrumDriver, memory::MemoryBus};
use emu198x_shell::{
    MachineCore,
    cycle_profile::{CycleCounts, ProfileMemory},
    debug_info::DebugSymbols,
};
use runtime_sinclair_zx_spectrum::{Pentagon128Runtime, ScorpionZS256Runtime};

fn conserved(counts: &CycleCounts) {
    let code: u64 = counts.addresses.values().map(|cost| cost.ticks).sum();
    assert_eq!(
        code,
        counts
            .mapped_addresses
            .iter()
            .map(|entry| entry.cost.ticks)
            .sum::<u64>()
    );
    assert_eq!(
        counts.ticks,
        code + counts.interrupt_ticks
            + counts.halt_ticks
            + counts.leading_partial_ticks
            + counts.trailing_partial_ticks
    );
}

fn pentagon(rom: &[u8]) -> Pentagon128Runtime {
    let mut runtime = Pentagon128Runtime::blank();
    runtime.machine_mut().memory.load_roms(rom, &[0; 16384]);
    runtime
}
fn scorpion(rom: &[u8]) -> ScorpionZS256Runtime {
    let mut runtime = ScorpionZS256Runtime::blank();
    runtime
        .machine_mut()
        .memory
        .load_roms(rom, &[0; 16384], &[0; 16384], &[0; 16384]);
    runtime
}

// The overlay is zero-filled (NOP), while base ROM holds HALT at the trap.
// Verify the identity of the byte actually read, not merely the paging flag.
macro_rules! overlay_checks {
    ($make:ident, $overlay_page:expr) => {{
        let mut rom = [0; 16384];
        rom[0x3d00] = 0x76;
        let mut runtime = $make(&rom);
        runtime.machine_mut().z80.regs.pc = 0x3d00;
        let entry = runtime.profile_cycles(16).expect("M1 overlay entry");
        conserved(&entry);
        assert!(runtime.machine().beta.trdos_paged);
        assert!(
            !runtime.machine().z80.halt,
            "overlay NOP replaces base HALT"
        );
        assert_eq!(
            entry.mapped_addresses[0].mapping.memory,
            ProfileMemory::RomOverlay
        );
        assert_eq!(entry.mapped_addresses[0].mapping.page, $overlay_page);
        runtime.machine_mut().memory.write(0xc000, 0x76);
        runtime.machine_mut().z80.regs.pc = 0xc000;
        let exit = runtime.profile_cycles(16).expect("M1 overlay exit");
        conserved(&exit);
        assert!(!runtime.machine().beta.trdos_paged);
        assert_eq!(exit.mapped_addresses[0].mapping.memory, ProfileMemory::Ram);
        assert_eq!(exit.mapped_addresses[0].mapping.page, 0);

        let mut rom = [0; 16384];
        rom[0x3cff] = 0xdd; // DD prefix in base ROM, NOP fetched from overlay.
        let mut runtime = $make(&rom);
        runtime.machine_mut().z80.regs.pc = 0x3cff;
        let prefix = runtime.profile_cycles(32).expect("prefix crosses trap");
        conserved(&prefix);
        assert!(runtime.machine().beta.trdos_paged);
        assert_eq!(prefix.mapped_addresses.len(), 1);
        assert_eq!(prefix.mapped_addresses[0].address, 0x3cff);
        assert_eq!(
            prefix.mapped_addresses[0].mapping.memory,
            ProfileMemory::Rom
        );
        assert_eq!(prefix.mapped_addresses[0].mapping.page, 0);
    }};
}

#[test]
fn pentagon_observes_overlay_after_the_first_fetch_only() {
    overlay_checks!(pentagon, 0);
}
#[test]
fn scorpion_observes_its_actual_overlay_backing_rom() {
    overlay_checks!(scorpion, 1);
}

fn sources() -> DebugSymbols {
    let mut records = vec![
        serde_json::json!({"t":"header","format":"debug198x","format_version":"0.1","tool":"test","tool_version":"0","cpu":"z80","dialect":"sjasmplus","sources":["banks.s"]}),
    ];
    for page in 0..16 {
        records.push(serde_json::json!({"t":"section","id":page,"name":format!("ram{page}"),"space":{"slot":3,"page":page}}));
        records.push(serde_json::json!({"t":"line","file":"banks.s","line":page+1,"section":page,"offset":0,"length":16384}));
    }
    DebugSymbols::from_ndjson(
        &records
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        "banks.debug198x",
    )
    .expect("paged sidecar")
}

#[test]
fn scorpion_reports_all_sixteen_banks_using_the_current_memory_decoder() {
    for page in 0..16u8 {
        let mut runtime = ScorpionZS256Runtime::blank();
        let machine = runtime.machine_mut();
        // This intentionally follows the current core's documented bit-0 high
        // bank selector, not the unresolved alternate hardware convention.
        machine.memory.write_1ffd(page >> 3);
        machine.memory.write_7ffd(page & 7);
        machine.memory.write(0xc000, 0x76);
        machine.z80.regs.pc = 0xc000;
        let counts = runtime.profile_cycles(16).expect("Scorpion RAM capture");
        conserved(&counts);
        assert_eq!(counts.mapped_addresses[0].mapping.page, u16::from(page));
        let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
        assert_eq!(profile.lines[0].source.line, u32::from(page) + 1);
        assert_eq!(profile.unmapped_ticks, 0);
    }
}

#[test]
fn scorpion_paging_instruction_keeps_its_old_bank_and_lock_is_observed() {
    // Current machine I/O decodes $1FFD as both paging registers, so OUT 1
    // selects bank 9. Observe that behaviour without changing the decoder.
    for locked in [false, true] {
        let mut runtime = ScorpionZS256Runtime::blank();
        let machine = runtime.machine_mut();
        machine.memory.write_1ffd(1);
        machine.memory.write_7ffd(1);
        machine.memory.write(0xc004, 0x76); // bank 9 continuation
        machine.memory.write_1ffd(0);
        machine.memory.write_7ffd(0);
        for (offset, byte) in [0x3e, 1, 0xed, 0x79, 0x76].into_iter().enumerate() {
            machine
                .memory
                .write(0xc000 + u16::try_from(offset).expect("small program"), byte);
        }
        if locked {
            machine.memory.write_7ffd(0x20);
        }
        machine.z80.regs.pc = 0xc000;
        machine.z80.regs.bc = 0x1ffd;
        let counts = runtime
            .profile_cycles(1000)
            .expect("extended paging capture");
        conserved(&counts);
        assert_eq!(counts.mapped_addresses[1].mapping.page, 0);
        assert_eq!(
            counts.mapped_addresses[2].mapping.page,
            if locked { 0 } else { 9 }
        );
    }
}

#[test]
fn clone_capture_preserves_state_and_snapshot_over_frame_wrap() {
    macro_rules! check {
        ($make:ident) => {{
            let mut rom = [0; 16384];
            rom[0x3cff] = 0xdd;
            let mut profiled = $make(&rom);
            let mut ordinary = $make(&rom);
            profiled.machine_mut().z80.regs.pc = 0x3cff;
            ordinary.machine_mut().z80.regs.pc = 0x3cff;
            let ticks = profiled.machine().frame_timing().halfcycles_per_frame * 2 + 3;
            let counts = profiled.profile_cycles(ticks).expect("clone capture");
            conserved(&counts);
            ordinary.machine_mut().advance_halfcycles(ticks);
            assert_eq!(
                serde_json::to_value(profiled.machine()).expect("profiled state"),
                serde_json::to_value(ordinary.machine()).expect("ordinary state")
            );
            let snapshot = profiled.snapshot().expect("snapshot");
            let mut restored = $make(&rom);
            restored.restore(&snapshot).expect("restore");
            assert_eq!(
                profiled.profile_cycles(1000).expect("continued capture"),
                restored.profile_cycles(1000).expect("restored capture")
            );
        }};
    }
    check!(pentagon);
    check!(scorpion);
}

#[test]
fn pentagon_banks_and_scorpion_base_roms_remain_distinct() {
    for bank in 0..8u8 {
        let mut runtime = Pentagon128Runtime::blank();
        runtime.machine_mut().memory.write_7ffd(bank);
        runtime.machine_mut().memory.write(0xc000, 0x76);
        runtime.machine_mut().z80.regs.pc = 0xc000;
        let counts = runtime.profile_cycles(16).expect("Pentagon bank");
        conserved(&counts);
        assert_eq!(counts.mapped_addresses[0].mapping.page, u16::from(bank));
        let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
        assert_eq!(profile.clock.rate.numerator_hz, 14_336_000);
        assert_eq!(profile.lines[0].source.line, u32::from(bank) + 1);
    }
    for rom in 0..4u8 {
        let mut runtime = ScorpionZS256Runtime::blank();
        runtime.machine_mut().memory.write_7ffd((rom & 1) << 4);
        runtime.machine_mut().memory.write_1ffd(rom & 2);
        let counts = runtime.profile_cycles(16).expect("Scorpion base ROM");
        conserved(&counts);
        assert_eq!(counts.mapped_addresses[0].mapping.page, u16::from(rom));
        assert_eq!(
            counts.mapped_addresses[0].mapping.memory,
            ProfileMemory::Rom
        );
        let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
        assert_eq!(profile.unmapped_ticks, 16);
    }
}
