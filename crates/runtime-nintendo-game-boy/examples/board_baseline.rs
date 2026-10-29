//! Capture original v4 saves and every emitted frame/audio sample before splitting.
use common_nintendo_game_boy::JoypadButton;
use emu198x_shell::{
    AudioPacket, AudioSink, CapturedFrame, FirmwareImage, FirmwareSet, FramePacket, FrameSink,
    HostIo, MachineCore, MachineError, MediaImage, MediaKind, MediaSet, NullTraceSink,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
use std::{error::Error, path::PathBuf};
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
fn cartridge(colour: bool, speed: bool) -> Vec<u8> {
    let mut bytes = vec![0; 32768];
    bytes[0x100..0x103].copy_from_slice(&[0xC3, 0x50, 1]);
    let mut code = vec![0xF3]; // DI
    if speed {
        code.extend([0x3E, 1, 0xE0, 0x4D, 0x10, 0]);
    }
    code.extend([0xC3, 0x80, 0xFF]);
    bytes[0x150..0x150 + code.len()].copy_from_slice(&code);
    bytes[0x143] = if colour { 0x80 } else { 0 };
    bytes[0x14D] = bytes[0x134..=0x14C]
        .iter()
        .fold(0_u8, |sum, &b| sum.wrapping_sub(b).wrapping_sub(1));
    bytes
}
fn firmware(size: usize) -> Vec<u8> {
    let mut bytes = vec![0; size];
    bytes[..3].copy_from_slice(&[0xC3, 0xF0, 0]);
    bytes[0xF0..0xF9].copy_from_slice(&[0x06, 17, 0x05, 0x20, 0xFD, 0xC3, 0xFC, 0, 0]);
    bytes[0xFC..0x100].copy_from_slice(&[0x3E, 1, 0xE0, 0x50]);
    bytes
}
fn firmware_description(model: Model) -> Option<(&'static str, usize)> {
    use runtime_nintendo_game_boy::{
        CGB_BOOT_ROM_ID, DMG_BOOT_ROM_ID, DMG0_BOOT_ROM_ID, MGB_BOOT_ROM_ID,
    };
    match model {
        Model::Dmg0 => Some((DMG0_BOOT_ROM_ID, 256)),
        Model::Dmg => Some((DMG_BOOT_ROM_ID, 256)),
        Model::Mgb => Some((MGB_BOOT_ROM_ID, 256)),
        Model::Cgb | Model::Agb => Some((CGB_BOOT_ROM_ID, 0x900)),
        Model::Sgb | Model::Sgb2 => None,
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir(&output)?;
    for model in Model::ALL {
        for booting in [false, true] {
            let boot = if booting {
                let Some((_, size)) = firmware_description(model) else {
                    continue;
                };
                Some(firmware(size))
            } else {
                None
            };
            let id = format!(
                "{}{}",
                model.variant_id(),
                if booting { "-boot" } else { "" }
            );
            let mut runtime = if let Some(boot) = boot.as_ref() {
                let (id, _) = firmware_description(model).expect("firmware model");
                let mut set = FirmwareSet::new();
                set.push(FirmwareImage::new(id, boot));
                GameBoyRuntime::from_firmware(model, &set)?
            } else {
                GameBoyRuntime::blank(model)
            };
            let rom = cartridge(model.is_cgb(), model.is_cgb() && !booting);
            let mut media = MediaSet::new();
            media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
            runtime.load_media(&media)?;
            let m = runtime.machine_mut().expect("machine");
            m.poke(0xFF80, 0x18);
            m.poke(0xFF81, 0xFE);
            for _ in 0..16 {
                m.step_m_cycle();
            }
            if !booting && model.is_cgb() {
                assert!(m.double_speed(), "guest switched speed");
            }
            m.poke(0xFF40, 0);
            for i in 0..32 {
                m.poke(0x8000 + i, if i % 2 == 0 { 0xAA } else { 0x55 });
            }
            for i in 0..1024 {
                m.poke(0x9800 + i, (i % 2) as u8);
            }
            m.poke(0xFF47, 0xE4);
            if model.is_cgb() {
                m.poke(0xFF68, 0x80);
                m.poke(0xFF6A, 0x80);
                for b in [0xFF, 0x7F, 0x1F, 0, 0xE0, 3, 0, 0x7C] {
                    m.poke(0xFF69, b);
                    m.poke(0xFF6B, b);
                }
                m.poke(0xFF70, 3);
                m.poke(0xD000, 0xA5);
                m.poke(0xFF70, 5);
                m.poke(0xD000, 0xC3);
                m.poke(0xFF70, 3);
                m.poke(0xFF4F, 1);
                m.poke(0x8000, 0x5A);
            }
            for (a, v) in [
                (0xFF26, 0x80),
                (0xFF24, 0x77),
                (0xFF25, 0x11),
                (0xFF10, 0),
                (0xFF11, 0x80),
                (0xFF12, 0xF3),
                (0xFF13, 0x40),
                (0xFF14, 0x87),
                (0xFF05, 0xFE),
                (0xFF07, 5),
                (0xFF00, 0x20),
            ] {
                m.poke(a, v);
            }
            m.set_button(JoypadButton::Right, true);
            m.set_button(JoypadButton::A, true);
            for i in 0..160 {
                m.poke(0xC000 + i, (i % 251) as u8);
            }
            m.poke(0xFF40, 0x91);
            if model.is_cgb() {
                for (a, v) in [
                    (0xFF51, 0xC0),
                    (0xFF52, 0),
                    (0xFF53, 0),
                    (0xFF54, 0x10),
                    (0xFF55, 0x83),
                ] {
                    m.poke(a, v);
                }
            }
            // OAM DMA blocks cartridge/firmware fetches. Normal execution has
            // reached the HRAM loop; mid-boot cases exercise firmware instead.
            if !booting {
                m.poke(0xFF46, 0xC0);
            }
            for _ in 0..37 {
                m.step_m_cycle();
            }
            assert_eq!(m.peek(0xFF00) & 1, 0, "right held");
            if booting {
                assert_ne!(m.peek(0xFF50), 0xFF, "capture still inside boot firmware");
            }
            if model.is_cgb() {
                assert_eq!(m.peek(0xD000), 0xA5);
            }
            // Already-produced host audio is intentionally not serialized. Start
            // both continuations at an empty host queue, without altering hardware.
            let before = postcard::to_allocvec(&*m)?;
            let mut prior_audio = [0.0; 4096];
            while m.drain_audio(&mut prior_audio) > 0 {}
            assert!(
                postcard::to_allocvec(&*m)? == before,
                "host drain changed hardware state"
            );
            std::fs::write(output.join(format!("{id}.state")), runtime.snapshot()?)?;
            let mut frames = Frames::default();
            let mut audio = Audio::default();
            for _ in 0..3 {
                let n = frames.0.len();
                let r = runtime.run_frame(&mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frames,
                    audio_sink: &mut audio,
                    trace_sink: &mut NullTraceSink,
                })?;
                assert_eq!(frames.0.len(), n + 1);
                assert_eq!(frames.0.last().expect("frame").timestamp, r.reached);
            }
            for f in &frames.0 {
                assert!(
                    f.rgba_pixels()?
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        > 1,
                    "guest picture"
                );
            }
            assert!(
                audio
                    .0
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|v| f32::from_le_bytes(*v).abs() > 0.000_001),
                "audible tone"
            );
            if booting {
                assert_eq!(
                    runtime.machine().expect("machine").peek(0xFF50),
                    0xFF,
                    "firmware handoff"
                );
            }
            std::fs::write(
                output.join(format!("{id}.frames")),
                postcard::to_allocvec(&frames.0)?,
            )?;
            std::fs::write(output.join(format!("{id}.audio")), audio.0)?;
            std::fs::write(
                output.join(format!("{id}.after.state")),
                runtime.snapshot()?,
            )?;
        }
    }
    Ok(())
}
