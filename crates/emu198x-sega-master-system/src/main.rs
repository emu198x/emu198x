//! Sega Master System / Game Gear headless runner — minimal first-port
//! binary.
//!
//! Commit 3 of 3 unlocking SMS. Headless-only CLI: load a cart (no BIOS
//! needed for most SMS carts — they boot directly from `$0000`), pick
//! variant (SMS NTSC / SMS PAL / Game Gear), run N frames, optionally
//! write a PNG screenshot. Mirrors the other `emu198x-*` TMS9918-family
//! and Coleco-family binaries.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_sega_master_system::{Sms, SmsVariant};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-sega-master-system --cart PATH [OPTIONS]

Required:
    --cart PATH               cartridge ROM (.sms / .gg / .bin)

Options:
    --variant KIND            sms | sms-pal | game-gear [default: sms]
    --frames N                native video frames to run [default: 300]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

Examples:
    emu198x-sega-master-system --cart alex-kidd.sms --frames 600 \\
        --screenshot alex-boot.png

    emu198x-sega-master-system --cart sonic.gg --variant game-gear \\
        --frames 600 --screenshot sonic.png
"
    );
}

struct Cli {
    cart: Option<PathBuf>,
    variant: SmsVariant,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            variant: SmsVariant::SmsNtsc,
            frames: 300,
            screenshot: None,
        }
    }
}

fn parse_variant(value: &str) -> Result<SmsVariant, String> {
    match value {
        "sms" | "sms-ntsc" => Ok(SmsVariant::SmsNtsc),
        "sms-pal" => Ok(SmsVariant::SmsPal),
        "game-gear" | "gg" => Ok(SmsVariant::GameGear),
        other => Err(format!(
            "--variant expects sms|sms-pal|game-gear, got {other}"
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
            "--cart" => cli.cart = Some(PathBuf::from(next_value()?)),
            "--variant" => cli.variant = parse_variant(&next_value()?)?,
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

fn framebuffer_dimensions(_variant: SmsVariant, sys: &Sms) -> (u32, u32) {
    // Read dimensions from the live machine — the VDP picks the
    // active height (192 / 224 / 240) and adds the canonical
    // TV-visible border around it (see sega-vdp BORDER_* constants).
    (sys.framebuffer_width(), sys.framebuffer_height())
}

fn write_screenshot(path: &Path, framebuffer: &[u32], width: u32, height: u32) -> Result<(), String> {
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
    if cart.is_empty() {
        return Err(format!("cart at {} is empty", cart_path.display()));
    }
    // SMS carts can start with a 512-byte SMD header in some dumps.
    // TOSEC .sms / .bin dumps are raw; if the cart length is exactly
    // 512 bytes more than a power-of-two multiple of 16 KB, strip it.
    let cart = if cart.len() % 0x4000 == 0x200 {
        cart[0x200..].to_vec()
    } else {
        cart
    };
    let mut sys = Sms::new(cart, cli.variant);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    let (w, h) = framebuffer_dimensions(cli.variant, &sys);
    println!(
        "SMS runtime: tstates={} frames={} variant={:?}",
        sys.cpu_tstates(),
        sys.frame_count(),
        cli.variant
    );
    if let Some(path) = cli.screenshot.as_deref() {
        write_screenshot(path, sys.framebuffer(), w, h)?;
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
        assert!(matches!(cli.variant, SmsVariant::SmsNtsc));
        assert_eq!(cli.frames, 300);
        assert!(cli.screenshot.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--cart".to_string(),
            "/tmp/sonic.gg".to_string(),
            "--variant".to_string(),
            "game-gear".to_string(),
            "--frames".to_string(),
            "60".to_string(),
            "--screenshot".to_string(),
            "/tmp/shot.png".to_string(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/sonic.gg"));
        assert!(matches!(cli.variant, SmsVariant::GameGear));
        assert_eq!(cli.frames, 60);
        assert_eq!(cli.screenshot.unwrap(), Path::new("/tmp/shot.png"));
    }

    #[test]
    fn parse_cli_variant_aliases() {
        assert!(matches!(
            parse_cli(vec!["--variant".into(), "sms-pal".into()]).unwrap().variant,
            SmsVariant::SmsPal
        ));
        assert!(matches!(
            parse_cli(vec!["--variant".into(), "gg".into()]).unwrap().variant,
            SmsVariant::GameGear
        ));
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        let argv = vec!["--frobozz".to_string()];
        assert!(parse_cli(argv).is_err());
    }
}
