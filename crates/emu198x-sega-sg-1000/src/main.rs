//! Sega SG-1000 / SC-3000 headless runner — minimal first-port binary.
//!
//! Commit 2 of 2 unlocking the SG-1000. Headless-only CLI: load a
//! cartridge (no BIOS — SG-1000 boots straight into cart code), run
//! N frames, optionally write a PNG screenshot of the last emitted
//! frame. Mirrors `emu198x-colecovision`; full shell parity is a
//! follow-up.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_sega_sg_1000::{Sg1000, Sg1000Region};

const FB_WIDTH: u32 = 256;
const FB_HEIGHT: u32 = 192;

fn usage() {
    eprintln!(
        "\
Usage: emu198x-sega-sg-1000 --cart PATH [OPTIONS]

Required:
    --cart PATH               cartridge ROM (up to 48 KB)

Options:
    --region MODE             ntsc | pal [default: ntsc]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

Examples:
    emu198x-sega-sg-1000 --cart game.sg --frames 200 --screenshot sg.png
    emu198x-sega-sg-1000 --cart game.sg --region pal --frames 600
"
    );
}

struct Cli {
    cart: Option<PathBuf>,
    region: Sg1000Region,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            region: Sg1000Region::Ntsc,
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
            "--cart" => cli.cart = Some(PathBuf::from(next_value()?)),
            "--region" => {
                cli.region = match next_value()?.as_str() {
                    "ntsc" => Sg1000Region::Ntsc,
                    "pal" => Sg1000Region::Pal,
                    other => return Err(format!("--region expects ntsc or pal, got {other}")),
                };
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
    let cart_path = cli
        .cart
        .ok_or_else(|| "--cart PATH is required".to_string())?;
    let cart = fs::read(&cart_path)
        .map_err(|e| format!("failed to read cart at {}: {e}", cart_path.display()))?;
    if cart.is_empty() {
        return Err(format!("cart at {} is empty", cart_path.display()));
    }
    if cart.len() > 0xC000 {
        return Err(format!(
            "cart at {} is {} bytes; SG-1000 ceiling is 48 KB",
            cart_path.display(),
            cart.len()
        ));
    }
    let mut sys = Sg1000::new(cart, cli.region);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "SG-1000 runtime: tstates={} frames={}",
        sys.cpu_tstates(),
        sys.frame_count()
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
        let cli = parse_cli(Vec::<String>::new()).expect("empty args ok");
        assert!(cli.cart.is_none());
        assert!(matches!(cli.region, Sg1000Region::Ntsc));
        assert_eq!(cli.frames, 200);
        assert!(cli.screenshot.is_none());
    }

    #[test]
    fn parse_cli_accepts_cart_frames_screenshot() {
        let argv = vec![
            "--cart".to_string(),
            "/tmp/game.sg".to_string(),
            "--frames".to_string(),
            "60".to_string(),
            "--screenshot".to_string(),
            "/tmp/shot.png".to_string(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/game.sg"));
        assert_eq!(cli.frames, 60);
        assert_eq!(cli.screenshot.unwrap(), Path::new("/tmp/shot.png"));
    }

    #[test]
    fn parse_cli_region_pal() {
        let argv = vec!["--region".to_string(), "pal".to_string()];
        let cli = parse_cli(argv).expect("ok");
        assert!(matches!(cli.region, Sg1000Region::Pal));
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        let argv = vec!["--frobozz".to_string()];
        assert!(parse_cli(argv).is_err());
    }
}
