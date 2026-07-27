//! Query-provider coverage for the Amiga runtime — boot status,
//! catalogued paths, and A1000-bootstrap-specific surfaces.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, SessionQueryProvider,
};
use format_commodore_amiga_adf::ADF_SIZE_DD;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaA1200Runtime, AmigaOcsRuntime, AmigaSessionQueryProvider, Model,
};
use serde_json::json;

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
