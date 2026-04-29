//! `NesSessionQueryProvider` paths against a runtime with a loaded
//! cartridge — covers the hermetic `nes.cartridge.*` and blargg
//! result-block paths.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, SessionQueryProvider,
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
        .query(&runtime, "nes.cartridge.loaded")
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
            .query(&runtime, "nes.test.blargg.valid")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "nes.test.blargg.status")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!(0)
    );
    assert_eq!(
        provider
            .query(&runtime, "nes.test.blargg.signature")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!([0xDE, 0xB0, 0x61])
    );
    assert_eq!(
        provider
            .query(&runtime, "nes.test.blargg.text")
            .expect("query should succeed")
            .expect("provider should own the path")
            .value,
        json!("ok\n")
    );
}
