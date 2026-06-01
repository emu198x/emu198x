//! Atari 7800 ProSystem headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_atari_7800::{Atari7800, Atari7800Region};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-atari-7800 --cart PATH [OPTIONS]

Required:
    --cart PATH               cartridge ROM (.a78 / .bin — 16 KB / 32 KB /
                              48 KB flat or 64-128 KB SuperGame; A78 header
                              auto-stripped)

Options:
    --region MODE             ntsc | pal [default: ntsc]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

Examples:
    emu198x-atari-7800 --cart asteroids.a78 --frames 300 --screenshot asteroids.png
"
    );
}

struct Cli {
    cart: Option<PathBuf>,
    region: Atari7800Region,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            region: Atari7800Region::Ntsc,
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
                    "ntsc" => Atari7800Region::Ntsc,
                    "pal" => Atari7800Region::Pal,
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
    let cart_path = cli
        .cart
        .ok_or_else(|| "--cart PATH is required".to_string())?;
    let cart = fs::read(&cart_path)
        .map_err(|e| format!("failed to read cart at {}: {e}", cart_path.display()))?;
    let mut sys = Atari7800::new(cart, cli.region)?;
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Atari 7800 runtime: master_clock={} frames={} region={:?}",
        sys.master_clock(),
        sys.frame_count(),
        cli.region
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
        let cli = parse_cli(Vec::<String>::new()).expect("ok");
        assert!(cli.cart.is_none());
        assert!(matches!(cli.region, Atari7800Region::Ntsc));
        assert_eq!(cli.frames, 200);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--cart".into(),
            "/tmp/asteroids.a78".into(),
            "--region".into(),
            "pal".into(),
            "--frames".into(),
            "60".into(),
            "--screenshot".into(),
            "/tmp/shot.png".into(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/asteroids.a78"));
        assert!(matches!(cli.region, Atari7800Region::Pal));
        assert_eq!(cli.frames, 60);
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }
}
