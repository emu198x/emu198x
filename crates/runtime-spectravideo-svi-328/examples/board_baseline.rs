use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, NullFrameSink, NullTraceSink,
};
use emu198x_zilog_z80::Z80Stepper;
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};
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
    let mut code = Vec::new();
    let out = |code: &mut Vec<u8>, port: u8, value: u8| code.extend([0x3E, value, 0xD3, port]);
    for (reg, value) in [(1, 0xD0), (2, 6), (4, 0), (7, 0xF1)] {
        out(&mut code, 0x81, value);
        out(&mut code, 0x81, 0x80 | reg);
    }
    out(&mut code, 0x81, 0);
    out(&mut code, 0x81, 0x40);
    for i in 0..16 {
        out(&mut code, 0x80, if i % 2 == 0 { 0xAA } else { 0x55 });
    }
    // Configure both PSG ports and the PPI before observing inputs/motor.
    for (reg, value) in [(0, 32), (1, 0), (7, 0xBE), (8, 15), (15, 0)] {
        out(&mut code, 0x88, reg);
        out(&mut code, 0x8C, value);
    }
    out(&mut code, 0x97, 0x92);
    out(&mut code, 0x96, 2);
    // Execute in fixed high RAM while low RAM overlays the BIOS and the cart
    // overlays $8000. Distinct sentinels prove both bank selections stay live.
    code.extend([0x3E, 0xA5, 0x32, 0, 0x10]);
    out(&mut code, 0x88, 14);
    let again = code.len();
    code.extend([0xDB, 0x99, 0x32, 0, 0xC0, 0xDB, 0x90, 0x32, 1, 0xC0]);
    code.extend([0x3A, 0, 0x80, 0x32, 3, 0xC0]);
    // Mark whether both cassette levels have actually reached the PPI input.
    code.extend([
        0xDB, 0x98, 0x32, 2, 0xC0, 0xE6, 0x80, 0x07, 0x3C, 0x47, 0x3A, 4, 0xC0, 0xB0, 0x32, 4, 0xC0,
    ]);
    let target = u16::try_from(0xC100 + again).expect("guest fits upper RAM");
    code.push(0xC3);
    code.extend(target.to_le_bytes());
    let length = u16::try_from(code.len()).expect("guest length");
    let [lo, hi] = length.to_le_bytes();
    let mut rom = vec![0; 0x8000];
    // DI; stack; LDIR guest into high RAM; JP high RAM.
    rom[..18].copy_from_slice(&[
        0xF3, 0x31, 0xF0, 0xFF, 0x21, 0, 1, 0x11, 0, 0xC1, 0x01, lo, hi, 0xED, 0xB0, 0xC3, 0, 0xC1,
    ]);
    rom[0x100..0x100 + code.len()].copy_from_slice(&code);
    rom
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory")?);
    std::fs::create_dir(&output)?;
    for model in Model::ALL {
        let id = model.model_id();
        let mut runtime = Svi328Runtime::new(model, rom())?;
        runtime.insert_cartridge(vec![0x5A; 16 * 1024])?;
        let tape = format198x_spectravideo_svi_cas::encode(
            &format198x_spectravideo_svi_cas::CasImage::new(vec![vec![0x42, 0x80, 0x7F]])?,
        )?;
        runtime.insert_cassette(&tape)?;
        let m = runtime.machine_mut().expect("machine");
        m.press_key(2, 5);
        m.set_joystick(1, true, false, false, false, true);
        // Capture an accepted v7 state with in-flight CPU/video/audio progress.
        for _ in 0..34_567 {
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
        assert_eq!(m.peek(0xC000), 0xDF);
        assert_eq!(m.peek(0xC001), 0xFE);
        assert_eq!(m.peek(0xC003), 0x5A);
        assert_eq!(m.peek(0xC004), 3, "guest observed both tape levels");
        assert_eq!(m.peek(0xC002) & 0x50, 0, "fire pressed, tape present");
        assert_eq!(m.peek(0x1000), 0xA5);
        assert_eq!(m.memory_control(), (true, true));
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
