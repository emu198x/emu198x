//! Capture recipe for the original, pre-split version-10 runtime.
use common_acorn_cassette::TapePulse;
use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, MediaImage, MediaKind, MediaSet,
    NullFrameSink, NullTraceSink,
};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, Model};
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
fn firmware() -> Vec<u8> {
    let mut rom = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-firmware/acorn-bbc-micro.rom"),
    )
    .expect("project firmware");
    let jumps: Vec<_> = (0..1024)
        .filter(|&i| {
            rom[i] == 0x4C && u16::from_le_bytes([rom[i + 1], rom[i + 2]]) == 0xC000 + i as u16
        })
        .collect();
    assert_eq!(jumps.len(), 1, "one final self-loop in the synthetic setup");
    let i = jumps[0];
    rom[i + 1..i + 3].copy_from_slice(&0xE200_u16.to_le_bytes());
    // A VDU-range subroutine writes a pixel; the final loop is outside HAZEL.
    rom[0x100..0x108].copy_from_slice(&[0xA9, 0xAA, 0x8D, 0, 0x30, 0x4C, 3, 0xE2]);
    // The helper jumps here, so no return stack is needed.
    rom[0x2200..0x2203].copy_from_slice(&[0x4C, 0, 0xC1]);
    // Execute the helper exactly once, then stay at E203.
    rom[0x2203..0x2212].copy_from_slice(&[
        0xAD, 0x40, 0xFE, 0x8D, 0, 2, 0xAD, 0x4F, 0xFE, 0x8D, 1, 2, 0x4C, 3, 0xE2,
    ]);
    rom[0x2100] = 0x40; // RTI: an FDC interrupt must not restart video setup.
    rom[0x3FFA..0x3FFC].copy_from_slice(&[0, 0xE1]);
    rom
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir(&output)?;
    for model in Model::ALL {
        let id = model.model_id();
        let mut runtime = BbcMicroRuntime::new(model, firmware())?;
        runtime.insert_sideways_rom(15, vec![0x5A; 16384]);
        let m = runtime.machine_mut().expect("machine");
        match model {
            Model::BbcModelB => {}
            Model::BbcBPlus => m.poke(0xFE34, 0x80),
            Model::Master128 => m.poke(0xFE34, 0x03),
        }
        for _ in 0..2 {
            runtime.run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut emu198x_shell::NullAudioSink,
                trace_sink: &mut NullTraceSink,
            })?;
        }
        if model.has_disc() {
            let disk: Vec<u8> = (0..3 * 2560).map(|i| (i % 251) as u8).collect();
            let mut media = MediaSet::new();
            media.push(MediaImage::new("drive-0", MediaKind::Disk, &disk).writable(true));
            runtime.load_media(&media)?;
        }
        let m = runtime.machine_mut().expect("machine");
        for logical in 0..16 {
            m.poke(0xFE21, (logical << 4) | if logical >= 8 { 0 } else { 7 });
        }
        for addr in 0x3000..0x3100 {
            m.poke(addr, 0x55);
        }
        m.press_key(3, 2);
        m.set_fire_button(1, true);
        m.set_adc_channel(1, 0xABC);
        m.poke(0xFE43, 0xFF);
        m.poke(0xFE42, 0x0F);
        for command in [0x85, 0x10, 0x90] {
            m.poke(0xFE4F, command);
            m.poke(0xFE40, 8);
            m.poke(0xFE40, 0);
        }
        m.poke(0xFE43, 0x7F);
        m.poke(0xFE4F, 0x23);
        m.poke(0xFE40, 3);
        m.insert_tape(vec![TapePulse::Cycles {
            half_period_ns: 208_333,
            count: 4096,
        }]);
        m.poke(0xFE10, 0x80);
        m.poke(0xFE08, 0x80);
        match model {
            Model::BbcModelB => m.poke(0xFEC0, 9),
            Model::BbcBPlus => {
                m.poke(0xFE30, 0x80);
                m.poke(0x8000, 0xA5);
                m.poke(0xFE80, 0x21);
                m.poke(0xFE87, 3);
                m.poke(0xFE84, 0x10);
                m.poke(0xFEC0, 9);
            }
            Model::Master128 => {
                m.poke(0xFE30, 5);
                m.poke(0x9000, 0xC3);
                m.poke(0xFE30, 0x80);
                m.poke(0x8000, 0xA5);
                m.poke(0xFE34, 0x09);
                m.poke(0xC000, 0x3C);
                m.poke(0xFE24, 0x25);
                m.poke(0xFE2B, 3);
                m.poke(0xFE28, 0x10);
                m.poke(0xFE18, 9);
            }
        }
        let start = m.cpu_cycles();
        while m.cpu_cycles() - start < 1234 {
            m.step_instruction();
        }
        assert!(m.cassette_motor_on());
        assert_eq!(m.adc_channel(1), 0xABC);
        if model.has_disc() {
            assert!(m.disk(0).is_some());
            assert_eq!(m.peek(0x8000), 0xA5);
        }
        std::fs::write(output.join(format!("{id}.state")), runtime.snapshot()?)?;
        let mut audio = Audio::default();
        let mut times = Vec::new();
        for _ in 0..3 {
            let r = runtime.run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut audio,
                trace_sink: &mut NullTraceSink,
            })?;
            times.push(r.reached.get());
        }
        let m = runtime.machine().expect("machine");
        assert!(
            m.framebuffer()
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1,
            "real raster content"
        );
        assert!(
            audio
                .0
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| f32::from_le_bytes(*p).abs() > 0.000_001),
            "audible tone"
        );
        let pixels: Vec<u8> = m
            .framebuffer()
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect();
        std::fs::write(output.join(format!("{id}.pixels")), pixels)?;
        std::fs::write(output.join(format!("{id}.audio")), audio.0)?;
        std::fs::write(
            output.join(format!("{id}.times.json")),
            serde_json::to_vec(&times)?,
        )?;
        std::fs::write(
            output.join(format!("{id}.after.state")),
            runtime.snapshot()?,
        )?;
    }
    Ok(())
}
