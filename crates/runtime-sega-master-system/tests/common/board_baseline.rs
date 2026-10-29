//! Original version-10 states and complete continued host output.
use emu198x_shell::{
    AudioPacket, AudioSink, CapturedFrame, FramePacket, FrameSink, HostIo, MachineCore,
    MachineError, NullTraceSink,
};
use runtime_sega_master_system_class::{SmsModel, SmsRuntime};
use std::{error::Error, path::Path};
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
fn cartridge(gg: bool) -> Vec<u8> {
    let name = if gg {
        "game-gear.gg"
    } else {
        "master-system.sms"
    };
    let original = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/sega/synthetic-cart")
            .join(name),
    )
    .expect("project cartridge");
    // Own guest instructions set a PSG tone, asymmetric GG stereo and a
    // keyed-on YM2413 channel where the board has the built-in chip.
    let mut code = vec![0xF3];
    for (port, value) in [
        (0x7F, 0x87),
        (0x7F, 0x12),
        (0x7F, 0x90),
        (0x06, 0x10),
        (0xF2, 1),
        (0xF0, 0x30),
        (0xF1, 0x10),
        (0xF0, 0x10),
        (0xF1, 0xA7),
        (0xF0, 0x20),
        (0xF1, 0x15),
    ] {
        code.extend([0x3E, value, 0xD3, port]);
    }
    code.extend(original);
    code.truncate(32768);
    code
}
type Continuation = (Vec<u8>, Vec<u8>, Vec<u8>);
fn continued<M: SmsModel>(r: &mut SmsRuntime<M>) -> Result<Continuation, Box<dyn Error>> {
    let mut frames = Frames::default();
    let mut audio = Audio::default();
    let mut trace = NullTraceSink;
    for _ in 0..3 {
        let result = r.run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        })?;
        assert_eq!(
            result.stop_reason,
            emu198x_shell::StopReason::ReachedTarget,
            "one real native boundary"
        );
    }
    assert_eq!(frames.0.len(), 3);
    assert!(!audio.0.is_empty());
    Ok((r.snapshot()?, postcard::to_allocvec(&frames.0)?, audio.0))
}
pub fn exercise<M: SmsModel>(
    model: M,
    gg: bool,
    root: &Path,
    capture: bool,
) -> Result<(), Box<dyn Error>> {
    let id = model.model_id();
    let mut r = SmsRuntime::new(model, cartridge(gg));
    let _ = continued(&mut r)?;
    let m = r.machine_mut().expect("machine");
    // Exercise live mapper/SRAM/controller state without changing immutable media.
    m.poke(0xFFFC, 8);
    m.poke(0x8000, 0x57);
    m.poke(0xBFFF, 0xA3);
    m.poke(0xFFFC, 12);
    m.poke(0x8000, 0x91);
    m.poke(0xFFFD, 0);
    m.set_port_dc(0xEE);
    m.set_port_dd(0xFD);
    m.set_gg_start(0x7F);
    m.set_light_phaser_aim(0, Some((42, 67)));
    m.set_light_phaser_trigger(0, true);
    // Drain prior host output before the snapshot; never count it twice.
    let _ = m.take_audio_buffer();
    let initial = r.snapshot()?;
    let files = ["state", "after.state", "frames", "audio"];
    let (after, frames, audio) = continued(&mut r)?;
    let values = [initial.clone(), after, frames, audio];
    if capture {
        for (suffix, value) in files.into_iter().zip(&values) {
            std::fs::write(root.join(format!("{id}.{suffix}")), value)?;
        }
    } else {
        for (suffix, value) in files.into_iter().zip(&values) {
            assert_eq!(
                *value,
                std::fs::read(root.join(format!("{id}.{suffix}")))?,
                "{id}.{suffix}"
            );
        }
    }
    let mut restored = SmsRuntime::blank(model);
    restored.restore(&initial)?;
    assert_eq!(restored.snapshot()?, initial, "original live bytes");
    let (after, frames, audio) = continued(&mut restored)?;
    for (suffix, value) in ["after.state", "frames", "audio"]
        .into_iter()
        .zip([after, frames, audio])
    {
        let expected = std::fs::read(root.join(format!("{id}.{suffix}")))?;
        assert!(
            value == expected,
            "{id}.{suffix} differs at {:?}",
            value.iter().zip(&expected).position(|(a, b)| a != b)
        );
    }
    Ok(())
}
pub fn all(root: &Path, capture: bool) -> Result<(), Box<dyn Error>> {
    for model in runtime_sega_master_system::Model::ALL {
        exercise(model, false, root, capture)?;
    }
    exercise(runtime_sega_game_gear::Model::GameGear, true, root, capture)
}
