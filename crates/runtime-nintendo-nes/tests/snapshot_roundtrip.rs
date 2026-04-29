//! Postcard snapshot envelope round-trips for the NES runtime.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_nintendo_nes::{Model, NesRuntime};

use common::{NTSC_FRAME_TICKS, minimal_ines};

#[test]
fn runtime_snapshot_round_trips_loaded_machine_state() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");
    runtime
        .machine_mut()
        .expect("cartridge loaded")
        .set_controller1(0b0000_1000);

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

    let snapshot = runtime.snapshot().expect("snapshot should encode");
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored
        .restore(&snapshot)
        .expect("snapshot should restore");

    assert_eq!(restored.time(), runtime.time());
    assert_eq!(
        restored.machine().expect("machine restored").frame_count(),
        runtime.machine().expect("machine present").frame_count()
    );
    assert_eq!(
        restored
            .machine()
            .expect("machine restored")
            .controller1_state,
        0b0000_1000
    );
}
