//! Headless ZX80 runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
};
use runtime_sinclair_zx80::{Model, Zx80Runtime, Zx80SessionQueryProvider};
use serde_json::json;

const FRAME_TICKS_PAL: u64 = 207 * 312;
const ROM_SIZE: usize = 4 * 1024;

const USAGE: &str = "\
Usage: emu198x-sinclair-zx80 [OPTIONS]

ROM:
    --rom PATH                 ZX80 monitor ROM (4 KB)
                               default: $EMU198X_ZX80_ROM, then
                               ~/.emu198x/roms/sinclair-zx80/zx80.rom

Hardware:
    --ram-bytes N              RAM size (power-of-two ≤ 16384) [default: 1024]
    --frames N                 PAL frames to run [default: 0]

Media:
    --tape PATH                put a .o/.80 cassette in the deck. This does
                               not press play: the loader's leader countdown
                               is at the front of the tape, so the script has
                               to type LOAD (the W key) first, then issue a
                               `media_transport` start step on slot `tape-1`.

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV (currently silent)

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug)]
struct Cli {
    rom: Option<PathBuf>,
    ram_bytes: usize,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
    tape: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            ram_bytes: 1024,
            frames: 0,
            screenshot: None,
            audio_capture: None,
            script: None,
            tape: None,
        }
    }
}

fn parse_cli<I: IntoIterator<Item = String>>(args: I) -> Cli {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--ram-bytes" => {
                cli.ram_bytes = next_arg(&mut iter, "--ram-bytes")
                    .parse()
                    .unwrap_or_else(|_| die("--ram-bytes requires a positive integer"));
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
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
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

fn default_rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX80_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/sinclair-zx80/zx80.rom"))
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for missing / wrong-size ROM, script parse / exec
/// failures, or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let rom_path = cli
        .rom
        .clone()
        .or_else(default_rom_path)
        .ok_or_else(|| "--rom PATH is required".to_string())?;
    let rom = read_rom(&rom_path)?;

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let mut runtime = Zx80Runtime::new(Model::Zx80, rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    runtime
        .set_ram_bytes(cli.ram_bytes)
        .map_err(|err| format!("invalid --ram-bytes: {err}"))?;

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS_PAL,
        Zx80SessionQueryProvider,
    );
    let tape_bytes = match &cli.tape {
        Some(path) => Some(
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        ),
        None => None,
    };
    let mut media = MediaSet::new();
    if let Some(bytes) = &tape_bytes {
        media.push(MediaImage::new("tape-1", MediaKind::Tape, bytes));
    }
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
    let rom_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    observations.extend(session.blank_frame_observation());
    Ok(json!({
        "rom_loaded": rom_loaded,
        "frames_run": frame_count,
        "time":       session.time().get(),
        "ram_bytes":  cli.ram_bytes,
        "tape_loaded": tape_bytes.is_some(),
        "observations": observations,
    }))
}

fn read_rom(path: &Path) -> Result<Vec<u8>, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read ROM {}: {err}", path.display()))?;
    if bytes.len() != ROM_SIZE {
        return Err(format!(
            "ROM at {} is {} bytes; expected {ROM_SIZE}",
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
        assert_eq!(cli.ram_bytes, 1024);
    }
}
