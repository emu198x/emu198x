use common_mattel_aquarius::{AquariusRegion, AquariusState as Aquarius};
use emu198x_shell::{HostIo, MachineCore, NullAudioSink, NullFrameSink, NullTraceSink};
use runtime_mattel_aquarius::{AquariusRuntime, Model};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Aquarius>,
    controller_cache: [[bool; 5]; 2],
}
#[derive(Serialize, Deserialize)]
struct LegacyEnvelope {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Aquarius>,
}

// Test construction from retained hardware only, with explicitly no logical host
// buttons. This is not a production v4 reader: that version remains rejected.
fn fixture_hardware_in_v5(original: &[u8]) -> Vec<u8> {
    let old: LegacyEnvelope = postcard::from_bytes(original).expect("original v4 fixture");
    assert_eq!(old.version, 4);
    postcard::to_allocvec(&Envelope {
        version: 5,
        time: old.time,
        model_id: old.model_id,
        machine: old.machine,
        controller_cache: [[false; 5]; 2],
    })
    .expect("explicit current test envelope")
}

fn original_metadata_of_current(bytes: &[u8]) -> Vec<u8> {
    let current: Envelope = postcard::from_bytes(bytes).expect("current v5 envelope");
    assert_eq!(current.version, 5);
    assert_eq!(
        current.controller_cache, [[false; 5]; 2],
        "this fixture sets hardware directly, with no host button events"
    );
    postcard::to_allocvec(&LegacyEnvelope {
        version: 4,
        time: current.time,
        model_id: current.model_id,
        machine: current.machine,
    })
    .expect("only envelope version/cache are normalized")
}

#[test]
fn foreign_hardware_under_matching_header_is_rejected_atomically() {
    for (model, foreign) in [
        (Model::Aquarius, AquariusRegion::Pal),
        (Model::AquariusPal, AquariusRegion::Ntsc),
    ] {
        let mut runtime = AquariusRuntime::new(model, vec![0; 8192]).expect("runtime");
        let before = runtime.snapshot().expect("before");
        let bytes = postcard::to_allocvec(&Envelope {
            version: 5,
            time: 123,
            model_id: model.model_id().to_owned(),
            machine: Some(Aquarius::new(vec![0; 8192], 0, foreign)),
            controller_cache: [[false; 5]; 2],
        })
        .expect("foreign state");
        let expected_region = match model {
            Model::Aquarius => AquariusRegion::Ntsc,
            Model::AquariusPal => AquariusRegion::Pal,
        };
        let control = postcard::to_allocvec(&Envelope {
            version: 5,
            time: 123,
            model_id: model.model_id().to_owned(),
            machine: Some(Aquarius::new(vec![0; 8192], 0, expected_region)),
            controller_cache: [[false; 5]; 2],
        })
        .expect("matching hardware control");
        AquariusRuntime::blank(model)
            .restore(&control)
            .expect("the current test envelope is accepted for its matching board");
        assert!(
            runtime.restore(&bytes).is_err(),
            "foreign hardware must not run under a matching header"
        );
        assert_eq!(runtime.snapshot().expect("after"), before);
    }
}
#[test]
fn original_ntsc_hardware_bytes_are_unchanged_in_the_v5_envelope() {
    let rom = include_bytes!("../../../test-data/synthetic-firmware/mattel-aquarius.rom").to_vec();
    let mut runtime = AquariusRuntime::new(Model::Aquarius, rom).expect("runtime");
    runtime.set_char_rom(vec![0xff; 2048]).expect("characters");
    runtime.set_expansion_kb(16);
    runtime.insert_cartridge(vec![0xa5; 8192]);
    let m = runtime.machine_mut().expect("machine");
    m.set_key(0, 2, true);
    m.set_joystick(1, true, false, false, false, true);
    m.poke(0x4000, 0x5a);
    runtime
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut NullFrameSink,
            audio_sink: &mut NullAudioSink,
            trace_sink: &mut NullTraceSink,
        })
        .expect("frame");
    let saved = runtime.snapshot().expect("snapshot");
    let original =
        include_bytes!("../../../test-data/mattel-aquarius/board-split-v4/ntsc-runtime.state");
    assert_eq!(original_metadata_of_current(&saved).as_slice(), original);
    let mut restored = AquariusRuntime::blank(Model::Aquarius);
    restored.restore(&saved).expect("current v5 snapshot");
    assert_eq!(restored.snapshot().expect("roundtrip"), saved);
}

#[test]
fn restored_firmware_cartridge_and_expansion_survive_reset() {
    use emu198x_shell::ResetKind;
    let original =
        include_bytes!("../../../test-data/mattel-aquarius/board-split-v4/ntsc-runtime.state");
    let mut restored = AquariusRuntime::blank(Model::Aquarius);
    restored
        .restore(&fixture_hardware_in_v5(original))
        .expect("current envelope around retained hardware");
    assert_eq!(
        restored.expansion_kb(),
        16,
        "reports must describe the restored expansion"
    );
    assert!(
        restored.cartridge_loaded(),
        "reports must describe the restored cartridge"
    );
    restored.reset(ResetKind::Hard);
    let m = restored.machine().expect("restored BIOS survives reset");
    assert_eq!(m.character_rom(), vec![0xff; 2048]);
    assert_eq!(m.peek(0xe000), 0xa5);
    assert_eq!(m.expansion_kb(), 16);
}

#[test]
fn both_runtime_profiles_continue_original_partial_fields_exactly() {
    use emu198x_shell::AudioCapture;
    type Frames = Vec<(u64, u64, u64, Vec<u32>, Vec<u32>)>;
    for (name, model) in [("ntsc", Model::Aquarius), ("pal", Model::AquariusPal)] {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/mattel-aquarius/board-split-v4");
        let raw: Aquarius = postcard::from_bytes(
            &std::fs::read(root.join(format!("{name}.state"))).expect("fixture"),
        )
        .expect("raw state");
        let time = raw.cpu_tstates();
        let bytes = postcard::to_allocvec(&Envelope {
            version: 5,
            time,
            model_id: model.model_id().to_owned(),
            machine: Some(raw),
            controller_cache: [[false; 5]; 2],
        })
        .expect("v5 test envelope");
        let expected = std::fs::read(root.join(format!("{name}.output"))).expect("output");
        let (frames, final_state): (Frames, Vec<u8>) =
            postcard::from_bytes(&expected).expect("expected output");
        let mut runtime = AquariusRuntime::blank(model);
        runtime.restore(&bytes).expect("checked restore");
        assert_eq!(runtime.snapshot().expect("unchanged envelope"), bytes);
        assert_eq!(
            runtime.profile().machine_id.as_str(),
            runtime.machine().expect("board").board_id()
        );
        for (ticks, time, count, pixels, audio_bits) in frames {
            let before = runtime.time().get();
            let mut audio = AudioCapture::default();
            runtime
                .run_frame(&mut HostIo {
                    input_events: &[],
                    frame_sink: &mut NullFrameSink,
                    audio_sink: &mut audio,
                    trace_sink: &mut NullTraceSink,
                })
                .expect("native frame");
            assert_eq!(runtime.time().get() - before, ticks);
            assert_eq!(runtime.time().get(), time);
            let m = runtime.machine().expect("board");
            assert_eq!(m.frame_count(), count);
            assert!(m.framebuffer() == pixels, "{name}: complete native pixels");
            assert_eq!(
                audio
                    .audio()
                    .expect("audio")
                    .samples
                    .iter()
                    .map(|s| s.to_bits())
                    .collect::<Vec<_>>(),
                audio_bits
            );
        }
        assert_eq!(
            postcard::to_allocvec(runtime.machine().expect("board")).expect("state"),
            final_state
        );
    }
}
