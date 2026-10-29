//! Frozen pre-split v8 states and observations from 3f29610e0.
use emu198x_shell::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, MachineCore, MachineError,
    NullTraceSink, StopReason,
};
use runtime_commodore_vic_20::{Model, Vic20Runtime};
use std::path::PathBuf;
#[derive(Default)]
struct Audio(Vec<u8>);
impl AudioSink for Audio {
    fn push_audio(&mut self, p: AudioPacket<'_>) -> Result<(), MachineError> {
        for sample in p.samples {
            self.0.extend(sample.to_bits().to_le_bytes());
        }
        Ok(())
    }
}
#[derive(Default)]
struct Frames(Vec<u64>);
impl FrameSink for Frames {
    fn push_frame(&mut self, p: FramePacket<'_>) -> Result<(), MachineError> {
        assert!(!p.pixels.is_empty());
        self.0.push(p.timestamp.get());
        Ok(())
    }
}
fn fixture(id: &str, suffix: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../test-data/commodore/vic-20/split-v8/{id}.{suffix}"
    )))
    .expect("frozen pre-split observation")
}
fn identical(actual: &[u8], expected: &[u8], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what} length");
    let first = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(first.is_none(), "{what} differs at {first:?}");
}
#[test]
fn pre_split_v8_states_replay_identical_cpu_video_audio_and_inputs() {
    for model in Model::ALL {
        let id = model.model_id();
        let initial = fixture(id, "state");
        let mut runtime = Vic20Runtime::blank(model);
        runtime.restore(&initial).expect("v8 restore");
        identical(
            &runtime.snapshot().expect("v8 encode"),
            &initial,
            "initial state",
        );
        let mut audio = Audio::default();
        let mut frames = Frames::default();
        for _ in 0..3 {
            let before = frames.0.len();
            let result = runtime
                .run_frame(&mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frames,
                    audio_sink: &mut audio,
                    trace_sink: &mut NullTraceSink,
                })
                .expect("native field");
            assert_eq!(result.stop_reason, StopReason::ReachedTarget);
            assert_eq!(frames.0.len(), before + 1);
        }
        let expected_times: Vec<u64> =
            serde_json::from_slice(&fixture(id, "times.json")).expect("times");
        assert_eq!(expected_times.len(), 3);
        assert_eq!(frames.0, expected_times);
        let machine = runtime.machine().expect("machine");
        assert_eq!(machine.peek(0x0200), 0xFD, "keyboard A held");
        assert_eq!(machine.peek(0x0201) & 0x3C, 0x18, "up and fire held");
        assert_eq!(
            machine.peek(0x0202) & 1,
            1,
            "idle host RX is high after stepping"
        );
        assert_eq!(machine.peek(0x0203), 0x5A, "cartridge is mapped");
        assert_eq!(machine.peek(0x0204), 0xA5, "3 KiB expansion");
        assert_eq!(machine.peek(0x0205), 0xC3, "independent BLK1 expansion");
        let pixels: Vec<u8> = machine
            .framebuffer()
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect();
        assert!(
            machine
                .framebuffer()
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1,
            "guest produces real pixel content"
        );
        assert!(
            audio
                .0
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| f32::from_le_bytes(*p).abs() > 0.000_001),
            "guest produces audible samples"
        );
        identical(&pixels, &fixture(id, "pixels"), "video");
        identical(&audio.0, &fixture(id, "audio"), "audio");
        identical(
            &runtime.snapshot().expect("continued state"),
            &fixture(id, "after.state"),
            "continued state",
        );
    }
}

#[test]
fn legacy_cartridge_container_survives_restore_and_reset() {
    use emu198x_shell::ResetKind;
    for model in Model::ALL {
        let mut runtime = Vic20Runtime::blank(model);
        runtime
            .restore(&fixture(model.model_id(), "state"))
            .expect("v8 restore");
        for kind in [ResetKind::Soft, ResetKind::Hard] {
            runtime.reset(kind);
            let m = runtime.machine().expect("machine after reset");
            assert_eq!(m.peek(0xA100), 0x5A, "original cartridge survives reset");
            assert_eq!(m.peek(0x0400), 0, "fresh expansion RAM");
            assert!(runtime.ram_expansion().exp_3k && runtime.ram_expansion().blk1);
            assert_eq!(m.master_clock(), 0);
        }
    }
}
