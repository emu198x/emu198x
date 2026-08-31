//! Headless VIC-20 runner.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
};
use runtime_commodore_vic_20::{Model, Vic20Runtime, Vic20SessionQueryProvider};
use serde_json::json;

const FRAME_TICKS_PAL: u64 = 71 * 312;
const FRAME_TICKS_NTSC: u64 = 65 * 261;

const USAGE: &str = "\
Usage: emu198x-commodore-vic-20 [OPTIONS]

ROMs (all required):
    --kernal PATH              KERNAL ROM (8 KB)
    --basic PATH               BASIC ROM (8 KB)
    --char PATH                character ROM (4 KB)
                               defaults: $EMU198X_VIC20_{KERNAL,BASIC,CHAR},
                               then ~/.emu198x/roms/commodore-vic-20/{kernal,basic,chargen}.rom

Hardware:
    --region MODE              ntsc | pal [default: pal]
    --ram-expansion-kb N       0 (unexpanded) / 3 (low) / 3+N (high) [default: 0]
    --prg PATH                 inject a .PRG after boot and auto-RUN it (match
                               RAM to the load address, e.g. +8K for $1201)
    --prg-sys                  launch the --prg with SYS <load-addr> (machine
                               code) instead of RUN (BASIC)
    --esp-at-tcp               attach an ESP-AT modem (9600 baud initially,
                               with AT+UART_CUR support); CIPSTART opens a real
                               TCP connection (64-byte frame reassembly)
    --frames N                 frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as WAV (currently silent)

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug)]
struct Cli {
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    char_rom: Option<PathBuf>,
    region: Region,
    ram_expansion_kb: usize,
    prg: Option<PathBuf>,
    prg_sys: bool,
    esp_at_tcp: bool,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            kernal: None,
            basic: None,
            char_rom: None,
            prg: None,
            prg_sys: false,
            esp_at_tcp: false,
            region: Region::Pal,
            ram_expansion_kb: 0,
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
            Self::Ntsc => Model::Vic20Ntsc,
            Self::Pal => Model::Vic20Pal,
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
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--char" => cli.char_rom = Some(PathBuf::from(next_arg(&mut iter, "--char"))),
            "--region" => {
                cli.region = match next_arg(&mut iter, "--region").as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
                    other => die(&format!("--region expects ntsc|pal, got {other}")),
                };
            }
            "--ram-expansion-kb" => {
                cli.ram_expansion_kb = next_arg(&mut iter, "--ram-expansion-kb")
                    .parse()
                    .unwrap_or_else(|_| die("--ram-expansion-kb requires a non-negative integer"));
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
            "--prg" => cli.prg = Some(PathBuf::from(next_arg(&mut iter, "--prg"))),
            "--prg-sys" => cli.prg_sys = true,
            "--esp-at-tcp" => cli.esp_at_tcp = true,
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
    let env_key = format!("EMU198X_VIC20_{kind}");
    if let Ok(p) = env::var(&env_key)
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!("./.emu198x/roms/commodore-vic-20/{default_file}")))
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
    let char_path = cli
        .char_rom
        .clone()
        .or_else(|| default_rom("CHAR", "chargen.rom"))
        .ok_or_else(|| "--char PATH is required".to_string())?;
    let kernal = read_rom(&kernal_path, "KERNAL", 8192)?;
    let basic = read_rom(&basic_path, "BASIC", 8192)?;
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

    let mut runtime = Vic20Runtime::new(cli.region.model(), kernal, basic, char_rom)
        .map_err(|err| format!("failed to construct runtime: {err}"))?;
    runtime.set_ram_expansion_kb(cli.ram_expansion_kb);
    if cli.esp_at_tcp {
        // The external modem keeps real baud time while the VIC-I CPU clock
        // differs by region: ~115 PAL cycles or ~107 NTSC cycles at 9600.
        let cycles_per_bit = match cli.region {
            Region::Pal => 115,
            Region::Ntsc => 107,
        };
        runtime.attach_esp_at_tcp_bridge(cycles_per_bit, 64);
    }

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        cli.region.frame_ticks(),
        Vic20SessionQueryProvider,
    );
    let prg_bytes = cli
        .prg
        .as_ref()
        .map(|path| {
            fs::read(path).map_err(|err| format!("failed to read --prg {}: {err}", path.display()))
        })
        .transpose()?;
    let mut media = MediaSet::new();
    if let Some(bytes) = prg_bytes.as_deref().filter(|_| !cli.prg_sys) {
        media.push(MediaImage::new("program-1", MediaKind::Program, bytes));
    }
    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    // `--prg-sys` remains the explicit machine-code side channel. Ordinary
    // BASIC PRGs use the standard `MediaSet` path above; the runtime delays
    // their injection until the KERNAL has reached READY.
    if cli.prg_sys
        && let Some(bytes) = &prg_bytes
    {
        session
            .run_frames(150)
            .map_err(|err| format!("boot-to-READY run failed: {err}"))?;
        session
            .machine_mut()
            .autoload_prg(bytes, true)
            .map_err(|err| format!("PRG autoload failed: {err}"))?;
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
    let roms_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    observations.extend(session.blank_frame_observation());
    Ok(json!({
        "roms_loaded": roms_loaded,
        "frames_run":  frame_count,
        "time":        session.time().get(),
        "ram_expansion_kb": machine.ram_expansion_kb(),
        "esp_at_tcp_error": machine.esp_at_tcp_bridge().and_then(|bridge| bridge.last_error()),
        "esp_at_received_hex": machine.esp_at_tcp_bridge().map(|bridge| {
            bridge.diagnostic_received().iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join("")
        }),
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
        assert_eq!(cli.region, Region::Pal);
        assert_eq!(cli.ram_expansion_kb, 0);
    }
}
