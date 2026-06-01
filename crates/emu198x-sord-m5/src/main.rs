//! Sord M5 headless runner — minimal first-port binary.
//!
//! Commit 2 of 2 unlocking the Sord M5. Headless-only CLI: load the
//! 8 KB Monitor / BASIC-I ROM (required) and optional cartridge,
//! run N frames, optionally write a PNG screenshot. Mirrors the
//! other emu198x-* TMS9918-family binaries.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_sord_m5::{M5Region, SordM5};

const FB_WIDTH: u32 = 256;
const FB_HEIGHT: u32 = 192;

fn usage() {
    eprintln!(
        "\
Usage: emu198x-sord-m5 --bios PATH [OPTIONS]

Required:
    --bios PATH               Sord M5 Monitor / BASIC-I ROM (8 KB)

Options:
    --cart PATH               cartridge ROM (up to 20 KB)
    --cart-ram-kb N           allocate N KB of cart RAM at $8000 [default: 0]
    --region MODE             ntsc | pal [default: ntsc]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

BIOS source (when --bios is omitted):
    1. EMU198X_SORD_M5_BIOS env var
    2. ~/.emu198x/roms/sord-m5/sord-m5.rom

Examples:
    emu198x-sord-m5 --bios ~/.emu198x/roms/sord-m5/sord-m5.rom \\
        --frames 300 --screenshot sord-boot.png

    emu198x-sord-m5 --bios sord-m5.rom --cart basic-g.bin \\
        --cart-ram-kb 16 --frames 600
"
    );
}

struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    cart_ram_kb: usize,
    region: M5Region,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            cart_ram_kb: 0,
            region: M5Region::Ntsc,
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
            "--cart-ram-kb" => {
                cli.cart_ram_kb = next_value()?
                    .parse()
                    .map_err(|e| format!("--cart-ram-kb expects a non-negative integer: {e}"))?;
            }
            "--region" => {
                cli.region = match next_value()?.as_str() {
                    "ntsc" => M5Region::Ntsc,
                    "pal" => M5Region::Pal,
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
    PathBuf::from(home).join(".emu198x/roms/sord-m5/sord-m5.rom")
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
    let bios_path = cli.bios.unwrap_or_else(default_bios_path);
    let bios = fs::read(&bios_path)
        .map_err(|e| format!("failed to read BIOS at {}: {e}", bios_path.display()))?;
    if bios.len() != 8192 {
        return Err(format!(
            "BIOS at {} is {} bytes; expected 8192 (8 KB)",
            bios_path.display(),
            bios.len()
        ));
    }
    let cart = match cli.cart.as_deref() {
        Some(path) => {
            let data = fs::read(path)
                .map_err(|e| format!("failed to read cart at {}: {e}", path.display()))?;
            if data.len() > 0x5000 {
                return Err(format!(
                    "cart at {} is {} bytes; M5 cart ceiling is 20 KB",
                    path.display(),
                    data.len()
                ));
            }
            data
        }
        None => Vec::new(),
    };
    let mut sys = SordM5::new(bios, cart, cli.region);
    if cli.cart_ram_kb > 0 {
        sys.set_cart_ram_size(cli.cart_ram_kb * 1024);
    }
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Sord M5 runtime: tstates={} frames={}",
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
        assert!(cli.bios.is_none());
        assert!(cli.cart.is_none());
        assert_eq!(cli.cart_ram_kb, 0);
        assert!(matches!(cli.region, M5Region::Ntsc));
        assert_eq!(cli.frames, 200);
        assert!(cli.screenshot.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".to_string(),
            "/tmp/m5.rom".to_string(),
            "--cart".to_string(),
            "/tmp/basic-g.bin".to_string(),
            "--cart-ram-kb".to_string(),
            "16".to_string(),
            "--region".to_string(),
            "pal".to_string(),
            "--frames".to_string(),
            "60".to_string(),
            "--screenshot".to_string(),
            "/tmp/shot.png".to_string(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/m5.rom"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/basic-g.bin"));
        assert_eq!(cli.cart_ram_kb, 16);
        assert!(matches!(cli.region, M5Region::Pal));
        assert_eq!(cli.frames, 60);
        assert_eq!(cli.screenshot.unwrap(), Path::new("/tmp/shot.png"));
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        let argv = vec!["--frobozz".to_string()];
        assert!(parse_cli(argv).is_err());
    }
}
