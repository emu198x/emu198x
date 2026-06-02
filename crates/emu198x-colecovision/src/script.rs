//! Headless ColecoVision runner — `--bios` / `--cart` / shared `--script` mode.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_coleco_colecovision::{CvRuntime, CvSessionQueryProvider, Model};
use serde_json::json;

const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
const BIOS_SIZE: usize = 8 * 1024;

const USAGE: &str = "\
Usage: emu198x-colecovision [OPTIONS]

BIOS:
    --bios PATH                ColecoVision BIOS ROM (8 KB)
                               default: $EMU198X_COLECO_BIOS, then
                               ~/.emu198x/roms/coleco-colecovision/colecovision.rom

Cartridge:
    --cart PATH                cartridge ROM image

Region / timing:
    --region MODE              ntsc | pal [default: ntsc]
    --frames N                 native video frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug)]
struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    region: Region,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
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
            Self::Ntsc => Model::CvNtsc,
            Self::Pal => Model::CvPal,
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
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
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
    if let Ok(p) = env::var("EMU198X_COLECO_BIOS") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/coleco-colecovision/colecovision.rom"))
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for unreadable BIOS / cart files, invalid BIOS
/// size, script parse / execution failures, or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let bios_path = cli
        .bios
        .clone()
        .or_else(default_bios_path)
        .ok_or_else(|| "--bios PATH is required (or set EMU198X_COLECO_BIOS)".to_string())?;
    let bios = fs::read(&bios_path)
        .map_err(|err| format!("failed to read BIOS {}: {err}", bios_path.display()))?;
    if bios.len() != BIOS_SIZE {
        return Err(format!(
            "BIOS at {} is {} bytes; expected {BIOS_SIZE}",
            bios_path.display(),
            bios.len()
        ));
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

    let cart_bytes = load_cart_bytes(cli.cart.as_deref())?;

    let mut runtime = CvRuntime::new(cli.region.model(), bios.clone())
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    if let Some(rom) = &cart_bytes {
        runtime.insert_cartridge(rom.clone());
    }

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        cli.region.frame_ticks(),
        CvSessionQueryProvider,
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
    let bios_loaded = machine.machine().is_some();
    let cart_loaded = bios_loaded && cart_bytes.is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    Ok(json!({
        "bios_loaded": bios_loaded,
        "cart_loaded": cart_loaded,
        "frames_run":  frame_count,
        "time":        session.time().get(),
        "observations": observations,
    }))
}

fn load_cart_bytes(path: Option<&Path>) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read --cart {}: {err}", path.display()))?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(Vec::<String>::new());
        assert!(cli.bios.is_none());
        assert!(cli.cart.is_none());
        assert_eq!(cli.region, Region::Ntsc);
        assert_eq!(cli.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".into(),
            "/tmp/bios".into(),
            "--cart".into(),
            "/tmp/cart".into(),
            "--region".into(),
            "pal".into(),
            "--frames".into(),
            "120".into(),
            "--screenshot".into(),
            "/tmp/shot.png".into(),
        ];
        let cli = parse_cli(argv);
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/bios"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/cart"));
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.frames, 120);
    }
}
