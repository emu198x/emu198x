//! Headless Atari 800XL runner.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
    read_media_asset,
};
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};
use serde_json::json;

const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;

const USAGE: &str = "\
Usage: emu198x-atari-800xl [OPTIONS]

ROMs (all optional, but need at least one of --os or --cart to boot):
    --os PATH                  16 KB OS ROM (atarixl.rom / atariosb.rom)
                               default: $EMU198X_A800XL_OS, then
                               ~/.emu198x/roms/atari-800xl/atarixl.rom
    --basic PATH               8 KB BASIC ROM (ataribas.rom)
                               default: $EMU198X_A800XL_BASIC, then
                               ~/.emu198x/roms/atari-800xl/ataribas.rom
    --cart PATH                cartridge image (flat, XEGS, MegaCart or OSS; .car headers honoured)

Media:
    --disk PATH                ATR disk image for D1: (a .zip holding one .atr works too)

Hardware:
    --region MODE              ntsc | pal [default: ntsc]
    --no-basic                 start with BASIC disabled (default: enabled)
    --frames N                 frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug)]
struct Cli {
    os: Option<PathBuf>,
    basic: Option<PathBuf>,
    cart: Option<PathBuf>,
    disk: Option<PathBuf>,
    basic_enabled: bool,
    region: Region,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            os: None,
            basic: None,
            cart: None,
            disk: None,
            basic_enabled: true,
            region: Region::Ntsc,
            frames: 0,
            screenshot: None,
            audio_capture: None,
            script: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Ntsc,
    Pal,
}

impl Region {
    const fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::A800xlNtsc,
            Self::Pal => Model::A800xlPal,
        }
    }
    const fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
            Self::Pal => FRAME_TICKS_PAL,
        }
    }
}

fn parse_cli<I: IntoIterator<Item = String>>(args: I) -> Cli {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--os" => cli.os = Some(PathBuf::from(next_arg(&mut iter, "--os"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--disk" => cli.disk = Some(PathBuf::from(next_arg(&mut iter, "--disk"))),
            "--no-basic" => cli.basic_enabled = false,
            "--region" => {
                cli.region = match next_arg(&mut iter, "--region").as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
                    other => die(&format!("--region expects ntsc|pal, got {other}")),
                };
            }
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames")
                    .parse()
                    .unwrap_or_else(|_| die("--frames requires a non-negative integer"));
            }
            "--screenshot" => {
                cli.screenshot = Some(PathBuf::from(next_arg(&mut iter, "--screenshot")));
            }
            "--audio-capture" => {
                cli.audio_capture = Some(PathBuf::from(next_arg(&mut iter, "--audio-capture")));
            }
            "--script" => cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script"))),
            "--headless" => {}
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            other => die(&format!("unknown argument: {other}")),
        }
    }
    cli
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

fn default_rom(env_key: &str, default_file: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var(env_key)
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(format!(".emu198x/roms/atari-800xl/{default_file}"));
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

fn read_optional(path: Option<&PathBuf>, label: &str) -> Result<Option<Vec<u8>>, String> {
    match path {
        Some(p) => fs::read(p)
            .map(Some)
            .map_err(|err| format!("failed to read {label} {}: {err}", p.display())),
        None => Ok(None),
    }
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for missing ROMs, script parse / exec failures,
/// or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let os_path = cli
        .os
        .clone()
        .or_else(|| default_rom("EMU198X_A800XL_OS", "atarixl.rom"));
    let basic_path = cli
        .basic
        .clone()
        .or_else(|| default_rom("EMU198X_A800XL_BASIC", "ataribas.rom"));
    let os = read_optional(os_path.as_ref(), "--os")?;
    let basic = read_optional(basic_path.as_ref(), "--basic")?;
    let cart = read_optional(cli.cart.as_ref(), "--cart")?;

    if os.is_none() && cart.is_none() {
        return Err(
            "either --os or --cart must be provided (cart-only boot uses the cart's reset vector)"
                .into(),
        );
    }

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let runtime = Atari800xlRuntime::new(cli.region.model(), os, basic, cart, cli.basic_enabled)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        cli.region.frame_ticks(),
        Atari800xlSessionQueryProvider,
    );
    let media = MediaSet::new();
    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    if let Some(path) = &cli.disk {
        let loaded = read_media_asset(path, MediaKind::Disk)
            .map_err(|err| format!("failed to load disk asset {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("disk-1", MediaKind::Disk, &loaded.bytes));
        session
            .load_media(&media)
            .map_err(|err| format!("disk load failed: {err}"))?;
    }

    let mut observations: Vec<ScriptObservation> = Vec::new();
    if let Some(path) = &cli.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        observations.extend(
            script
                .execute_collect(&mut session)
                .map_err(|err| format!("script execution failed: {err}"))?,
        );
    }

    if cli.frames > 0 {
        session
            .run_frames(cli.frames)
            .map_err(|err| format!("run failed: {err}"))?;
    }
    if let Some(path) = &cli.screenshot {
        session
            .save_screenshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    if let Some(path) = &cli.audio_capture {
        session
            .save_audio_capture(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    let machine = session.machine();
    let machine_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    observations.extend(session.blank_frame_observation());
    Ok(json!({
        "machine_loaded": machine_loaded,
        "frames_run":     frame_count,
        "time":           session.time().get(),
        "basic_enabled":  cli.basic_enabled,
        "observations":   observations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(Vec::<String>::new());
        assert_eq!(cli.region, Region::Ntsc);
        assert!(cli.basic_enabled);
        assert!(cli.disk.is_none());
    }

    #[test]
    fn parse_cli_takes_a_disk_path() {
        let cli = parse_cli(["--disk".to_owned(), "dos.atr".to_owned()]);
        assert_eq!(cli.disk, Some(PathBuf::from("dos.atr")));
    }
}
