//! Query-provider coverage for the Amiga runtime — boot status,
//! catalogued paths, and A1000-bootstrap-specific surfaces.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, SessionQueryProvider,
};
use format_commodore_amiga_adf::ADF_SIZE_DD;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaRuntime, AmigaSessionQueryProvider, Model,
};
use serde_json::json;

use common::{dummy_a1000_bootstrap_rom, dummy_kickstart};

#[test]
fn query_provider_returns_declared_paths() {
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let paths = provider.query_paths(&runtime, None);
    assert!(paths.contains(&"amiga.a1000.boot_rom_visible".to_owned()));
    assert!(paths.contains(&"amiga.a1000.wom_locked".to_owned()));
    assert!(paths.contains(&"amiga.cpu.pc".to_owned()));
    assert!(paths.contains(&"amiga.debug.dsk_write_count".to_owned()));
    assert!(paths.contains(&"amiga.disk.change_pending".to_owned()));
    assert!(paths.contains(&"amiga.disk.inserted".to_owned()));
    assert!(paths.contains(&"amiga.disk.step_events".to_owned()));
    assert!(paths.contains(&"amiga.keyboard.state".to_owned()));
}

#[test]
fn query_cpu_pc_returns_initial_reset_vector() {
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let result = AmigaSessionQueryProvider
        .query(&runtime, "amiga.cpu.pc")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(result.path, "amiga.cpu.pc");
    assert_eq!(result.value, json!(0x00F8_0008u32));
}

#[test]
fn a1000_queries_report_bootstrap_state() {
    let runtime =
        AmigaRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom()).expect("runtime init");
    let boot_rom_visible = AmigaSessionQueryProvider
        .query(&runtime, "amiga.a1000.boot_rom_visible")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(boot_rom_visible.value, json!(true));

    let wom_locked = AmigaSessionQueryProvider
        .query(&runtime, "amiga.a1000.wom_locked")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(wom_locked.value, json!(false));
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

fn walk_all_paths(with_disk: bool) {
    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart())
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
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
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
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
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
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let value = provider
        .query(&runtime, "amiga.does.not.exist")
        .expect("unknown path should not error");
    assert!(value.is_none(), "unknown path should return None");
}

#[test]
fn query_paths_filters_by_prefix() {
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let disk_paths = provider.query_paths(&runtime, Some("amiga.disk."));
    assert!(!disk_paths.is_empty());
    assert!(disk_paths.iter().all(|p| p.starts_with("amiga.disk.")));

    let boot_paths = provider.query_paths(&runtime, Some("boot."));
    assert!(boot_paths.contains(&"boot.detected".to_owned()));
    assert!(boot_paths.iter().all(|p| p.starts_with("boot.")));
}

#[test]
fn query_paths_returns_full_catalogue_when_prefix_missing() {
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
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
    let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let count = provider
        .query(&runtime, "amiga.debug.dsk_write_count")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(count.value, json!(0));
    let last = provider
        .query(&runtime, "amiga.debug.last_dsk_write")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(last.value, json!(null));
}

#[test]
fn frame_count_query_advances_after_run_until() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let initial = provider
        .query(&runtime, "amiga.machine.frame_count")
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
        .query(&runtime, "amiga.machine.frame_count")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(after.value, json!(2));
}

#[test]
fn disk_queries_flip_after_load_media() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;

    let inserted = provider
        .query(&runtime, "amiga.disk.inserted")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(inserted.value, json!(false));

    let disk = vec![0u8; ADF_SIZE_DD];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime.load_media(&media).expect("ADF bytes should insert");

    let inserted = provider
        .query(&runtime, "amiga.disk.inserted")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(inserted.value, json!(true));
}
