//! Headless Aquarius runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_mattel_aquarius::{AquariusRuntime, AquariusSessionQueryProvider, Model};
use serde_json::json;

const FRAME_TICKS_PAL: u64 = 71_590;
const BIOS_SIZE: usize = 8 * 1024;

const USAGE: &str = "\
Usage: emu198x-mattel-aquarius [OPTIONS]

BIOS:
    --bios PATH                Aquarius BASIC ROM (8 KB)
                               default: $EMU198X_AQUARIUS_BIOS, then
                               ~/.emu198x/roms/mattel-aquarius/aquarius.rom

Cartridge:
    --cart PATH                cartridge ROM (mapped at $E000-$FFFF, up to 8 KB)

Hardware:
    --expansion-kb N           RAM expansion in KB (0..=16) [default: 0]
    --frames N                 native PAL frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV (currently silent)

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug, Default)]
struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    expansion_kb: usize,
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
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--expansion-kb" => {
                cli.expansion_kb = next_arg(&mut iter, "--expansion-kb")
                    .parse()
                    .unwrap_or_else(|_| die("--expansion-kb requires a non-negative integer"));
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
    if let Ok(p) = env::var("EMU198X_AQUARIUS_BIOS")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/mattel-aquarius/aquarius.rom"))
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for unreadable / wrong-size BIOS or cart, script
/// parse / execution failures, or capture I/O.
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
        .ok_or_else(|| "--bios PATH is required (or set EMU198X_AQUARIUS_BIOS)".to_string())?;
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

    let mut runtime = AquariusRuntime::new(Model::Aquarius, bios)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    runtime.set_expansion_kb(cli.expansion_kb);
    if let Some(rom) = &cart_bytes {
        runtime.insert_cartridge(rom.clone());
    }

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS_PAL,
        AquariusSessionQueryProvider,
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
        "expansion_kb": cli.expansion_kb,
        "observations": observations,
    }))
}

fn load_cart_bytes(path: Option<&Path>) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read --cart {}: {err}", path.display()))?;
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
        assert_eq!(cli.expansion_kb, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".into(),
            "/tmp/aq.rom".into(),
            "--cart".into(),
            "/tmp/game".into(),
            "--expansion-kb".into(),
            "16".into(),
            "--frames".into(),
            "60".into(),
        ];
        let cli = parse_cli(argv);
        assert_eq!(cli.bios.expect("parsed by CLI"), Path::new("/tmp/aq.rom"));
        assert_eq!(cli.expansion_kb, 16);
    }
}
