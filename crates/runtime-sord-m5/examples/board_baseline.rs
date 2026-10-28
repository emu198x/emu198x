use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, NullFrameSink, NullTraceSink,
};
use runtime_sord_m5::{M5Runtime, Model};
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
    let mut code = vec![0xF3, 0x31, 0xF0, 0x7F];
    let out = |code: &mut Vec<u8>, port: u8, value: u8| code.extend([0x3E, value, 0xD3, port]);
    for (reg, value) in [(1, 0xD0), (2, 6), (4, 0), (7, 0xF1)] {
        out(&mut code, 0x11, value);
        out(&mut code, 0x11, 0x80 | reg);
    }
    out(&mut code, 0x11, 0);
    out(&mut code, 0x11, 0x40);
    for i in 0..16 {
        out(&mut code, 0x10, if i % 2 == 0 { 0xAA } else { 0x55 });
    }
    for value in [0x85, 0x08, 0x90] {
        out(&mut code, 0x20, value);
    }
    for value in [0x40, 0x05, 7] {
        out(&mut code, 0, value);
    }
    code.extend([
        0xDB, 0x32, 0x32, 0, 0x71, 0xDB, 0x37, 0x32, 1, 0x71, 0x18, 0xF4,
    ]);
    let mut rom = vec![0; 8192];
    rom[..code.len()].copy_from_slice(&code);
    rom
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir(&output)?;
    for model in Model::ALL {
        let id = model.model_id();
        let mut runtime = M5Runtime::new(model, rom());
        let m = runtime.machine_mut().expect("machine");
        m.press_key(2, 5);
        m.set_joystick(1, true, false, false, false);
        // Capture an accepted v5 state with in-flight CPU/video/audio progress.
        m.run_until_pc(0xFFFF, 12_345);
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
        assert_eq!(m.peek(0x7100), 0x20);
        assert_eq!(m.peek(0x7101), 2);
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
