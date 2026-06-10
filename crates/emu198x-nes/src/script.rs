//! Headless NES runner — `--script` / `--headless` mode.
//!
//! Loads cartridge media, runs native NES frames, executes shared JSON
//! session steps, captures screenshots / audio, asserts Blargg-style
//! test ROM status, and emits a local smoke-matrix report. This is the
//! non-interactive half of the `emu198x-nes` binary; the dispatcher in
//! `main.rs` routes here whenever any automation flag is present.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
    read_media_asset,
};
use runtime_nintendo_nes::{Model, NesRuntime, NesSessionQueryProvider};
use serde::Serialize;

const DEFAULT_CARTRIDGE_SLOT: &str = "cartridge-1";
const NES_FRAME_TICKS: u64 = 341 * 262;

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    media: Vec<MediaArg>,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
    frames: u32,
    assert_blargg: bool,
    smoke_root: Option<PathBuf>,
    smoke_report: Option<PathBuf>,
    smoke_screenshot_dir: Option<PathBuf>,
    battery_save: Option<PathBuf>,
    no_battery_save: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct MediaArg {
    slot: String,
    kind: MediaKind,
    path: PathBuf,
}

#[derive(Debug)]
struct LoadedMedia {
    slot: String,
    kind: MediaKind,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct RunnerReport {
    observations: Vec<ScriptObservation>,
    time: u64,
    cartridge_loaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_result: Option<BlarggTestResult>,
}

#[derive(Debug, Serialize)]
struct BlarggTestResult {
    kind: &'static str,
    status: u8,
    signature: [u8; 3],
    text: String,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct SmokeMatrixReport {
    rom_count: usize,
    rows: Vec<SmokeMatrixRow>,
}

#[derive(Debug, Serialize)]
struct SmokeMatrixRow {
    path: String,
    mapper: Option<u16>,
    prg_banks: Option<u8>,
    chr_banks: Option<u8>,
    result: String,
    time: Option<u64>,
    screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_result: Option<BlarggTestResult>,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
struct InesHeaderSummary {
    mapper: Option<u16>,
    prg_banks: Option<u8>,
    chr_banks: Option<u8>,
}

const USAGE: &str = "\
Usage: emu198x-nes --script [OPTIONS]   (headless; add --no-default-features for graphics-free builds)

Media:
    --media SLOT:KIND=PATH     media image by slot and kind
    --rom PATH                 alias for --media cartridge-1:cartridge=PATH

Automation:
    --script PATH              execute shared JSON session steps
    --headless                 force headless mode without a script
    --frames N                 number of native NES video frames to run
    --assert-blargg            assert Blargg-style status output at $6000
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV
    --smoke-root PATH          recursively smoke every .nes ROM under PATH
    --smoke-report PATH        write smoke matrix JSON to PATH
    --smoke-screenshot-dir PATH
                               write one PNG per successful smoke row

Battery save:
    --battery-save PATH        load/write cartridge battery RAM sidecar
    --no-battery-save          disable automatic .sav load/write

Other:
    --help, -h                 show this help

Examples:
    emu198x-nes --rom nestest.nes --frames 60 --screenshot frame.png
    emu198x-nes --rom apu_test.nes --frames 600 --assert-blargg
    emu198x-nes --rom smb.nes --script steps.json
";

/// Headless entry point. Parses the automation CLI, dispatches to the
/// smoke matrix or single-run path, and prints the JSON or summary
/// report. Errors propagate to the dispatcher, which prints them and
/// exits non-zero (Blargg failures included, so CI sees a red step).
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    if cli.smoke_root.is_some() {
        let report = run_smoke_matrix(&cli)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to serialize smoke matrix report: {err}"))?;
        if let Some(path) = &cli.smoke_report {
            fs::write(path, json.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }

    let json_mode = cli.script.is_some() || cli.assert_blargg;
    let report = run_cli(cli)?;
    if json_mode {
        let json = serde_json::to_string(&report)
            .map_err(|err| format!("failed to serialize runner report: {err}"))?;
        println!("{json}");
    } else {
        println!(
            "NES runtime: time={} cartridge_loaded={}",
            report.time, report.cartridge_loaded
        );
    }
    Ok(())
}

fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--media" => cli
                .media
                .push(parse_media_arg(&next_arg(&mut iter, "--media"))),
            "--rom" => cli.media.push(MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: PathBuf::from(next_arg(&mut iter, "--rom")),
            }),
            "--headless" => {}
            "--script" => {
                cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script")));
            }
            "--screenshot" => {
                cli.screenshot = Some(PathBuf::from(next_arg(&mut iter, "--screenshot")));
            }
            "--audio-capture" => {
                cli.audio_capture = Some(PathBuf::from(next_arg(&mut iter, "--audio-capture")));
            }
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames")
                    .parse()
                    .unwrap_or_else(|_| die("--frames requires a non-negative integer"));
            }
            "--assert-blargg" => {
                cli.assert_blargg = true;
            }
            "--smoke-root" => {
                cli.smoke_root = Some(PathBuf::from(next_arg(&mut iter, "--smoke-root")));
            }
            "--smoke-report" => {
                cli.smoke_report = Some(PathBuf::from(next_arg(&mut iter, "--smoke-report")));
            }
            "--smoke-screenshot-dir" => {
                cli.smoke_screenshot_dir =
                    Some(PathBuf::from(next_arg(&mut iter, "--smoke-screenshot-dir")));
            }
            "--battery-save" => {
                cli.battery_save = Some(PathBuf::from(next_arg(&mut iter, "--battery-save")));
            }
            "--no-battery-save" => {
                cli.no_battery_save = true;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn parse_media_arg(spec: &str) -> MediaArg {
    let (slot_and_kind, path) = spec
        .split_once('=')
        .unwrap_or_else(|| die("--media requires SLOT:KIND=PATH"));
    let (slot, kind) = slot_and_kind
        .split_once(':')
        .unwrap_or_else(|| die("--media requires SLOT:KIND=PATH"));
    if slot.is_empty() || kind.is_empty() || path.is_empty() {
        die("--media requires SLOT:KIND=PATH");
    }

    MediaArg {
        slot: slot.to_owned(),
        kind: parse_media_kind(kind),
        path: PathBuf::from(path),
    }
}

fn parse_media_kind(kind: &str) -> MediaKind {
    match kind {
        "cartridge" => MediaKind::Cartridge,
        "disk" => MediaKind::Disk,
        "optical" => MediaKind::Optical,
        "program" => MediaKind::Program,
        "snapshot" => MediaKind::Snapshot,
        "tape" => MediaKind::Tape,
        _ => die(&format!("unknown media kind: {kind}")),
    }
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a path or value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

fn run_cli(cli: Cli) -> Result<RunnerReport, String> {
    if cli.no_battery_save && cli.battery_save.is_some() {
        return Err("--battery-save conflicts with --no-battery-save".into());
    }
    if cli.media.is_empty() {
        return Err(
            "a cartridge image is required; use --rom or --media cartridge-1:cartridge=PATH".into(),
        );
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

    let media_storage = load_media_bytes(&cli.media)?;
    let mut media = MediaSet::new();
    for image in &media_storage {
        media.push(MediaImage::new(
            image.slot.clone(),
            image.kind,
            &image.bytes,
        ));
    }

    let machine = NesRuntime::blank(Model::NesNtsc);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, NES_FRAME_TICKS, NesSessionQueryProvider);
    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    let battery_save_path = resolve_battery_save_path(&cli);
    if let Some(path) = &battery_save_path {
        load_battery_save(session.machine_mut(), path, cli.battery_save.is_some())?;
    }

    let mut observations = Vec::new();
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

    if let Some(path) = &battery_save_path {
        write_battery_save(session.machine(), path)?;
    }

    let test_result = if cli.assert_blargg {
        Some(assert_blargg_result(&session)?)
    } else {
        None
    };

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        cartridge_loaded: session.machine().machine().is_some(),
        test_result,
    })
}

/// Resolve the battery-save sidecar path: an explicit `--battery-save`
/// wins; otherwise default to the cartridge ROM's path with a `.sav`
/// extension. `--no-battery-save` suppresses it entirely.
fn resolve_battery_save_path(cli: &Cli) -> Option<PathBuf> {
    if cli.no_battery_save {
        return None;
    }
    cli.battery_save.clone().or_else(|| {
        cli.media
            .iter()
            .find(|entry| {
                entry.slot == DEFAULT_CARTRIDGE_SLOT && entry.kind == MediaKind::Cartridge
            })
            .map(|entry| default_battery_save_path(&entry.path))
    })
}

fn default_battery_save_path(rom_path: &Path) -> PathBuf {
    let mut path = rom_path.to_path_buf();
    path.set_extension("sav");
    path
}

/// Load a `.sav` sidecar onto the cartridge's battery RAM. A missing file
/// is fine (first run); an explicit `--battery-save` on a non-battery cart
/// is an error, but the implicit default silently skips.
fn load_battery_save(runtime: &mut NesRuntime, path: &Path, explicit: bool) -> Result<(), String> {
    if !runtime.has_battery_backed_ram() {
        if explicit {
            return Err("loaded cartridge does not have battery-backed RAM".to_owned());
        }
        return Ok(());
    }

    match std::fs::read(path) {
        Ok(bytes) => runtime
            .restore_cartridge_ram(&bytes)
            .map_err(|err| format!("failed to restore battery save {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "failed to read battery save {}: {err}",
            path.display()
        )),
    }
}

fn write_battery_save(runtime: &NesRuntime, path: &Path) -> Result<(), String> {
    if !runtime.has_battery_backed_ram() {
        return Ok(());
    }
    let Some(ram) = runtime.cartridge_ram() else {
        return Ok(());
    };
    std::fs::write(path, ram)
        .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
}

fn assert_blargg_result(
    session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
) -> Result<BlarggTestResult, String> {
    let result = read_blargg_result(session)?;
    match result.status {
        0 => Ok(result),
        0x80 => Err(format!(
            "Blargg test is still running after the requested frames: {}",
            result.text.trim()
        )),
        0x81 => Err(format!(
            "Blargg test requested reset after the requested frames: {}",
            result.text.trim()
        )),
        status => Err(format!(
            "Blargg test failed with status {status}: {}",
            result.text.trim()
        )),
    }
}

fn read_blargg_result(
    session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
) -> Result<BlarggTestResult, String> {
    let signature = query_u8_array3(session, "test.blargg.signature")?;
    let valid = query_bool(session, "test.blargg.valid")?;
    if !valid {
        return Err(format!(
            "Blargg signature missing at $6001-$6003: {:02X} {:02X} {:02X}",
            signature[0], signature[1], signature[2]
        ));
    }

    let status = query_u8(session, "test.blargg.status")?;
    let text = query_string(session, "test.blargg.text")?;
    Ok(BlarggTestResult {
        kind: "blargg",
        status,
        signature,
        text,
        passed: status == 0,
    })
}

fn query_bool(
    session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
    path: &str,
) -> Result<bool, String> {
    session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_bool()
        .ok_or_else(|| format!("query {path} did not return a boolean"))
}

fn query_u8(
    session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
    path: &str,
) -> Result<u8, String> {
    let value = session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_u64()
        .ok_or_else(|| format!("query {path} did not return an integer"))?;
    u8::try_from(value).map_err(|_| format!("query {path} returned out-of-range byte {value}"))
}

fn query_u8_array3(
    session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
    path: &str,
) -> Result<[u8; 3], String> {
    let value = session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value;
    let array = value
        .as_array()
        .ok_or_else(|| format!("query {path} did not return an array"))?;
    if array.len() != 3 {
        return Err(format!(
            "query {path} returned {} bytes, expected 3",
            array.len()
        ));
    }

    let mut bytes = [0; 3];
    for (index, value) in array.iter().enumerate() {
        let byte = value
            .as_u64()
            .ok_or_else(|| format!("query {path} byte {index} was not an integer"))?;
        bytes[index] = u8::try_from(byte)
            .map_err(|_| format!("query {path} byte {index} was out of range: {byte}"))?;
    }
    Ok(bytes)
}

fn query_string(
    session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
    path: &str,
) -> Result<String, String> {
    session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("query {path} did not return a string"))
}

fn run_smoke_matrix(cli: &Cli) -> Result<SmokeMatrixReport, String> {
    let root = cli
        .smoke_root
        .as_deref()
        .ok_or_else(|| "--smoke-root is required".to_string())?;
    let frames = if cli.frames == 0 { 300 } else { cli.frames };
    let mut roms = Vec::new();
    collect_nes_roms(root, &mut roms)?;
    roms.sort();

    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }

    let mut rows = Vec::with_capacity(roms.len());
    for (index, rom) in roms.iter().enumerate() {
        let header = read_ines_header(rom).unwrap_or_default();
        let screenshot = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}.png", safe_stem(rom))));
        let result = run_cli(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom.clone(),
            }],
            screenshot: screenshot.clone(),
            audio_capture: None,
            script: None,
            frames,
            assert_blargg: cli.assert_blargg,
            smoke_root: None,
            smoke_report: None,
            smoke_screenshot_dir: None,
            battery_save: None,
            no_battery_save: false,
        });

        match result {
            Ok(report) => rows.push(SmokeMatrixRow {
                path: rom.display().to_string(),
                mapper: header.mapper,
                prg_banks: header.prg_banks,
                chr_banks: header.chr_banks,
                result: "ok".to_string(),
                time: Some(report.time),
                screenshot: screenshot.map(|path| path.display().to_string()),
                test_result: report.test_result,
                error: None,
            }),
            Err(error) => rows.push(SmokeMatrixRow {
                path: rom.display().to_string(),
                mapper: header.mapper,
                prg_banks: header.prg_banks,
                chr_banks: header.chr_banks,
                result: "error".to_string(),
                time: None,
                screenshot: None,
                test_result: None,
                error: Some(error),
            }),
        }
    }

    Ok(SmokeMatrixReport {
        rom_count: rows.len(),
        rows,
    })
}

fn collect_nes_roms(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("nes"))
        {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_nes_roms(&entry.path(), out)?;
    }
    Ok(())
}

fn read_ines_header(path: &Path) -> Result<InesHeaderSummary, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    if bytes.len() < 16 || &bytes[0..4] != b"NES\x1a" {
        return Ok(InesHeaderSummary::default());
    }
    let flags6 = bytes[6];
    let flags7 = bytes[7];
    let mapper = u16::from((flags7 & 0xF0) | (flags6 >> 4));
    Ok(InesHeaderSummary {
        mapper: Some(mapper),
        prg_banks: Some(bytes[4]),
        chr_banks: Some(bytes[5]),
    })
}

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("rom")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn load_media_bytes(entries: &[MediaArg]) -> Result<Vec<LoadedMedia>, String> {
    entries
        .iter()
        .map(|entry| {
            read_media_asset(&entry.path, entry.kind)
                .map(|loaded| LoadedMedia {
                    slot: entry.slot.clone(),
                    kind: entry.kind,
                    bytes: loaded.bytes,
                })
                .map_err(|err| {
                    format!(
                        "failed to read media {} from {}: {err}",
                        entry.slot,
                        entry.path.display()
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn minimal_ines() -> Vec<u8> {
        let mut prg = vec![0xea; 16 * 1024];
        prg[0x3ffc] = 0x00;
        prg[0x3ffd] = 0x80;
        let chr = vec![0u8; 8 * 1024];
        let mut data = vec![0u8; 16 + prg.len() + chr.len()];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = 1;
        data[5] = 1;
        data[16..16 + prg.len()].copy_from_slice(&prg);
        data[16 + prg.len()..].copy_from_slice(&chr);
        data
    }

    /// `minimal_ines` with the battery flag (flags6 bit 1) set, so the
    /// loaded NROM cart exposes battery-backed PRG-RAM at $6000-$7FFF.
    fn battery_ines() -> Vec<u8> {
        let mut data = minimal_ines();
        data[6] |= 0x02;
        data
    }

    fn blargg_ines(status: u8, text: &[u8]) -> Vec<u8> {
        let mut prg = vec![0xea; 16 * 1024];
        let mut cursor = 0usize;
        for (addr, value) in [
            (0x6001, 0xDE),
            (0x6002, 0xB0),
            (0x6003, 0x61),
            (0x6000, status),
        ] {
            emit_store(&mut prg, &mut cursor, addr, value);
        }
        for (index, &byte) in text.iter().enumerate() {
            emit_store(&mut prg, &mut cursor, 0x6004 + index as u16, byte);
        }
        emit_store(&mut prg, &mut cursor, 0x6004 + text.len() as u16, 0);
        let loop_addr = 0x8000 + cursor as u16;
        prg[cursor] = 0x4C;
        prg[cursor + 1] = (loop_addr & 0x00FF) as u8;
        prg[cursor + 2] = (loop_addr >> 8) as u8;

        prg[0x3ffc] = 0x00;
        prg[0x3ffd] = 0x80;
        let chr = vec![0u8; 8 * 1024];
        let mut data = vec![0u8; 16 + prg.len() + chr.len()];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = 1;
        data[5] = 1;
        data[16..16 + prg.len()].copy_from_slice(&prg);
        data[16 + prg.len()..].copy_from_slice(&chr);
        data
    }

    fn emit_store(prg: &mut [u8], cursor: &mut usize, addr: u16, value: u8) {
        prg[*cursor] = 0xA9;
        prg[*cursor + 1] = value;
        prg[*cursor + 2] = 0x8D;
        prg[*cursor + 3] = (addr & 0x00FF) as u8;
        prg[*cursor + 4] = (addr >> 8) as u8;
        *cursor += 5;
    }

    #[test]
    fn parse_cli_accepts_rom_and_capture_flags() {
        let cli = parse_cli([
            "--rom".to_string(),
            "demo.nes".to_string(),
            "--frames".to_string(),
            "12".to_string(),
            "--screenshot".to_string(),
            "frame.png".to_string(),
            "--audio-capture".to_string(),
            "audio.wav".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                media: vec![MediaArg {
                    slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                    kind: MediaKind::Cartridge,
                    path: PathBuf::from("demo.nes"),
                }],
                screenshot: Some(PathBuf::from("frame.png")),
                audio_capture: Some(PathBuf::from("audio.wav")),
                script: None,
                frames: 12,
                assert_blargg: false,
                smoke_root: None,
                smoke_report: None,
                smoke_screenshot_dir: None,
                battery_save: None,
                no_battery_save: false,
            }
        );
    }

    #[test]
    fn default_battery_save_path_replaces_rom_extension() {
        assert_eq!(
            default_battery_save_path(Path::new("zelda.nes")),
            PathBuf::from("zelda.sav")
        );
    }

    #[test]
    fn parse_cli_accepts_battery_save_controls() {
        let cli = parse_cli([
            "--rom".to_string(),
            "zelda.nes".to_string(),
            "--battery-save".to_string(),
            "zelda.sav".to_string(),
        ]);
        assert_eq!(cli.battery_save, Some(PathBuf::from("zelda.sav")));
        assert_eq!(
            resolve_battery_save_path(&cli),
            Some(PathBuf::from("zelda.sav"))
        );

        // `--no-battery-save` suppresses the implicit default sidecar.
        let cli = parse_cli([
            "--rom".to_string(),
            "zelda.nes".to_string(),
            "--no-battery-save".to_string(),
        ]);
        assert!(cli.no_battery_save);
        assert_eq!(resolve_battery_save_path(&cli), None);
    }

    #[test]
    fn run_loads_and_writes_battery_save() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!("emu198x-nes-{}-battery.nes", std::process::id()));
        let save_path = temp_dir.join(format!("emu198x-nes-{}-battery.sav", std::process::id()));
        let save = vec![0x5A; 0x2000];

        fs::write(&rom_path, battery_ines()).expect("temporary ROM write should succeed");
        fs::write(&save_path, &save).expect("temporary save write should succeed");

        let result = run_cli(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            screenshot: None,
            audio_capture: None,
            script: None,
            frames: 0,
            assert_blargg: false,
            smoke_root: None,
            smoke_report: None,
            smoke_screenshot_dir: None,
            battery_save: Some(save_path.clone()),
            no_battery_save: false,
        });

        assert!(
            result.is_ok(),
            "runner should preserve battery save: {result:?}"
        );
        assert_eq!(
            fs::read(&save_path).expect("battery save should be readable"),
            save,
            "the loaded .sav round-trips back to disk on exit"
        );

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(save_path);
    }

    #[test]
    fn run_can_capture_png_and_wav() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!("emu198x-nes-{}-capture.nes", std::process::id()));
        let screenshot_path =
            temp_dir.join(format!("emu198x-nes-{}-capture.png", std::process::id()));
        let audio_path = temp_dir.join(format!("emu198x-nes-{}-capture.wav", std::process::id()));

        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");

        let result = run_cli(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            screenshot: Some(screenshot_path.clone()),
            audio_capture: Some(audio_path.clone()),
            script: None,
            frames: 1,
            assert_blargg: false,
            smoke_root: None,
            smoke_report: None,
            smoke_screenshot_dir: None,
            battery_save: None,
            no_battery_save: false,
        });

        assert!(result.is_ok(), "runner should capture outputs: {result:?}");
        let png = fs::read(&screenshot_path).expect("screenshot should be written");
        let wav = fs::read(&audio_path).expect("wav should be written");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(screenshot_path);
        let _ = fs::remove_file(audio_path);
    }

    #[test]
    fn run_can_execute_shared_json_script() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!("emu198x-nes-{}-script.nes", std::process::id()));
        let script_path = temp_dir.join(format!("emu198x-nes-{}-steps.json", std::process::id()));

        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");
        fs::write(
            &script_path,
            r#"
            [
              {"action":"run_frames","frames":1},
              {"action":"query","path":"cartridge.loaded"}
            ]
            "#,
        )
        .expect("script fixture should write");

        let result = run_cli(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            screenshot: None,
            audio_capture: None,
            script: Some(script_path.clone()),
            frames: 0,
            assert_blargg: false,
            smoke_root: None,
            smoke_report: None,
            smoke_screenshot_dir: None,
            battery_save: None,
            no_battery_save: false,
        });

        assert!(result.is_ok(), "runner should execute script: {result:?}");
        let report = result.expect("script result should be available");
        assert_eq!(report.observations.len(), 2);

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(script_path);
    }

    #[test]
    fn run_smoke_matrix_reports_successful_rom() {
        let temp_dir =
            std::env::temp_dir().join(format!("emu198x-nes-{}-smoke", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("temporary smoke dir should be created");
        let rom_path = temp_dir.join("demo.nes");
        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");

        let report = run_smoke_matrix(&Cli {
            media: Vec::new(),
            screenshot: None,
            audio_capture: None,
            script: None,
            frames: 1,
            assert_blargg: false,
            smoke_root: Some(temp_dir.clone()),
            smoke_report: None,
            smoke_screenshot_dir: None,
            battery_save: None,
            no_battery_save: false,
        })
        .expect("smoke matrix should run");

        assert_eq!(report.rom_count, 1);
        assert_eq!(report.rows[0].mapper, Some(0));
        assert_eq!(report.rows[0].result, "ok");

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn run_can_assert_blargg_success() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!("emu198x-nes-{}-blargg.nes", std::process::id()));
        fs::write(&rom_path, blargg_ines(0, b"ok\n")).expect("temporary ROM write should succeed");

        let report = run_cli(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            screenshot: None,
            audio_capture: None,
            script: None,
            frames: 1,
            assert_blargg: true,
            smoke_root: None,
            smoke_report: None,
            smoke_screenshot_dir: None,
            battery_save: None,
            no_battery_save: false,
        })
        .expect("Blargg assertion should pass");

        let result = report.test_result.expect("test result should be reported");
        assert_eq!(result.status, 0);
        assert!(result.passed);
        assert_eq!(result.text, "ok\n");

        let _ = fs::remove_file(rom_path);
    }
}
