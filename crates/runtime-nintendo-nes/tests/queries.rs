//! `NesSessionQueryProvider` paths against a runtime with a loaded
//! cartridge — covers the hermetic `nes.cartridge.*` and blargg
//! result-block paths.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, QueryError, SessionQueryProvider,
};
use runtime_nintendo_nes::{Model, NesRuntime, NesSessionQueryProvider};
use serde_json::json;

use common::{NTSC_FRAME_TICKS, blargg_ines, minimal_ines};

#[test]
fn query_provider_reports_loaded_cartridge_state() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let provider = NesSessionQueryProvider;
    let loaded = provider
        .query(&runtime, "cartridge.loaded")
        .expect("query should succeed")
        .expect("provider should own the path");

    assert_eq!(loaded.value, json!(true));
}

#[test]
fn query_provider_reports_blargg_result_block() {
    let rom = blargg_ines(0, b"ok\n");
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(NTSC_FRAME_TICKS), &mut host)
        .expect("one frame should run");

    let provider = NesSessionQueryProvider;
    assert_eq!(
        provider
            .query(&runtime, "test.blargg.valid")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "test.blargg.status")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!(0)
    );
    assert_eq!(
        provider
            .query(&runtime, "test.blargg.signature")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!([0xDE, 0xB0, 0x61])
    );
    assert_eq!(
        provider
            .query(&runtime, "test.blargg.text")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!("ok\n")
    );
}

/// Walk every advertised query path against a blank runtime (no
/// cartridge inserted). The four paths that don't depend on a loaded
/// machine — `nes.cartridge.*` and `nes.machine.*` — resolve. The
/// rest map to `QueryError::UnavailablePath` because they need a
/// machine. This drives every closure in the queries match ladder
/// that llvm-cov was reporting as uncalled.
#[test]
fn every_advertised_query_path_resolves_or_reports_unavailable_when_blank() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let provider = NesSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        let result = provider.query(&runtime, &path);
        match path.as_str() {
            "cartridge.loaded"
            | "cartridge.mapper"
            | "machine.frame_count"
            | "machine.master_clock" => {
                let value = result
                    .unwrap_or_else(|err| panic!("path {path} should not fail: {err:?}"))
                    .unwrap_or_else(|| panic!("path {path} should resolve"));
                assert_eq!(value.path, path);
            }
            _ => {
                let err = result.expect_err(&format!("path {path} should report unavailable"));
                assert!(
                    matches!(err, QueryError::UnavailablePath { .. }),
                    "path {path} produced unexpected error variant: {err:?}",
                );
            }
        }
    }
}

/// With a cartridge loaded every advertised path resolves. This
/// drives the loaded-machine arms — cpu.pc, ppu.scanline, ppu.dot —
/// that the blank-runtime walk above couldn't reach.
#[test]
fn every_advertised_query_path_resolves_with_cartridge_loaded() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let provider = NesSessionQueryProvider;
    for path in provider.query_paths(&runtime, None) {
        let result = provider.query(&runtime, &path);
        let value = result
            .unwrap_or_else(|err| panic!("path {path} should not fail: {err:?}"))
            .unwrap_or_else(|| panic!("path {path} should resolve"));
        assert_eq!(value.path, path);
    }
}

/// The folded chip snapshots (`cpu` / `ppu` / `apu` / `mapper`) resolve
/// both as a grouped object and per-leaf, the leaf equals the group's
/// field, and an unknown sub-field is an unknown path (not a null).
#[test]
fn folded_chip_snapshots_resolve_grouped_and_as_leaves() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let provider = NesSessionQueryProvider;
    let resolve = |path: &str| {
        provider
            .query(&runtime, path)
            .unwrap_or_else(|err| panic!("path {path} should not fail: {err:?}"))
            .unwrap_or_else(|| panic!("path {path} should resolve"))
            .value
    };

    // Grouped objects carry the folded fields; leaves match.
    let cpu = resolve("cpu");
    assert!(cpu.get("pc").is_some());
    assert!(cpu.get("flags").is_some());
    assert_eq!(resolve("cpu.pc"), cpu["pc"]);
    assert_eq!(resolve("cpu.flags"), cpu["flags"]);

    let ppu = resolve("ppu");
    assert!(ppu.get("scanline").is_some());
    assert_eq!(resolve("ppu.oam_addr"), ppu["oam_addr"]);

    let apu = resolve("apu");
    assert!(apu.get("dmc").is_some());
    assert_eq!(resolve("apu.dmc"), apu["dmc"]);

    let mapper = resolve("mapper");
    assert!(mapper.get("mapper_number").is_some());
    assert_eq!(resolve("mapper.mirroring"), mapper["mirroring"]);

    // Raw numbers, not hex strings — fleet convention.
    assert!(cpu["pc"].is_number(), "cpu.pc should be a raw number");
    assert!(ppu["ctrl"].is_number(), "ppu.ctrl should be a raw number");

    // An unknown sub-field is an unknown path, not a null value.
    assert!(
        provider
            .query(&runtime, "cpu.bogus")
            .expect("unknown sub-field should not error")
            .is_none(),
        "an unknown chip sub-field is an unknown path, not a null value"
    );
}

#[test]
fn query_paths_filters_by_prefix() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let provider = NesSessionQueryProvider;

    let blargg = provider.query_paths(&runtime, Some("test.blargg."));
    assert!(!blargg.is_empty());
    assert!(blargg.iter().all(|p| p.starts_with("test.blargg.")));

    let cartridge = provider.query_paths(&runtime, Some("cartridge."));
    assert!(!cartridge.is_empty());
    assert!(cartridge.iter().all(|p| p.starts_with("cartridge.")));

    let none = provider.query_paths(&runtime, Some("does.not.exist."));
    assert!(none.is_empty());
}

#[test]
fn unknown_query_path_returns_none() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let provider = NesSessionQueryProvider;
    let result = provider
        .query(&runtime, "does.not.exist")
        .expect("unknown path should not error");
    assert!(result.is_none(), "unknown path should be Ok(None)");
}

/// blargg.text decoder maps non-printable bytes to '.' and stops at
/// a null terminator; whitespace bytes (\n, \r, \t) pass through.
/// Drive the bytes through the CPU using `blargg_ines`'s emit_store
/// sequence so the result block is laid down by real STA instructions.
#[test]
fn blargg_text_passes_whitespace_and_replaces_non_printable_bytes() {
    let text = [b'A', b'\n', b'\t', 0x01, b'B'];
    let rom = blargg_ines(0, &text);
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("one frame should run");

    let provider = NesSessionQueryProvider;
    let value = provider
        .query(&runtime, "test.blargg.text")
        .expect("blargg.text should not error")
        .expect("blargg.text should resolve");
    assert_eq!(value.value, json!("A\n\t.B"));
}

/// `blargg.valid` is false when the signature bytes are absent.
/// `minimal_ines()` doesn't write the magic into PRG-RAM, so the
/// query reports the inverse.
#[test]
fn blargg_valid_is_false_without_signature_bytes() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("one frame should run");

    let provider = NesSessionQueryProvider;
    let value = provider
        .query(&runtime, "test.blargg.valid")
        .expect("blargg.valid should not error")
        .expect("blargg.valid should resolve");
    assert_eq!(value.value, json!(false));
}
