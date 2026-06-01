//! ColecoVision headless runner — minimal first-port binary.
//!
//! Commit 4 of 4 unlocking ColecoVision. Headless-only CLI: load a BIOS
//! (required) and optional cartridge ROM, run N frames, optionally
//! write a PNG screenshot of the last emitted frame. Mirrors the
//! `--headless` mode of `emu198x-nes`/`emu198x-c64`/`emu198x-amiga`;
//! full shell parity (native verifier window with `wgpu` raw/lcd/crt,
//! keyboard, audio, scripts, snapshots, smoke matrix) is scoped as a
//! follow-up commit once the headless path has proven the machine
//! actually boots a real BIOS.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_coleco_colecovision::{ColecoVision, CvRegion};

const FB_WIDTH: u32 = 256;
const FB_HEIGHT: u32 = 192;

fn usage() {
    eprintln!(
        "\
Usage: emu198x-colecovision --bios PATH [OPTIONS]

Required:
    --bios PATH               ColecoVision BIOS ROM (8 KB)

Options:
    --cart PATH               cartridge ROM (up to 32 KB)
    --region MODE             ntsc | pal [default: ntsc]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

ROM directory convention (when --bios is omitted):
    ~/.emu198x/roms/coleco-colecovision/colecovision.rom

Examples:
    emu198x-colecovision --bios ~/.emu198x/roms/coleco-colecovision/colecovision.rom \\
        --frames 200 --screenshot coleco-boot.png

    emu198x-colecovision --bios colecovision.rom --cart game.col --frames 600
"
    );
}

struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    region: CvRegion,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            region: CvRegion::Ntsc,
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
            "--bios" => cli.bios = Some(PathBuf::from(next_value()?)),
            "--cart" => cli.cart = Some(PathBuf::from(next_value()?)),
            "--region" => {
                cli.region = match next_value()?.as_str() {
                    "ntsc" => CvRegion::Ntsc,
                    "pal" => CvRegion::Pal,
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

fn default_bios_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".emu198x/roms/coleco-colecovision/colecovision.rom")
}

fn load_rom(label: &str, path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("failed to read {label} at {}: {e}", path.display()))
}

fn write_screenshot(path: &Path, framebuffer: &[u32]) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &px in framebuffer {
        // VDP framebuffer is ARGB32 little-endian; PNG wants RGBA.
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
    let bios_path = cli.bios.unwrap_or_else(default_bios_path);
    let bios = load_rom("BIOS", &bios_path)?;
    if bios.len() != 8192 {
        return Err(format!(
            "BIOS at {} is {} bytes; expected 8192 (8 KB)",
            bios_path.display(),
            bios.len()
        ));
    }
    let cart = match cli.cart.as_deref() {
        Some(path) => load_rom("cartridge", path)?,
        None => Vec::new(),
    };
    let mut cv = ColecoVision::new(bios, cart, cli.region);
    for _ in 0..cli.frames {
        cv.run_frame();
    }
    println!(
        "ColecoVision runtime: cycles={} frames={}",
        cv.cpu_cycles(),
        cv.frame_count()
    );
    if let Some(path) = cli.screenshot.as_deref() {
        write_screenshot(path, cv.framebuffer())?;
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
        assert!(cli.bios.is_none());
        assert!(cli.cart.is_none());
        assert!(matches!(cli.region, CvRegion::Ntsc));
        assert_eq!(cli.frames, 200);
        assert!(cli.screenshot.is_none());
    }

    #[test]
    fn parse_cli_accepts_bios_cart_frames_screenshot() {
        let argv = vec![
            "--bios".to_string(),
            "/tmp/bios.rom".to_string(),
            "--cart".to_string(),
            "/tmp/game.col".to_string(),
            "--frames".to_string(),
            "60".to_string(),
            "--screenshot".to_string(),
            "/tmp/shot.png".to_string(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/bios.rom"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/game.col"));
        assert_eq!(cli.frames, 60);
        assert_eq!(cli.screenshot.unwrap(), Path::new("/tmp/shot.png"));
    }

    #[test]
    fn parse_cli_region_pal() {
        let argv = vec!["--region".to_string(), "pal".to_string()];
        let cli = parse_cli(argv).expect("ok");
        assert!(matches!(cli.region, CvRegion::Pal));
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        let argv = vec!["--frobozz".to_string()];
        assert!(parse_cli(argv).is_err());
    }
}
