//! Headless Amstrad CPC464 runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
    read_media_asset,
};
use runtime_amstrad_cpc::{AmstradCpcRuntime, AmstradCpcSessionQueryProvider, Model};
use serde_json::json;

/// One PAL frame: 64 character clocks per line x 312 lines x 4 T-states.
///
/// Must not exceed the machine's own `run_frame` budget, or the harness runs
/// two machine frames per displayed frame and everything plays at double
/// speed.
const FRAME_TICKS_PAL: u64 = 64 * 312 * 4;

/// 16 KB of OS followed by 16 KB of BASIC.
const FIRMWARE_SIZE: usize = 32 * 1024;

const USAGE: &str = "\
Usage: emu198x-amstrad-cpc [OPTIONS]

ROM:
    --rom PATH                 CPC464 firmware (32 KB: 16 KB OS + 16 KB BASIC)
                               default: $EMU198X_CPC464_ROM, then
                               ~/.emu198x/roms/amstrad-cpc/cpc464.rom

Media:
    --tape PATH                .cdt cassette image

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
    rom: Option<PathBuf>,
    tape: Option<PathBuf>,
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
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
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

pub(crate) fn default_rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_CPC464_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/amstrad-cpc/cpc464.rom"))
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for a missing or wrong-size firmware image, an unreadable
/// or malformed tape, script parse / execution failures, or capture I/O.
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
    let firmware = read_firmware(&rom_path)?;

    // Capture with nothing to run would write the power-on frame and look like
    // a working screenshot, so refuse rather than produce a misleading file.
    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let runtime = AmstradCpcRuntime::new(Model::Cpc464, firmware)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS_PAL,
        AmstradCpcSessionQueryProvider,
    );
    session
        .prepare(&MediaSet::new(), &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    if let Some(path) = &cli.tape {
        let loaded = read_media_asset(path, MediaKind::Tape)
            .map_err(|err| format!("failed to load tape asset {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
        session
            .load_media(&media)
            .map_err(|err| format!("tape load failed: {err}"))?;
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
    observations.extend(session.blank_frame_observation());
    Ok(json!({
        "rom_loaded":   machine.machine().is_some(),
        "tape_loaded":  machine.machine().is_some_and(|m| m.tape().has_tape()),
        "frames_run":   machine.machine().map_or(0, |m| m.frame_count()),
        "time":         session.time().get(),
        "observations": observations,
    }))
}

fn read_firmware(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read firmware {}: {err}", path.display()))?;
    if bytes.len() != FIRMWARE_SIZE {
        return Err(format!(
            "firmware at {} is {} bytes; expected {FIRMWARE_SIZE} (16 KB OS + 16 KB BASIC)",
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
        assert!(cli.rom.is_none());
        assert!(cli.tape.is_none());
        assert_eq!(cli.frames, 0);
    }

    #[test]
    fn parse_cli_reads_the_paths_and_frame_count() {
        let cli = parse_cli(
            [
                "--rom",
                "cpc464.rom",
                "--tape",
                "game.cdt",
                "--frames",
                "600",
                "--screenshot",
                "out.png",
            ]
            .map(ToOwned::to_owned),
        );
        assert_eq!(cli.rom, Some(PathBuf::from("cpc464.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("game.cdt")));
        assert_eq!(cli.frames, 600);
        assert_eq!(cli.screenshot, Some(PathBuf::from("out.png")));
    }

    #[test]
    fn a_frame_is_the_machines_own_frame_budget() {
        // Exceeding `AmstradCpc::run_frame` runs two machine frames per
        // displayed one, which plays everything at double speed with no error.
        assert_eq!(FRAME_TICKS_PAL, 79_872);
    }

    #[test]
    fn capture_without_anything_to_run_is_refused() {
        // Otherwise the PNG is the power-on frame and reads as a success.
        let cli = Cli {
            rom: Some(PathBuf::from("/nonexistent/cpc464.rom")),
            screenshot: Some(PathBuf::from("out.png")),
            ..Cli::default()
        };
        let err = run_cli(cli).expect_err("should refuse");
        // The firmware read fails first on a path that does not exist, which
        // is itself the right order; assert on a real 32 KB stub instead.
        assert!(err.contains("failed to read firmware"), "{err}");

        let dir = std::env::temp_dir().join("emu198x-cpc-capture-guard");
        fs::create_dir_all(&dir).expect("temp dir");
        let rom = dir.join("stub.rom");
        fs::write(&rom, vec![0u8; FIRMWARE_SIZE]).expect("write stub");
        let cli = Cli {
            rom: Some(rom),
            screenshot: Some(PathBuf::from("out.png")),
            ..Cli::default()
        };
        let err = run_cli(cli).expect_err("should refuse");
        assert!(err.contains("capture requests require"), "{err}");
    }

    #[test]
    fn a_wrong_size_firmware_is_named_rather_than_truncated() {
        let dir = std::env::temp_dir().join("emu198x-cpc-firmware-size");
        fs::create_dir_all(&dir).expect("temp dir");
        let rom = dir.join("short.rom");
        fs::write(&rom, vec![0u8; 16 * 1024]).expect("write short rom");
        let err = read_firmware(&rom).expect_err("16 KB should be refused");
        assert!(err.contains("16384 bytes"), "{err}");
        assert!(err.contains("32768"), "{err}");
    }
}
