use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, NullFrameSink, NullTraceSink,
};
use emu198x_zilog_z80::Z80Stepper;
use runtime_coleco_colecovision::{CvRuntime, Model};
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
    let mut code = vec![0xF3, 0x31, 0xF0, 0x63];
    let out = |code: &mut Vec<u8>, port: u8, value: u8| code.extend([0x3E, value, 0xD3, port]);
    for (reg, value) in [(1, 0xF0), (2, 6), (4, 0), (7, 0xF1)] {
        out(&mut code, 0xBF, value);
        out(&mut code, 0xBF, 0x80 | reg);
    }
    out(&mut code, 0xBF, 0);
    out(&mut code, 0xBF, 0x40);
    for i in 0..16 {
        out(&mut code, 0xBE, if i % 2 == 0 { 0xAA } else { 0x55 });
    }
    for value in [0x85, 0x08, 0x90] {
        out(&mut code, 0xE0, value);
    }
    let again = code.len();
    out(&mut code, 0xC0, 0);
    code.extend([0xDB, 0xE0, 0x32, 0, 0x60]);
    out(&mut code, 0x80, 0);
    code.extend([0xDB, 0xE2, 0x32, 1, 0x60]);
    let displacement =
        i8::try_from(again as isize - (code.len() + 2) as isize).expect("short loop");
    code.extend([0x18, displacement.cast_unsigned()]);
    let mut rom = vec![0; 8192];
    rom[..3].copy_from_slice(&[0xC3, 0, 1]);
    // Real NMI wiring: acknowledge VDP status, preserve AF, return through RETN.
    rom[0x66..0x6C].copy_from_slice(&[0xF5, 0xDB, 0xBF, 0xF1, 0xED, 0x45]);
    rom[0x100..0x100 + code.len()].copy_from_slice(&code);
    rom
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir(&output)?;
    for model in Model::ALL {
        let id = model.model_id();
        let mut runtime = CvRuntime::new(model, rom())?;
        let m = runtime.machine_mut().expect("machine");
        m.controller1_mut().up = true;
        m.controller1_mut().left_button = true;
        m.controller2_mut().right_button = true;
        // Capture an accepted v5 state with in-flight CPU/video/audio progress.
        for _ in 0..12_345 {
            m.step_tick();
        }
        let initial = runtime.snapshot()?;
        std::fs::write(output.join(format!("{id}.state")), &initial)?;
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
        assert_eq!(m.peek(0x6000), 0xB7);
        assert_eq!(m.peek(0x6001), 0x3F);
        let pixels: Vec<u8> = m
            .framebuffer()
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect();
        assert!(
            pixels
                .as_chunks::<4>()
                .0
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
