//! Freeze original runtime state and complete native output before owner extraction.
use crate::{Atari7800Runtime, Model};
use emu198x_shell::{
    AudioPacket, AudioSink, CapturedFrame, FramePacket, FrameSink, HostIo, MachineCore,
    MachineError, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink, StopReason,
};
use std::{
    error::Error,
    path::{Path, PathBuf},
};
#[derive(Default)]
struct Frames(Vec<CapturedFrame>);
impl FrameSink for Frames {
    fn push_frame(&mut self, p: FramePacket<'_>) -> Result<(), MachineError> {
        self.0.push(CapturedFrame::from_packet(p));
        Ok(())
    }
}
#[derive(Default)]
struct Audio {
    bytes: Vec<u8>,
    samples: usize,
    nonzero: usize,
}
impl AudioSink for Audio {
    fn push_audio(&mut self, p: AudioPacket<'_>) -> Result<(), MachineError> {
        self.samples += p.samples.len();
        self.nonzero += p.samples.iter().filter(|&&s| s != 0.0).count();
        let bits: Vec<u32> = p.samples.iter().map(|s| s.to_bits()).collect();
        self.bytes.extend(
            postcard::to_allocvec(&(p.timestamp.get(), p.sample_rate, p.channels, bits))
                .expect("test packet encoding"),
        );
        Ok(())
    }
}
type Output = (Vec<u8>, Vec<u8>, Vec<u8>);
fn continued(r: &mut Atari7800Runtime) -> Result<Output, Box<dyn Error>> {
    let mut frames = Frames::default();
    let mut audio = Audio::default();
    for _ in 0..3 {
        let result = r.run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })?;
        assert_eq!(
            result.stop_reason,
            StopReason::ReachedTarget,
            "a real completed native boundary"
        );
    }
    assert_eq!(frames.0.len(), 3);
    assert!(audio.samples > 0);
    assert!(audio.nonzero > 0, "active TIA audio");
    for frame in &frames.0 {
        assert!(frame.width > 0 && frame.height > 0 && frame.pixels.len() >= 4);
        assert!(
            frame
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p != &frame.pixels[..4]),
            "guest-drawn content"
        );
    }
    assert_eq!(r.time().get(), r.machine().expect("machine").master_clock());
    let pokey = r.machine_mut().expect("machine").take_pokey_audio_samples();
    assert!(
        !pokey.is_empty() && pokey.iter().any(|&v| v != 0.0),
        "active cartridge POKEY"
    );
    audio.bytes.extend(postcard::to_allocvec(
        &pokey.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
    )?);
    Ok((
        r.snapshot()?,
        postcard::to_allocvec(&frames.0)?,
        audio.bytes,
    ))
}
fn warmup(r: &mut Atari7800Runtime) {
    for _ in 0..3 {
        assert_eq!(
            r.run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink
            })
            .expect("warmup")
            .stop_reason,
            StopReason::ReachedTarget
        );
    }
}

fn prepared(model: Model) -> Atari7800Runtime {
    let rom = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-cartridges/atari-7800-logo.bin"),
    )
    .expect("project cartridge");
    let mut image = vec![0; 128];
    image[0] = 4;
    image[1..10].copy_from_slice(b"ATARI7800");
    image[49..53].copy_from_slice(&(rom.len() as u32).to_be_bytes());
    image[66..68].copy_from_slice(&5_u16.to_be_bytes());
    image.extend(rom);
    let mut r = Atari7800Runtime::new(model, image).expect("POKEY cartridge");
    warmup(&mut r);
    let m = r.machine_mut().expect("machine");
    m.set_joystick(true, false, true, false);
    m.set_console(false, true, false);
    m.set_fire(true);
    m.set_fire2(true);
    m.poke(0x1800, 0xA5);
    assert_eq!(m.peek(0x1800), 0xA5);
    for (a, v) in [
        (0x15, 4),
        (0x17, 7),
        (0x19, 15),
        (0x400F, 3),
        (0x4000, 17),
        (0x4001, 0xAF),
    ] {
        m.poke(a, v);
    }
    for _ in 0..37 {
        m.step_instruction();
    }
    m.take_audio_samples();
    m.take_pokey_audio_samples();
    let clock = m.master_clock();
    assert_ne!(clock % model.frame_ticks(), 0, "partial MARIA field");
    r.set_time(MachineTime::new(clock));
    r
}

fn all(root: &Path, capture: bool) -> Result<(), Box<dyn Error>> {
    assert_eq!(Model::ALL.len(), 2);
    for model in Model::ALL {
        let id = model.variant_id();
        let mut r = prepared(model);
        let initial = r.snapshot()?;
        let (after, frames, audio) = continued(&mut r)?;
        for (suffix, value) in ["state", "after.state", "frames", "audio"]
            .into_iter()
            .zip([initial.clone(), after, frames, audio])
        {
            let file = root.join(format!("{id}.{suffix}"));
            if capture {
                std::fs::write(file, value)?;
            } else {
                assert!(value == std::fs::read(file)?, "original {id}.{suffix}");
            }
        }
        let mut restored = Atari7800Runtime::blank(model);
        restored.restore(&initial)?;
        assert_eq!(restored.snapshot()?, initial);
        let (after, frames, audio) = continued(&mut restored)?;
        for (suffix, value) in ["after.state", "frames", "audio"]
            .into_iter()
            .zip([after, frames, audio])
        {
            let expected = std::fs::read(root.join(format!("{id}.{suffix}")))?;
            assert!(value == expected, "restored {id}.{suffix}");
        }
    }
    Ok(())
}
#[test]
fn original_version_6_states_and_native_output_continue_identically() {
    all(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/atari-7800/board-split-v6"),
        false,
    )
    .expect("original complete states and output");
}
#[test]
#[ignore = "DIAGNOSTIC: capture original board states into an explicit empty directory"]
fn capture_original_board_states() {
    let path = PathBuf::from(
        std::env::var_os("EMU198X_ATARI_BOARD_CAPTURE").expect("explicit output directory"),
    );
    std::fs::create_dir(&path).expect("new output directory");
    all(&path, true).expect("capture and immediate restored replay");
}
