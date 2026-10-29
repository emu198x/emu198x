//! Frozen pre-split v7 states and observations from 94564bea3.
use emu198x_shell::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, MachineCore, MachineError,
    NullTraceSink, StopReason,
};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};
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
        "../../test-data/spectravideo/split-v7/{id}.{suffix}"
    )))
    .expect("frozen pre-split observation")
}
fn identical(actual: &[u8], expected: &[u8], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what} length");
    let first = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(first.is_none(), "{what} differs at {first:?}");
}
#[test]
fn pre_split_v7_states_replay_identical_cpu_video_audio_and_inputs() {
    for model in Model::ALL {
        let id = model.model_id();
        let initial = fixture(id, "state");
        let mut runtime = Svi328Runtime::blank(model);
        runtime.restore(&initial).expect("v7 restore");
        identical(
            &runtime.snapshot().expect("v7 encode"),
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
        assert_eq!(machine.peek(0xC000), 0xDF);
        assert_eq!(machine.peek(0xC001), 0xFE);
        assert_eq!(machine.peek(0xC003), 0x5A);
        assert_eq!(
            machine.peek(0xC004),
            3,
            "guest observed both cassette levels"
        );
        assert_eq!(machine.peek(0xC002) & 0x50, 0);
        assert_eq!(machine.peek(0x1000), 0xA5);
        assert_eq!(machine.memory_control(), (true, true));
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
