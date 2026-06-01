//! Commodore VIC-20 headless runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_commodore_vic_20::{Vic20, Vic20Model, FB_HEIGHT, FB_WIDTH};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-commodore-vic-20 --kernal PATH --basic PATH --char PATH [OPTIONS]

Required:
    --kernal PATH             8 KB Kernal ROM
    --basic PATH              8 KB BASIC ROM
    --char PATH               4 KB character ROM

Options:
    --model NAME              pal | ntsc [default: pal]
    --ram-kb N                expansion RAM in KB (0 / 3 / 8 / 16 / 24) [default: 0]
    --frames N                native frames to run [default: 200]
    --screenshot PATH         write the last frame as PNG
    --help, -h                show this help
"
    );
}

struct Cli {
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    char_rom: Option<PathBuf>,
    model: Vic20Model,
    ram_kb: usize,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            kernal: None,
            basic: None,
            char_rom: None,
            model: Vic20Model::Pal,
            ram_kb: 0,
            frames: 200,
            screenshot: None,
        }
    }
}

fn parse_cli<I: IntoIterator<Item = String>>(args: I) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let mut next_value = || iter.next().ok_or_else(|| format!("{arg} expects a value"));
        match arg.as_str() {
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            "--kernal" => cli.kernal = Some(PathBuf::from(next_value()?)),
            "--basic" => cli.basic = Some(PathBuf::from(next_value()?)),
            "--char" => cli.char_rom = Some(PathBuf::from(next_value()?)),
            "--model" => {
                cli.model = match next_value()?.as_str() {
                    "pal" => Vic20Model::Pal,
                    "ntsc" => Vic20Model::Ntsc,
                    other => return Err(format!("--model expects pal or ntsc, got {other}")),
                };
            }
            "--ram-kb" => {
                cli.ram_kb = next_value()?
                    .parse()
                    .map_err(|e| format!("--ram-kb expects a non-negative integer: {e}"))?;
            }
            "--frames" => {
                cli.frames = next_value()?
                    .parse()
                    .map_err(|e| format!("--frames expects a positive integer: {e}"))?;
            }
            "--screenshot" => cli.screenshot = Some(PathBuf::from(next_value()?)),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(cli)
}

fn write_screenshot(path: &Path, framebuffer: &[u32]) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &px in framebuffer {
        rgba.push(((px >> 16) & 0xFF) as u8);
        rgba.push(((px >> 8) & 0xFF) as u8);
        rgba.push((px & 0xFF) as u8);
        rgba.push(0xFF);
    }
    let file = fs::File::create(path)
        .map_err(|e| format!("failed to create screenshot {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(file, FB_WIDTH, FB_HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("PNG header write failed: {e}"))?;
    writer
        .write_image_data(&rgba)
        .map_err(|e| format!("PNG body write failed: {e}"))?;
    Ok(())
}

fn read_required(path: &Option<PathBuf>, flag: &str) -> Result<Vec<u8>, String> {
    let p = path.as_ref().ok_or_else(|| format!("{flag} is required"))?;
    fs::read(p).map_err(|e| format!("failed to read {}: {e}", p.display()))
}

fn run(cli: Cli) -> Result<(), String> {
    let kernal = read_required(&cli.kernal, "--kernal")?;
    let basic = read_required(&cli.basic, "--basic")?;
    let char_rom = read_required(&cli.char_rom, "--char")?;
    let mut sys = Vic20::new(kernal, basic, char_rom, cli.model, cli.ram_kb);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "VIC-20 runtime: master_clock={} frames={} model={:?} ram_kb={}",
        sys.master_clock(),
        sys.frame_count(),
        cli.model,
        cli.ram_kb
    );
    if let Some(path) = cli.screenshot.as_deref() {
        write_screenshot(path, sys.framebuffer())?;
        println!("Screenshot written: {}", path.display());
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cli = match parse_cli(args) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("error: {e}");
            usage();
            return ExitCode::from(2);
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(Vec::<String>::new()).expect("ok");
        assert!(cli.kernal.is_none());
        assert_eq!(cli.ram_kb, 0);
        assert!(matches!(cli.model, Vic20Model::Pal));
    }
}
