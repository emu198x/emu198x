//! Headless BBC Micro runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider, Model};
use serde_json::json;

const FRAME_TICKS_PAL: u64 = 2_000_000 / 50;
const MOS_SIZE: usize = 16 * 1024;

const USAGE: &str = "\
Usage: emu198x-acorn-bbc-micro [OPTIONS]

ROM:
    --mos PATH                 BBC MOS ROM (16 KB)
                               default: $EMU198X_BBC_MOS, then
                               ~/.emu198x/roms/acorn-bbc-micro/os.rom

Sideways ROMs (repeatable):
    --sideways BANK=PATH       install a sideways ROM into bank 0..=15

Run:
    --frames N                 PAL frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug, Default)]
struct Cli {
    mos: Option<PathBuf>,
    sideways: Vec<(usize, PathBuf)>,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
}

fn parse_cli<I: IntoIterator<Item = String>>(args: I) -> Cli {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--mos" => cli.mos = Some(PathBuf::from(next_arg(&mut iter, "--mos"))),
            "--sideways" => {
                let spec = next_arg(&mut iter, "--sideways");
                let Some((bank_str, path_str)) = spec.split_once('=') else {
                    die("--sideways expects BANK=PATH");
                };
                let bank: usize = bank_str
                    .parse()
                    .unwrap_or_else(|_| die("--sideways bank must be an integer 0..=15"));
                if bank > 15 {
                    die("--sideways bank must be 0..=15");
                }
                cli.sideways.push((bank, PathBuf::from(path_str)));
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

fn default_mos_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_BBC_MOS")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/os.rom"))
}

fn default_font_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_BBC_SAA5050")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/saa5050.rom");
    p.exists().then_some(p)
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for missing / wrong-size MOS ROM, unreadable
/// sideways ROMs, script parse / exec failures, or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let mos_path = cli
        .mos
        .clone()
        .or_else(default_mos_path)
        .ok_or_else(|| "--mos PATH is required".to_string())?;
    let mos = read_rom(&mos_path)?;

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let mut runtime = BbcMicroRuntime::new(Model::BbcModelB, mos)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    for (bank, path) in &cli.sideways {
        let rom = fs::read(path).map_err(|err| {
            format!(
                "failed to read sideways ROM bank {bank} {}: {err}",
                path.display()
            )
        })?;
        runtime.insert_sideways_rom(*bank, rom);
    }

    // Load the SAA5050 teletext character ROM (MODE 7) if one is available.
    if let Some(font_path) = default_font_path()
        && let Ok(font) = fs::read(&font_path)
    {
        runtime.set_teletext_font(font);
    }

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS_PAL,
        BbcMicroSessionQueryProvider,
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
    let mos_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    Ok(json!({
        "mos_loaded":     mos_loaded,
        "sideways_count": cli.sideways.len(),
        "frames_run":     frame_count,
        "time":           session.time().get(),
        "observations":   observations,
    }))
}

fn read_rom(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read MOS ROM {}: {err}", path.display()))?;
    if bytes.len() != MOS_SIZE {
        return Err(format!(
            "MOS ROM at {} is {} bytes; expected {MOS_SIZE}",
            path.display(),
            bytes.len()
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(Vec::<String>::new());
        assert!(cli.mos.is_none());
    }
}
