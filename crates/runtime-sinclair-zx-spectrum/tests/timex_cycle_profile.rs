use common_sinclair_zx_spectrum::{driver::SpectrumDriver, memory::MemoryBus};
use emu198x_shell::{
    MachineCore,
    cycle_profile::{CycleCounts, ProfileMemory},
    debug_info::DebugSymbols,
};
use machine_timex_ts2068::memory::{MemorySource, MemoryTimex};
use runtime_sinclair_zx_spectrum::{Model, TimexTC2048Runtime, TimexTS2068Runtime};

fn conserved(counts: &CycleCounts) {
    let code: u64 = counts.addresses.values().map(|cost| cost.ticks).sum();
    if !counts.mapped_addresses.is_empty() {
        assert_eq!(
            code,
            counts
                .mapped_addresses
                .iter()
                .map(|entry| entry.cost.ticks)
                .sum::<u64>()
        );
    }
    assert_eq!(
        counts.ticks,
        code + counts.interrupt_ticks
            + counts.halt_ticks
            + counts.leading_partial_ticks
            + counts.trailing_partial_ticks
    );
}
fn runtime(model: Model) -> TimexTS2068Runtime {
    let mut rom = [0; 16384];
    rom[0x38] = 0x76;
    TimexTS2068Runtime::new_ts2068(model, rom, [0; 8192])
}
fn sources() -> DebugSymbols {
    let mut records = vec![
        serde_json::json!({"t":"header","format":"debug198x","format_version":"0.1","tool":"test","tool_version":"0","cpu":"z80","dialect":"sjasmplus","sources":["home.s"]}),
    ];
    for page in 2..8 {
        records.push(serde_json::json!({"t":"section","id":page,"name":format!("home{page}"),"space":{"slot":page,"page":page}}));
        records.push(serde_json::json!({"t":"line","file":"home.s","line":page,"section":page,"offset":0,"length":8192}));
    }
    DebugSymbols::from_ndjson(
        &records
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        "home.debug198x",
    )
    .expect("HOME sidecar")
}

#[test]
fn every_dock_mask_matches_the_existing_memory_read_priority() {
    let mut memory = MemoryTimex::new();
    memory.load_rom_data(&[0x11; 16384]);
    memory.load_exrom_data(&[0x22; 8192]);
    for slot in 2..8u16 {
        memory.write(slot << 13, 0x33);
    }
    for exrom in [false, true] {
        memory.set_exrom_enabled(exrom);
        for mask in 0..=255u8 {
            memory.write_f4(mask);
            for slot in 0..8u16 {
                let (source, byte) = if mask & (1 << slot) != 0 {
                    (MemorySource::EmptyDock, 0xff)
                } else if slot == 0 && exrom {
                    (MemorySource::Exrom, 0x22)
                } else if slot < 2 {
                    (MemorySource::HomeRom, 0x11)
                } else {
                    (MemorySource::HomeRam, 0x33)
                };
                assert_eq!(memory.mapped_source(slot << 13), source);
                assert_eq!(memory.read(slot << 13), byte);
            }
        }
    }
}

#[test]
fn home_ram_windows_join_by_eight_k_page_on_pal_and_ntsc() {
    for model in [Model::TimexTC2068, Model::TimexTS2068] {
        for slot in 2..8u16 {
            let mut runtime = runtime(model);
            let address = (slot << 13) + 0x100;
            runtime.machine_mut().memory.write(address, 0x76);
            runtime.machine_mut().z80.regs.pc = address;
            let counts = runtime.profile_cycles(5000).expect("HOME RAM capture");
            conserved(&counts);
            assert_eq!(counts.mapped_addresses.len(), 1);
            assert_eq!(counts.mapped_addresses[0].mapping.slot, slot as u8);
            assert_eq!(
                counts.mapped_addresses[0].mapping.base,
                u32::from(slot << 13)
            );
            let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
            assert_eq!(profile.lines[0].source.line, u32::from(slot));
            assert_eq!(profile.unmapped_ticks, 0);
            assert_eq!(
                profile.clock.rate.numerator_hz,
                if model == Model::TimexTS2068 {
                    14_112_000
                } else {
                    14_000_000
                }
            );
        }
    }
}

#[test]
fn every_empty_dock_window_is_unmapped_execution_not_home_ram() {
    for model in [Model::TimexTC2068, Model::TimexTS2068] {
        for slot in 0..8u16 {
            let mut runtime = runtime(model);
            let address = (slot << 13) + 0x100;
            runtime.machine_mut().memory.write_f4(1 << slot);
            runtime.machine_mut().memory.write(address, 0x76); // ignored
            assert_eq!(runtime.machine().memory.read(address), 0xff);
            runtime.machine_mut().z80.regs.pc = address;
            let counts = runtime.profile_cycles(5000).expect("empty DOCK capture");
            conserved(&counts);
            let entry = counts
                .mapped_addresses
                .iter()
                .find(|entry| entry.address == u32::from(address))
                .expect("first RST instruction");
            assert_eq!(entry.mapping.memory, ProfileMemory::Unmapped);
            assert_eq!(entry.mapping.page, slot);
            let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
            assert!(
                profile
                    .addresses
                    .iter()
                    .find(|entry| entry.address == u32::from(address))
                    .expect("annotated RST")
                    .source
                    .is_none()
            );
        }
    }
}

#[test]
fn paging_out_own_home_code_preserves_the_out_instruction_source() {
    for model in [Model::TimexTC2068, Model::TimexTS2068] {
        let mut runtime = runtime(model);
        for (offset, byte) in [0x3e, 0x40, 0xd3, 0xf4, 0x76].into_iter().enumerate() {
            runtime
                .machine_mut()
                .memory
                .write(0xc000 + u16::try_from(offset).expect("small fixture"), byte);
        }
        runtime.machine_mut().z80.regs.pc = 0xc000;
        let counts = runtime
            .profile_cycles(1000)
            .expect("page code to empty DOCK");
        conserved(&counts);
        let out = counts
            .mapped_addresses
            .iter()
            .find(|entry| entry.address == 0xc002)
            .expect("OUT captured");
        assert_eq!(out.mapping.memory, ProfileMemory::Ram);
        assert_eq!(out.mapping.page, 6);
        let home_ticks = 28 + out.cost.ticks;
        assert!(out.cost.ticks >= 44); // Port $40F4 can be contended.
        let rst = counts
            .mapped_addresses
            .iter()
            .find(|entry| entry.address == 0xc004)
            .expect("DOCK RST captured");
        assert_eq!(rst.mapping.memory, ProfileMemory::Unmapped);
        let unmapped_ticks = rst.cost.ticks + counts.addresses[&0x38].ticks;
        let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&sources()));
        assert_eq!(profile.lines.len(), 1);
        assert_eq!(profile.lines[0].cost.ticks, home_ticks);
        assert_eq!(profile.unmapped_ticks, unmapped_ticks);
    }
}

#[test]
fn enabling_exrom_keeps_base_rom_costs_separate() {
    for model in [Model::TimexTC2068, Model::TimexTS2068] {
        let mut rom = [0; 16384];
        rom[..4].copy_from_slice(&[0x3e, 0x80, 0xd3, 0xff]);
        let mut exrom = [0; 8192];
        exrom[4] = 0x76;
        let mut runtime = TimexTS2068Runtime::new_ts2068(model, rom, exrom);
        let counts = runtime
            .profile_cycles(1000)
            .expect("enable EXROM through CPU I/O");
        conserved(&counts);
        assert_eq!(counts.mapped_addresses.len(), 3);
        assert_eq!(
            counts.mapped_addresses[1].mapping.memory,
            ProfileMemory::Rom
        );
        assert_eq!(
            counts.mapped_addresses[2].mapping.memory,
            ProfileMemory::RomOverlay
        );
        assert_eq!(counts.mapped_addresses[2].address, 4);
    }
}

#[test]
fn pal_and_ntsc_profile_preserves_machine_and_snapshot_state() {
    // Timex embeds ROM/RAM arrays; match the existing snapshot tests' stack.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            for model in [Model::TimexTC2068, Model::TimexTS2068] {
                let make = || {
                    let mut runtime = runtime(model);
                    for (offset, byte) in [0xc3, 0, 0x40].into_iter().enumerate() {
                        runtime
                            .machine_mut()
                            .memory
                            .write(0x4000 + u16::try_from(offset).expect("small fixture"), byte);
                    }
                    runtime.machine_mut().z80.regs.pc = 0x4000;
                    runtime
                };
                let mut profiled = make();
                let mut ordinary = make();
                let ticks = profiled.machine().frame_timing().halfcycles_per_frame * 2 + 3;
                let counts = profiled.profile_cycles(ticks).expect("contended capture");
                conserved(&counts);
                ordinary.machine_mut().advance_halfcycles(ticks);
                assert_eq!(
                    serde_json::to_value(profiled.machine()).expect("profiled state"),
                    serde_json::to_value(ordinary.machine()).expect("ordinary state")
                );
                assert!(
                    counts.addresses[&0x4000].ticks > counts.addresses[&0x4000].executions * 40
                );
                let snapshot = profiled.snapshot().expect("snapshot");
                let mut restored = make();
                restored.restore(&snapshot).expect("restore");
                assert_eq!(
                    profiled.profile_cycles(1000).expect("continued capture"),
                    restored.profile_cycles(1000).expect("restored capture")
                );
            }
        })
        .expect("snapshot worker")
        .join()
        .expect("snapshot checks");
}

#[test]
fn tc2048_reuses_flat_costs_and_every_family_model_advertises_profiling() {
    let mut runtime = TimexTC2048Runtime::blank();
    for (offset, &byte) in
        include_bytes!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.bin")
            .iter()
            .enumerate()
    {
        runtime
            .machine_mut()
            .memory
            .write(0xc000 + u16::try_from(offset).expect("small fixture"), byte);
    }
    runtime.machine_mut().z80.regs.pc = 0xc000;
    let counts = runtime.profile_cycles(260).expect("TC2048 fixture");
    conserved(&counts);
    assert!(counts.mapped_addresses.is_empty());
    assert_eq!(counts.addresses[&0xc003].ticks, 136);
    assert_eq!(counts.halt_ticks, 32);
    for profile in runtime_sinclair_zx_spectrum::profiles() {
        assert!(
            profile
                .capabilities
                .contains(&emu198x_shell::known_capability("cycle-profile")),
            "{}",
            profile.display_name
        );
    }
}
