//! Headless Acorn Atom runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_acorn_atom::{AtomRuntime, AtomSessionQueryProvider, Model};
use serde_json::json;

const FRAME_TICKS: u64 = 20_000;

const USAGE: &str = "\
Usage: emu198x-acorn-atom [OPTIONS]

ROM (required):
    --rom PATH                 24 KB combined ROM (BASIC1 + FP + BASIC2 + OS)
                               default: $EMU198X_ACORN_ATOM_ROM, then
                               ~/.emu198x/roms/acorn-atom/atom.rom

Hardware:
    --ram-kb N                 base RAM in KB (~2, or >=12 for a fully-expanded 32K) [default: 2]
    --frames N                 frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV (currently silent)
    --save-tape PATH           write any cassette SAVE captured during the run as a .uef

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug)]
struct Cli {
    rom: Option<PathBuf>,
    ram_kb: usize,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    save_tape: Option<PathBuf>,
    script: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            ram_kb: 2,
            frames: 0,
            screenshot: None,
            audio_capture: None,
            save_tape: None,
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
            "--ram-kb" => {
                cli.ram_kb = next_arg(&mut iter, "--ram-kb")
                    .parse()
                    .unwrap_or_else(|_| die("--ram-kb requires a non-negative integer"));
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
            "--save-tape" => {
                cli.save_tape = Some(PathBuf::from(next_arg(&mut iter, "--save-tape")));
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
    if let Ok(p) = env::var("EMU198X_ACORN_ATOM_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/acorn-atom/atom.rom");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

fn model_for(ram_kb: usize) -> Model {
    if ram_kb >= 12 {
        Model::AtomFull
    } else {
        Model::AtomBase
    }
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
    let rom = read_rom(&rom_path, "Atom", 24 * 1024)?;

    if (cli.screenshot.is_some() || cli.audio_capture.is_some() || cli.save_tape.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let runtime = AtomRuntime::new(model_for(cli.ram_kb), rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, FRAME_TICKS, AtomSessionQueryProvider);
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
    if let Some(path) = &cli.save_tape {
        let uef = session
            .machine_mut()
            .flush_tape_image()
            .ok_or_else(|| "--save-tape: no cassette SAVE was captured".to_string())?;
        fs::write(path, &uef)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    let machine = session.machine();
    let rom_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    Ok(json!({
        "rom_loaded":   rom_loaded,
        "frames_run":   frame_count,
        "time":         session.time().get(),
        "ram_kb":       cli.ram_kb,
        "observations": observations,
    }))
}

fn read_rom(path: &Path, kind: &str, expected: usize) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read {kind} ROM {}: {err}", path.display()))?;
    if bytes.len() != expected {
        return Err(format!(
            "{kind} ROM at {} is {} bytes; expected {expected}",
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
        assert_eq!(cli.ram_kb, 2);
    }

    #[test]
    fn model_selects_by_ram() {
        assert_eq!(model_for(2), Model::AtomBase);
        assert_eq!(model_for(12), Model::AtomFull);
    }
}
