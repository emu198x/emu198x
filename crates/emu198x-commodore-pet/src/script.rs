//! Headless PET runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_commodore_pet::{Model, PetRuntime, PetSessionQueryProvider};
use serde_json::json;

const FRAME_TICKS: u64 = 20_000;

const USAGE: &str = "\
Usage: emu198x-commodore-pet [OPTIONS]

ROMs (all required):
    --kernal PATH              KERNAL ROM (4 KB)
    --basic PATH               BASIC ROM (8 KB)
    --editor PATH              editor ROM (2 KB)
    --char PATH                character ROM (4 KB)
                               defaults: $EMU198X_PET_{KERNAL,BASIC,EDITOR,CHAR},
                               then ~/.emu198x/roms/commodore-pet/{kernal,basic,editor,chargen}.rom

Display:
    --columns N                40 or 80 [default: 40]
    --frames N                 frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV (silent — PET has no audio)

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug, Default)]
struct Cli {
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    editor: Option<PathBuf>,
    char_rom: Option<PathBuf>,
    columns: Option<u32>,
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
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--editor" => cli.editor = Some(PathBuf::from(next_arg(&mut iter, "--editor"))),
            "--char" => cli.char_rom = Some(PathBuf::from(next_arg(&mut iter, "--char"))),
            "--columns" => {
                cli.columns = Some(
                    next_arg(&mut iter, "--columns")
                        .parse()
                        .unwrap_or_else(|_| die("--columns expects 40 or 80")),
                );
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

fn default_rom(kind: &str, default_file: &str) -> Option<PathBuf> {
    let env_key = format!("EMU198X_PET_{kind}");
    if let Ok(p) = env::var(&env_key)
        && !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!(".emu198x/roms/commodore-pet/{default_file}")))
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for missing / wrong-size ROMs, script parse / exec
/// failures, or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let kernal_path = cli
        .kernal
        .clone()
        .or_else(|| default_rom("KERNAL", "kernal.rom"))
        .ok_or_else(|| "--kernal PATH is required".to_string())?;
    let basic_path = cli
        .basic
        .clone()
        .or_else(|| default_rom("BASIC", "basic.rom"))
        .ok_or_else(|| "--basic PATH is required".to_string())?;
    let editor_path = cli
        .editor
        .clone()
        .or_else(|| default_rom("EDITOR", "editor.rom"))
        .ok_or_else(|| "--editor PATH is required".to_string())?;
    let char_path = cli
        .char_rom
        .clone()
        .or_else(|| default_rom("CHAR", "chargen.rom"))
        .ok_or_else(|| "--char PATH is required".to_string())?;
    let kernal = read_rom(&kernal_path, "KERNAL", 4096)?;
    let basic = read_rom(&basic_path, "BASIC", 8192)?;
    let editor = read_rom(&editor_path, "editor", 2048)?;
    let char_rom = read_rom(&char_path, "character", 4096)?;

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let model = match cli.columns.unwrap_or(40) {
        80 => Model::Pet80Col,
        _ => Model::Pet40Col,
    };

    let runtime = PetRuntime::new(model, kernal, basic, editor, char_rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS,
        PetSessionQueryProvider,
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
    let roms_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    Ok(json!({
        "roms_loaded": roms_loaded,
        "frames_run":  frame_count,
        "time":        session.time().get(),
        "columns":     model.screen_chars(),
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
        assert!(cli.kernal.is_none());
        assert_eq!(cli.frames, 0);
    }
}
