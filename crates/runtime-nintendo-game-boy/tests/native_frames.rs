//! A safety timeout is elapsed machine time, never a completed LCD frame.
mod common;
use emu198x_shell::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, MachineCore, MachineError, MediaImage,
    MediaKind, MediaSet, NullTraceSink, StopReason,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};

#[derive(Default)]
struct Frames(usize);
impl FrameSink for Frames {
    fn push_frame(&mut self, _: FramePacket<'_>) -> Result<(), MachineError> {
        self.0 += 1;
        Ok(())
    }
}
#[derive(Default)]
struct Audio(usize);
impl AudioSink for Audio {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        self.0 += packet.samples.len();
        Ok(())
    }
}

#[test]
fn lcd_off_stops_without_a_fake_frame_and_resumes_after_restore() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = common::loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("cartridge");
    runtime.machine_mut().expect("machine").poke(0xFF40, 0);
    let state = runtime.snapshot().expect("LCD-off state");
    let mut restored = GameBoyRuntime::blank(Model::Dmg);
    restored.restore(&state).expect("restore");
    for live in [&mut runtime, &mut restored] {
        let mut frames = Frames::default();
        let mut audio = Audio::default();
        let before = live.time();
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        };
        let result = live.run_frame(&mut host).expect("bounded attempt");
        assert_eq!(result.stop_reason, StopReason::NoFrame);
        assert!(live.time() > before, "CPU still advances");
        assert_eq!(frames.0, 0);
        assert!(audio.0 > 0, "elapsed audio is retained");
        live.machine_mut().expect("machine").poke(0xFF40, 0x91);
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        };
        assert_eq!(
            live.run_frame(&mut host).expect("LCD field").stop_reason,
            StopReason::ReachedTarget
        );
        assert_eq!(frames.0, 1);
    }
    assert_eq!(
        runtime.snapshot().expect("live"),
        restored.snapshot().expect("replayed")
    );
}
