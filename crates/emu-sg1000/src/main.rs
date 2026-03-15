//! Sega SG-1000 emulator binary.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;

use emu_core::runner::Runner;
use emu_sg1000::{Sg1000, Sg1000Region};
use winit::keyboard::KeyCode;

const FB_WIDTH: u32 = 256;
const FB_HEIGHT: u32 = 192;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct CliArgs {
    rom_path: Option<PathBuf>,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    mute: bool,
    region: Sg1000Region,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        rom_path: None,
        headless: false,
        frames: 200,
        screenshot_path: None,
        mute: false,
        region: Sg1000Region::Ntsc,
    };
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" => {
                i += 1;
                cli.rom_path = args.get(i).map(PathBuf::from);
            }
            "--headless" => cli.headless = true,
            "--frames" => {
                i += 1;
                cli.frames = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(200);
            }
            "--screenshot" => {
                i += 1;
                cli.screenshot_path = args.get(i).map(PathBuf::from);
            }
            "--mute" => cli.mute = true,
            "--region" => {
                i += 1;
                cli.region = match args.get(i).map(|s| s.as_str()) {
                    Some("pal") => Sg1000Region::Pal,
                    _ => Sg1000Region::Ntsc,
                };
            }
            "--help" | "-h" => {
                eprintln!("Usage: emu-sg1000 [OPTIONS] [rom.sg]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --rom <file>         SG-1000 cartridge ROM (.sg, .bin)");
                eprintln!("  --region <ntsc|pal>  Video region [default: ntsc]");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --frames <n>         Frames in headless mode [default: 200]");
                eprintln!("  --screenshot <file>  Save PNG screenshot (headless)");
                eprintln!("  --mute               Disable host audio playback (windowed)");
                process::exit(0);
            }
            other if other.starts_with('-') => {
                eprintln!("Unknown argument: {other}");
                process::exit(1);
            }
            // Positional argument: treat as ROM file
            _ => {
                cli.rom_path = Some(PathBuf::from(&args[i]));
            }
        }
        i += 1;
    }
    if cli.screenshot_path.is_some() {
        cli.headless = true;
    }
    cli
}

// ---------------------------------------------------------------------------
// Screenshot (headless)
// ---------------------------------------------------------------------------

fn save_screenshot(fb: &[u32], width: u32, height: u32, path: &Path) -> Result<(), Box<dyn Error>> {
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for (i, &argb) in fb.iter().enumerate() {
        let o = i * 4;
        rgba[o] = ((argb >> 16) & 0xFF) as u8;
        rgba[o + 1] = ((argb >> 8) & 0xFF) as u8;
        rgba[o + 2] = (argb & 0xFF) as u8;
        rgba[o + 3] = 0xFF;
    }
    writer.write_image_data(&rgba)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// A minimal ROM that just halts (DI + HALT).
fn stub_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 32];
    rom[0] = 0xF3; // DI
    rom[1] = 0x76; // HALT
    rom
}

fn load_rom(path: &PathBuf, region: Sg1000Region) -> Sg1000 {
    let rom_data = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("Failed to read ROM: {e}");
        process::exit(1);
    });
    eprintln!("Loaded ROM: {}", path.display());
    Sg1000::new(rom_data, region)
}

fn main() {
    let cli = parse_args();

    if cli.headless {
        let rom_path = cli.rom_path.as_ref().unwrap_or_else(|| {
            eprintln!("No ROM file specified. Use --rom <file>");
            process::exit(1);
        });
        let mut system = load_rom(rom_path, cli.region);
        for _ in 0..cli.frames {
            system.run_frame();
        }
        if let Some(ref path) = cli.screenshot_path {
            if let Err(e) = save_screenshot(system.framebuffer(), FB_WIDTH, FB_HEIGHT, path) {
                eprintln!("Screenshot error: {e}");
                process::exit(1);
            }
            eprintln!("Screenshot saved to {}", path.display());
        }
        return;
    }

    let frame_duration = match cli.region {
        Sg1000Region::Ntsc => Duration::from_micros(16_639),
        Sg1000Region::Pal => Duration::from_micros(20_000),
    };

    let system = if let Some(ref path) = cli.rom_path {
        load_rom(path, cli.region)
    } else {
        Sg1000::new(stub_rom(), cli.region)
    };

    let region = cli.region;
    Runner::new(system, "Sega SG-1000", 3, frame_duration)
        .with_audio_enabled(!cli.mute)
        .with_key_handler(|machine, keycode, pressed| {
            let ctrl = machine.controller1_mut();
            match keycode {
                KeyCode::ArrowUp => ctrl.up = pressed,
                KeyCode::ArrowDown => ctrl.down = pressed,
                KeyCode::ArrowLeft => ctrl.left = pressed,
                KeyCode::ArrowRight => ctrl.right = pressed,
                KeyCode::KeyZ => ctrl.button1 = pressed,
                KeyCode::KeyX => ctrl.button2 = pressed,
                _ => {}
            }
        })
        .with_open_handler(&["sg", "bin", "rom"], move |path| {
            let rom_data = std::fs::read(path).ok()?;
            Some(Sg1000::new(rom_data, region))
        })
        .run();
}
