//! Headless Acorn Electron runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};
use serde_json::json;

// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const FRAME_TICKS_PAL: u64 = 39_936;
const ROM_SIZE: usize = 16 * 1024;

const USAGE: &str = "\
Usage: emu198x-acorn-electron [OPTIONS]

ROMs (both required):
    --os PATH                  Electron OS ROM (16 KB)
    --basic PATH               BBC BASIC II ROM (16 KB)
                               defaults: $EMU198X_ELECTRON_OS / $EMU198X_ELECTRON_BASIC,
                               then ~/.emu198x/roms/acorn-electron/{os,basic}.rom

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
    os: Option<PathBuf>,
    basic: Option<PathBuf>,
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
            "--os" => cli.os = Some(PathBuf::from(next_arg(&mut iter, "--os"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
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

fn default_rom_path(kind: &str) -> Option<PathBuf> {
    let env_key = format!("EMU198X_ELECTRON_{}", kind.to_ascii_uppercase());
    if let Ok(p) = env::var(&env_key)
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!(
        ".emu198x/roms/acorn-electron/{}.rom",
        kind.to_ascii_lowercase()
    )))
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
    let os_path = cli
        .os
        .clone()
        .or_else(|| default_rom_path("os"))
        .ok_or_else(|| "--os PATH is required (or set EMU198X_ELECTRON_OS)".to_string())?;
    let basic_path = cli
        .basic
        .clone()
        .or_else(|| default_rom_path("basic"))
        .ok_or_else(|| "--basic PATH is required (or set EMU198X_ELECTRON_BASIC)".to_string())?;
    let os = read_rom(&os_path, "OS")?;
    let basic = read_rom(&basic_path, "BASIC")?;

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let runtime = ElectronRuntime::new(Model::Electron, os, basic)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS_PAL,
        ElectronSessionQueryProvider,
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
        "observations": observations,
    }))
}

fn read_rom(path: &Path, kind: &str) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read {kind} ROM {}: {err}", path.display()))?;
    if bytes.len() != ROM_SIZE {
        return Err(format!(
            "{kind} ROM at {} is {} bytes; expected {ROM_SIZE}",
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
        assert!(cli.os.is_none());
        assert!(cli.basic.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--os".into(),
            "/tmp/os".into(),
            "--basic".into(),
            "/tmp/basic".into(),
            "--frames".into(),
            "30".into(),
        ];
        let cli = parse_cli(argv);
        assert_eq!(cli.os.expect("parsed by CLI"), Path::new("/tmp/os"));
        assert_eq!(cli.basic.expect("parsed by CLI"), Path::new("/tmp/basic"));
    }
}
