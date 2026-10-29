//! Complete pre-split v4 machine and host-output continuation.
use emu198x_shell::{
    AudioPacket, AudioSink, CapturedFrame, FramePacket, FrameSink, HostIo, MachineCore,
    MachineError, NullTraceSink, StopReason,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
#[derive(Default)]
struct Frames(Vec<CapturedFrame>);
impl FrameSink for Frames {
    fn push_frame(&mut self, p: FramePacket<'_>) -> Result<(), MachineError> {
        self.0.push(CapturedFrame::from_packet(p));
        Ok(())
    }
}
#[derive(Default)]
struct Audio(Vec<u8>);
impl AudioSink for Audio {
    fn push_audio(&mut self, p: AudioPacket<'_>) -> Result<(), MachineError> {
        for s in p.samples {
            self.0.extend(s.to_bits().to_le_bytes());
        }
        Ok(())
    }
}
fn fixture(id: &str, suffix: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../test-data/game-boy/board-split-v4/{id}.{suffix}"
        )),
    )
    .expect("frozen pre-split observation")
}
fn identical(a: &[u8], b: &[u8], what: &str) {
    assert_eq!(a.len(), b.len(), "{what} length");
    let first = a.iter().zip(b).position(|(a, b)| a != b);
    assert!(first.is_none(), "{what} differs at {first:?}");
}
#[test]
fn original_states_replay_every_frame_palette_and_audio_sample() {
    for model in Model::ALL {
        for booting in [false, true] {
            if booting && matches!(model, Model::Sgb | Model::Sgb2) {
                continue;
            }
            let id = format!(
                "{}{}",
                model.variant_id(),
                if booting { "-boot" } else { "" }
            );
            let initial = fixture(&id, "state");
            let mut runtime = GameBoyRuntime::blank(model);
            runtime.restore(&initial).expect("v4 restore");
            identical(
                &runtime.snapshot().expect("re-encode"),
                &initial,
                "initial state",
            );
            let m = runtime.machine().expect("machine");
            assert_eq!(m.is_cgb(), model.is_cgb());
            assert_eq!(m.peek(0xFF00) & 1, 0);
            if booting {
                assert_ne!(m.peek(0xFF50), 0xFF);
            } else if model.is_cgb() {
                assert!(m.double_speed());
            }
            let mut frames = Frames::default();
            let mut audio = Audio::default();
            for _ in 0..3 {
                let n = frames.0.len();
                let result = runtime
                    .run_frame(&mut HostIo {
                        input_events: &[],
                        frame_sink: &mut frames,
                        audio_sink: &mut audio,
                        trace_sink: &mut NullTraceSink,
                    })
                    .expect("native frame");
                assert_eq!(result.stop_reason, StopReason::ReachedTarget);
                assert_eq!(frames.0.len(), n + 1);
                assert_eq!(frames.0.last().expect("frame").timestamp, result.reached);
            }
            for frame in &frames.0 {
                assert_eq!((frame.width, frame.height), (160, 144));
                assert!(
                    frame
                        .rgba_pixels()
                        .expect("RGBA")
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        > 1
                );
            }
            assert!(
                audio
                    .0
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|v| f32::from_le_bytes(*v).abs() > 0.000_001)
            );
            identical(
                &postcard::to_allocvec(&frames.0).expect("all frames"),
                &fixture(&id, "frames"),
                "frames and palettes",
            );
            identical(&audio.0, &fixture(&id, "audio"), "audio");
            identical(
                &runtime.snapshot().expect("continued"),
                &fixture(&id, "after.state"),
                "continued state",
            );
            if booting {
                assert_eq!(runtime.machine().expect("machine").peek(0xFF50), 0xFF);
            }
        }
    }
}
