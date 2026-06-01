//! Oric-1 / Atmos headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_oric_atmos::{FB_HEIGHT, FB_WIDTH, OricAtmos, OricModel};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-oric-atmos --rom PATH [OPTIONS]

Required:
    --rom PATH                Oric BASIC + OS ROM (16 KB at $C000-$FFFF)

Options:
    --model MODEL             oric-1 | atmos [default: atmos]
    --frames N                native video frames to run [default: 300]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

ROM source (when --rom is omitted):
    1. EMU198X_ORIC_ATMOS_ROM env var
    2. ~/.emu198x/roms/oric-atmos/atmos.rom (atmos model)
    3. ~/.emu198x/roms/oric-atmos/oric1.rom (oric-1 model)

Examples:
    emu198x-oric-atmos --rom ~/.emu198x/roms/oric-atmos/atmos.rom \\
        --frames 300 --screenshot oric-boot.png
"
    );
}

struct Cli {
    rom: Option<PathBuf>,
    model: OricModel,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            model: OricModel::Atmos,
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
            "--rom" => cli.rom = Some(PathBuf::from(next_value()?)),
            "--model" => {
                cli.model = match next_value()?.as_str() {
                    "oric-1" | "oric1" => OricModel::Oric1,
                    "atmos" => OricModel::Atmos,
                    other => return Err(format!("--model expects oric-1 or atmos, got {other}")),
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

fn default_rom_path(model: OricModel) -> PathBuf {
    let home = env::var("HOME").unwrap_or_default();
    let name = match model {
        OricModel::Oric1 => "oric1.rom",
        OricModel::Atmos => "atmos.rom",
    };
    PathBuf::from(home).join(format!(".emu198x/roms/oric-atmos/{name}"))
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
    let rom_path = cli.rom.unwrap_or_else(|| default_rom_path(cli.model));
    let rom = fs::read(&rom_path)
        .map_err(|e| format!("failed to read ROM at {}: {e}", rom_path.display()))?;
    if rom.len() != 0x4000 {
        return Err(format!(
            "ROM at {} is {} bytes; expected 16384 (16 KB)",
            rom_path.display(),
            rom.len()
        ));
    }
    let mut sys = OricAtmos::new(rom, cli.model);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Oric runtime: cycles={} frames={} model={:?}",
        sys.cpu_cycles(),
        sys.frame_count(),
        cli.model
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
        assert!(cli.rom.is_none());
        assert!(matches!(cli.model, OricModel::Atmos));
        assert_eq!(cli.frames, 300);
    }

    #[test]
    fn parse_cli_accepts_model_oric_1() {
        let argv = vec!["--model".into(), "oric-1".into()];
        let cli = parse_cli(argv).expect("ok");
        assert!(matches!(cli.model, OricModel::Oric1));
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }
}
