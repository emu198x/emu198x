//! Headless Atari 5200 runner.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_atari_5200::{Atari5200Runtime, Atari5200SessionQueryProvider, Model};
use serde_json::json;

const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;

const USAGE: &str = "\
Usage: emu198x-atari-5200 [OPTIONS]

Cartridge (required):
    --cart PATH                Atari 5200 cartridge ROM

BIOS (optional):
    --bios PATH                Atari 5200 BIOS ROM (2 KB)
                               default: $EMU198X_A5200_BIOS, then
                               ~/.emu198x/roms/atari-5200/bios.rom or 5200.rom

Region / timing:
    --region MODE              ntsc | pal [default: ntsc]
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
    cart: Option<PathBuf>,
    bios: Option<PathBuf>,
    region: Region,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            bios: None,
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
            Self::Ntsc => Model::A5200Ntsc,
            Self::Pal => Model::A5200Pal,
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
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
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

fn default_bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_A5200_BIOS")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let dir = PathBuf::from(env::var("HOME").ok()?).join(".emu198x/roms/atari-5200");
    // The 5200 BIOS ships under either name; return the first that exists so a
    // conventionally-named `5200.rom` is found.
    ["bios.rom", "5200.rom"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|p| p.exists())
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for missing / wrong-size cart, script parse / exec
/// failures, or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let cart_path = cli
        .cart
        .clone()
        .ok_or_else(|| "--cart PATH is required".to_string())?;
    let cart = fs::read(&cart_path)
        .map_err(|err| format!("failed to read --cart {}: {err}", cart_path.display()))?;
    let bios = if let Some(path) = cli.bios.clone().or_else(default_bios_path) {
        fs::read(&path).unwrap_or_default()
    } else {
        Vec::new()
    };

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let runtime = Atari5200Runtime::new(cli.region.model(), cart, bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        cli.region.frame_ticks(),
        Atari5200SessionQueryProvider,
    );
    let media = MediaSet::new();
    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

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
    let cart_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    observations.extend(session.blank_frame_observation());
    Ok(json!({
        "cart_loaded": cart_loaded,
        "frames_run":  frame_count,
        "time":        session.time().get(),
        "observations": observations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(Vec::<String>::new());
        assert!(cli.cart.is_none());
        assert_eq!(cli.region, Region::Ntsc);
    }
}
