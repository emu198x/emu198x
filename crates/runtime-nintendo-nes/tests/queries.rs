//! `NesSessionQueryProvider` paths against a runtime with a loaded
//! cartridge — covers the hermetic `nes.cartridge.*` and blargg
//! result-block paths.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, QueryError, SessionQueryProvider,
};
use machine_nintendo_nes::Nes;
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

/// OAM and the derived sprite view, which exist so sprite dropout can be
/// counted rather than inferred from pixels (#904).
#[test]
fn query_provider_exposes_oam_and_counts_sprites_per_scanline() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    {
        let nes = runtime.machine_mut().expect("cartridge loaded");
        // Ten sprites on scanline 100 — two more than the PPU can draw —
        // and one on its own at scanline 50.
        for i in 0..10u8 {
            nes.ppu.write_oam(i * 4, 100);
            nes.ppu.write_oam(i * 4 + 1, i);
            nes.ppu.write_oam(i * 4 + 2, 0);
            nes.ppu.write_oam(i * 4 + 3, i * 8);
        }
        nes.ppu.write_oam(10 * 4, 50);
        // Park the rest off the visible area.
        for i in 11..64u8 {
            nes.ppu.write_oam(i * 4, 0xF8);
        }
    }

    let provider = NesSessionQueryProvider;
    let q = |path: &str| {
        provider
            .query(&runtime, path)
            .expect("query should succeed")
            .unwrap_or_else(|| panic!("provider should own {path}"))
            .value
    };

    // Raw OAM, as the issue asked for: 256 bytes, first sprite's Y.
    let oam = q("sprites.oam");
    let oam = oam.as_array().expect("oam is an array");
    assert_eq!(oam.len(), 256);
    assert_eq!(oam[0], json!(100));

    // Decoded list, so a caller need not do the arithmetic.
    let list = q("sprites.list");
    let list = list.as_array().expect("list is an array");
    assert_eq!(list.len(), 64);
    assert_eq!(list[0]["y"], json!(100));
    assert_eq!(list[0]["x"], json!(0));
    assert_eq!(list[10]["y"], json!(50));

    // 8x8 sprites cover their Y through Y+7 — the same in-range test the
    // core's own evaluation uses.
    // A sprite at Y draws on Y+1..=Y+8, so Y=100 covers lines 101-108.
    assert_eq!(q("sprites.height"), json!(8));
    let per_line = q("sprites.per_scanline");
    let per_line = per_line.as_array().expect("per_scanline is an array");
    assert_eq!(per_line.len(), 240);
    assert_eq!(per_line[100], json!(0), "the sprite's own Y line is clear");
    assert_eq!(per_line[101], json!(10), "first line they cover");
    assert_eq!(per_line[108], json!(10), "last line they cover");
    assert_eq!(per_line[109], json!(0), "one line below");
    assert_eq!(per_line[51], json!(1), "the lone sprite");

    // The answer the issue actually wants: which lines drop sprites.
    let overflow = q("sprites.overflow_lines");
    let overflow = overflow.as_array().expect("overflow_lines is an array");
    assert_eq!(
        overflow
            .iter()
            .map(|v| v.as_u64().expect("scanline number"))
            .collect::<Vec<_>>(),
        (101..=108).collect::<Vec<u64>>(),
        "ten sprites on a line exceeds the eight the PPU can draw"
    );
}

/// Switching to 8x16 sprites doubles the lines each covers, so the same
/// OAM overflows twice as many.
#[test]
fn sprite_scanline_counts_follow_the_sprite_height_bit() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    // A frame first: the PPU ignores $2000 for roughly one frame after
    // reset, so writing before that would silently leave 8x8 sprites and
    // the test would be asserting on the default rather than the write.
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS * 2),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("a loaded runtime should run");

    {
        let nes = runtime.machine_mut().expect("cartridge loaded");
        for i in 0..9u8 {
            nes.ppu.write_oam(i * 4, 100);
        }
        for i in 9..64u8 {
            nes.ppu.write_oam(i * 4, 0xF8);
        }
        // Through the real register write, not a test-only setter.
        let Nes { ppu, mapper, .. } = nes;
        ppu.cpu_write(0x2000, 0x20, mapper.as_mut()); // 8x16 sprites
    }

    let provider = NesSessionQueryProvider;
    let height = provider
        .query(&runtime, "sprites.height")
        .expect("query should succeed")
        .expect("provider should own the path")
        .value;
    assert_eq!(height, json!(16));

    let overflow = provider
        .query(&runtime, "sprites.overflow_lines")
        .expect("query should succeed")
        .expect("provider should own the path")
        .value;
    let lines: Vec<u64> = overflow
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_u64().expect("scanline number"))
        .collect();
    assert_eq!(lines, (101..=116).collect::<Vec<u64>>());
}

/// A blank runtime has no PPU to read, so these report the path as
/// unavailable rather than inventing empty sprite data.
#[test]
fn sprite_paths_are_unavailable_without_a_cartridge() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let provider = NesSessionQueryProvider;
    for path in ["sprites", "sprites.oam", "sprites.overflow_lines"] {
        match provider.query(&runtime, path) {
            Err(QueryError::UnavailablePath { .. }) => {}
            other => panic!("{path}: expected UnavailablePath, got {other:?}"),
        }
    }
}
