use common_commodore_vic_20::{Vic20Key, Vic20RamExpansion};
use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, NullFrameSink, NullTraceSink,
};
use emu198x_shell::{MediaImage, MediaKind, MediaSet};
use runtime_commodore_vic_20::{Model, Vic20Runtime};
use std::{error::Error, path::PathBuf};
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
fn rom() -> Vec<u8> {
    let mut code = vec![0x78]; // SEI: exercise the VIAs without an interrupt handler.
    let store = |code: &mut Vec<u8>, addr: u16, value: u8| {
        code.extend([0xA9, value, 0x8D]);
        code.extend(addr.to_le_bytes());
    };
    // VIC picture, audible bass tone, VIA2 keyboard columns, both timers.
    for (addr, value) in [
        (0x9000, 12),
        (0x9001, 38),
        (0x9002, 22),
        (0x9003, 46),
        (0x9005, 0xF0),
        (0x900A, 0xF1),
        (0x900E, 15),
        (0x900F, 0x1B),
        (0x9122, 0x7F),
        (0x9120, 0),
        (0x911B, 0x40),
        (0x9114, 0x87),
        (0x9115, 0x04),
        (0x912B, 0x40),
        (0x9124, 0x43),
        (0x9125, 0x02),
        (0x0400, 0xA5),
        (0x2000, 0xC3),
    ] {
        store(&mut code, addr, value);
    }
    // Distinct character/colour rows in screen RAM; all glyphs alternate bits.
    for i in 0..=255_u16 {
        store(&mut code, 0x1E00 + i, (i & 7) as u8);
        store(&mut code, 0x9600 + i, 2);
    }
    let again = u16::try_from(0xE000 + code.len()).expect("guest fits ROM");
    for (src, dst) in [
        (0x9121_u16, 0x0200_u16),
        (0x9111, 0x0201),
        (0x9110, 0x0202),
        (0xA100, 0x0203),
        (0x0400, 0x0204),
        (0x2000, 0x0205),
    ] {
        code.push(0xAD);
        code.extend(src.to_le_bytes());
        code.push(0x8D);
        code.extend(dst.to_le_bytes());
    }
    code.push(0x4C);
    code.extend(again.to_le_bytes());
    let mut rom = vec![0xEA; 8192];
    rom[..code.len()].copy_from_slice(&code);
    rom[0x1FFA..].copy_from_slice(&[0, 0xE0, 0, 0xE0, 0, 0xE0]);
    rom
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir_all(&output)?;
    for model in Model::ALL {
        let id = model.model_id();
        let mut runtime = Vic20Runtime::new(model, rom(), vec![0; 8192], vec![0xAA; 4096])?;
        runtime.set_ram_expansion(Vic20RamExpansion {
            exp_3k: true,
            blk1: true,
            ..Vic20RamExpansion::NONE
        });
        let cart = vec![0x5A; 8192];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &cart));
        runtime.load_media(&media)?;
        let m = runtime.machine_mut().expect("machine");
        m.press_key(Vic20Key::A);
        m.set_joystick(true, false, false, true, true);
        m.set_user_port_pb0(false);
        while m.master_clock() < 12_345 {
            m.step_instruction();
        }
        assert_eq!(m.peek(0x0203), 0x5A);
        assert_eq!(m.peek(0x0204), 0xA5);
        assert_eq!(m.peek(0x0205), 0xC3);
        assert_eq!(m.peek(0x0200), 0xFD);
        assert_eq!(m.peek(0x0202) & 1, 0);
        std::fs::write(output.join(format!("{id}.state")), runtime.snapshot()?)?;
        let mut audio = Audio::default();
        let mut frames = Vec::new();
        for _ in 0..3 {
            let result = runtime.run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut audio,
                trace_sink: &mut NullTraceSink,
            })?;
            frames.push(result.reached.get());
        }
        let m = runtime.machine().expect("machine");
        assert_eq!(m.peek(0x0200), 0xFD);
        assert_eq!(m.peek(0x0201) & 0x3C, 0x18);
        assert_eq!(m.peek(0x0202) & 1, 1);
        assert_eq!(m.peek(0x0203), 0x5A);
        let pixels: Vec<u8> = m
            .framebuffer()
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect();
        assert!(
            m.framebuffer()
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1
        );
        assert!(
            audio
                .0
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| f32::from_le_bytes(*p).abs() > 0.000_001)
        );
        std::fs::write(output.join(format!("{id}.pixels")), pixels)?;
        std::fs::write(output.join(format!("{id}.audio")), audio.0)?;
        std::fs::write(
            output.join(format!("{id}.after.state")),
            runtime.snapshot()?,
        )?;
        std::fs::write(
            output.join(format!("{id}.times.json")),
            serde_json::to_vec(&frames)?,
        )?;
    }
    Ok(())
}
