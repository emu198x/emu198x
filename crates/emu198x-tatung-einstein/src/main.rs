//! Tatung Einstein TC-01 headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_tatung_einstein::{Einstein, EinsteinRegion};

const FB_WIDTH: u32 = 256;
const FB_HEIGHT: u32 = 192;

fn usage() {
    eprintln!(
        "\
Usage: emu198x-tatung-einstein --bios PATH [OPTIONS]

Required:
    --bios PATH               X-TAL MOS ROM (8 KB)

Options:
    --region MODE             ntsc | pal [default: pal]
    --frames N                native video frames to run [default: 300]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

BIOS source (when --bios is omitted):
    1. EMU198X_EINSTEIN_BIOS env var
    2. ~/.emu198x/roms/tatung-einstein/einstein.rom

Examples:
    emu198x-tatung-einstein \\
        --bios ~/.emu198x/roms/tatung-einstein/einstein.rom \\
        --frames 300 --screenshot einstein-boot.png
"
    );
}

struct Cli {
    bios: Option<PathBuf>,
    region: EinsteinRegion,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            // Einstein is a UK machine — default to PAL.
            region: EinsteinRegion::Pal,
            frames: 300,
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
            "--region" => {
                cli.region = match next_value()?.as_str() {
                    "ntsc" => EinsteinRegion::Ntsc,
                    "pal" => EinsteinRegion::Pal,
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
    PathBuf::from(home).join(".emu198x/roms/tatung-einstein/einstein.rom")
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
    if bios.len() != 0x2000 {
        return Err(format!(
            "BIOS at {} is {} bytes; expected 8192 (8 KB)",
            bios_path.display(),
            bios.len()
        ));
    }
    let mut sys = Einstein::new(bios, cli.region);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Einstein runtime: tstates={} frames={}",
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
        let cli = parse_cli(Vec::<String>::new()).expect("ok");
        assert!(cli.bios.is_none());
        assert!(matches!(cli.region, EinsteinRegion::Pal));
        assert_eq!(cli.frames, 300);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".into(),
            "/tmp/einstein.rom".into(),
            "--region".into(),
            "ntsc".into(),
            "--frames".into(),
            "60".into(),
            "--screenshot".into(),
            "/tmp/shot.png".into(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/einstein.rom"));
        assert!(matches!(cli.region, EinsteinRegion::Ntsc));
        assert_eq!(cli.frames, 60);
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }
}
