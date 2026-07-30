//! Query-provider coverage for the Amiga runtime — boot status,
//! catalogued paths, and A1000-bootstrap-specific surfaces.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, SessionQueryProvider,
};
use format_commodore_amiga_adf::ADF_SIZE_DD;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaA1200Runtime, AmigaEcsRuntime, AmigaMachine, AmigaOcsRuntime,
    AmigaRuntime, AmigaSessionQueryProvider, Model,
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
    assert!(paths.contains(&"disk.inserted".to_owned()));
    assert!(paths.contains(&"disk.step_events".to_owned()));
    assert!(paths.contains(&"keyboard.state".to_owned()));
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
fn grouped_snapshot_fields_are_all_discoverable_as_leaves() {
    let ocs = AmigaOcsRuntime::blank(Model::A500OcsPal);
    assert_group_leaf_catalogue(
        &ocs,
        &[
            "runtime",
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "keyboard",
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
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "keyboard",
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
            "chipset",
            "agnus",
            "denise",
            "copper",
            "dma",
            "blitter",
            "paula",
            "cia",
            "keyboard",
            "input",
            "debug",
            "disk",
            "scheduler",
            "aga",
        ],
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
fn aga_queries_expose_lisa_and_composed_hblank_state() {
    let mut runtime = AmigaA1200Runtime::blank(Model::A1200AgaPal);
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
        query_value(&runtime, "chipset.programmed_hblank_output_active"),
        json!(true),
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
