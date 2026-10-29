//! Original v10 models and complete continued frame/audio output.
use common_msx::MapperType;
use emu198x_shell::{
    AudioPacket, AudioSink, CapturedFrame, FramePacket, FrameSink, HostIo, MachineCore,
    MachineError, NullTraceSink,
};
use emu198x_zilog_z80::Z80Stepper;
use runtime_msx::{Model, MsxRuntime};
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
type Output = (Vec<u8>, Vec<u8>, Vec<u8>);
fn continued(r: &mut MsxRuntime) -> Result<Output, Box<dyn Error>> {
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
        assert_eq!(result.stop_reason, emu198x_shell::StopReason::ReachedTarget);
    }
    assert_eq!(frames.0.len(), 3);
    assert!(!audio.0.is_empty());
    Ok((r.snapshot()?, postcard::to_allocvec(&frames.0)?, audio.0))
}
fn prepared(model: Model) -> MsxRuntime {
    let layout = model.layout();
    let ram = layout
        .mapper
        .as_ref()
        .map(|m| m.slot)
        .unwrap_or_else(|| layout.ram[0].slot);
    let cart = layout.cartridge_ports[0];
    let primary =
        layout.bios.primary | (cart.primary << 2) | (cart.primary << 4) | (ram.primary << 6);
    let mut code = vec![0xF3];
    for (port, value) in [
        (0xAB, 0x82),
        (0xA8, primary),
        (0xAA, 0xA3),
        (0xA0, 0),
        (0xA1, 0x34),
        (0xA0, 1),
        (0xA1, 2),
        (0xA0, 7),
        (0xA1, 0x3E),
        (0xA0, 8),
        (0xA1, 15),
        (0xFC, 2),
        (0xFD, 3),
        (0xFE, 4),
        (0xFF, 5),
        (0xB4, 13),
        (0xB5, 8),
        (0xB4, 0),
        (0xB5, 7),
    ] {
        code.extend([0x3E, value, 0xD3, port]);
    }
    if let Some(sub) = ram.secondary {
        code.extend([0x3E, sub << 6, 0x32, 0xFF, 0xFF]);
    }
    let original = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/synthetic-firmware/msx.rom"),
    )
    .expect("project firmware");
    code.extend(original);
    code.truncate(32768);
    let mut r = MsxRuntime::blank(model);
    if model.is_msx2() {
        r.set_sub_rom(vec![0xC9; 16384]).expect("own SUB-ROM");
    }
    r.set_bios(code).expect("own BIOS");
    r.insert_cartridge1(vec![0; 128 * 1024], MapperType::KonamiScc);
    r.insert_cartridge2(vec![0x53; 65536], MapperType::Ascii8);
    let _ = continued(&mut r).expect("warm up");
    let m = r.machine_mut().expect("machine");
    for offset in 0..256_u16 {
        m.poke(0xC000 + offset, (offset % 251) as u8);
    }
    assert_eq!(m.peek(0xC055), 0x55, "model's selected RAM really exists");
    m.press_key(2, 4);
    m.set_joystick(1, true, false, true, false, true);
    m.poke(0x9000, 0x3F);
    for i in 0..32_u16 {
        m.poke(0x9800 + i, if i < 16 { 0x70 } else { 0x90 });
    }
    m.poke(0x9880, 253);
    m.poke(0x988A, 15);
    m.poke(0x988F, 1);
    for _ in 0..37 {
        m.step_tick();
    }
    let mut scratch = [0.0; 4096];
    while m.end_audio_frame(&mut scratch) > 0 {}
    r
}
pub fn all(root: &Path, capture: bool) -> Result<(), Box<dyn Error>> {
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
        let mut restored = MsxRuntime::blank(model);
        restored.restore(&initial)?;
        assert_eq!(restored.snapshot()?, initial);
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
    }
    Ok(())
}
