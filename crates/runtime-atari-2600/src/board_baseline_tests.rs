//! Freeze original runtime state and complete native output before owner extraction.
use crate::{Atari2600Runtime, Model};
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
fn continued(r: &mut Atari2600Runtime) -> Result<Output, Box<dyn Error>> {
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

    Ok((
        r.snapshot()?,
        postcard::to_allocvec(&frames.0)?,
        audio.bytes,
    ))
}
fn warmup(r: &mut Atari2600Runtime) {
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

fn prepared(model: Model, banked: bool) -> Atari2600Runtime {
    let mut rom = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-cartridges/atari-2600-logo.bin"),
    )
    .expect("project cartridge");
    if banked {
        let bank = rom.clone();
        assert!(rom[0xFE0..0xFF8].iter().all(|&b| b == 0xFF));
        rom[0xFF0] = 0x53;
        rom.extend(bank);
    }
    let mut r = Atari2600Runtime::new(model, rom).expect("cartridge");
    warmup(&mut r);
    let m = r.machine_mut().expect("machine");
    m.set_joystick_input(0xAF);
    m.set_switch_input(0x7F);
    m.set_fire(0, true);
    m.set_paddle(1, 73);
    m.set_keypad_key(1, 2, 1, true);
    m.poke(0x80, 0xA5);
    assert_eq!(m.peek(0x80), 0xA5);
    if banked {
        m.poke(0x1FF8, 0);
        assert_eq!(m.peek(0x1FF0), 0x53, "selected F8 bank");
    }
    for (a, v) in [(0x15, 4), (0x17, 7), (0x19, 15)] {
        m.poke(a, v);
    }
    for _ in 0..37 {
        m.step_instruction();
    }
    m.take_audio_samples();
    let clock = m.master_clock();
    r.set_time(MachineTime::new(clock));
    r
}

fn all(root: &Path, capture: bool) -> Result<(), Box<dyn Error>> {
    assert_eq!(Model::ALL.len(), 2);
    for model in Model::ALL {
        for banked in [false, true] {
            let id = format!("{}{}", model.variant_id(), if banked { "-f8" } else { "" });
            let mut r = prepared(model, banked);
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
            let mut restored = Atari2600Runtime::blank(model);
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
    }
    Ok(())
}
#[test]
fn original_version_2_states_and_native_output_continue_identically() {
    all(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/atari-2600/board-split-v2"),
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
