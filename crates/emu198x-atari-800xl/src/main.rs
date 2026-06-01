//! Atari 800XL headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-atari-800xl [OPTIONS]

Options:
    --os PATH                 16 KB OS ROM (atarixl.rom or atariosb.rom)
    --basic PATH              8 KB BASIC ROM (ataribas.rom)
    --cart PATH               8 KB or 16 KB cartridge ROM
    --no-basic                start with BASIC disabled (default: enabled)
    --region MODE             ntsc | pal [default: ntsc]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

Notes:
    With no --os, the reset vector is taken from the cartridge entry point
    (cart-only boot). Most real software needs the OS ROM.

Examples:
    emu198x-atari-800xl --os ~/.emu198x/roms/atari-800xl/atarixl.rom \\
        --basic ~/.emu198x/roms/atari-800xl/ataribas.rom \\
        --frames 300 --screenshot atari-800xl.png
"
    );
}

struct Cli {
    os: Option<PathBuf>,
    basic: Option<PathBuf>,
    cart: Option<PathBuf>,
    basic_enabled: bool,
    region: Atari800xlRegion,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            os: None,
            basic: None,
            cart: None,
            basic_enabled: true,
            region: Atari800xlRegion::Ntsc,
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
            "--os" => cli.os = Some(PathBuf::from(next_value()?)),
            "--basic" => cli.basic = Some(PathBuf::from(next_value()?)),
            "--cart" => cli.cart = Some(PathBuf::from(next_value()?)),
            "--no-basic" => cli.basic_enabled = false,
            "--region" => {
                cli.region = match next_value()?.as_str() {
                    "ntsc" => Atari800xlRegion::Ntsc,
                    "pal" => Atari800xlRegion::Pal,
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

fn read_optional(path: Option<&PathBuf>) -> Result<Option<Vec<u8>>, String> {
    match path {
        Some(p) => {
            fs::read(p).map(Some).map_err(|e| format!("failed to read {}: {e}", p.display()))
        }
        None => Ok(None),
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let os = read_optional(cli.os.as_ref())?;
    let basic = read_optional(cli.basic.as_ref())?;
    let cart = read_optional(cli.cart.as_ref())?;
    if os.is_none() && cart.is_none() {
        return Err(
            "either --os or --cart must be provided (cart-only boot uses the cart's reset vector)"
                .into(),
        );
    }
    let mut sys =
        Atari800xl::new(os, basic, cart, cli.region, cli.basic_enabled)?;
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Atari 800XL runtime: master_clock={} frames={} region={:?}",
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
        assert!(cli.os.is_none());
        assert!(cli.cart.is_none());
        assert!(cli.basic_enabled);
        assert_eq!(cli.frames, 200);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--os".into(),
            "/tmp/atarixl.rom".into(),
            "--basic".into(),
            "/tmp/ataribas.rom".into(),
            "--cart".into(),
            "/tmp/cart.bin".into(),
            "--no-basic".into(),
            "--region".into(),
            "pal".into(),
            "--frames".into(),
            "60".into(),
            "--screenshot".into(),
            "/tmp/shot.png".into(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.os.unwrap(), Path::new("/tmp/atarixl.rom"));
        assert_eq!(cli.basic.unwrap(), Path::new("/tmp/ataribas.rom"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/cart.bin"));
        assert!(!cli.basic_enabled);
        assert!(matches!(cli.region, Atari800xlRegion::Pal));
        assert_eq!(cli.frames, 60);
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }
}
