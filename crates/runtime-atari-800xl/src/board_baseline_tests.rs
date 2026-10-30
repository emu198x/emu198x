//! Freeze original runtime state and complete native output before owner extraction.
use crate::{Atari800xlRuntime, Model};
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
fn continued(r: &mut Atari800xlRuntime) -> Result<Output, Box<dyn Error>> {
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
    assert!(audio.nonzero > 0, "active POKEY audio");
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
fn warmup(r: &mut Atari800xlRuntime) {
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

fn prepared(model: Model, banked: bool) -> Atari800xlRuntime {
    let probe = include_bytes!("../../../test-data/atari/gtia-modes/atari-800xl-gtia-modes.bin");
    assert_eq!(probe.len(), 8192);
    let image = if banked {
        let mut rom = vec![0xFF; 32768];
        rom[..3].copy_from_slice(&[0x4C, 0x00, 0xA0]);
        for bank in 0..3 {
            rom[bank * 8192 + 0x100] = 0x40 + bank as u8;
        }
        rom[3 * 8192..].copy_from_slice(probe);
        let mut header = vec![0; 16];
        header[..4].copy_from_slice(b"CART");
        header[4..8].copy_from_slice(&12u32.to_be_bytes());
        header.extend(rom);
        header
    } else {
        probe.to_vec()
    };
    let mut r =
        Atari800xlRuntime::new(model, None, None, Some(image), false).expect("project cartridge");
    warmup(&mut r);
    let mut media = emu198x_shell::MediaSet::new();
    let xex = [0xFF, 0xFF, 0x00, 0x60, 0x00, 0x60, 0x60];
    media.push(emu198x_shell::MediaImage::new(
        "program-1",
        emu198x_shell::MediaKind::Program,
        &xex,
    ));
    r.load_media(&media).expect("pending XEX");
    assert!(r.xex_pending());
    let m = r.machine_mut().expect("machine");
    let mut disk = common_atari_800xl::Disk::new(128, vec![1; 256], false).expect("disk");
    assert!(disk.write_sector(2, &[0x5A; 128]));
    m.sio_mut().insert_disk(1, disk);
    m.set_joystick(true, false, true, false);
    m.set_fire(true);
    m.set_console_keys(false, true, false);
    m.press_key(0x3F);
    m.poke(0x0600, 0xA5);
    assert_eq!(m.peek(0x0600), 0xA5);
    if matches!(model, Model::A130xeNtsc | Model::A130xePal) {
        for bank in 0..4 {
            m.load_program_byte(0xD301, 0xE3 | (bank << 2));
            m.poke(0x4000, 0xA0 | bank);
            assert_eq!(m.extended_ram(bank as usize, 0), Some(0xA0 | bank));
        }
        m.load_program_byte(0xD301, 0xEB);
        assert_eq!(m.peek(0x4000), 0xA2);
    } else {
        m.poke(0x4000, 0x77);
        assert_eq!(m.extended_ram(0, 0), None);
    }
    if banked {
        m.load_program_byte(0xD500, 2);
        assert_eq!(m.peek(0x8100), 0x42);
    }
    for (a, v) in [(0xD20F, 3), (0xD200, 17), (0xD201, 0xAF)] {
        m.load_program_byte(a, v);
    }
    for _ in 0..37 {
        m.step_instruction();
    }
    m.take_audio_buffer();
    let clock = m.master_clock();
    assert_ne!(clock % model.frame_ticks(), 0, "partial ANTIC field");
    r.set_time(MachineTime::new(clock));
    r
}

fn all(root: &Path, capture: bool) -> Result<(), Box<dyn Error>> {
    assert_eq!(Model::ALL.len(), 4);
    for model in Model::ALL {
        for banked in [false, true] {
            let id = format!(
                "{}{}",
                model.variant_id(),
                if banked { "-xegs" } else { "" }
            );
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
            let mut restored = Atari800xlRuntime::blank(model);
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
fn original_version_21_states_and_native_output_continue_identically() {
    all(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/atari-800xl/board-split-v21"),
        false,
    )
    .expect("original complete states and output");
}
#[test]
#[ignore = "DIAGNOSTIC: capture original board states into an explicit empty directory"]
fn capture_original_board_states() {
    let path = PathBuf::from(
        std::env::var_os("EMU198X_XL_XE_BOARD_CAPTURE").expect("explicit output directory"),
    );
    std::fs::create_dir(&path).expect("new output directory");
    all(&path, true).expect("capture and immediate restored replay");
}
