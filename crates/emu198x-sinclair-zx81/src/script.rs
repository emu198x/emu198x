//! Headless ZX81 runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_sinclair_zx81::{Model, Zx81Runtime, Zx81SessionQueryProvider};
use serde_json::json;

/// Frame budget for the board the runtime is configured as. The 60 Hz strap
/// lays out a much shorter field, so this cannot be one shared constant.
fn frame_ticks(model: Model) -> u64 {
    u64::from(model.television_standard().slow_mode_frame_tstates())
}
const ROM_SIZE: usize = 8 * 1024;

const USAGE: &str = "\
Usage: emu198x-sinclair-zx81 [OPTIONS]

ROM:
    --rom PATH                 ZX81 monitor ROM (8 KB)
                               default: $EMU198X_ZX81_ROM, then
                               ~/.emu198x/roms/sinclair-zx81/zx81.rom

Hardware:
    --ram-bytes N              RAM size (power-of-two ≤ 16384) [default: 1024]
    --frames N                 PAL frames to run [default: 0]

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
    if let Ok(p) = env::var("EMU198X_ZX81_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/sinclair-zx81/zx81.rom"))
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

    let mut runtime = Zx81Runtime::new(Model::Zx81, rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    runtime
        .set_ram_bytes(cli.ram_bytes)
        .map_err(|err| format!("invalid --ram-bytes: {err}"))?;

    let budget = frame_ticks(runtime.model());
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, budget, Zx81SessionQueryProvider);
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
    let rom_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    Ok(json!({
        "rom_loaded": rom_loaded,
        "frames_run": frame_count,
        "time":       session.time().get(),
        "ram_bytes":  cli.ram_bytes,
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
