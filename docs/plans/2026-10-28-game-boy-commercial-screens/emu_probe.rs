use emu198x_shell::{
    FirmwareImage, FirmwareSet, FramePacket, FrameSink, HostIo, MachineCore, MachineError,
    MediaImage, MediaKind, MediaSet, NullAudioSink, NullTraceSink, PixelFormat, StopReason,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
use std::{error::Error, path::Path};
#[derive(Default)]
struct Picture {
    pixels: Vec<u8>,
    count: usize,
}
impl FrameSink for Picture {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        assert_eq!(frame.format, PixelFormat::Indexed8);
        assert_eq!((frame.width, frame.height), (160, 144));
        self.pixels = frame.pixels.to_vec();
        self.count += 1;
        Ok(())
    }
}
fn step(runtime: &mut GameBoyRuntime, picture: &mut Picture) -> Result<StopReason, MachineError> {
    Ok(runtime
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: picture,
            audio_sink: &mut NullAudioSink,
            trace_sink: &mut NullTraceSink,
        })?
        .stop_reason)
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("ROM BOOT OUTPUT_PREFIX".into());
    }
    let rom = std::fs::read(&args[1])?;
    let boot = std::fs::read(&args[2])?;
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dmg-boot-rom", &boot));
    let mut runtime = GameBoyRuntime::from_firmware(Model::Dmg, &firmware)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime.load_media(&media)?;
    let mut picture = Picture::default();
    let mut anchored = false;
    for _ in 0..3000 {
        let result = step(&mut runtime, &mut picture)?;
        if result == StopReason::ReachedTarget
            && runtime.machine().expect("loaded").peek(0xFF50) & 1 != 0
        {
            anchored = true;
            break;
        }
    }
    assert!(anchored, "boot must reach its first completed LCD field");
    let mut frames = 0;
    let mut attempts = 0;
    while frames < 720 && attempts < 5000 {
        attempts += 1;
        let result = step(&mut runtime, &mut picture)?;
        if result == StopReason::NoFrame {
            continue;
        }
        assert_eq!(result, StopReason::ReachedTarget);
        frames += 1;
        if [240, 480, 720].contains(&frames) {
            assert!(picture.pixels.iter().all(|p| *p < 4));
            std::fs::write(format!("{}-{frames}.gray", args[3]), &picture.pixels)?;
            let m = runtime.machine().expect("loaded");
            println!(
                "frame={frames} attempts={attempts} time={} pc={:04x} div={:02x}",
                runtime.time().get(),
                m.cpu().pc,
                m.peek(0xFF04)
            );
        }
    }
    assert_eq!(frames, 720);
    assert_eq!(
        std::fs::read(Path::new(&args[1]))?,
        rom,
        "source ROM changed"
    );
    Ok(())
}
