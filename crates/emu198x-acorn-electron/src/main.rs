//! Acorn Electron headless runner — minimal first-port binary.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use machine_acorn_electron::{AcornElectron, FB_HEIGHT, FB_WIDTH};

fn usage() {
    eprintln!(
        "\
Usage: emu198x-acorn-electron --os PATH --basic PATH [OPTIONS]

Required:
    --os PATH                 OS ROM (16 KB at $C000-$FFFF)
    --basic PATH              BASIC ROM (16 KB at $8000-$BFFF)

Options:
    --frames N                native video frames to run [default: 200]
    --screenshot PATH         write the last emitted frame as PNG
    --help, -h                show this help

ROM source (when --os / --basic are omitted):
    1. EMU198X_ELECTRON_OS / EMU198X_ELECTRON_BASIC env vars
    2. ~/.emu198x/roms/acorn-electron/os.rom + basic.rom

Examples:
    emu198x-acorn-electron \\
        --os ~/.emu198x/roms/acorn-electron/os.rom \\
        --basic ~/.emu198x/roms/acorn-electron/basic.rom \\
        --frames 300 --screenshot electron-boot.png
"
    );
}

struct Cli {
    os: Option<PathBuf>,
    basic: Option<PathBuf>,
    frames: u32,
    screenshot: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            os: None,
            basic: None,
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

fn default_rom_path(name: &str) -> PathBuf {
    let home = env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(format!(".emu198x/roms/acorn-electron/{name}"))
}

fn resolve_path(arg: Option<PathBuf>, env_key: &str, default_name: &str) -> PathBuf {
    if let Some(p) = arg {
        return p;
    }
    if let Ok(p) = env::var(env_key) {
        return PathBuf::from(p);
    }
    default_rom_path(default_name)
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

fn read_rom(label: &str, path: &Path) -> Result<Vec<u8>, String> {
    let data = fs::read(path)
        .map_err(|e| format!("failed to read {label} at {}: {e}", path.display()))?;
    if data.len() != 0x4000 {
        return Err(format!(
            "{label} at {} is {} bytes; expected 16384 (16 KB)",
            path.display(),
            data.len()
        ));
    }
    Ok(data)
}

fn run(cli: Cli) -> Result<(), String> {
    let os_path = resolve_path(cli.os, "EMU198X_ELECTRON_OS", "os.rom");
    let basic_path = resolve_path(cli.basic, "EMU198X_ELECTRON_BASIC", "basic.rom");
    let os = read_rom("OS ROM", &os_path)?;
    let basic = read_rom("BASIC ROM", &basic_path)?;
    let mut sys = AcornElectron::new(os, basic);
    for _ in 0..cli.frames {
        sys.run_frame();
    }
    println!(
        "Electron runtime: cycles={} frames={} mode={}",
        sys.cpu_cycles(),
        sys.frame_count(),
        sys.display_mode()
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
        assert!(cli.basic.is_none());
        assert_eq!(cli.frames, 200);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--os".into(),
            "/tmp/os.rom".into(),
            "--basic".into(),
            "/tmp/basic.rom".into(),
            "--frames".into(),
            "60".into(),
            "--screenshot".into(),
            "/tmp/shot.png".into(),
        ];
        let cli = parse_cli(argv).expect("ok");
        assert_eq!(cli.os.unwrap(), Path::new("/tmp/os.rom"));
        assert_eq!(cli.basic.unwrap(), Path::new("/tmp/basic.rom"));
        assert_eq!(cli.frames, 60);
    }

    #[test]
    fn parse_cli_rejects_unknown_arg() {
        assert!(parse_cli(vec!["--frobozz".into()]).is_err());
    }
}
