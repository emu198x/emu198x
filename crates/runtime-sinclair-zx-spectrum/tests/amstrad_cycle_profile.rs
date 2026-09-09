use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum_amstrad_class::{
    AmstradVariant, SpectrumAmstradClassCore,
    memory::MappedBank,
    variant::{Plus2AMarker, Plus2BMarker, Plus3Marker},
};
use emu198x_shell::{
    MachineCore,
    cycle_profile::{CycleCounts, ProfileMemory},
    debug_info::DebugSymbols,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumPlus3Runtime, SpectrumRuntime};

fn sources() -> DebugSymbols {
    let mut records = vec![
        serde_json::json!({"t":"header","format":"debug198x","format_version":"0.1","tool":"test","tool_version":"0","cpu":"z80","dialect":"sjasmplus","sources":["ram.s"]}),
    ];
    for page in 0..8 {
        records.push(serde_json::json!({"t":"section","id":page,"name":format!("ram{page}"),"space":{"slot":3,"page":page}}));
        records.push(serde_json::json!({"t":"line","file":"ram.s","line":page+1,"section":page,"offset":0,"length":16384}));
    }
    DebugSymbols::from_ndjson(
        &records
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        "ram.debug198x",
    )
    .expect("paged source fixture")
}

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

fn all_ram_modes<V: AmstradVariant>(model: Model) {
    // Independent expected maps, shared by +2A, +2B and +3.
    for (config, banks) in [[0u8, 1, 2, 3], [4, 5, 6, 7], [4, 5, 6, 3], [4, 7, 6, 3]]
        .into_iter()
        .enumerate()
    {
        for (slot, bank) in banks.into_iter().enumerate() {
            let mut runtime = SpectrumRuntime::new(model, SpectrumAmstradClassCore::<V>::new());
            let machine = runtime.machine_mut();
            for page in 0..8 {
                machine.memory.ram_bank_mut(page)[0] = 0x76;
            }
            machine
                .memory
                .write_1ffd(1 | (u8::try_from(config).expect("four configurations") << 1));
            let address = u16::try_from(slot).expect("four slots") << 14;
            assert_eq!(machine.memory.mapped_bank(address), MappedBank::Ram(bank));
            assert_eq!(machine.memory.read(address), 0x76);
            machine.z80.regs.pc = address;
            let counts = runtime.profile_cycles(5000).expect("all-RAM capture");
            conserved(&counts);
            assert_eq!(counts.mapped_addresses.len(), 1);
            let entry = &counts.mapped_addresses[0];
            assert_eq!(entry.address, u32::from(address));
            assert_eq!(entry.mapping.page, u16::from(bank));
            assert_eq!(entry.mapping.memory, ProfileMemory::Ram);
            let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
            assert_eq!(profile.lines.len(), 1);
            assert_eq!(profile.lines[0].source.line, u32::from(bank) + 1);
            assert_eq!(profile.unmapped_ticks, 0);
        }
    }
}

#[test]
fn every_all_ram_configuration_profiles_on_all_three_variants() {
    all_ram_modes::<Plus2AMarker>(Model::SpectrumPlus2A);
    all_ram_modes::<Plus2BMarker>(Model::SpectrumPlus2B);
    all_ram_modes::<Plus3Marker>(Model::SpectrumPlus3);
}

#[test]
fn all_four_roms_and_normal_ram_pages_keep_their_namespaces() {
    for rom in 0..4u8 {
        for bank in 0..8u8 {
            let mut runtime = SpectrumPlus3Runtime::blank();
            let machine = runtime.machine_mut();
            let halt = [0x76; 16384];
            machine.memory.load_roms(&halt, &halt, &halt, &halt);
            machine.memory.ram_bank_mut(usize::from(bank))[0] = 0x76;
            machine.memory.write_7ffd(((rom & 1) << 4) | bank);
            machine.memory.write_1ffd((rom & 2) << 1);
            assert_eq!(machine.memory.mapped_bank(0), MappedBank::Rom(rom));
            assert_eq!(machine.memory.mapped_bank(0xc000), MappedBank::Ram(bank));
            let counts = runtime.profile_cycles(20).expect("ROM HALT");
            conserved(&counts);
            assert_eq!(counts.mapped_addresses[0].mapping.page, u16::from(rom));
            assert_eq!(
                counts.mapped_addresses[0].mapping.memory,
                ProfileMemory::Rom
            );
            let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
            assert_eq!(
                profile.unmapped_ticks, 20,
                "ROM must not borrow a RAM source line"
            );
        }
    }
}

#[test]
fn entering_all_ram_keeps_the_paging_instruction_in_its_original_bank() {
    let mut runtime = SpectrumPlus3Runtime::blank();
    let machine = runtime.machine_mut();
    machine.memory.ram_bank_mut(0)[..4].copy_from_slice(&[0x3e, 1, 0xed, 0x79]); // LD A,1; OUT (C),A
    machine.memory.ram_bank_mut(3)[4] = 0x76; // after the switch, slot 3 holds bank 3
    machine.z80.regs.pc = 0xc000;
    machine.z80.regs.bc = 0x1ffd;
    let counts = runtime
        .profile_cycles(5000)
        .expect("enter all-RAM through CPU I/O");
    conserved(&counts);
    let out = counts
        .mapped_addresses
        .iter()
        .find(|entry| entry.address == 0xc002)
        .expect("paging instruction");
    assert_eq!(out.mapping.page, 0);
    let halt = counts
        .mapped_addresses
        .iter()
        .find(|entry| entry.address == 0xc004)
        .expect("instruction in new bank");
    assert_eq!(halt.mapping.page, 3);
    assert_eq!(runtime.machine().memory.mapped_bank(0), MappedBank::Ram(0));
    let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
    assert_eq!(
        profile
            .lines
            .iter()
            .map(|line| line.source.line)
            .collect::<Vec<_>>(),
        vec![1, 4]
    );
}

#[test]
fn leaving_all_ram_at_zero_keeps_ram_and_rom_execution_distinct() {
    let mut runtime = SpectrumPlus3Runtime::blank();
    let machine = runtime.machine_mut();
    machine.memory.ram_bank_mut(0)[..4].copy_from_slice(&[0x3e, 0, 0xed, 0x79]);
    let mut rom = [0; 16384];
    rom[4] = 0x76;
    machine.memory.load_roms(&rom, &rom, &rom, &rom);
    machine.memory.write_1ffd(1);
    machine.z80.regs.bc = 0x1ffd;
    let counts = runtime
        .profile_cycles(5000)
        .expect("restore ROM through CPU I/O");
    conserved(&counts);
    assert_eq!(counts.mapped_addresses.len(), 3);
    assert_eq!(
        counts.mapped_addresses[1].mapping.memory,
        ProfileMemory::Ram
    );
    assert_eq!(
        counts.mapped_addresses[2].mapping.memory,
        ProfileMemory::Rom
    );
    let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
    assert_eq!(profile.unmapped_ticks, 20);
}

#[test]
fn locked_paging_cannot_relabel_execution() {
    let mut runtime = SpectrumPlus3Runtime::blank();
    let machine = runtime.machine_mut();
    machine.memory.ram_bank_mut(0)[..5].copy_from_slice(&[0x3e, 1, 0xed, 0x79, 0x76]);
    machine.memory.write_7ffd(0x20);
    machine.z80.regs.pc = 0xc000;
    machine.z80.regs.bc = 0x1ffd;
    let counts = runtime.profile_cycles(5000).expect("attempt locked switch");
    conserved(&counts);
    assert!(
        counts
            .mapped_addresses
            .iter()
            .all(|entry| entry.mapping.page == 0 && entry.mapping.memory == ProfileMemory::Ram)
    );
    assert_eq!(runtime.machine().memory.mapped_bank(0), MappedBank::Rom(0));
}

#[test]
fn contended_all_ram_capture_preserves_machine_and_snapshot_state() {
    let make = || {
        let mut runtime = SpectrumPlus3Runtime::blank();
        runtime.machine_mut().memory.ram_bank_mut(4)[..3].copy_from_slice(&[0xc3, 0, 0]);
        runtime.machine_mut().memory.write_1ffd(3);
        runtime
    };
    let mut profiled = make();
    let mut ordinary = make();
    let ticks = profiled.machine().frame_timing().halfcycles_per_frame * 2 + 7;
    let counts = profiled
        .profile_cycles(ticks)
        .expect("contended all-RAM capture");
    conserved(&counts);
    ordinary.machine_mut().advance_halfcycles(ticks);
    assert_eq!(
        serde_json::to_value(profiled.machine()).expect("serialize profiled"),
        serde_json::to_value(ordinary.machine()).expect("serialize ordinary")
    );
    assert!(counts.addresses[&0].ticks > counts.addresses[&0].executions * 50);
    let snapshot = profiled.snapshot().expect("capture snapshot");
    let mut restored = SpectrumPlus3Runtime::blank();
    restored.restore(&snapshot).expect("restore snapshot");
    assert_eq!(
        profiled.profile_cycles(5000).expect("continued capture"),
        restored.profile_cycles(5000).expect("restored capture")
    );
}
