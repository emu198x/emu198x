//! The NES as a [`MachineApp`]: its flags, runtime, and report fields, plus
//! the two headless harnesses that ride on the shared loop — the Blargg
//! status assertion (`--assert-blargg`) and the smoke matrix (`--smoke-*`).

use std::fs;
use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, CommonCli, LaunchError, MachineApp, script_report};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::register_tools_for;
use emu198x_shell::query::SessionQueryProvider;
use emu198x_shell::{HeadlessSession, MachineCore, MediaKind, read_media_asset, startup_media};
use runtime_nintendo_nes::{Model, NesRuntime, NesSessionQueryProvider};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::mcp_tools::register_nes_tools;

const DEFAULT_CARTRIDGE_SLOT: &str = "cartridge-1";
/// PPU dots per NTSC frame — 341 dots × 262 lines.
pub const NES_FRAME_TICKS: u64 = 341 * 262;

/// One `--media SLOT:KIND=PATH` (or `--rom PATH`) from the command line.
#[derive(Debug, PartialEq, Eq)]
pub struct MediaArg {
    pub slot: String,
    pub kind: MediaKind,
    pub path: PathBuf,
}

impl MediaArg {
    fn cartridge(path: PathBuf) -> Self {
        Self {
            slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
            kind: MediaKind::Cartridge,
            path,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Nes {
    pub media: Vec<MediaArg>,
    /// `--assert-blargg`: fail the run unless the Blargg status at $6000 is 0.
    pub assert_blargg: bool,
    pub smoke_root: Option<PathBuf>,
    pub smoke_report: Option<PathBuf>,
    pub smoke_screenshot_dir: Option<PathBuf>,
    pub battery_save: Option<PathBuf>,
    pub no_battery_save: bool,
    /// Whether a bare ROM path (no flag) has been taken; only one is allowed.
    positional_rom: bool,
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
    test_result: Option<Value>,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
struct InesHeaderSummary {
    mapper: Option<u16>,
    prg_banks: Option<u8>,
    chr_banks: Option<u8>,
}

fn parse_media_arg(spec: &str) -> Result<MediaArg, LaunchError> {
    let malformed = || LaunchError::Usage("--media requires SLOT:KIND=PATH".to_owned());
    let (slot_and_kind, path) = spec.split_once('=').ok_or_else(malformed)?;
    let (slot, kind) = slot_and_kind.split_once(':').ok_or_else(malformed)?;
    if slot.is_empty() || kind.is_empty() || path.is_empty() {
        return Err(malformed());
    }
    Ok(MediaArg {
        slot: slot.to_owned(),
        kind: parse_media_kind(kind)?,
        path: PathBuf::from(path),
    })
}

fn parse_media_kind(kind: &str) -> Result<MediaKind, LaunchError> {
    Ok(match kind {
        "cartridge" => MediaKind::Cartridge,
        "disk" => MediaKind::Disk,
        "optical" => MediaKind::Optical,
        "program" => MediaKind::Program,
        "snapshot" => MediaKind::Snapshot,
        "tape" => MediaKind::Tape,
        _ => return Err(LaunchError::Usage(format!("unknown media kind: {kind}"))),
    })
}

/// Resolve the battery-save sidecar path: an explicit `--battery-save`
/// wins; otherwise default to the cartridge ROM's path with a `.sav`
/// extension. `--no-battery-save` suppresses it entirely.
pub(crate) fn resolve_battery_save_path(app: &Nes) -> Option<PathBuf> {
    if app.no_battery_save {
        return None;
    }
    app.battery_save.clone().or_else(|| {
        app.media
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
fn load_battery_save(
    runtime: &mut NesRuntime,
    path: &Path,
    explicit: bool,
) -> Result<(), LaunchError> {
    if !runtime.has_battery_backed_ram() {
        if explicit {
            return Err(LaunchError::Run(
                "loaded cartridge does not have battery-backed RAM".to_owned(),
            ));
        }
        return Ok(());
    }

    match fs::read(path) {
        Ok(bytes) => runtime.restore_cartridge_ram(&bytes).map_err(|err| {
            LaunchError::Run(format!(
                "failed to restore battery save {}: {err}",
                path.display()
            ))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(LaunchError::Run(format!(
            "failed to read battery save {}: {err}",
            path.display()
        ))),
    }
}

/// Persist the cartridge's battery PRG-RAM to its `.sav`.
pub(crate) fn write_battery_save(runtime: &NesRuntime, path: &Path) -> Result<(), String> {
    if !runtime.has_battery_backed_ram() {
        return Ok(());
    }
    let Some(ram) = runtime.cartridge_ram() else {
        return Ok(());
    };
    fs::write(path, ram)
        .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
}

fn load_media_bytes(
    entries: &[MediaArg],
) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
    entries
        .iter()
        .map(|entry| {
            read_media_asset(&entry.path, entry.kind)
                .map(|loaded| (entry.slot.clone(), entry.kind, loaded.bytes))
                .map_err(|err| {
                    LaunchError::Run(format!(
                        "failed to read media {} from {}: {err}",
                        entry.slot,
                        entry.path.display()
                    ))
                })
        })
        .collect()
}

impl MachineApp for Nes {
    type Runtime = NesRuntime;
    type Query = NesSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-nes";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    /// Headless-only flags; their presence routes to script mode, so
    /// `--rom x --assert-blargg` and `--smoke-root DIR` work without an
    /// explicit `--script`. `--rom` is shared with the window.
    const SCRIPT_FLAGS: &'static [&'static str] = &[
        "--media",
        "--assert-blargg",
        "--smoke-root",
        "--smoke-report",
        "--smoke-screenshot-dir",
    ];
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      iNES/NES 2.0 ROM image or zip containing one ROM candidate
                    (also accepted as a bare positional path)
    --media SLOT:KIND=PATH  media image by slot and kind; --rom is an
                    alias for --media cartridge-1:cartridge=PATH
    --battery-save PATH     load/write cartridge battery RAM sidecar (default <rom>.sav)
    --no-battery-save       disable automatic .sav load/write
    --assert-blargg assert Blargg-style status output at $6000 after the
                    run; a failing test exits non-zero
    --smoke-root PATH       recursively smoke every .nes ROM under PATH
                    (300 frames per ROM when --frames is 0)
    --smoke-report PATH     write smoke matrix JSON to PATH instead of stdout
    --smoke-screenshot-dir PATH
                    write one PNG per successful smoke row";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    Arrow keys      D-pad
    Z               B
    X               A
    Right Shift     Select
    Enter           Start
    1-5             toggle Pulse 1, Pulse 2, Triangle, Noise, DMC
    6-0             cycle Pulse 1, Pulse 2, Triangle, Noise, DMC gain";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--media" => self.media.push(parse_media_arg(&args.value(flag)?)?),
            "--rom" => self.media.push(MediaArg::cartridge(args.path(flag)?)),
            "--assert-blargg" => self.assert_blargg = true,
            "--smoke-root" => self.smoke_root = Some(args.path(flag)?),
            "--smoke-report" => self.smoke_report = Some(args.path(flag)?),
            "--smoke-screenshot-dir" => self.smoke_screenshot_dir = Some(args.path(flag)?),
            "--battery-save" => self.battery_save = Some(args.path(flag)?),
            "--no-battery-save" => self.no_battery_save = true,
            positional if !positional.starts_with('-') => {
                if self.positional_rom {
                    return Err(LaunchError::Usage(
                        "only one positional ROM path is supported".to_owned(),
                    ));
                }
                self.positional_rom = true;
                self.media
                    .push(MediaArg::cartridge(PathBuf::from(positional)));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        NES_FRAME_TICKS
    }

    fn query_provider(&self) -> NesSessionQueryProvider {
        NesSessionQueryProvider
    }

    /// Blank NTSC machine, then the media, then the battery `.sav`. The
    /// window and the headless loop both start from this.
    fn build_runtime(&self) -> Result<NesRuntime, LaunchError> {
        if self.no_battery_save && self.battery_save.is_some() {
            return Err(LaunchError::Run(
                "--battery-save conflicts with --no-battery-save".to_owned(),
            ));
        }
        if self.media.is_empty() {
            return Err(LaunchError::Run(
                "a cartridge image is required; use --rom or --media cartridge-1:cartridge=PATH"
                    .to_owned(),
            ));
        }

        let loaded = load_media_bytes(&self.media)?;
        let mut runtime = NesRuntime::blank(Model::NesNtsc);
        runtime
            .load_media(&startup_media::media_set(&loaded))
            .map_err(|err| LaunchError::Run(format!("machine preparation failed: {err}")))?;
        if let Some(path) = resolve_battery_save_path(self) {
            load_battery_save(&mut runtime, &path, self.battery_save.is_some())?;
        }
        Ok(runtime)
    }

    /// MCP starts blank; the cartridge arrives via `load_media`.
    fn build_mcp_runtime(&self) -> Result<NesRuntime, LaunchError> {
        Ok(NesRuntime::blank(Model::NesNtsc))
    }

    /// Write the battery save, then assert the Blargg status so a failing
    /// test ROM exits non-zero (CI sees a red step).
    fn after_run(
        &self,
        session: &mut HeadlessSession<NesRuntime, NesSessionQueryProvider>,
    ) -> Result<(), LaunchError> {
        if let Some(path) = resolve_battery_save_path(self) {
            write_battery_save(session.machine(), &path)?;
        }
        if self.assert_blargg {
            assert_blargg_result(session.machine())?;
        }
        Ok(())
    }

    fn report(&self, runtime: &NesRuntime, report: &mut Map<String, Value>) {
        report.insert(
            "cartridge_loaded".to_owned(),
            runtime.machine().is_some().into(),
        );
        // `after_run` already proved this reads; a failure here would be a
        // machine that changed between the two calls, so it is just omitted.
        if self.assert_blargg
            && let Ok(result) = read_blargg_result(runtime)
        {
            report.insert(
                "test_result".to_owned(),
                serde_json::to_value(result).unwrap_or_default(),
            );
        }
    }

    /// The smoke sweep replaces the loop; otherwise JSON when a script ran
    /// or a Blargg assertion was asked for, and a one-line summary for a
    /// bare frame run.
    fn run_script(&self, common: &CommonCli, _raw_args: &[String]) -> Result<(), LaunchError> {
        if self.smoke_root.is_some() {
            let report = run_smoke_matrix(self, common)?;
            let json = serde_json::to_string_pretty(&report).map_err(|err| {
                LaunchError::Run(format!("failed to serialize smoke matrix report: {err}"))
            })?;
            if let Some(path) = &self.smoke_report {
                fs::write(path, json.as_bytes()).map_err(|err| {
                    LaunchError::Run(format!("failed to write {}: {err}", path.display()))
                })?;
            } else {
                println!("{json}");
            }
            return Ok(());
        }

        let json_mode = common.script.is_some() || self.assert_blargg;
        let report = script_report(self, common)?;
        if json_mode {
            let json = serde_json::to_string(&report).map_err(|err| {
                LaunchError::Run(format!("failed to serialize runner report: {err}"))
            })?;
            println!("{json}");
        } else {
            println!(
                "NES runtime: time={} cartridge_loaded={}",
                report.get("time").and_then(Value::as_u64).unwrap_or(0),
                report
                    .get("cartridge_loaded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            );
        }
        Ok(())
    }

    /// The shared base surface — run/query/media/capture plus the generic
    /// 6502 debug verbs (`query_cpu`, `memory_read`, `disasm`, `step`,
    /// `poke_*`, `run_until_pc`, `run_until_any_pc`, `run_until_mem_change`)
    /// driven through the NES `DebugTarget` — and the NES-specific PPU
    /// dumps (`dump_palette`, `dump_oam`, `dump_nametable`). The debug verbs
    /// the NES once shadowed are served by the shared tier (RULES.md #30);
    /// the chip-register snapshots (`cpu` / `ppu` / `apu` / `mapper`) are
    /// folded query paths on the generic `query` tool, not bespoke tools
    /// (#456). No keyboard: the NES has none.
    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<NesRuntime, NesSessionQueryProvider>>,
        session: &HeadlessSession<NesRuntime, NesSessionQueryProvider>,
    ) {
        register_tools_for(registry, session);
        register_nes_tools(registry);
    }
}

fn assert_blargg_result(runtime: &NesRuntime) -> Result<BlarggTestResult, LaunchError> {
    let result = read_blargg_result(runtime)?;
    match result.status {
        0 => Ok(result),
        0x80 => Err(LaunchError::Run(format!(
            "Blargg test is still running after the requested frames: {}",
            result.text.trim()
        ))),
        0x81 => Err(LaunchError::Run(format!(
            "Blargg test requested reset after the requested frames: {}",
            result.text.trim()
        ))),
        status => Err(LaunchError::Run(format!(
            "Blargg test failed with status {status}: {}",
            result.text.trim()
        ))),
    }
}

fn read_blargg_result(runtime: &NesRuntime) -> Result<BlarggTestResult, LaunchError> {
    let signature = query_u8_array3(runtime, "test.blargg.signature")?;
    let valid = query_bool(runtime, "test.blargg.valid")?;
    if !valid {
        return Err(LaunchError::Run(format!(
            "Blargg signature missing at $6001-$6003: {:02X} {:02X} {:02X}",
            signature[0], signature[1], signature[2]
        )));
    }

    let status = query_u8(runtime, "test.blargg.status")?;
    let text = query_string(runtime, "test.blargg.text")?;
    Ok(BlarggTestResult {
        kind: "blargg",
        status,
        signature,
        text,
        passed: status == 0,
    })
}

/// One `test.blargg.*` value, served by the NES query provider straight
/// from the runtime.
fn query_value(runtime: &NesRuntime, path: &str) -> Result<Value, LaunchError> {
    NesSessionQueryProvider
        .query(runtime, path)
        .map_err(|err| LaunchError::Run(format!("failed to query {path}: {err}")))?
        .map(|result| result.value)
        .ok_or_else(|| LaunchError::Run(format!("failed to query {path}: unknown query path")))
}

fn query_bool(runtime: &NesRuntime, path: &str) -> Result<bool, LaunchError> {
    query_value(runtime, path)?
        .as_bool()
        .ok_or_else(|| LaunchError::Run(format!("query {path} did not return a boolean")))
}

fn query_u8(runtime: &NesRuntime, path: &str) -> Result<u8, LaunchError> {
    let value = query_value(runtime, path)?
        .as_u64()
        .ok_or_else(|| LaunchError::Run(format!("query {path} did not return an integer")))?;
    u8::try_from(value)
        .map_err(|_| LaunchError::Run(format!("query {path} returned out-of-range byte {value}")))
}

fn query_u8_array3(runtime: &NesRuntime, path: &str) -> Result<[u8; 3], LaunchError> {
    let value = query_value(runtime, path)?;
    let array = value
        .as_array()
        .ok_or_else(|| LaunchError::Run(format!("query {path} did not return an array")))?;
    if array.len() != 3 {
        return Err(LaunchError::Run(format!(
            "query {path} returned {} bytes, expected 3",
            array.len()
        )));
    }

    let mut bytes = [0; 3];
    for (index, value) in array.iter().enumerate() {
        let byte = value.as_u64().ok_or_else(|| {
            LaunchError::Run(format!("query {path} byte {index} was not an integer"))
        })?;
        bytes[index] = u8::try_from(byte).map_err(|_| {
            LaunchError::Run(format!(
                "query {path} byte {index} was out of range: {byte}"
            ))
        })?;
    }
    Ok(bytes)
}

fn query_string(runtime: &NesRuntime, path: &str) -> Result<String, LaunchError> {
    query_value(runtime, path)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| LaunchError::Run(format!("query {path} did not return a string")))
}

/// Run every `.nes` under `--smoke-root` through the shared loop, one
/// fresh machine each, and tabulate the outcomes. `--frames 0` means 300.
fn run_smoke_matrix(app: &Nes, common: &CommonCli) -> Result<SmokeMatrixReport, LaunchError> {
    let root = app
        .smoke_root
        .as_deref()
        .ok_or_else(|| LaunchError::Run("--smoke-root is required".to_owned()))?;
    let frames = if common.frames == 0 {
        300
    } else {
        common.frames
    };
    let mut roms = Vec::new();
    collect_nes_roms(root, &mut roms)?;
    roms.sort();

    if let Some(dir) = &app.smoke_screenshot_dir {
        fs::create_dir_all(dir).map_err(|err| {
            LaunchError::Run(format!("failed to create {}: {err}", dir.display()))
        })?;
    }

    let mut rows = Vec::with_capacity(roms.len());
    for (index, rom) in roms.iter().enumerate() {
        let header = read_ines_header(rom).unwrap_or_default();
        let screenshot = app
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}.png", safe_stem(rom))));
        let result = script_report(
            &Nes {
                media: vec![MediaArg::cartridge(rom.clone())],
                assert_blargg: app.assert_blargg,
                ..Nes::default()
            },
            &CommonCli {
                frames,
                screenshot: screenshot.clone(),
                ..CommonCli::default()
            },
        );

        match result {
            Ok(report) => rows.push(SmokeMatrixRow {
                path: rom.display().to_string(),
                mapper: header.mapper,
                prg_banks: header.prg_banks,
                chr_banks: header.chr_banks,
                result: "ok".to_string(),
                time: report.get("time").and_then(Value::as_u64),
                screenshot: screenshot.map(|path| path.display().to_string()),
                test_result: report.get("test_result").cloned(),
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
                error: Some(error.to_string()),
            }),
        }
    }

    Ok(SmokeMatrixReport {
        rom_count: rows.len(),
        rows,
    })
}

fn collect_nes_roms(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), LaunchError> {
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

    for entry in fs::read_dir(path)
        .map_err(|err| LaunchError::Run(format!("failed to read {}: {err}", path.display())))?
    {
        let entry = entry.map_err(|err| {
            LaunchError::Run(format!(
                "failed to read entry under {}: {err}",
                path.display()
            ))
        })?;
        collect_nes_roms(&entry.path(), out)?;
    }
    Ok(())
}

fn read_ines_header(path: &Path) -> Result<InesHeaderSummary, LaunchError> {
    let bytes = fs::read(path)
        .map_err(|err| LaunchError::Run(format!("failed to read {}: {err}", path.display())))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn parsed(list: &[&str]) -> (Nes, CommonCli, Mode) {
        match parse::<Nes>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    fn cartridge(path: &Path) -> Nes {
        Nes {
            media: vec![MediaArg::cartridge(path.to_path_buf())],
            ..Nes::default()
        }
    }

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
    fn flags_set_rom_and_capture_flags() {
        let (app, common, mode) = parsed(&[
            "--rom",
            "demo.nes",
            "--frames",
            "12",
            "--screenshot",
            "frame.png",
            "--audio-capture",
            "audio.wav",
        ]);
        assert_eq!(
            app.media,
            vec![MediaArg::cartridge(PathBuf::from("demo.nes"))]
        );
        assert!(!app.assert_blargg);
        assert!(app.smoke_root.is_none());
        assert_eq!(common.frames, 12);
        assert_eq!(common.screenshot, Some(PathBuf::from("frame.png")));
        assert_eq!(common.audio_capture, Some(PathBuf::from("audio.wav")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn headless_only_flags_route_to_script_mode() {
        for flags in [
            &["--media", "cartridge-1:cartridge=game.nes"][..],
            &["--rom", "game.nes", "--assert-blargg"],
            &["--smoke-root", "roms"],
            &["--smoke-root", "roms", "--smoke-report", "matrix.json"],
            &["--smoke-root", "roms", "--smoke-screenshot-dir", "shots"],
        ] {
            let (.., mode) = parsed(flags);
            assert_eq!(mode, Mode::Script, "{flags:?} should be script");
        }
    }

    #[test]
    fn a_positional_rom_opens_the_window() {
        let (app, common, mode) = parsed(&["--scale", "2", "game.nes"]);
        assert_eq!(
            app.media,
            vec![MediaArg::cartridge(PathBuf::from("game.nes"))]
        );
        assert_eq!(common.scale, Some(2));
        assert_eq!(mode, Mode::Ui);

        let err = parse::<Nes>(&args(&["a.nes", "b.nes"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("only one positional ROM path is supported".to_owned())
        );
    }

    #[test]
    fn smoke_and_blargg_flags_parse() {
        let (app, ..) = parsed(&[
            "--smoke-root",
            "roms",
            "--smoke-report",
            "matrix.json",
            "--smoke-screenshot-dir",
            "shots",
            "--assert-blargg",
        ]);
        assert_eq!(app.smoke_root, Some(PathBuf::from("roms")));
        assert_eq!(app.smoke_report, Some(PathBuf::from("matrix.json")));
        assert_eq!(app.smoke_screenshot_dir, Some(PathBuf::from("shots")));
        assert!(app.assert_blargg);
    }

    #[test]
    fn default_battery_save_path_replaces_rom_extension() {
        assert_eq!(
            default_battery_save_path(Path::new("zelda.nes")),
            PathBuf::from("zelda.sav")
        );
    }

    #[test]
    fn battery_save_controls_resolve_the_sidecar() {
        let (app, ..) = parsed(&["--rom", "zelda.nes", "--battery-save", "zelda.sav"]);
        assert_eq!(app.battery_save, Some(PathBuf::from("zelda.sav")));
        assert_eq!(
            resolve_battery_save_path(&app),
            Some(PathBuf::from("zelda.sav"))
        );

        // `--no-battery-save` suppresses the implicit default sidecar.
        let (app, ..) = parsed(&["--rom", "zelda.nes", "--no-battery-save"]);
        assert!(app.no_battery_save);
        assert_eq!(resolve_battery_save_path(&app), None);

        // Default: <rom>.sav.
        let (app, ..) = parsed(&["game.nes"]);
        assert_eq!(
            resolve_battery_save_path(&app),
            Some(PathBuf::from("game.sav"))
        );
    }

    #[test]
    fn run_loads_and_writes_battery_save() {
        let temp_dir = std::env::temp_dir();
        let stem = format!("emu198x-nes-{}-battery", std::process::id());
        let rom_path = temp_dir.join(format!("{stem}.nes"));
        let save_path = temp_dir.join(format!("{stem}.sav"));
        let save = vec![0x5A; 0x2000];

        fs::write(&rom_path, battery_ines()).expect("temporary ROM write should succeed");
        fs::write(&save_path, &save).expect("temporary save write should succeed");

        let result = script_report(
            &Nes {
                battery_save: Some(save_path.clone()),
                ..cartridge(&rom_path)
            },
            &CommonCli::default(),
        );

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
        let stem = format!("emu198x-nes-{}-capture", std::process::id());
        let rom_path = temp_dir.join(format!("{stem}.nes"));
        let screenshot_path = temp_dir.join(format!("{stem}.png"));
        let audio_path = temp_dir.join(format!("{stem}.wav"));

        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");

        let result = script_report(
            &cartridge(&rom_path),
            &CommonCli {
                frames: 1,
                screenshot: Some(screenshot_path.clone()),
                audio_capture: Some(audio_path.clone()),
                ..CommonCli::default()
            },
        );

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
        let stem = format!("emu198x-nes-{}-script", std::process::id());
        let rom_path = temp_dir.join(format!("{stem}.nes"));
        let script_path = temp_dir.join(format!("{stem}.json"));

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

        let result = script_report(
            &cartridge(&rom_path),
            &CommonCli {
                script: Some(script_path.clone()),
                ..CommonCli::default()
            },
        );

        assert!(result.is_ok(), "runner should execute script: {result:?}");
        let report = result.expect("script result should be available");
        // The two script steps; the launcher may append a blank-frame note.
        let script_steps = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .filter(|observation| observation["kind"] != "blank_frame")
            .count();
        assert_eq!(script_steps, 2);

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

        let report = run_smoke_matrix(
            &Nes {
                smoke_root: Some(temp_dir.clone()),
                ..Nes::default()
            },
            &CommonCli {
                frames: 1,
                ..CommonCli::default()
            },
        )
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

        let report = script_report(
            &Nes {
                assert_blargg: true,
                ..cartridge(&rom_path)
            },
            &CommonCli {
                frames: 1,
                ..CommonCli::default()
            },
        )
        .expect("Blargg assertion should pass");

        let result = &report["test_result"];
        assert_eq!(result["status"], 0);
        assert_eq!(result["passed"], true);
        assert_eq!(result["text"], "ok\n");

        let _ = fs::remove_file(rom_path);
    }

    #[test]
    fn a_missing_blargg_signature_fails_the_run() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!("emu198x-nes-{}-noblargg.nes", std::process::id()));
        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");

        let err = script_report(
            &Nes {
                assert_blargg: true,
                ..cartridge(&rom_path)
            },
            &CommonCli {
                frames: 1,
                ..CommonCli::default()
            },
        )
        .expect_err("a ROM without the signature must fail the assertion");
        assert!(
            matches!(err, LaunchError::Run(message) if message.starts_with("Blargg signature missing"))
        );

        let _ = fs::remove_file(rom_path);
    }
}
