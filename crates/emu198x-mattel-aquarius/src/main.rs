//! Mattel Aquarius headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_mattel_aquarius::{Aquarius, FB_HEIGHT, FB_WIDTH};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-mattel-aquarius --bios PATH [OPTIONS]

Required:
    --bios PATH               Aquarius BASIC ROM (8 KB)

Options:
    --cart PATH               cartridge ROM (up to 8 KB at $E000-$FFFF)
    --expansion-kb N          RAM expansion in KB at $4000-$7FFF [default: 0]
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

BIOS source (when --bios is omitted):
    1. EMU198X_AQUARIUS_BIOS env var
    2. ~/.emu198x/roms/mattel-aquarius/aquarius.rom

Examples:
    emu198x-mattel-aquarius \\
        --bios ~/.emu198x/roms/mattel-aquarius/aquarius.rom \\
        --frames 300 --screenshot aquarius-boot.png
"
    );
}

struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    expansion_kb: usize,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            expansion_kb: 0,
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
            "--expansion-kb" => {
                cli.expansion_kb = next_value()?
                    .parse()
                    .map_err(|e| format!("--expansion-kb expects an integer: {e}"))?;
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
    PathBuf::from(home).join(".emu198x/roms/mattel-aquarius/aquarius.rom")
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
    let mut sys = Aquarius::new(bios, cli.expansion_kb);
    if let Some(cart_path) = cli.cart.as_deref() {
        let cart = fs::read(cart_path)
            .map_err(|e| format!("failed to read cart at {}: {e}", cart_path.display()))?;
        if cart.len() > 0x2000 {
            return Err(format!(
                "cart at {} is {} bytes; Aquarius cart ceiling is 8 KB",
                cart_path.display(),
                cart.len()
            ));
        }
        sys.insert_cart(cart);
    }
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Aquarius runtime: tstates={} frames={}",
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
        assert_eq!(cli.expansion_kb, 0);
        assert_eq!(cli.frames, 200);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".into(),
            "/tmp/aquarius.rom".into(),
            "--cart".into(),
            "/tmp/cart.bin".into(),
            "--expansion-kb".into(),
            "16".into(),
            "--frames".into(),
            "60".into(),
            "--screenshot".into(),
            "/tmp/shot.png".into(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/aquarius.rom"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/cart.bin"));
        assert_eq!(cli.expansion_kb, 16);
        assert_eq!(cli.frames, 60);
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }
}
