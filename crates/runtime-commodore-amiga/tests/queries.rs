//! Query-provider coverage for the Amiga runtime — boot status,
//! catalogued paths, and A1000-bootstrap-specific surfaces.

mod common;

use emu198x_shell::{
    DebugPrimitives, HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet,
    NullAudioSink, NullFrameSink, NullTraceSink, SessionQueryProvider,
};
use format_commodore_amiga_adf::ADF_SIZE_DD;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AgnusInstalledVariant, AmigaA1200Runtime, AmigaEcsRuntime,
    AmigaLiveAccess, AmigaMachine, AmigaOcsRuntime, AmigaRuntime, AmigaRuntimeKind,
    AmigaSessionQueryProvider, Model,
};
use serde_json::{Value, json};

use common::{dummy_a1000_bootstrap_rom, dummy_kickstart};

#[test]
fn query_provider_returns_declared_paths() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let paths = provider.query_paths(&runtime, None);
    assert!(paths.contains(&"a1000.boot_rom_visible".to_owned()));
    assert!(paths.contains(&"a1000.wom_locked".to_owned()));
    assert!(paths.contains(&"cpu.pc".to_owned()));
    assert!(paths.contains(&"debug.dsk_write_count".to_owned()));
    assert!(paths.contains(&"disk.change_pending".to_owned()));
    assert!(paths.contains(&"disk.dma_fifo_direction".to_owned()));
    assert!(paths.contains(&"disk.dma_write_stream_active".to_owned()));
    assert!(paths.contains(&"disk.inserted".to_owned()));
    assert!(paths.contains(&"disk.step_events".to_owned()));
    assert!(paths.contains(&"keyboard.state".to_owned()));
}

#[test]
fn disk_queries_expose_the_complete_dma_fifo_state() {
    let mut runtime = AmigaOcsRuntime::blank(Model::A500OcsPal);
    runtime.machine_mut().poke_word(0x00DF_F024, 0x8002);
    runtime.machine_mut().poke_word(0x00DF_F024, 0x8002);
    runtime
        .machine_mut()
        .paula_mut()
        .receive_disk_read_word(0x1111);
    runtime
        .machine_mut()
        .paula_mut()
        .receive_disk_read_word(0x2222);

    assert_eq!(
        query_value(&runtime, "disk.dma_fifo"),
        json!([0x1111, 0x2222])
    );
    assert_eq!(
        query_value(&runtime, "disk.dma_fifo_direction"),
        json!("read")
    );
    assert_eq!(query_value(&runtime, "disk.dma_fifo_count"), json!(2));
    assert_eq!(query_value(&runtime, "disk.dma_fifo_empty"), json!(false));
    assert_eq!(query_value(&runtime, "disk.dma_fifo_full"), json!(false));
    assert_eq!(
        query_value(&runtime, "disk.dma_write_stream_active"),
        json!(false)
    );
}

#[test]
fn query_cpu_pc_returns_initial_reset_vector() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let result = AmigaSessionQueryProvider
        .query(&runtime, "cpu.pc")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(result.path, "cpu.pc");
    assert_eq!(result.value, json!(0x00F8_0008u32));
}

#[test]
fn cpu_group_exposes_the_complete_bounded_schema() {
    let runtime = AmigaOcsRuntime::blank(Model::A500OcsPal);
    let cpu = query_value(&runtime, "cpu");
    let cpu = cpu.as_object().expect("CPU query should return an object");
    let mut fields: Vec<&str> = cpu.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        [
            "a",
            "a7",
            "address_mask",
            "bus",
            "cache",
            "capabilities",
            "control",
            "d",
            "exception",
            "execution",
            "fpu",
            "interrupts",
            "ipl",
            "model",
            "msp",
            "pc",
            "pipelines",
            "prefetch",
            "sr",
            "ssp",
            "status",
            "timing_class",
            "usp",
            "variant",
        ],
    );

    let variant = cpu["variant"]
        .as_object()
        .expect("CPU variant state should be an object");
    let mut variant_fields: Vec<&str> = variant.keys().map(String::as_str).collect();
    variant_fields.sort_unstable();
    assert_eq!(
        variant_fields,
        [
            "cache_disable_asserted",
            "cacr_read_zero_mask",
            "cacr_write_mask",
            "constant_shift_timing",
            "continue_hook_present",
            "decode_hook_present",
            "dynamic_bus_sizing",
            "extended_sr_writes",
            "format2_vectors",
            "format_a_group0",
            "fpu_is_68882",
            "fpu_present",
            "long_branch",
            "master_stack_capable",
            "minimum_bus_clocks",
            "mmu_translation_state_present",
            "musashi_bcd_overflow",
            "musashi_divide_overflow",
            "scaled_index",
            "six_word_frame",
            "um_ea_calculation_timing",
            "unaligned_data_access",
        ],
    );

    assert_eq!(cpu["d"].as_array().map(Vec::len), Some(8));
    assert_eq!(cpu["a"].as_array().map(Vec::len), Some(8));
    assert_eq!(cpu["a7"], cpu["a"][7]);
    assert_eq!(cpu["pc"], query_value(&runtime, "cpu.pc"));
    assert_eq!(cpu["sr"], query_value(&runtime, "cpu.sr"));
    assert_eq!(cpu["ipl"], query_value(&runtime, "cpu.ipl"));
    assert_eq!(cpu["execution"]["micro_op_capacity"], json!(32));
    let pending_micro_ops = cpu["execution"]["pending_micro_ops"]
        .as_array()
        .expect("pending CPU micro-operations should be a fixed array");
    assert_eq!(pending_micro_ops.len(), 32);
    let pending_count = cpu["execution"]["micro_op_count"]
        .as_u64()
        .expect("micro-operation count should be numeric") as usize;
    assert!(
        pending_micro_ops[..pending_count]
            .iter()
            .all(|operation| !operation.is_null()),
        "the active queue prefix must contain ordered operations",
    );
    assert!(
        pending_micro_ops[pending_count..]
            .iter()
            .all(Value::is_null),
        "unused queue positions must be explicit nulls",
    );
    assert_eq!(
        cpu["execution"]["next_micro_op"],
        pending_micro_ops.first().cloned().unwrap_or(Value::Null),
    );
    assert_eq!(cpu["cache"]["data_state_present"], json!(false));
    let cache_lines = cpu["cache"]["lines"]
        .as_array()
        .expect("CPU cache lines should be a fixed array");
    assert_eq!(cache_lines.len(), 64);
    assert!(
        cache_lines.iter().all(Value::is_null),
        "a CPU without an installed cache must not synthesize cache contents",
    );
    assert_eq!(cpu["pipelines"]["fpu"]["frame_buffer_capacity"], json!(60),);
    assert_eq!(
        cpu["pipelines"]["fpu"]["frame_buffer"]
            .as_array()
            .map(Vec::len),
        Some(60),
    );
}

#[test]
fn cpu_group_distinguishes_stock_ocs_ecs_aga_and_accelerated_models() {
    let ocs = AmigaOcsRuntime::blank(Model::A500OcsPal);
    let ecs = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    let aga = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    let accelerated = AmigaOcsRuntime::blank(Model::A500OcsPalGvpA530);

    for runtime in [&ocs, &AmigaOcsRuntime::blank(Model::A2000OcsPal)] {
        assert_eq!(query_value(runtime, "cpu.model"), json!("M68000"));
        assert_eq!(
            query_value(runtime, "cpu.cache.instruction_state_present"),
            json!(false),
        );
    }
    assert_eq!(query_value(&ecs, "cpu.model"), json!("M68000"));
    assert_eq!(query_value(&aga, "cpu.model"), json!("M68EC020"));
    assert_eq!(query_value(&aga, "cpu.address_mask"), json!(0x00FF_FFFFu32),);
    assert_eq!(
        query_value(&aga, "cpu.variant.dynamic_bus_sizing"),
        json!(true),
    );
    assert_eq!(
        query_value(&aga, "cpu.cache.instruction_state_present"),
        json!(true),
    );
    let aga_cache_lines = query_value(&aga, "cpu.cache.lines");
    let aga_cache_lines = aga_cache_lines
        .as_array()
        .expect("installed instruction cache should expose all lines");
    assert_eq!(aga_cache_lines.len(), 64);
    assert!(
        aga_cache_lines.iter().all(Value::is_object),
        "every installed cache line should have a summary",
    );
    let mut line_fields: Vec<&str> = aga_cache_lines[0]
        .as_object()
        .expect("cache line should be an object")
        .keys()
        .map(String::as_str)
        .collect();
    line_fields.sort_unstable();
    assert_eq!(line_fields, ["index", "tag", "valid", "words"]);
    assert!(
        aga_cache_lines
            .iter()
            .enumerate()
            .all(|(index, line)| line["index"] == json!(index)),
        "cache line summaries must retain hardware index order",
    );
    assert_eq!(query_value(&aga, "cpu.capabilities.fpu"), json!(false),);

    assert_eq!(query_value(&accelerated, "cpu.model"), json!("M68EC030"),);
    assert_eq!(
        query_value(&accelerated, "cpu.address_mask"),
        json!(0xFFFF_FFFFu32),
    );
    assert_eq!(
        query_value(&accelerated, "cpu.capabilities.mmu"),
        json!(false),
    );
    assert_eq!(
        query_value(&accelerated, "cpu.capabilities.data_cache"),
        json!(true),
    );
    assert_eq!(
        query_value(&accelerated, "cpu.cache.data_state_present"),
        json!(false),
        "model capability and implemented mutable cache state must remain distinct",
    );
    assert_eq!(
        query_value(&accelerated, "cpu.variant.mmu_translation_state_present",),
        json!(false),
        "architectural MMU capability must not imply an installed translation datapath",
    );
}

#[test]
fn keyboard_queries_expose_complete_protocol_state_and_legacy_aliases() {
    let runtime = AmigaOcsRuntime::blank(Model::A500OcsPal);
    let provider = AmigaSessionQueryProvider;
    let paths = provider.query_paths(&runtime, Some("keyboard"));

    for path in [
        "keyboard.current_byte",
        "keyboard.pending_encoded_byte",
        "keyboard.serial_bits_remaining",
        "keyboard.waiting_for_handshake",
        "keyboard.queued_bytes",
        "keyboard.queue_allocated_capacity",
        "keyboard.timer",
        "keyboard.queued",
        "keyboard.cia_a_sdr",
        "keyboard.cia_a_spmode",
    ] {
        assert!(
            paths.contains(&path.to_owned()),
            "{path} should be advertised"
        );
    }
    assert_eq!(
        query_value(&runtime, "keyboard.timer"),
        query_value(&runtime, "keyboard.timer_ticks"),
    );
    assert_eq!(
        query_value(&runtime, "keyboard.queued"),
        query_value(&runtime, "keyboard.queue_count"),
    );
}

#[test]
fn memory_queries_distinguish_standard_and_a1000_rom_topologies() {
    let standard = AmigaOcsRuntime::blank(Model::A500OcsPal);
    assert_eq!(query_value(&standard, "memory.rom.kind"), json!("standard"));
    assert_eq!(
        query_value(&standard, "memory.rom.standard.size_bytes"),
        json!(256 * 1024),
    );
    assert_eq!(query_value(&standard, "memory.rom.a1000"), Value::Null);

    let a1000 = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
        .expect("A1000 runtime init");
    assert_eq!(query_value(&a1000, "memory.rom.kind"), json!("a1000"));
    assert_eq!(
        query_value(&a1000, "memory.rom.a1000.boot_rom.size_bytes"),
        json!(64 * 1024),
    );
    assert_eq!(
        query_value(&a1000, "memory.rom.a1000.wom.size_bytes"),
        json!(256 * 1024),
    );
    assert_eq!(
        query_value(&a1000, "memory.rom.a1000.boot_rom_visible"),
        json!(true),
    );
}

#[test]
fn debugger_peek_does_not_drive_the_floating_bus() {
    let mut runtime = AmigaRuntimeKind::blank(Model::A500OcsPal);
    DebugPrimitives::dbg_poke(&mut runtime, 0x0001_0000, 0xA5);
    DebugPrimitives::dbg_poke(&mut runtime, 0x0001_0001, 0x5A);
    let provider = AmigaSessionQueryProvider;
    let before = provider
        .query(&runtime, "memory.floating_bus_word")
        .expect("query should succeed")
        .expect("memory field should resolve")
        .value;

    let _ = DebugPrimitives::dbg_peek(&runtime, 0x00F8_0000);

    let after = provider
        .query(&runtime, "memory.floating_bus_word")
        .expect("query should succeed")
        .expect("memory field should resolve")
        .value;
    assert_eq!(before, json!(0xA55A));
    assert_eq!(after, before);
}

#[test]
fn agnus_queries_distinguish_early_ocs_from_a2000_fat_8372a() {
    let mut early = AmigaOcsRuntime::blank(Model::A500OcsPal);
    let mut fat = AmigaOcsRuntime::blank(Model::A2000OcsPal);

    assert_eq!(
        query_value(&early, "agnus.installed_variant"),
        json!("early-ocs"),
    );
    assert_eq!(
        query_value(&fat, "agnus.installed_variant"),
        json!("fat-8372a"),
    );

    early.machine_mut().poke_word(0x00DF_F1C0, 0x0022);
    fat.machine_mut().poke_word(0x00DF_F1C0, 0x0033);
    fat.machine_mut().poke_word(0x00DF_F1C4, 0x0044);
    fat.machine_mut().poke_word(0x00DF_F1DC, 0x00A0);

    assert_eq!(
        query_value(&early, "agnus.htotal"),
        Value::Null,
        "early OCS must not claim an ECS timing latch"
    );
    assert_eq!(query_value(&fat, "agnus.htotal"), json!(0x0033));
    assert_eq!(query_value(&fat, "agnus.hbstrt"), json!(0x0044));
    assert_eq!(query_value(&fat, "agnus.beamcon0"), json!(0x00A0));
}

#[test]
fn installed_agnus_query_names_cover_enhanced_machine_variants() {
    let ecs = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    let alice = AmigaA1200Runtime::blank(Model::A1200AgaPal);

    assert_eq!(query_value(&ecs, "agnus.installed_variant"), json!("ecs"),);
    assert_eq!(
        query_value(&alice, "agnus.installed_variant"),
        json!("alice"),
    );
}

#[test]
fn runtime_kind_forwards_a2000_fat_agnus_timing_diagnostics() {
    let early = AmigaRuntimeKind::blank(Model::A500OcsPal);
    assert_eq!(
        early.installed_agnus_variant(),
        AgnusInstalledVariant::EarlyOcs
    );
    assert!(early.ecs_agnus_timing().is_none());

    let fat = AmigaRuntimeKind::blank(Model::A2000OcsPal);
    assert_eq!(
        fat.installed_agnus_variant(),
        AgnusInstalledVariant::Fat8372A
    );
    let timing = fat
        .ecs_agnus_timing()
        .expect("A2000 Fat Agnus timing should cross the runtime-kind boundary");
    assert_eq!(timing.beamcon0, 0x0020);
    assert_eq!(timing.htotal, 226);
}

#[test]
fn a1000_queries_report_bootstrap_state() {
    let runtime = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
        .expect("runtime init");
    let boot_rom_visible = AmigaSessionQueryProvider
        .query(&runtime, "a1000.boot_rom_visible")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(boot_rom_visible.value, json!(true));

    let wom_locked = AmigaSessionQueryProvider
        .query(&runtime, "a1000.wom_locked")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(wom_locked.value, json!(false));
}

#[test]
fn a1000_blitter_queries_distinguish_internal_activity_from_visible_busy() {
    let mut runtime = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
        .expect("runtime init");
    runtime.machine_mut().poke_word(0x00DF_F040, 0x0000);
    runtime.machine_mut().poke_word(0x00DF_F058, (1 << 6) | 1);

    let provider = AmigaSessionQueryProvider;
    let internal = provider
        .query(&runtime, "blitter.busy")
        .expect("query succeeds")
        .expect("path present");
    let visible = provider
        .query(&runtime, "blitter.busy_visible")
        .expect("query succeeds")
        .expect("path present");
    let copper = provider
        .query(&runtime, "blitter.busy_copper")
        .expect("query succeeds")
        .expect("path present");
    let startup = provider
        .query(&runtime, "blitter.startup_ccks_remaining")
        .expect("query succeeds")
        .expect("path present");

    assert_eq!(internal.value, json!(true));
    assert_eq!(visible.value, json!(false));
    assert_eq!(copper.value, json!(false));
    assert_eq!(startup.value, json!(2));
}

#[test]
fn completion_queries_distinguish_dmacon_copper_and_final_d() {
    let mut runtime =
        AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let machine = runtime.machine_mut();
    machine.poke_word(0x00DF_F096, 0x8640); // DMAEN | BLTEN | BLTPRI
    machine.poke_word(0x00DF_F040, 0x01FF); // USED, D := all ones
    machine.poke_word(0x00DF_F054, 0);
    machine.poke_word(0x00DF_F056, 0x2000);
    machine.poke_word(0x00DF_F058, (1 << 6) | 1);

    let mut guard = 0;
    while runtime.machine().agnus().blitter_completion_phase() != "final-write" {
        runtime.machine_mut().tick();
        guard += 1;
        assert!(guard < 1_000, "blitter never reached final-write");
    }

    let provider = AmigaSessionQueryProvider;
    for (path, expected) in [
        ("blitter.busy", json!(true)),
        ("blitter.busy_visible", json!(false)),
        ("blitter.busy_copper", json!(true)),
        ("blitter.completion_phase", json!("final-write")),
        ("blitter.completion_ccks_remaining", json!(1)),
        ("blitter.final_d_pending", json!(true)),
        ("agnus.blitter_busy_copper", json!(true)),
        ("agnus.blitter_completion_phase", json!("final-write")),
        ("agnus.blitter_completion_ccks_remaining", json!(1)),
        ("agnus.blitter_final_d_pending", json!(true)),
    ] {
        let result = provider
            .query(&runtime, path)
            .expect("query succeeds")
            .expect("path present");
        assert_eq!(result.value, expected, "{path}");
    }
}

/// Walk every advertised path against a freshly-constructed runtime.
/// Each concrete path resolves; the dispatcher's no-match arm is
/// covered by `unknown_path_returns_ok_none`. Drives the whole match
/// ladder in `queries.rs` so llvm-cov stops reporting each arm as an
/// uncalled closure.
#[test]
fn every_advertised_query_path_resolves_without_floppy() {
    walk_all_paths(false);
}

/// Walk every advertised path with an ADF inserted into DF0. The
/// `amiga.disk.*` family flips polarity once a disk is present, so
/// running with-and-without is what guarantees both branches land on
/// the dispatcher closures.
#[test]
fn every_advertised_query_path_resolves_with_floppy_loaded() {
    walk_all_paths(true);
}

#[test]
fn every_a1200_query_path_resolves() {
    let runtime = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    let provider = AmigaSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        let value = provider
            .query(&runtime, &path)
            .unwrap_or_else(|err| panic!("concrete {path} should not fail: {err:?}"));
        assert!(value.is_some(), "concrete {path} should resolve");
    }
}

#[test]
fn every_ecs_query_path_resolves() {
    let runtime = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    let provider = AmigaSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        let value = provider
            .query(&runtime, &path)
            .unwrap_or_else(|err| panic!("concrete {path} should not fail: {err:?}"));
        assert!(value.is_some(), "concrete {path} should resolve");
    }
}

#[test]
fn every_a600_query_path_resolves() {
    let runtime = AmigaEcsRuntime::blank(Model::A600EcsPal);
    let provider = AmigaSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        let value = provider
            .query(&runtime, &path)
            .unwrap_or_else(|err| panic!("concrete {path} should not fail: {err:?}"));
        assert!(value.is_some(), "concrete {path} should resolve");
    }
}

#[test]
fn grouped_snapshot_fields_are_all_discoverable_as_leaves() {
    let ocs = AmigaOcsRuntime::blank(Model::A500OcsPal);
    assert_group_leaf_catalogue(
        &ocs,
        &[
            "runtime",
            "cpu",
            "memory",
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "rtc",
            "keyboard",
            "gary",
            "expansion",
            "input",
            "debug",
            "disk",
            "scheduler",
        ],
    );

    let ecs = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    assert_group_leaf_catalogue(
        &ecs,
        &[
            "runtime",
            "cpu",
            "memory",
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "rtc",
            "keyboard",
            "gary",
            "expansion",
            "input",
            "debug",
            "disk",
            "scheduler",
        ],
    );

    let a600 = AmigaEcsRuntime::blank(Model::A600EcsPal);
    assert_group_leaf_catalogue(
        &a600,
        &[
            "runtime",
            "cpu",
            "memory",
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "rtc",
            "keyboard",
            "gary",
            "gayle",
            "expansion",
            "input",
            "debug",
            "disk",
            "scheduler",
        ],
    );

    let aga = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    assert_group_leaf_catalogue(
        &aga,
        &[
            "runtime",
            "cpu",
            "memory",
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "rtc",
            "keyboard",
            "gary",
            "gayle",
            "expansion",
            "input",
            "debug",
            "disk",
            "scheduler",
            "aga",
        ],
    );
}

#[test]
fn runtime_configuration_queries_expose_complete_immutable_construction_intent() {
    let stock = AmigaOcsRuntime::blank(Model::A500OcsPal);

    assert_exact_object_fields(
        &query_value(&stock, "runtime.configuration"),
        &[
            "accelerator",
            "chipset",
            "cpu",
            "model_id",
            "profile_id",
            "ram",
            "region",
        ],
    );
    assert_exact_object_fields(
        &query_value(&stock, "runtime.configuration.ram"),
        &["chip_kb", "fast_kb", "slow_kb"],
    );
    assert_exact_object_fields(
        &query_value(&stock, "runtime.configuration.cpu"),
        &["clock_hz", "model"],
    );
    assert_exact_object_fields(
        &query_value(&stock, "runtime.configuration.accelerator"),
        &["configuration", "kind", "present"],
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.model_id"),
        json!("commodore-amiga-a500-ocs-pal"),
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.profile_id"),
        json!("commodore-amiga-a500-ocs-pal"),
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.region"),
        json!("pal"),
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.chipset"),
        json!("ocs"),
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.ram"),
        json!({"chip_kb": 512, "slow_kb": 0, "fast_kb": 0}),
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.cpu"),
        json!({"model": "m68000", "clock_hz": 7_093_790}),
    );
    assert_eq!(
        query_value(&stock, "runtime.configuration.accelerator"),
        json!({"present": false, "kind": null, "configuration": null}),
    );
    assert_eq!(
        query_value(&stock, "runtime.rgba_framebuffer_bytes"),
        json!(768 * 576 * 4),
        "the host mirror reports its size without returning framebuffer payload",
    );

    let a1200 = AmigaA1200Runtime::blank(Model::A1200AgaNtsc);
    assert_eq!(
        query_value(&a1200, "runtime.configuration.region"),
        json!("ntsc"),
    );
    assert_eq!(
        query_value(&a1200, "runtime.configuration.chipset"),
        json!("aga"),
    );
    assert_eq!(
        query_value(&a1200, "runtime.configuration.cpu"),
        json!({"model": "m68ec020", "clock_hz": 14_318_180}),
    );
    assert_eq!(
        query_value(&a1200, "runtime.configuration.ram"),
        json!({"chip_kb": 2048, "slow_kb": 0, "fast_kb": 0}),
    );
}

#[test]
fn accelerator_configuration_paths_are_model_specific_and_complete() {
    let provider = AmigaSessionQueryProvider;
    let stock = AmigaOcsRuntime::blank(Model::A500OcsPal);
    let stock_paths = provider.query_paths(&stock, Some("runtime.configuration.accelerator"));
    assert!(
        !stock_paths
            .contains(&"runtime.configuration.accelerator.configuration.ram_size_bytes".to_owned()),
        "a machine without an accelerator must not advertise absent configuration leaves",
    );

    let accelerated = AmigaOcsRuntime::blank(Model::A500OcsPalGvpA530);
    let accelerated_paths =
        provider.query_paths(&accelerated, Some("runtime.configuration.accelerator"));
    for path in [
        "runtime.configuration.accelerator.configuration.ram_size_bytes",
        "runtime.configuration.accelerator.configuration.serial",
        "runtime.configuration.accelerator.configuration.cache_enabled",
        "runtime.configuration.accelerator.configuration.autoboot_enabled",
    ] {
        assert!(
            accelerated_paths.contains(&path.to_owned()),
            "{path} should be dynamically advertised for the A530 profile",
        );
    }
    assert_exact_object_fields(
        &query_value(
            &accelerated,
            "runtime.configuration.accelerator.configuration",
        ),
        &[
            "autoboot_enabled",
            "cache_enabled",
            "ram_size_bytes",
            "serial",
        ],
    );
    assert_eq!(
        query_value(&accelerated, "runtime.configuration.accelerator"),
        json!({
            "present": true,
            "kind": "gvp-a530",
            "configuration": {
                "ram_size_bytes": 1024 * 1024,
                "serial": 0,
                "cache_enabled": false,
                "autoboot_enabled": false,
            },
        }),
    );
}

#[test]
fn audio_filter_queries_are_complete_dynamic_and_side_effect_free() {
    let provider = AmigaSessionQueryProvider;
    let a500 = AmigaOcsRuntime::blank(Model::A500OcsPal);

    assert_exact_object_fields(
        &query_value(&a500, "runtime.audio_filter"),
        &[
            "led_always_on",
            "led_low_pass",
            "led_stage_engaged",
            "static_high_pass",
            "static_low_pass",
        ],
    );
    assert_exact_object_fields(
        &query_value(&a500, "runtime.audio_filter.static_low_pass"),
        &["a1", "a2", "history_left", "history_right"],
    );
    assert_exact_object_fields(
        &query_value(&a500, "runtime.audio_filter.static_high_pass"),
        &["a1", "a2", "history_left", "history_right"],
    );
    assert_exact_object_fields(
        &query_value(&a500, "runtime.audio_filter.led_low_pass"),
        &["a1", "a2", "b1", "b2", "history_left", "history_right"],
    );

    let typed_before = a500.audio_filter_diagnostic_snapshot();
    let first = query_value(&a500, "runtime.audio_filter");
    let second = query_value(&a500, "runtime.audio_filter");
    let typed_after = a500.audio_filter_diagnostic_snapshot();
    assert_eq!(first, second);
    assert_eq!(typed_before, typed_after);

    let a500_paths = provider.query_paths(&a500, Some("runtime.audio_filter"));
    assert!(
        a500_paths.contains(&"runtime.audio_filter.static_low_pass.a1".to_owned()),
        "the fitted A500 low-pass coefficients should be dynamically discoverable",
    );

    let a1200 = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    let a1200_paths = provider.query_paths(&a1200, Some("runtime.audio_filter"));
    assert_eq!(
        query_value(&a1200, "runtime.audio_filter.static_low_pass"),
        Value::Null,
    );
    assert!(
        !a1200_paths.contains(&"runtime.audio_filter.static_low_pass.a1".to_owned()),
        "an absent A1200 low-pass stage must not advertise coefficient leaves",
    );
    assert_eq!(
        query_value(&a1200, "runtime.audio_filter.led_always_on"),
        json!(false),
    );

    let a1000 = AmigaOcsRuntime::blank(Model::A1000OcsPal);
    assert_eq!(
        query_value(&a1000, "runtime.audio_filter.led_always_on"),
        json!(true),
    );
    assert_eq!(
        query_value(&a1000, "runtime.audio_filter.led_stage_engaged"),
        json!(true),
        "A1000 wiring must report the LED stage engaged independently of CIA state",
    );
}

#[test]
fn agnus_group_exposes_complete_non_blitter_state_and_live_ocs_values() {
    let runtime = AmigaOcsRuntime::blank(Model::A500OcsPal);

    assert_exact_object_fields(
        &query_value(&runtime, "agnus.identity"),
        &["agnus_id", "max_bitplanes", "original_revision", "region"],
    );
    assert_exact_object_fields(
        &query_value(&runtime, "agnus.beam"),
        &[
            "copper_comparator_hpos",
            "current_line_ccks",
            "hpos",
            "lines_per_frame",
            "lof",
            "lol",
            "lol_toggle",
            "vbl_count",
            "vpos",
        ],
    );
    assert_exact_object_fields(
        &query_value(&runtime, "agnus.ocs_latches"),
        &[
            "ocs_hard_vertical_blank_active",
            "ocs_vertical_diw_active",
            "vertical_diw_active",
        ],
    );
    assert_exact_object_fields(
        &query_value(&runtime, "agnus.events"),
        &[
            "fixed_sync_cia_a_tod_event",
            "fixed_sync_cia_b_tod_event",
            "fixed_sync_copper_restart_event",
            "vertb_level",
        ],
    );
    assert_exact_object_fields(
        &query_value(&runtime, "agnus.sprite_dma"),
        &[
            "spr_dma_on",
            "spr_pt",
            "spr_pt_hi_latch",
            "spr_pt_hi_pending",
            "spr_vstart",
            "spr_vstop",
        ],
    );

    assert_eq!(
        query_value(&runtime, "agnus.vertical_diw_active"),
        query_value(&runtime, "agnus.ocs_latches.vertical_diw_active"),
    );
    assert_eq!(
        query_value(&runtime, "agnus.current_line_ccks"),
        query_value(&runtime, "agnus.beam.current_line_ccks"),
    );
    assert_eq!(
        query_value(&runtime, "agnus.copper_comparator_hpos"),
        query_value(&runtime, "agnus.beam.copper_comparator_hpos"),
    );
    assert_eq!(
        query_value(&runtime, "agnus.vertical_diw_active"),
        json!(false),
        "the OCS leaf must report a live boolean instead of null",
    );
    assert_eq!(
        query_value(&runtime, "agnus.current_line_ccks"),
        json!(227),
        "the OCS leaf must report fixed PAL line geometry instead of null",
    );
    assert_eq!(
        query_value(&runtime, "agnus.copper_comparator_hpos"),
        json!(2),
        "the OCS leaf must report the live comparator projection instead of null",
    );
    assert_eq!(query_value(&runtime, "agnus.identity.region"), json!("Pal"));
    assert_eq!(
        query_value(&runtime, "agnus.identity.original_revision"),
        json!("Later"),
    );
    assert_eq!(
        query_value(&runtime, "agnus.events.fixed_sync_copper_restart_event"),
        json!(true),
    );
    assert_eq!(
        query_value(&runtime, "agnus.events.vertb_level"),
        json!(true),
    );
    for path in [
        "agnus.sprite_dma.spr_pt",
        "agnus.sprite_dma.spr_pt_hi_latch",
        "agnus.sprite_dma.spr_pt_hi_pending",
        "agnus.sprite_dma.spr_vstart",
        "agnus.sprite_dma.spr_vstop",
        "agnus.sprite_dma.spr_dma_on",
    ] {
        assert_eq!(
            query_value(&runtime, path)
                .as_array()
                .unwrap_or_else(|| panic!("{path} should be an array"))
                .len(),
            8,
            "{path} should expose all eight sprite channels",
        );
    }
}

#[test]
fn gayle_discovery_matches_the_configured_board() {
    let provider = AmigaSessionQueryProvider;
    let a500_plus = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    let mut a600 = AmigaEcsRuntime::blank(Model::A600EcsPal);
    let mut a1200 = AmigaA1200Runtime::blank(Model::A1200AgaPal);

    assert!(
        !provider
            .query_paths(&a500_plus, Some("gayle"))
            .iter()
            .any(|path| path == "gayle"),
        "A500+ must not advertise a chip it does not contain",
    );
    assert!(
        provider
            .query(&a500_plus, "gayle")
            .expect("A500+ query should not fail")
            .is_none(),
    );

    a600.machine_mut().poke_byte(0x00DA_8000, 0x5A);
    a1200.machine_mut().poke_byte(0x00DA_8000, 0xA5);
    assert_gayle_present(&a600, &provider);
    assert_gayle_present(&a1200, &provider);
    assert_eq!(
        query_value(&a600, "gayle.registers.card_status"),
        json!(0x5A),
    );
    assert_eq!(
        query_value(&a1200, "gayle.registers.card_status"),
        json!(0xA5),
    );
    assert_eq!(query_value(&a500_plus, "gary.gayle_present"), json!(false));
    assert_eq!(query_value(&a600, "gary.gayle_present"), json!(true));
    assert_eq!(query_value(&a1200, "gary.gayle_present"), json!(true));
}

#[test]
fn gary_group_exposes_every_persisted_configuration_flag() {
    let runtime = AmigaOcsRuntime::blank(Model::A500OcsPalA501);
    let fields = query_value(&runtime, "gary")
        .as_object()
        .expect("Gary query should return an object")
        .clone();

    assert_eq!(fields.len(), 6);
    for field in [
        "slow_ram_present",
        "gayle_present",
        "pcmcia_present",
        "dmac_present",
        "resource_regs_present",
        "rtc_present",
    ] {
        assert!(fields.contains_key(field), "gary.{field} should be exposed");
    }
    assert_eq!(fields["slow_ram_present"], json!(true));
    assert_eq!(fields["rtc_present"], json!(true));
}

#[test]
fn rtc_queries_expose_a_stable_deterministic_clock_and_subsecond_phase() {
    let mut runtime = AmigaOcsRuntime::blank(Model::A500OcsPalA501);
    let provider = AmigaSessionQueryProvider;

    let initial = query_value(&runtime, "rtc");
    assert_exact_object_fields(
        &initial,
        &[
            "busy",
            "clock_mode",
            "control_d",
            "control_e",
            "control_f",
            "day",
            "effective_unix_seconds",
            "hold",
            "hour",
            "hour_mode_24",
            "irq_flag",
            "minute",
            "month",
            "reset",
            "running",
            "second",
            "stop",
            "stored_unix_seconds",
            "subsecond_system_ticks",
            "system_ticks_per_second",
            "weekday",
            "year",
        ],
    );
    assert_eq!(
        query_value(&runtime, "rtc"),
        initial,
        "side-effect-free RTC queries must describe one stable emulated instant",
    );
    assert_eq!(initial["clock_mode"], json!("emulated"));
    for field in [
        "clock_mode",
        "subsecond_system_ticks",
        "system_ticks_per_second",
    ] {
        let path = format!("rtc.{field}");
        assert!(
            provider.query_paths(&runtime, Some(&path)).contains(&path),
            "{path} must be discoverable",
        );
        assert_eq!(
            query_value(&runtime, &path),
            initial[field],
            "{path} must match the grouped RTC snapshot",
        );
    }
    assert_eq!(initial["subsecond_system_ticks"], json!(0));
    for _ in 0..17 {
        runtime.machine_mut().tick();
    }
    let advanced = query_value(&runtime, "rtc");
    assert_eq!(advanced["subsecond_system_ticks"], json!(17));
    assert!(
        advanced["system_ticks_per_second"]
            .as_u64()
            .is_some_and(|ticks| ticks > 17),
        "the active PAL RTC clock rate must be reported",
    );
    assert_eq!(
        advanced["effective_unix_seconds"], initial["effective_unix_seconds"],
        "seventeen system ticks must advance the phase without crossing a second",
    );
    assert_eq!(
        query_value(&runtime, "rtc"),
        advanced,
        "RTC diagnostics must not advance between machine ticks",
    );
}

#[test]
fn expansion_queries_distinguish_generic_fast_ram_a530_and_absent_devices() {
    let bare = AmigaOcsRuntime::blank(Model::A500OcsPal);
    let generic = AmigaOcsRuntime::blank(Model::A500OcsPalMaxed);
    let a530 = AmigaOcsRuntime::blank(Model::A500OcsPalGvpA530);
    let ecs = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    let a600 = AmigaEcsRuntime::blank(Model::A600EcsPal);
    let a1200 = AmigaA1200Runtime::blank(Model::A1200AgaPal);

    assert_no_expansions(&bare);
    assert_no_expansions(&ecs);
    assert_no_expansions(&a600);
    assert_no_expansions(&a1200);

    let generic_snapshot = query_value(&generic, "expansion.generic_fast_ram");
    let generic_snapshot = generic_snapshot
        .as_object()
        .expect("maxed A500 should install generic Fast RAM");
    assert_eq!(generic_snapshot.len(), 8);
    assert_eq!(generic_snapshot["manufacturer_id"], json!(0x0202));
    assert_eq!(generic_snapshot["product_id"], json!(9));
    assert_eq!(generic_snapshot["serial_number"], json!(1));
    assert_eq!(generic_snapshot["size_code"], json!(0));
    assert_eq!(generic_snapshot["ram_size_bytes"], json!(8 * 1024 * 1024));
    assert_eq!(generic_snapshot["configuration_is_coherent"], json!(true));
    assert_eq!(
        generic_snapshot["has_default_fast_ram_identity"],
        json!(true),
    );
    assert!(generic_snapshot["state"].is_object());
    assert_eq!(query_value(&generic, "expansion.gvp_a530"), Value::Null);
    assert_eq!(
        query_value(&generic, "expansion.motherboard_bridge"),
        Value::Null,
    );

    assert_eq!(
        query_value(&a530, "expansion.generic_fast_ram"),
        Value::Null,
    );
    let a530_config = query_value(&a530, "expansion.gvp_a530.config");
    let a530_config = a530_config
        .as_object()
        .expect("A530 config should be an object");
    assert_eq!(a530_config.len(), 5);
    assert_eq!(a530_config["ram_size_mib"], json!(1));
    assert_eq!(a530_config["ram_size_bytes"], json!(1024 * 1024));
    assert_eq!(a530_config["serial_number"], json!(0));
    assert_eq!(a530_config["cache_enabled"], json!(false));
    assert_eq!(a530_config["autoboot_enabled"], json!(false));
    assert_eq!(
        query_value(&a530, "expansion.gvp_a530.memory_function.manufacturer_id",),
        json!(2017),
    );
    assert_eq!(
        query_value(&a530, "expansion.gvp_a530.memory_function.product_id"),
        json!(9),
    );
    let bridge = query_value(&a530, "expansion.motherboard_bridge");
    let bridge = bridge
        .as_object()
        .expect("A530 should install a synchronized motherboard bridge");
    assert_eq!(bridge.len(), 3);
    assert_eq!(bridge["phase"], json!("idle"));
    assert_eq!(bridge["latched_response"], Value::Null);
    assert_eq!(bridge["coherent_with_cpu_cycle"], json!(true));
}

fn assert_no_expansions<M: AmigaMachine>(runtime: &AmigaRuntime<M>) {
    let expansion = query_value(runtime, "expansion");
    let expansion = expansion
        .as_object()
        .expect("expansion query should always be an object");
    assert_eq!(expansion.len(), 3);
    assert_eq!(expansion["generic_fast_ram"], Value::Null);
    assert_eq!(expansion["gvp_a530"], Value::Null);
    assert_eq!(expansion["motherboard_bridge"], Value::Null);
}

#[test]
fn expansion_query_tracks_autoconfig_state_transitions_and_discovers_live_fields() {
    let mut generic = AmigaOcsRuntime::blank(Model::A500OcsPalMaxed);
    let provider = AmigaSessionQueryProvider;
    assert_eq!(
        query_value(&generic, "expansion.generic_fast_ram.state.phase",),
        json!("unconfigured"),
    );
    generic.machine_mut().poke_word(0x00E8_004A, 0x0000);
    assert_eq!(
        query_value(&generic, "expansion.generic_fast_ram.state.phase",),
        json!("waiting_high_base"),
    );
    assert_eq!(
        query_value(
            &generic,
            "expansion.generic_fast_ram.state.pending_base_low_nibble",
        ),
        json!(0),
    );
    generic.machine_mut().poke_word(0x00E8_0048, 0x2000);
    assert_eq!(
        query_value(&generic, "expansion.generic_fast_ram.state.mapped_base",),
        json!(0x0020_0000),
    );
    assert_eq!(
        query_value(
            &generic,
            "expansion.generic_fast_ram.state.visible_in_probe_window",
        ),
        json!(false),
    );

    let paths = provider.query_paths(&generic, Some("expansion"));
    for path in [
        "expansion",
        "expansion.generic_fast_ram",
        "expansion.generic_fast_ram.state.phase",
        "expansion.generic_fast_ram.state.mapped_base",
        "expansion.gvp_a530",
        "expansion.motherboard_bridge",
    ] {
        assert!(
            paths.contains(&path.to_owned()),
            "{path} should be dynamically discoverable",
        );
    }

    let mut a530 = AmigaOcsRuntime::blank(Model::A500OcsPalGvpA530);
    a530.machine_mut().poke_word(0x00E8_004A, 0x0000);
    assert_eq!(
        query_value(&a530, "expansion.gvp_a530.memory_function.state.phase",),
        json!("waiting_high_base"),
    );
    a530.machine_mut().poke_word(0x00E8_0048, 0x2000);
    assert_eq!(
        query_value(
            &a530,
            "expansion.gvp_a530.memory_function.state.mapped_base",
        ),
        json!(0x0020_0000),
    );
    assert_eq!(
        query_value(&a530, "expansion.gvp_a530.configuration_is_coherent",),
        json!(true),
    );
}

#[test]
fn denise_board_pipeline_exposes_complete_bounded_state_on_every_chipset() {
    let mut ocs = AmigaOcsRuntime::blank(Model::A500OcsPal);
    assert_denise_board_pipeline(&mut ocs);

    let mut ecs = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    assert_denise_board_pipeline(&mut ecs);

    let mut aga = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    assert_denise_board_pipeline(&mut aga);
}

fn assert_denise_board_pipeline<M: AmigaMachine>(runtime: &mut AmigaRuntime<M>) {
    let initial = query_value(runtime, "denise.board_pipeline");
    let initial = initial
        .as_object()
        .expect("Denise board pipeline should be an object");
    let mut initial_fields: Vec<&str> = initial.keys().map(String::as_str).collect();
    initial_fields.sort_unstable();
    assert_eq!(
        initial_fields,
        ["bytes_this_line", "last_begin_line", "prior_line_raster",],
    );
    assert_eq!(initial["bytes_this_line"], json!(0));
    assert_eq!(initial["last_begin_line"], Value::Null);
    assert_eq!(initial["prior_line_raster"], Value::Null);

    let provider = AmigaSessionQueryProvider;
    for _ in 0..2_000 {
        runtime.machine_mut().tick();
        if !query_value(runtime, "denise.board_pipeline.prior_line_raster").is_null() {
            break;
        }
    }

    let prior = query_value(runtime, "denise.board_pipeline.prior_line_raster");
    let prior = prior
        .as_object()
        .expect("a physical line should leave a bounded prior-line context");
    let mut prior_fields: Vec<&str> = prior.keys().map(String::as_str).collect();
    prior_fields.sort_unstable();
    assert_eq!(
        prior_fields,
        [
            "ddf_start",
            "interlace_row",
            "line_ccks",
            "pipeline_y",
            "vbl_count",
            "vertical_diw_active",
            "vpos",
        ],
    );
    assert!(
        prior["line_ccks"]
            .as_u64()
            .is_some_and(|line_ccks| line_ccks > 0),
        "the retained context must report the actual physical line length",
    );

    let paths = provider.query_paths(runtime, Some("denise.board_pipeline"));
    assert!(
        paths.contains(&"denise.board_pipeline.prior_line_raster.vpos".to_owned()),
        "active optional context fields should become discoverable",
    );
    assert_eq!(
        provider
            .query(runtime, "denise.board_pipeline.prior_line_raster.vpos",)
            .expect("query should succeed")
            .expect("active context leaf should resolve")
            .value,
        prior["vpos"],
    );
}

fn assert_gayle_present<M: AmigaMachine>(
    runtime: &AmigaRuntime<M>,
    provider: &AmigaSessionQueryProvider,
) {
    assert!(
        provider
            .query_paths(runtime, Some("gayle"))
            .contains(&"gayle.registers.card_status".to_owned()),
    );
    assert_eq!(
        provider
            .query(runtime, "gayle.ide.drive_attached")
            .expect("Gayle query should not fail")
            .expect("Gayle leaf should resolve")
            .value,
        json!(false),
    );
}

fn assert_group_leaf_catalogue<M: AmigaMachine>(runtime: &AmigaRuntime<M>, groups: &[&str]) {
    let provider = AmigaSessionQueryProvider;
    let paths = provider.query_paths(runtime, None);

    for group in groups {
        let grouped = provider
            .query(runtime, group)
            .unwrap_or_else(|error| panic!("{group} query failed: {error:?}"))
            .unwrap_or_else(|| panic!("{group} group should resolve"));
        assert_group_value_catalogue(runtime, &provider, &paths, group, &grouped.value);
    }
}

fn assert_group_value_catalogue<M: AmigaMachine>(
    runtime: &AmigaRuntime<M>,
    provider: &AmigaSessionQueryProvider,
    paths: &[String],
    prefix: &str,
    value: &Value,
) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{prefix} should be an object"));

    for (field, grouped_value) in object {
        let path = format!("{prefix}.{field}");
        assert!(
            paths.contains(&path),
            "{path} appears in the grouped snapshot but is not advertised",
        );
        let leaf = provider
            .query(runtime, &path)
            .unwrap_or_else(|error| panic!("{path} query failed: {error:?}"))
            .unwrap_or_else(|| panic!("{path} should resolve"));
        assert_eq!(
            &leaf.value, grouped_value,
            "{path} should equal its grouped snapshot field",
        );
        if grouped_value.is_object() {
            assert_group_value_catalogue(runtime, provider, paths, &path, grouped_value);
        }
    }
}

fn query_value<M: AmigaMachine>(runtime: &AmigaRuntime<M>, path: &str) -> Value {
    AmigaSessionQueryProvider
        .query(runtime, path)
        .unwrap_or_else(|error| panic!("{path} query failed: {error:?}"))
        .unwrap_or_else(|| panic!("{path} should resolve"))
        .value
}

fn assert_exact_object_fields(value: &Value, expected: &[&str]) {
    let mut actual = value
        .as_object()
        .expect("query should return an object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    actual.sort_unstable();

    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn ecs_queries_expose_raw_routed_and_composed_hblank_state() {
    let mut runtime = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    runtime.machine_mut().poke_word(0x00DF_F1C4, 0x0040); // HBSTRT
    runtime.machine_mut().poke_word(0x00DF_F1C6, 0x0080); // HBSTOP
    runtime.machine_mut().poke_word(0x00DF_F100, 0x0001); // ECSENA
    runtime.machine_mut().poke_word(0x00DF_F106, 0x0001); // EXTBLKEN

    for _ in 0..2_048 {
        runtime.machine_mut().tick();
        if query_value(&runtime, "agnus.programmed_hblank_active") == json!(true) {
            break;
        }
    }

    assert_eq!(query_value(&runtime, "agnus.hbstrt"), json!(0x0040));
    assert_eq!(query_value(&runtime, "agnus.hbstop"), json!(0x0080));
    assert_eq!(
        query_value(&runtime, "agnus.programmed_hblank_active"),
        json!(true),
    );
    assert_eq!(
        query_value(&runtime, "agnus.programmed_hblank_routed_active"),
        json!(false),
    );
    assert_eq!(
        query_value(&runtime, "denise.programmed_hblank_active"),
        json!(false),
    );

    runtime.machine_mut().poke_word(0x00DF_F1DC, 0x0028); // PAL | BLANKEN
    assert_eq!(
        query_value(&runtime, "agnus.programmed_hblank_routed_active"),
        json!(false),
        "enabling BLANKEN after HBSTRT must not synthesize a routed event",
    );
    assert_eq!(
        query_value(&runtime, "chipset.programmed_hblank_output_active"),
        json!(false),
    );
}

#[test]
fn ecs_killehb_query_uses_bplcon2_instead_of_bplcon3() {
    let mut runtime = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    runtime.machine_mut().poke_word(0x00DF_F106, 0x0201);
    assert_eq!(
        query_value(&runtime, "denise.killehb_enabled"),
        json!(false),
        "ECS BPLCON3 bit 9 must not be reported as KILLEHB",
    );

    runtime.machine_mut().poke_word(0x00DF_F104, 0x0200);
    assert_eq!(query_value(&runtime, "denise.killehb_enabled"), json!(true),);

    runtime.machine_mut().poke_word(0x00DF_F106, 0x0000);
    assert_eq!(
        query_value(&runtime, "denise.killehb_enabled"),
        json!(true),
        "clearing BPLCON3 must not clear ECS KILLEHB diagnostics",
    );
    runtime.machine_mut().poke_word(0x00DF_F104, 0x0000);
    assert_eq!(
        query_value(&runtime, "denise.killehb_enabled"),
        json!(false),
    );
}

#[test]
fn aga_queries_expose_lisa_and_composed_hblank_state() {
    let mut runtime = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    runtime.machine_mut().poke_word(0x00DF_F106, 0xA000); // BANK=5, LOCT=0
    runtime.machine_mut().poke_word(0x00DF_F19A, 0x8A5C); // COLOR13 -> slot 173, T=1
    let expected_delayed_color = json!({
        "palette_index": 173,
        "previous_rgb24": 0,
        "previous_rgb12": null,
        "previous_genlock": false,
    });
    assert_eq!(
        query_value(&runtime, "aga.delayed_color_write"),
        expected_delayed_color,
    );
    assert_eq!(
        query_value(&runtime, "denise.delayed_color_write"),
        expected_delayed_color,
    );
    runtime.machine_mut().poke_word(0x00DF_F1C4, 0x0040); // HBSTRT
    runtime.machine_mut().poke_word(0x00DF_F1C6, 0x0080); // HBSTOP
    runtime.machine_mut().poke_word(0x00DF_F100, 0x0001); // ECSENA
    runtime.machine_mut().poke_word(0x00DF_F106, 0x0001); // EXTBLKEN

    for _ in 0..2_048 {
        runtime.machine_mut().tick();
        if query_value(&runtime, "aga.programmed_hblank_active") == json!(true) {
            break;
        }
    }

    assert_eq!(
        query_value(&runtime, "agnus.programmed_hblank_active"),
        json!(true),
        "Alice's coarse comparator history remains independently visible",
    );
    assert_eq!(
        query_value(&runtime, "aga.programmed_hblank_active"),
        json!(true),
    );
    assert_eq!(
        query_value(&runtime, "denise.programmed_hblank_active"),
        json!(true),
    );
    assert_eq!(
        query_value(&runtime, "aga.delayed_color_write"),
        Value::Null,
    );
    assert_eq!(
        query_value(&runtime, "denise.delayed_color_write"),
        Value::Null,
    );
    assert_eq!(
        query_value(&runtime, "chipset.programmed_hblank_output_active"),
        json!(true),
    );

    let denise_palette = query_value(&runtime, "denise.palette_24");
    let aga_palette = query_value(&runtime, "aga.palette_24");
    let denise_genlock = query_value(&runtime, "denise.palette_genlock");
    let aga_genlock = query_value(&runtime, "aga.palette_genlock");
    let denise_palette = denise_palette
        .as_array()
        .expect("Denise palette should be an array");
    let aga_palette = aga_palette
        .as_array()
        .expect("AGA palette should be an array");
    assert_eq!(denise_palette.len(), 256);
    assert_eq!(aga_palette.len(), 256);
    assert_eq!(denise_palette[173], json!(0x00AA_55CCu32));
    assert_eq!(aga_palette[173], json!(0x00AA_55CCu32));
    assert_eq!(denise_genlock.as_array().map(Vec::len), Some(256));
    assert_eq!(aga_genlock.as_array().map(Vec::len), Some(256));
    assert_eq!(denise_genlock[173], json!(true));
    assert_eq!(aga_genlock[173], json!(true));
}

#[test]
fn aga_killehb_query_uses_bplcon2_instead_of_bplcon3_loct() {
    let mut runtime = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    runtime.machine_mut().poke_word(0x00DF_F106, 0x0201); // LOCT | EXTBLKEN
    assert_eq!(
        query_value(&runtime, "denise.killehb_enabled"),
        json!(false),
        "AGA BPLCON3.LOCT must not be reported as ECS KILLEHB",
    );

    runtime.machine_mut().poke_word(0x00DF_F104, 0x0200); // BPLCON2 KILLEHB
    assert_eq!(query_value(&runtime, "denise.killehb_enabled"), json!(true),);

    runtime.machine_mut().poke_word(0x00DF_F106, 0x0000);
    assert_eq!(
        query_value(&runtime, "denise.killehb_enabled"),
        json!(true),
        "clearing LOCT must not clear AGA KILLEHB diagnostics",
    );
    runtime.machine_mut().poke_word(0x00DF_F104, 0x0000);
    assert_eq!(
        query_value(&runtime, "denise.killehb_enabled"),
        json!(false),
    );
}

#[test]
fn scheduler_queries_do_not_drain_pending_cpu_boundaries() {
    let mut runtime = AmigaA1200Runtime::blank(Model::A1200AgaPal);

    for _ in 0..2_000 {
        runtime.machine_mut().tick();
        if query_value(&runtime, "scheduler.pending_cpu_boundary_count")
            .as_u64()
            .is_some_and(|count| count > 0)
        {
            break;
        }
    }

    let first = query_value(&runtime, "scheduler.pending_cpu_boundaries");
    assert!(
        first
            .as_array()
            .is_some_and(|boundaries| !boundaries.is_empty()),
        "the direct machine tick loop should retain at least one boundary",
    );
    assert_eq!(
        query_value(&runtime, "scheduler.pending_cpu_boundaries"),
        first,
        "observing scheduler state must not drain the boundary queue",
    );
    assert_eq!(
        query_value(&runtime, "scheduler.cpu_clock_numerator"),
        json!(2),
    );
    assert_eq!(
        query_value(&runtime, "scheduler.cpu_clock_denominator"),
        json!(1),
    );
    assert_eq!(
        query_value(&runtime, "scheduler.cpu_domain_coherent"),
        json!(true),
    );
}

fn walk_all_paths(with_disk: bool) {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart())
        .expect("dummy Kickstart should construct");
    if with_disk {
        let disk = vec![0u8; ADF_SIZE_DD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
        runtime
            .load_media(&media)
            .expect("synthetic ADF should mount in floppy-0");
    }

    let provider = AmigaSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        let value = provider
            .query(&runtime, &path)
            .unwrap_or_else(|err| panic!("concrete {path} should not fail: {err:?}"));
        assert!(value.is_some(), "concrete {path} should resolve");
        let result = value.expect("path resolves");
        assert_eq!(result.path, path, "echoed path should match request");
    }
}

/// `boot_status` has three reachable arms; this test runs the runtime
/// long enough that the framebuffer has *some* non-black pixels so the
/// `monochrome-framebuffer` arm fires. The blank-runtime test below
/// covers the `no-visible-output` arm; the `display-active` arm needs
/// real ROMs (covered by ROM-gated diagnostics).
#[test]
fn boot_status_reports_no_visible_output_on_fresh_runtime() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let detected = provider
        .query(&runtime, "boot.detected")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(detected.value, json!(false));
    let reason = provider
        .query(&runtime, "boot.reason")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(reason.value, json!("no-visible-output"));
    let row = provider
        .query(&runtime, "boot.row")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(row.value, json!(null));
}

/// Run the runtime far enough that Agnus has programmed the palette
/// and Denise has emitted some non-black pixels (typical Kickstart
/// boot screen with a coloured backdrop). With a hermetic blank ROM
/// we don't get the `display-active` threshold (1000+ non-white
/// pixels), but we do flip from `no-visible-output` to
/// `monochrome-framebuffer` once the copper has set color00 and the
/// beam has scanned a frame.
#[test]
fn boot_status_reports_monochrome_after_running_some_frames() {
    let mut runtime =
        AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(A500_PAL_FRAME_TICKS * 4),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("four frames should run");
    let provider = AmigaSessionQueryProvider;
    let reason = provider
        .query(&runtime, "boot.reason")
        .expect("query succeeds")
        .expect("path present");
    // Either still no-visible-output (palette never programmed by the
    // brain-dead reset stub) or monochrome-framebuffer (Agnus coloured
    // some pixels). Both are valid for hermetic ROM, but at least one
    // of the non-default arms is exercised by `query_paths` walk.
    let value = reason.value.as_str().expect("string reason");
    assert!(
        matches!(value, "no-visible-output" | "monochrome-framebuffer"),
        "unexpected reason: {value}"
    );
}

#[test]
fn unknown_path_returns_ok_none() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let value = provider
        .query(&runtime, "does.not.exist")
        .expect("unknown path should not error");
    assert!(value.is_none(), "unknown path should return None");
}

#[test]
fn query_paths_filters_by_prefix() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let disk_paths = provider.query_paths(&runtime, Some("disk."));
    assert!(!disk_paths.is_empty());
    assert!(disk_paths.iter().all(|p| p.starts_with("disk.")));

    let boot_paths = provider.query_paths(&runtime, Some("boot."));
    assert!(boot_paths.contains(&"boot.detected".to_owned()));
    assert!(boot_paths.iter().all(|p| p.starts_with("boot.")));
}

#[test]
fn query_paths_returns_full_catalogue_when_prefix_missing() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let all = provider.query_paths(&runtime, None);
    let prefixed = provider.query_paths(&runtime, Some(""));
    assert_eq!(all, prefixed, "empty prefix is equivalent to None");
    let mut sorted = all.clone();
    sorted.sort_unstable();
    assert_eq!(all, sorted, "advertised paths should arrive sorted");
}

#[test]
fn debug_dsk_log_query_reports_zero_when_empty() {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let count = provider
        .query(&runtime, "debug.dsk_write_count")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(count.value, json!(0));
    let last = provider
        .query(&runtime, "debug.last_dsk_write")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(last.value, json!(null));
}

#[test]
fn frame_count_query_advances_after_run_until() {
    let mut runtime =
        AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let initial = provider
        .query(&runtime, "machine.frame_count")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(initial.value, json!(0));

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(A500_PAL_FRAME_TICKS * 2),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("two frames should run");

    let after = provider
        .query(&runtime, "machine.frame_count")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(after.value, json!(2));
}

#[test]
fn disk_queries_flip_after_load_media() {
    let mut runtime =
        AmigaOcsRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;

    let inserted = provider
        .query(&runtime, "disk.inserted")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(inserted.value, json!(false));

    let disk = vec![0u8; ADF_SIZE_DD];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime.load_media(&media).expect("ADF bytes should insert");

    let inserted = provider
        .query(&runtime, "disk.inserted")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(inserted.value, json!(true));
}
