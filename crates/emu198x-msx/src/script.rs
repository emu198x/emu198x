//! Headless MSX1 runner — `--script` / legacy `--bios` / `--cart` mode.
//!
//! Brings the existing MSX1 binary's `--bios PATH` / `--cart PATH` /
//! `--frames N` / `--screenshot` / `--audio-capture` surface onto the
//! shared `emu198x-shell` runner, and adds the cross-system
//! `--script` JSON runner. The dispatcher in `main.rs` routes here for
//! everything except `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaSet, ScriptObservation,
};
use machine_msx::MapperType;
use runtime_msx::{Model, MsxRuntime, MsxSessionQueryProvider};
use serde_json::json;

const MSX_FRAME_TICKS_NTSC: u64 = 228 * 262;
const MSX_FRAME_TICKS_PAL: u64 = 228 * 313;
const BIOS_SIZE: usize = 32 * 1024;

const USAGE: &str = "\
Usage: emu198x-msx [OPTIONS]   (headless; add --mcp for the MCP stdio server)

BIOS:
    --bios PATH                MSX1 BIOS ROM (32 KB)
                               default: $EMU198X_MSX_BIOS, then
                               ~/.emu198x/roms/microsoft-msx/msx.rom

Cartridge:
    --cart PATH                cartridge ROM (slot 1)
    --mapper KIND              plain | konami | konami-scc | ascii8 | ascii16
                               [default: plain]
    --cart2 PATH               cartridge ROM (slot 2)
    --mapper2 KIND             slot-2 mapper [default: plain]

Region / timing:
    --region MODE              ntsc | pal [default: ntsc]
    --frames N                 native video frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help

Examples:
    emu198x-msx --bios ~/.emu198x/roms/microsoft-msx/msx.rom \\
        --frames 200 --screenshot msx-boot.png

    emu198x-msx --bios msx.rom --cart game.rom --mapper konami \\
        --frames 600 --audio-capture game.wav

    emu198x-msx --bios msx.rom --script steps.json
";

#[derive(Debug)]
struct Cli {
    bios: Option<PathBuf>,
    cart: Option<PathBuf>,
    mapper: MapperType,
    cart2: Option<PathBuf>,
    mapper2: MapperType,
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
            mapper: MapperType::Plain,
            cart2: None,
            mapper2: MapperType::Plain,
            region: Region::default(),
            frames: 0,
            screenshot: None,
            audio_capture: None,
            script: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Region {
    #[default]
    Ntsc,
    Pal,
}

impl Region {
    const fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::Msx1Ntsc,
            Self::Pal => Model::Msx1Pal,
        }
    }

    const fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => MSX_FRAME_TICKS_NTSC,
            Self::Pal => MSX_FRAME_TICKS_PAL,
        }
    }
}

fn parse_mapper(value: &str) -> MapperType {
    match value {
        "plain" => MapperType::Plain,
        "konami" => MapperType::Konami,
        "konami-scc" => MapperType::KonamiScc,
        "ascii8" => MapperType::Ascii8,
        "ascii16" => MapperType::Ascii16,
        other => die(&format!(
            "--mapper expects plain|konami|konami-scc|ascii8|ascii16, got {other}"
        )),
    }
}

fn parse_cli<I: IntoIterator<Item = String>>(args: I) -> Cli {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--bios" => cli.bios = Some(PathBuf::from(next_arg(&mut iter, "--bios"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--mapper" => cli.mapper = parse_mapper(&next_arg(&mut iter, "--mapper")),
            "--cart2" => cli.cart2 = Some(PathBuf::from(next_arg(&mut iter, "--cart2"))),
            "--mapper2" => cli.mapper2 = parse_mapper(&next_arg(&mut iter, "--mapper2")),
            "--region" => {
                cli.region = match next_arg(&mut iter, "--region").as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
                    other => die(&format!("--region expects ntsc or pal, got {other}")),
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
    if let Ok(p) = env::var("EMU198X_MSX_BIOS")
        && !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/microsoft-msx/msx.rom"))
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error string for unreadable BIOS / cart files, invalid
/// BIOS size, script parse / execution failures, or capture I/O.
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
        .ok_or_else(|| "--bios PATH is required (or set EMU198X_MSX_BIOS)".to_string())?;
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

    let model = cli.region.model();
    let frame_ticks = cli.region.frame_ticks();
    let mut runtime = MsxRuntime::new(model, bios.clone())
        .map_err(|err| format!("failed to construct runtime: {err}"))?;

    let cart_bytes = load_cart_bytes(cli.cart.as_deref(), "--cart")?;
    if let Some(bytes) = &cart_bytes {
        runtime.insert_cartridge1(bytes.clone(), cli.mapper);
    }
    let cart2_bytes = load_cart_bytes(cli.cart2.as_deref(), "--cart2")?;
    if let Some(bytes) = &cart2_bytes {
        runtime.insert_cartridge2(bytes.clone(), cli.mapper2);
    }

    let mut session =
        HeadlessSession::new_with_query_provider(runtime, frame_ticks, MsxSessionQueryProvider);

    // The cart slots are already inserted directly into the machine
    // via `insert_cartridge1/2`. `prepare` is called with an empty
    // media set so the session bookkeeping (loaded media tracking) is
    // primed; cart bytes don't need to round-trip through MediaSet.
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
    let cart1_loaded = bios_loaded && cart_bytes.is_some();
    let cart2_loaded = bios_loaded && cart2_bytes.is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);

    Ok(json!({
        "bios_loaded": bios_loaded,
        "cart1_loaded": cart1_loaded,
        "cart2_loaded": cart2_loaded,
        "frames_run": frame_count,
        "time": session.time().get(),
        "observations": observations,
    }))
}

fn load_cart_bytes(path: Option<&Path>, flag: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read {flag} {}: {err}", path.display()))?;
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
        assert!(matches!(cli.mapper, MapperType::Plain));
        assert_eq!(cli.region, Region::Ntsc);
        assert_eq!(cli.frames, 0);
        assert!(cli.script.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--bios".to_owned(),
            "/tmp/msx.rom".to_owned(),
            "--cart".to_owned(),
            "/tmp/game.rom".to_owned(),
            "--mapper".to_owned(),
            "konami-scc".to_owned(),
            "--region".to_owned(),
            "pal".to_owned(),
            "--frames".to_owned(),
            "120".to_owned(),
            "--screenshot".to_owned(),
            "/tmp/shot.png".to_owned(),
            "--audio-capture".to_owned(),
            "/tmp/audio.wav".to_owned(),
            "--script".to_owned(),
            "/tmp/steps.json".to_owned(),
        ];
        let cli = parse_cli(argv);
        assert_eq!(cli.bios.unwrap(), Path::new("/tmp/msx.rom"));
        assert_eq!(cli.cart.unwrap(), Path::new("/tmp/game.rom"));
        assert!(matches!(cli.mapper, MapperType::KonamiScc));
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.frames, 120);
        assert_eq!(cli.screenshot.unwrap(), Path::new("/tmp/shot.png"));
        assert_eq!(cli.audio_capture.unwrap(), Path::new("/tmp/audio.wav"));
        assert_eq!(cli.script.unwrap(), Path::new("/tmp/steps.json"));
    }
}
