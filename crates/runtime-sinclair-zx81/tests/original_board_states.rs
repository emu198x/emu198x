//! Exact legacy envelopes and complete runtime output captured before extraction.
use emu198x_shell::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, MachineCore, MachineError,
    NullTraceSink, PixelFormat, StopReason,
};
use runtime_sinclair_zx81::{Model, Zx81Runtime};
use std::path::Path;
#[derive(Default)]
struct Frames(Vec<(u64, u32, u32, Vec<u32>)>);
impl FrameSink for Frames {
    fn push_frame(&mut self, p: FramePacket<'_>) -> Result<(), MachineError> {
        assert_eq!(p.format, PixelFormat::Rgba8888);
        let pixels = p
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
            .collect();
        self.0.push((p.timestamp.get(), p.width, p.height, pixels));
        Ok(())
    }
}
#[derive(Default)]
struct Audio(Vec<(u64, Vec<u32>)>);
impl AudioSink for Audio {
    fn push_audio(&mut self, p: AudioPacket<'_>) -> Result<(), MachineError> {
        assert_eq!(p.sample_rate, 48000);
        assert_eq!(p.channels, 1);
        self.0.push((
            p.timestamp.get(),
            p.samples.iter().map(|s| s.to_bits()).collect(),
        ));
        Ok(())
    }
}
#[test]
fn original_envelopes_and_every_native_output_byte_survive_owner_extraction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/zx81/board-split-v5");
    assert_eq!(Model::ALL.len(), 3);
    for model in Model::ALL {
        let id = model.profile_id();
        let initial = std::fs::read(root.join(format!("{id}.state"))).expect("original state");
        let mut runtime = Zx81Runtime::blank(model);
        runtime
            .restore(&initial)
            .expect("matching selected profile");
        assert_eq!(runtime.snapshot().expect("initial"), initial);
        assert_eq!(runtime.model(), model);
        let mut frames = Frames::default();
        let mut audio = Audio::default();
        for _ in 0..3 {
            let result = runtime
                .run_frame(&mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frames,
                    audio_sink: &mut audio,
                    trace_sink: &mut NullTraceSink,
                })
                .expect("run");
            assert_eq!(result.stop_reason, StopReason::ReachedTarget);
        }
        assert_eq!(frames.0.len(), 3);
        assert_eq!(audio.0.len(), 3);
        for (suffix, value) in [
            ("after.state", runtime.snapshot().expect("after")),
            ("frames", postcard::to_allocvec(&frames.0).expect("frames")),
            ("audio", postcard::to_allocvec(&audio.0).expect("audio")),
        ] {
            assert!(
                value
                    == std::fs::read(root.join(format!("{id}.{suffix}"))).expect("original output"),
                "runtime original {id}.{suffix}"
            );
        }
    }
}
