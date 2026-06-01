//! BBC Micro Model B headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_acorn_bbc_micro::{BbcMicro, FB_HEIGHT, FB_WIDTH};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-acorn-bbc-micro --os PATH [OPTIONS]

Required:
    --os PATH                 16 KB MOS ROM ($C000-$FFFF)

Options:
    --rom BANK:PATH           install a 16 KB sideways ROM into BANK (0-15)
                              (repeat for multiple ROMs — e.g. BASIC in 15)
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

ROM source (when --os is omitted):
    1. EMU198X_BBC_OS env var
    2. ~/.emu198x/roms/acorn-bbc-micro/os.rom

Examples:
    emu198x-acorn-bbc-micro \\
        --os ~/.emu198x/roms/acorn-bbc-micro/os.rom \\
        --rom 15:~/.emu198x/roms/acorn-bbc-micro/basic.rom \\
        --frames 300 --screenshot bbc-boot.png
"
    );
}

struct Cli {
    os: Option<PathBuf>,
    roms: Vec<(usize, PathBuf)>,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            os: None,
            roms: Vec::new(),
            frames: 200,
            screenshot: None,
        }
    }
}

fn parse_rom_spec(spec: &str) -> Result<(usize, PathBuf), String> {
    let (bank, path) = spec
        .split_once(':')
        .ok_or_else(|| format!("--rom expects BANK:PATH, got {spec}"))?;
    let bank: usize = bank
        .parse()
        .map_err(|e| format!("--rom bank must be 0-15: {e}"))?;
    if bank > 15 {
        return Err(format!("--rom bank must be 0-15, got {bank}"));
    }
    Ok((bank, PathBuf::from(path)))
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
            "--rom" => cli.roms.push(parse_rom_spec(&next_value()?)?),
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

fn default_os_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/os.rom")
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
    let os_path = cli.os.unwrap_or_else(default_os_path);
    let os = fs::read(&os_path).map_err(|e| format!("failed to read OS at {}: {e}", os_path.display()))?;
    if os.len() != 0x4000 {
        return Err(format!(
            "OS at {} is {} bytes; expected 16384 (16 KB)",
            os_path.display(),
            os.len()
        ));
    }
    let mut sys = BbcMicro::new(os);
    for (bank, path) in cli.roms {
        let rom = fs::read(&path).map_err(|e| format!("failed to read ROM at {}: {e}", path.display()))?;
        if rom.len() != 0x4000 {
            return Err(format!(
                "ROM at {} is {} bytes; expected 16384 (16 KB)",
                path.display(),
                rom.len()
            ));
        }
        sys.insert_rom(bank, rom);
    }
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "BBC runtime: cycles={} frames={} bank={}",
        sys.cpu_cycles(),
        sys.frame_count(),
        sys.rom_bank()
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
        assert!(cli.os.is_none());
        assert!(cli.roms.is_empty());
        assert_eq!(cli.frames, 200);
    }

    #[test]
    fn parse_cli_accepts_rom_spec() {
        let argv = vec!["--rom".into(), "15:/tmp/basic.rom".into()];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.roms[0].0, 15);
        assert_eq!(cli.roms[0].1, Path::new("/tmp/basic.rom"));
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }

    #[test]
    fn parse_rom_spec_rejects_bad_format() {
        assert!(parse_rom_spec("/tmp/no-bank").is_err());
        assert!(parse_rom_spec("16:/tmp/over-range").is_err());
    }
}
