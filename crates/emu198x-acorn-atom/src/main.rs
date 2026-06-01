//! Acorn Atom headless runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_acorn_atom::{AcornAtom, FB_HEIGHT, FB_WIDTH};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-acorn-atom --rom PATH [OPTIONS]

Required:
    --rom PATH                24 KB combined ROM
                              (BASIC1 + FP + BASIC2 + OS)

Options:
    --ram-bytes N             RAM size (2560-12288) [default: 2560]
    --frames N                native frames to run [default: 200]
    --screenshot PATH         write the last frame as PNG
    --help, -h                show this help
"
    );
}

struct Cli {
    rom: Option<PathBuf>,
    ram_bytes: usize,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            ram_bytes: 2560,
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
            "--rom" => cli.rom = Some(PathBuf::from(next_value()?)),
            "--ram-bytes" => {
                cli.ram_bytes = next_value()?
                    .parse()
                    .map_err(|e| format!("--ram-bytes expects a positive integer: {e}"))?;
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

fn run(cli: Cli) -> Result<(), String> {
    let rom_path = cli.rom.ok_or_else(|| "--rom PATH is required".to_string())?;
    let rom = fs::read(&rom_path)
        .map_err(|e| format!("failed to read ROM at {}: {e}", rom_path.display()))?;
    let mut sys = AcornAtom::new(rom, cli.ram_bytes);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Atom runtime: master_clock={} frames={} ram_bytes={}",
        sys.master_clock(),
        sys.frame_count(),
        cli.ram_bytes
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
        assert_eq!(cli.ram_bytes, 2560);
    }
}
