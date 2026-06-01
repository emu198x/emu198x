//! MSX1 headless runner — minimal first-port binary.
//!
//! Commit 3 of 3 unlocking MSX1. Headless-only CLI: load a 32 KB
//! BIOS ROM (required) and an optional cartridge with mapper
//! selection, run N frames, optionally write a PNG screenshot.
//! Mirrors `emu198x-colecovision` / `emu198x-sega-sg-1000`. Full
//! shell parity is a follow-up.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_msx::{MapperType, Msx, MsxRegion};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-msx --bios PATH [OPTIONS]

Required:
    --bios PATH               MSX BIOS ROM (32 KB)

Options:
    --cart PATH               cartridge ROM (slot 1)
    --mapper KIND             plain | konami | konami-scc | ascii8 | ascii16
                              [default: plain]
    --region MODE             ntsc | pal [default: ntsc]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

BIOS source (when --bios is omitted):
    1. EMU198X_MSX_BIOS env var
    2. ~/.emu198x/roms/microsoft-msx/msx.rom

Examples:
    emu198x-msx --bios ~/.emu198x/roms/microsoft-msx/msx.rom \\
        --frames 200 --screenshot msx-boot.png

    emu198x-msx --bios msx.rom --cart game.rom --mapper konami \\
        --frames 600
"
    );
}

struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    mapper: MapperType,
    region: MsxRegion,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            mapper: MapperType::Plain,
            region: MsxRegion::Ntsc,
            frames: 200,
            screenshot: None,
        }
    }
}

fn parse_mapper(value: &str) -> Result<MapperType, String> {
    match value {
        "plain" => Ok(MapperType::Plain),
        "konami" => Ok(MapperType::Konami),
        "konami-scc" => Ok(MapperType::KonamiScc),
        "ascii8" => Ok(MapperType::Ascii8),
        "ascii16" => Ok(MapperType::Ascii16),
        other => Err(format!(
            "--mapper expects plain|konami|konami-scc|ascii8|ascii16, got {other}"
        )),
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
            "--mapper" => cli.mapper = parse_mapper(&next_value()?)?,
            "--region" => {
                cli.region = match next_value()?.as_str() {
                    "ntsc" => MsxRegion::Ntsc,
                    "pal" => MsxRegion::Pal,
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
    PathBuf::from(home).join(".emu198x/roms/microsoft-msx/msx.rom")
}

fn write_screenshot(
    path: &Path,
    framebuffer: &[u32],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &px in framebuffer {
        rgba.push(((px >> 16) & 0xFF) as u8);
        rgba.push(((px >> 8) & 0xFF) as u8);
        rgba.push((px & 0xFF) as u8);
        rgba.push(0xFF);
    }
    let file = fs::File::create(path)
        .map_err(|e| format!("failed to create screenshot {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(file, width, height);
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
    if bios.len() != 32768 {
        return Err(format!(
            "BIOS at {} is {} bytes; expected 32768 (32 KB)",
            bios_path.display(),
            bios.len()
        ));
    }
    let mut sys = Msx::new(bios, cli.region);
    if let Some(cart_path) = cli.cart.as_deref() {
        let cart = fs::read(cart_path)
            .map_err(|e| format!("failed to read cart at {}: {e}", cart_path.display()))?;
        sys.insert_cart1(cart, cli.mapper);
    }
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "MSX runtime: tstates={} frames={}",
        sys.cpu_tstates(),
        sys.frame_count()
    );
    if let Some(path) = cli.screenshot.as_deref() {
        write_screenshot(
            path,
            sys.framebuffer(),
            sys.framebuffer_width(),
            sys.framebuffer_height(),
        )?;
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
        assert!(matches!(cli.mapper, MapperType::Plain));
        assert!(matches!(cli.region, MsxRegion::Ntsc));
        assert_eq!(cli.frames, 200);
        assert!(cli.screenshot.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".to_string(),
            "/tmp/msx.rom".to_string(),
            "--cart".to_string(),
            "/tmp/game.rom".to_string(),
            "--mapper".to_string(),
            "konami-scc".to_string(),
            "--region".to_string(),
            "pal".to_string(),
            "--frames".to_string(),
            "60".to_string(),
            "--screenshot".to_string(),
            "/tmp/shot.png".to_string(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/msx.rom"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/game.rom"));
        assert!(matches!(cli.mapper, MapperType::KonamiScc));
        assert!(matches!(cli.region, MsxRegion::Pal));
        assert_eq!(cli.frames, 60);
        assert_eq!(cli.screenshot.unwrap(), Path::new("/tmp/shot.png"));
    }

    #[test]
    fn parse_cli_rejects_unknown_mapper() {
        let argv = vec!["--mapper".to_string(), "frobozz".to_string()];
        assert!(parse_cli(argv).is_err());
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        let argv = vec!["--frobozz".to_string()];
        assert!(parse_cli(argv).is_err());
    }
}
