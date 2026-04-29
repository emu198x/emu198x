//! Minimal Dragon 32 ROM bring-up harness.
//!
//! This is deliberately smaller than the full machine/runtime path. It gives us
//! an executable ROM/CPU loop while PIA, SAM, and VDG are still being rebuilt.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

use emu198x_shell::{
    CapturedFrame, HeadlessSession, InputEvent, MachineTime, MediaImage, MediaKind, MediaSet,
    PixelFormat, read_media_asset,
};
use format_dragon_cas::{CasFileType, CasHeader, CasImage, parse_cas_tolerant};
use format_dragon_pak::{
    DragonCartridgeKind as ParsedDragonCartridgeKind, DragonPakImage, PcDragonPeripherals,
    PcDragonSnapshot, parse_dragon_pak, parse_pcdragon_snapshot,
};
use machine_dragon_32::{
    AddressRange, CpuInterruptAcceptTrace, CpuInterruptKind, CpuInterruptLineTrace,
    CpuRegisterTrace, DRAGON_CPU_HZ, DRAGON_FRAME_CYCLES, DeviceAccess, DeviceRegion, Dragon32,
    DragonCartridgeKind, DragonKey, DragonKeyboard, FetchTrace, MatrixKey, MemoryWriteTrace,
    PiaSignalTrace, ROM_SIZE, ReadonlyWrite, RunOptions, RunReport, StopReason, VdgSampleTrace,
    VdgModeWriteTrace, WatchedFetchTrace,
};
use motorola_vdg_6847::{
    TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TextPalette,
};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};
use serde::Serialize;
use zip::ZipArchive;

const DEFAULT_CYCLES: u64 = 100_000;
const DEFAULT_TRACE_LIMIT: usize = 64;
const DEFAULT_SMOKE_RUN_LIMIT: usize = 8;
const DEFAULT_XROAR_SETTLE_SECONDS: f32 = 3.0;
const DEFAULT_XROAR_TIMEOUT_SECONDS: f32 = 45.0;
const XROAR_ZOOMED_WIDTH: u32 = 512;
const XROAR_ZOOMED_HEIGHT: u32 = 384;
const BOOT_FRAME_BUDGET: u32 = 100;
const KEY_EDGE_FRAMES: u32 = 8;
const SMOKE_START_SETTLE_FRAMES: u32 = 60;

const USAGE: &str = "\
Usage: emu198x-script-dragon --rom PATH [OPTIONS]

Firmware:
    --rom PATH          Dragon 32 BASIC ROM, exactly 16 KiB; .zip archives are accepted
    --cart PATH         Dragon cartridge ROM/DGN image; .zip archives are accepted
    --snapshot PATH     PC-Dragon PAK snapshot; .zip archives are accepted

Execution:
    --cycles N         maximum MC6809 bus cycles to run [default: 100000]
    --trace-limit N    number of recent instruction fetches to retain [default: 64]
    --watch-fetch A[-B]
                       retain opcode fetches in inclusive hex/decimal address range A..B
    --watch-write A[-B]
                       retain bus writes to inclusive hex/decimal address range A..B
    --press KEY        hold a named Dragon key closed; may be repeated
    --press-matrix R,C hold a raw keyboard matrix switch closed; may be repeated
    --dump-text        print the current 32x16 MC6847 text snapshot
    --dump-text-png P  write the current border-inclusive MC6847 text framebuffer as a PNG
    --screenshot P     write the current border-inclusive MC6847 framebuffer as a PNG
    --screenshot-format FORMAT
                       screenshot format: diagnostic | xroar-zoomed [default: diagnostic]
    --screenshot-phase PHASE
                       screenshot capture phase: immediate | completed-frame [default: immediate]
    --smoke-root PATH  recursively scan .cas/.zip Dragon tape images
    --snapshot-smoke-root PATH
                       recursively scan .pak/.zip PC-Dragon snapshots
    --smoke-run-limit N
                       run real-ROM CLOAD/CLOADM or snapshot smoke for first N parsed media [default: 8]
    --smoke-report P   write smoke matrix JSON to PATH
    --smoke-screenshot-dir PATH
                       write load/start screenshots for runtime-smoked tapes
    --smoke-screenshot-format FORMAT
                       screenshot format: diagnostic | xroar-zoomed [default: diagnostic]
    --smoke-audio-dir PATH
                       write load/start WAV audio captures for runtime-smoked tapes
    --smoke-joystick PORT,CONTROL,FRAMES
                       after start, hold joystick control on port 1/2 for N frames;
                       CONTROL is up, down, left, right, fire, or idle; may be repeated
    --smoke-idle-after-start FRAMES
                       after start, run N frames without extra input and capture idle output
    --xroar-bin PATH   patched XRoar binary used to write reference PNGs
    --xroar-reference-dir PATH
                       write patched-XRoar reference PNGs for runtime-smoked media
    --xroar-snapshot-out PATH
                       write the synthetic XRoar v2 snapshot used for reference comparison
    --xroar-motoroff N capture CAS reference on the Nth tape motor-off [default: auto]
    --xroar-settle-seconds N
                       wait N emulated seconds after CAS reference trigger before capture [default: 3];
                       snapshot references instead use the local screenshot cycle count
    --xroar-timeout-seconds N
                       hard XRoar run timeout in emulated seconds [default: 45]

Other:
    --help             print this help text
";

#[derive(Debug, Clone, PartialEq)]
struct Cli {
    rom: PathBuf,
    cart: Option<PathBuf>,
    snapshot: Option<PathBuf>,
    cycles: u64,
    trace_limit: usize,
    fetch_watch: Option<AddressRange>,
    write_watch: Option<AddressRange>,
    pressed_keys: Vec<MatrixKey>,
    dump_text: bool,
    dump_text_png: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    screenshot_format: SmokeScreenshotFormat,
    screenshot_phase: SmokeScreenshotPhase,
    smoke_root: Option<PathBuf>,
    snapshot_smoke_root: Option<PathBuf>,
    smoke_run_limit: usize,
    smoke_report: Option<PathBuf>,
    smoke_screenshot_dir: Option<PathBuf>,
    smoke_screenshot_format: SmokeScreenshotFormat,
    smoke_audio_dir: Option<PathBuf>,
    smoke_joystick: Vec<SmokeJoystickStep>,
    smoke_idle_after_start: u32,
    xroar_bin: Option<PathBuf>,
    xroar_reference_dir: Option<PathBuf>,
    xroar_snapshot_out: Option<PathBuf>,
    xroar_motoroff: Option<usize>,
    xroar_settle_seconds: f32,
    xroar_timeout_seconds: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessReport {
    stop_reason: StopReason,
    cycles: u64,
    instructions: u64,
    pc: u16,
    addr: u16,
    rw: bool,
    last_fetch: Option<FetchTrace>,
    trace: Vec<FetchTrace>,
    dropped_trace: usize,
    device_accesses: Vec<DeviceAccess>,
    dropped_device_accesses: usize,
    readonly_writes: Vec<ReadonlyWrite>,
    dropped_readonly_writes: usize,
    watched_fetches: Vec<WatchedFetchTrace>,
    dropped_watched_fetches: usize,
    watched_writes: Vec<MemoryWriteTrace>,
    dropped_watched_writes: usize,
    pia_signals: Vec<PiaSignalTrace>,
    dropped_pia_signals: usize,
    interrupt_lines: Vec<CpuInterruptLineTrace>,
    dropped_interrupt_lines: usize,
    interrupt_accepts: Vec<CpuInterruptAcceptTrace>,
    dropped_interrupt_accepts: usize,
    vdg_samples: Vec<VdgSampleTrace>,
    dropped_vdg_samples: usize,
    vdg_mode_writes: Vec<VdgModeWriteTrace>,
    dropped_vdg_mode_writes: usize,
    text_screen_base: u16,
    text_screen: Option<String>,
    text_framebuffer: Option<Vec<u32>>,
    framebuffer: Option<Vec<u32>>,
    framebuffer_cycles: Option<u64>,
}

#[derive(Debug, Serialize)]
struct SmokeMatrixReport {
    tape_count: usize,
    runtime_smokes: usize,
    rows: Vec<SmokeMatrixRow>,
}

#[derive(Debug, Serialize)]
struct SmokeMatrixRow {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_member: Option<String>,
    parse_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksums_valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored_segments: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    header: Option<CasHeaderSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<CasRuntimeSmoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SnapshotSmokeMatrixReport {
    snapshot_count: usize,
    runtime_smokes: usize,
    rows: Vec<SnapshotSmokeRow>,
}

#[derive(Debug, Serialize)]
struct SnapshotSmokeRow {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_member: Option<String>,
    parse_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<SnapshotRuntimeSmoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SnapshotRuntimeSmoke {
    classification: SnapshotSmokeClassification,
    stop_reason: String,
    cycles: u64,
    instructions: u64,
    pc: u16,
    load_address: u16,
    ram_len: usize,
    text_screen_base: u16,
    distinct_colors: usize,
    non_background_pixels: usize,
    screen_text: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_cycles: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_frame_phase_cycles: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_settle_seconds: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison: Option<ImageComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vdg_trace: Option<VdgTraceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct VdgTraceSummary {
    dropped_samples: usize,
    dropped_mode_writes: usize,
    dropped_device_accesses: usize,
    samples: Vec<VdgSampleSummary>,
    mode_writes: Vec<VdgModeWriteSummary>,
}

#[derive(Debug, Serialize)]
struct VdgSampleSummary {
    cycle: u64,
    frame_master_tick: u64,
    line: usize,
    active_y: usize,
    byte_x: usize,
    display_base: u16,
    sam_video_mode: u8,
    sam_display_offset: u8,
    pia1_pb: u8,
    graphics: bool,
    css: bool,
    int_ext: bool,
    gm: u8,
}

#[derive(Debug, Serialize)]
struct VdgModeWriteSummary {
    cycle: u64,
    frame_master_tick: u64,
    line: Option<usize>,
    active_y: Option<usize>,
    active_x: Option<usize>,
    addr: u16,
    value: u8,
    graphics: bool,
    css: bool,
    int_ext: bool,
    gm: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SnapshotSmokeClassification {
    RunningVisible,
    RunningBlank,
    HaltedVisible,
    HaltedBlank,
    Error,
}

#[derive(Debug, Serialize)]
struct CasHeaderSummary {
    name: String,
    file_type: String,
    ascii_flag: u8,
    gap_flag: u8,
    first_address: u16,
    second_address: u16,
}

#[derive(Debug, Serialize)]
struct CasRuntimeSmoke {
    command: String,
    classification: RuntimeSmokeClassification,
    load_result: String,
    start_command: String,
    start_result: String,
    load_visible_change: bool,
    load_basic_error: bool,
    load_pc_before: u16,
    load_pc_after: u16,
    start_pc_after: u16,
    load_video: DragonVideoState,
    start_video: DragonVideoState,
    tape_position_bits: u64,
    tape_length_bits: u64,
    tape_finished: bool,
    visible_change_after_start: bool,
    start_video_changed: bool,
    start_settle_visible_change: bool,
    basic_error: bool,
    load_screen_text: Vec<String>,
    screen_text: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_audio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_audio: Option<String>,
    idle_after_start_frames: u32,
    idle_visible_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    idle_screen_text: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idle_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    joystick_steps: Vec<SmokeJoystickStep>,
    joystick_visible_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    joystick_screen_text: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joystick_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_motoroff: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison: Option<ImageComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ImageComparison {
    emu_width: u32,
    emu_height: u32,
    reference_width: u32,
    reference_height: u32,
    dimensions_match: bool,
    compared_pixels: u64,
    differing_pixels: u64,
    differing_pixel_percent: f64,
    max_channel_delta: u8,
    mean_abs_channel_delta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum RuntimeSmokeClassification {
    Error,
    LoadBasicError,
    LoadedNoVisibleChange,
    LoadedVisibleChange,
    MachineCodeRunningAfterLoad,
    StartedBasicError,
    StartedNoVisibleChange,
    StartedTextDrawing,
    StartedVideoControlChanged,
    StartedGraphicsBlank,
    StartedGraphicsDrawing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct DragonVideoState {
    sam_video_mode: u8,
    sam_display_offset: u8,
    display_base: u16,
    pia1_output_b: u8,
    pia1_ddr_b: u8,
    pia1_control_b: u8,
    pia1_cb2: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct XroarReferenceConfig {
    bin: PathBuf,
    output_dir: PathBuf,
    motoroff: Option<usize>,
    settle_seconds: f32,
    timeout_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeScreenshotFormat {
    Diagnostic,
    XroarZoomed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeScreenshotPhase {
    Immediate,
    CompletedFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct SmokeJoystickStep {
    port: u8,
    control: SmokeJoystickControl,
    frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SmokeJoystickControl {
    Up,
    Down,
    Left,
    Right,
    Fire,
    Idle,
}

impl SmokeJoystickControl {
    const fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Fire => "fire",
            Self::Idle => "idle",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RuntimeSmokeOptions<'a> {
    run_limit: usize,
    screenshot_stem: Option<&'a Path>,
    screenshot_format: SmokeScreenshotFormat,
    audio_stem: Option<&'a Path>,
    joystick: &'a [SmokeJoystickStep],
    idle_after_start_frames: u32,
    xroar: Option<&'a XroarReferenceConfig>,
    xroar_stem: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug)]
struct SnapshotSmokeOptions<'a> {
    run_limit: usize,
    screenshot_path: Option<&'a Path>,
    screenshot_format: SmokeScreenshotFormat,
    screenshot_phase: SmokeScreenshotPhase,
    cycle_limit: u64,
    trace_limit: usize,
    xroar: Option<&'a XroarReferenceConfig>,
    xroar_stem: Option<&'a Path>,
}

fn main() {
    if let Err(err) = run_main() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run_main() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }

    let cli = parse_cli(args)?;
    let rom = load_rom(&cli.rom)?;
    let cart = cli
        .cart
        .as_ref()
        .map(|path| load_cartridge(path))
        .transpose()?;
    let snapshot = cli
        .snapshot
        .as_ref()
        .map(|path| load_snapshot(path))
        .transpose()?;
    if let Some(path) = &cli.xroar_snapshot_out {
        let snapshot = snapshot
            .as_ref()
            .ok_or_else(|| "--xroar-snapshot-out requires --snapshot".to_owned())?;
        write_xroar_snapshot_out(&cli, snapshot, path)?;
        println!("xroar snapshot: {}", path.display());
    }
    if cli.smoke_root.is_some() {
        let report = run_smoke_matrix(&cli, &rom)?;
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
    if cli.snapshot_smoke_root.is_some() {
        let report = run_snapshot_smoke_matrix(&cli, &rom)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to serialize snapshot smoke report: {err}"))?;
        if let Some(path) = &cli.smoke_report {
            fs::write(path, json.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }

    let keyboard =
        DragonKeyboard::with_pressed_keys(&cli.pressed_keys).map_err(|err| err.to_string())?;
    let report = run_harness_with_keyboard(
        &rom,
        keyboard,
        HarnessRunOptions {
            cartridge: cart.as_ref(),
            snapshot: snapshot.as_ref(),
            cycle_limit: cli.cycles,
            trace_limit: cli.trace_limit,
            fetch_watch: cli.fetch_watch,
            write_watch: cli.write_watch,
            dump_text: cli.dump_text,
            dump_text_framebuffer: cli.dump_text_png.is_some(),
            capture_framebuffer: cli.screenshot.is_some(),
            capture_framebuffer_phase: cli.screenshot_phase,
        },
    );
    print_report(&report);
    if let Some(path) = &cli.dump_text_png {
        let framebuffer = report
            .text_framebuffer
            .as_deref()
            .ok_or_else(|| "text framebuffer was not captured".to_owned())?;
        write_text_png(path, framebuffer)?;
        println!("text png: {}", path.display());
    }
    if let Some(path) = &cli.screenshot {
        let framebuffer = report
            .framebuffer
            .as_deref()
            .ok_or_else(|| "framebuffer was not captured".to_owned())?;
        write_screenshot_png(
            path,
            framebuffer,
            cli.screenshot_format,
            report.framebuffer_cycles.unwrap_or(report.cycles),
        )?;
        println!("screenshot: {}", path.display());
    }
    Ok(())
}

fn parse_cli<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut rom = None;
    let mut cart = None;
    let mut snapshot = None;
    let mut cycles = DEFAULT_CYCLES;
    let mut trace_limit = DEFAULT_TRACE_LIMIT;
    let mut fetch_watch = None;
    let mut write_watch = None;
    let mut pressed_keys = Vec::new();
    let mut dump_text = false;
    let mut dump_text_png = None;
    let mut screenshot = None;
    let mut screenshot_format = SmokeScreenshotFormat::Diagnostic;
    let mut screenshot_phase = SmokeScreenshotPhase::Immediate;
    let mut smoke_root = None;
    let mut snapshot_smoke_root = None;
    let mut smoke_run_limit = DEFAULT_SMOKE_RUN_LIMIT;
    let mut smoke_report = None;
    let mut smoke_screenshot_dir = None;
    let mut smoke_screenshot_format = SmokeScreenshotFormat::Diagnostic;
    let mut smoke_audio_dir = None;
    let mut smoke_joystick = Vec::new();
    let mut smoke_idle_after_start = 0;
    let mut xroar_bin = None;
    let mut xroar_reference_dir = None;
    let mut xroar_snapshot_out = None;
    let mut xroar_motoroff = None;
    let mut xroar_settle_seconds = DEFAULT_XROAR_SETTLE_SECONDS;
    let mut xroar_timeout_seconds = DEFAULT_XROAR_TIMEOUT_SECONDS;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => {
                rom = Some(PathBuf::from(next_value(&mut iter, "--rom")?));
            }
            "--cart" => {
                cart = Some(PathBuf::from(next_value(&mut iter, "--cart")?));
            }
            "--snapshot" => {
                snapshot = Some(PathBuf::from(next_value(&mut iter, "--snapshot")?));
            }
            "--cycles" => {
                cycles = parse_u64(&next_value(&mut iter, "--cycles")?, "--cycles")?;
            }
            "--trace-limit" => {
                trace_limit =
                    parse_usize(&next_value(&mut iter, "--trace-limit")?, "--trace-limit")?;
            }
            "--watch-fetch" => {
                fetch_watch = Some(parse_address_range(
                    &next_value(&mut iter, "--watch-fetch")?,
                    "--watch-fetch",
                )?);
            }
            "--watch-write" => {
                write_watch = Some(parse_address_range(
                    &next_value(&mut iter, "--watch-write")?,
                    "--watch-write",
                )?);
            }
            "--press" => {
                let key = parse_dragon_key(&next_value(&mut iter, "--press")?)?;
                pressed_keys.push(MatrixKey::from_dragon_key(key));
            }
            "--press-matrix" => {
                pressed_keys.push(parse_matrix_key(&next_value(&mut iter, "--press-matrix")?)?);
            }
            "--dump-text" => {
                dump_text = true;
            }
            "--dump-text-png" => {
                dump_text_png = Some(PathBuf::from(next_value(&mut iter, "--dump-text-png")?));
            }
            "--screenshot" => {
                screenshot = Some(PathBuf::from(next_value(&mut iter, "--screenshot")?));
            }
            "--screenshot-format" => {
                screenshot_format = parse_screenshot_format(
                    &next_value(&mut iter, "--screenshot-format")?,
                    "--screenshot-format",
                )?;
            }
            "--screenshot-phase" => {
                screenshot_phase = parse_screenshot_phase(
                    &next_value(&mut iter, "--screenshot-phase")?,
                    "--screenshot-phase",
                )?;
            }
            "--smoke-root" => {
                smoke_root = Some(PathBuf::from(next_value(&mut iter, "--smoke-root")?));
            }
            "--snapshot-smoke-root" => {
                snapshot_smoke_root = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--snapshot-smoke-root",
                )?));
            }
            "--smoke-run-limit" => {
                smoke_run_limit = parse_usize(
                    &next_value(&mut iter, "--smoke-run-limit")?,
                    "--smoke-run-limit",
                )?;
            }
            "--smoke-report" => {
                smoke_report = Some(PathBuf::from(next_value(&mut iter, "--smoke-report")?));
            }
            "--smoke-screenshot-dir" => {
                smoke_screenshot_dir = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--smoke-screenshot-dir",
                )?));
            }
            "--smoke-screenshot-format" => {
                smoke_screenshot_format = parse_screenshot_format(
                    &next_value(&mut iter, "--smoke-screenshot-format")?,
                    "--smoke-screenshot-format",
                )?;
            }
            "--smoke-audio-dir" => {
                smoke_audio_dir = Some(PathBuf::from(next_value(&mut iter, "--smoke-audio-dir")?));
            }
            "--smoke-joystick" => {
                smoke_joystick.push(parse_smoke_joystick_step(&next_value(
                    &mut iter,
                    "--smoke-joystick",
                )?)?);
            }
            "--smoke-idle-after-start" => {
                smoke_idle_after_start = parse_u32(
                    &next_value(&mut iter, "--smoke-idle-after-start")?,
                    "--smoke-idle-after-start",
                )?;
            }
            "--xroar-bin" => {
                xroar_bin = Some(PathBuf::from(next_value(&mut iter, "--xroar-bin")?));
            }
            "--xroar-reference-dir" => {
                xroar_reference_dir = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--xroar-reference-dir",
                )?));
            }
            "--xroar-snapshot-out" => {
                xroar_snapshot_out = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--xroar-snapshot-out",
                )?));
            }
            "--xroar-motoroff" => {
                xroar_motoroff = Some(parse_usize(
                    &next_value(&mut iter, "--xroar-motoroff")?,
                    "--xroar-motoroff",
                )?);
            }
            "--xroar-settle-seconds" => {
                xroar_settle_seconds = parse_f32(
                    &next_value(&mut iter, "--xroar-settle-seconds")?,
                    "--xroar-settle-seconds",
                )?;
            }
            "--xroar-timeout-seconds" => {
                xroar_timeout_seconds = parse_f32(
                    &next_value(&mut iter, "--xroar-timeout-seconds")?,
                    "--xroar-timeout-seconds",
                )?;
            }
            "--help" | "-h" => return Err(USAGE.to_owned()),
            _ => return Err(format!("unknown argument: {arg}\n\n{USAGE}")),
        }
    }

    if smoke_root.is_some() && snapshot_smoke_root.is_some() {
        return Err("--smoke-root and --snapshot-smoke-root cannot be used together".to_owned());
    }

    Ok(Cli {
        rom: rom.ok_or_else(|| format!("missing required --rom PATH\n\n{USAGE}"))?,
        cart,
        snapshot,
        cycles,
        trace_limit,
        fetch_watch,
        write_watch,
        pressed_keys,
        dump_text,
        dump_text_png,
        screenshot,
        screenshot_format,
        screenshot_phase,
        smoke_root,
        snapshot_smoke_root,
        smoke_run_limit,
        smoke_report,
        smoke_screenshot_dir,
        smoke_screenshot_format,
        smoke_audio_dir,
        smoke_joystick,
        smoke_idle_after_start,
        xroar_bin,
        xroar_reference_dir,
        xroar_snapshot_out,
        xroar_motoroff,
        xroar_settle_seconds,
        xroar_timeout_seconds,
    })
}

fn next_value<I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .ok_or_else(|| format!("{flag} requires a value\n\n{USAGE}"))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|err| format!("invalid {flag} value {value}: {err}"))
    } else {
        value
            .parse()
            .map_err(|err| format!("invalid {flag} value {value}: {err}"))
    }
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    let parsed = parse_u64(value, flag)?;
    usize::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_u8(value: &str, flag: &str) -> Result<u8, String> {
    let parsed = parse_u64(value, flag)?;
    u8::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_u16(value: &str, flag: &str) -> Result<u16, String> {
    let parsed = parse_u64(value, flag)?;
    u16::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_u32(value: &str, flag: &str) -> Result<u32, String> {
    let parsed = parse_u64(value, flag)?;
    u32::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_address_range(value: &str, flag: &str) -> Result<AddressRange, String> {
    let (start, end) = value
        .split_once('-')
        .map_or((value, value), |(start, end)| (start, end));
    if start.is_empty() || end.is_empty() {
        return Err(format!("invalid {flag} value {value}; expected A or A-B"));
    }
    let start = parse_u16(start, flag)?;
    let end = parse_u16(end, flag)?;
    if start > end {
        return Err(format!(
            "invalid {flag} value {value}; start must be <= end"
        ));
    }
    Ok(AddressRange::new(start, end))
}

fn parse_f32(value: &str, flag: &str) -> Result<f32, String> {
    let parsed: f32 = value
        .parse()
        .map_err(|err| format!("invalid {flag} value {value}: {err}"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(format!(
            "{flag} value {value} must be a non-negative finite number"
        ));
    }
    Ok(parsed)
}

fn parse_screenshot_format(value: &str, flag: &str) -> Result<SmokeScreenshotFormat, String> {
    match value {
        "diagnostic" => Ok(SmokeScreenshotFormat::Diagnostic),
        "xroar-zoomed" => Ok(SmokeScreenshotFormat::XroarZoomed),
        _ => Err(format!(
            "invalid {flag} value {value}; expected diagnostic or xroar-zoomed"
        )),
    }
}

fn parse_screenshot_phase(value: &str, flag: &str) -> Result<SmokeScreenshotPhase, String> {
    match value {
        "immediate" => Ok(SmokeScreenshotPhase::Immediate),
        "completed-frame" => Ok(SmokeScreenshotPhase::CompletedFrame),
        _ => Err(format!(
            "invalid {flag} value {value}; expected immediate or completed-frame"
        )),
    }
}

fn parse_smoke_joystick_step(value: &str) -> Result<SmokeJoystickStep, String> {
    let mut parts = value.split(',');
    let port = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_value(value))?;
    let control = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_value(value))?;
    let frames = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_value(value))?;
    if parts.next().is_some() {
        return Err(invalid_smoke_joystick_value(value));
    }

    let port = parse_u8(port, "--smoke-joystick port")?;
    if !matches!(port, 1 | 2) {
        return Err(format!(
            "invalid --smoke-joystick port {port}; expected 1 or 2"
        ));
    }
    let frames = parse_u32(frames, "--smoke-joystick frames")?;
    if frames == 0 {
        return Err("--smoke-joystick frames must be greater than zero".to_owned());
    }
    Ok(SmokeJoystickStep {
        port,
        control: parse_smoke_joystick_control(control)?,
        frames,
    })
}

fn invalid_smoke_joystick_value(value: &str) -> String {
    format!("invalid --smoke-joystick value {value}; expected PORT,CONTROL,FRAMES")
}

fn parse_smoke_joystick_control(value: &str) -> Result<SmokeJoystickControl, String> {
    match value.to_ascii_lowercase().as_str() {
        "up" => Ok(SmokeJoystickControl::Up),
        "down" => Ok(SmokeJoystickControl::Down),
        "left" => Ok(SmokeJoystickControl::Left),
        "right" => Ok(SmokeJoystickControl::Right),
        "fire" => Ok(SmokeJoystickControl::Fire),
        "idle" => Ok(SmokeJoystickControl::Idle),
        _ => Err(format!(
            "invalid --smoke-joystick control {value}; expected up, down, left, right, fire, or idle"
        )),
    }
}

fn parse_matrix_key(value: &str) -> Result<MatrixKey, String> {
    let (row, column) = value
        .split_once(',')
        .ok_or_else(|| format!("invalid --press-matrix value {value}; expected R,C"))?;
    Ok(MatrixKey::new(
        parse_usize(row, "--press-matrix row")?,
        parse_usize(column, "--press-matrix column")?,
    ))
}

fn parse_dragon_key(value: &str) -> Result<DragonKey, String> {
    DragonKey::from_label(value).ok_or_else(|| {
        format!(
            "unknown Dragon key {value:?}; use a Dragon key label such as A, 1, @, enter, clear, break, shift, space, up, down, left, or right"
        )
    })
}

fn run_smoke_matrix(cli: &Cli, rom: &[u8; ROM_SIZE]) -> Result<SmokeMatrixReport, String> {
    let root = cli
        .smoke_root
        .as_deref()
        .ok_or_else(|| "--smoke-root is required".to_owned())?;
    let xroar = xroar_reference_config(cli)?;
    let mut tapes = Vec::new();
    collect_tape_candidates(root, &mut tapes)?;
    tapes.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    if let Some(dir) = &cli.smoke_audio_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    if let Some(config) = &xroar {
        fs::create_dir_all(&config.output_dir).map_err(|err| {
            format!(
                "failed to create XRoar reference dir {}: {err}",
                config.output_dir.display()
            )
        })?;
    }

    let mut rows = Vec::with_capacity(tapes.len());
    let mut runtime_smokes = 0usize;
    for (index, tape_path) in tapes.iter().enumerate() {
        let screenshot_stem = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}", safe_stem(tape_path))));
        let xroar_stem = xroar.as_ref().map(|config| {
            config
                .output_dir
                .join(format!("{index:04}-{}", safe_stem(tape_path)))
        });
        let audio_stem = cli
            .smoke_audio_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}", safe_stem(tape_path))));
        let row = scan_tape_candidate(
            tape_path,
            rom,
            &mut runtime_smokes,
            RuntimeSmokeOptions {
                run_limit: cli.smoke_run_limit,
                screenshot_stem: screenshot_stem.as_deref(),
                screenshot_format: cli.smoke_screenshot_format,
                audio_stem: audio_stem.as_deref(),
                joystick: &cli.smoke_joystick,
                idle_after_start_frames: cli.smoke_idle_after_start,
                xroar: xroar.as_ref(),
                xroar_stem: xroar_stem.as_deref(),
            },
        );
        rows.push(row);
    }

    Ok(SmokeMatrixReport {
        tape_count: rows.len(),
        runtime_smokes,
        rows,
    })
}

fn run_snapshot_smoke_matrix(
    cli: &Cli,
    rom: &[u8; ROM_SIZE],
) -> Result<SnapshotSmokeMatrixReport, String> {
    let root = cli
        .snapshot_smoke_root
        .as_deref()
        .ok_or_else(|| "--snapshot-smoke-root is required".to_owned())?;
    let xroar = xroar_reference_config(cli)?;
    let mut snapshots = Vec::new();
    collect_snapshot_candidates(root, &mut snapshots)?;
    snapshots.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    if let Some(config) = &xroar {
        fs::create_dir_all(&config.output_dir).map_err(|err| {
            format!(
                "failed to create XRoar reference dir {}: {err}",
                config.output_dir.display()
            )
        })?;
    }

    let mut rows = Vec::with_capacity(snapshots.len());
    let mut runtime_smokes = 0usize;
    for (index, snapshot_path) in snapshots.iter().enumerate() {
        let screenshot_path = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}.png", safe_stem(snapshot_path))));
        let xroar_stem = xroar.as_ref().map(|config| {
            config
                .output_dir
                .join(format!("{index:04}-{}", safe_stem(snapshot_path)))
        });
        let row = scan_snapshot_candidate(
            snapshot_path,
            rom,
            &mut runtime_smokes,
            SnapshotSmokeOptions {
                run_limit: cli.smoke_run_limit,
                screenshot_path: screenshot_path.as_deref(),
                screenshot_format: cli.smoke_screenshot_format,
                screenshot_phase: cli.screenshot_phase,
                cycle_limit: cli.cycles,
                trace_limit: cli.trace_limit,
                xroar: xroar.as_ref(),
                xroar_stem: xroar_stem.as_deref(),
            },
        );
        rows.push(row);
    }

    Ok(SnapshotSmokeMatrixReport {
        snapshot_count: rows.len(),
        runtime_smokes,
        rows,
    })
}

fn xroar_reference_config(cli: &Cli) -> Result<Option<XroarReferenceConfig>, String> {
    match (&cli.xroar_bin, &cli.xroar_reference_dir) {
        (None, None) => Ok(None),
        (Some(bin), Some(output_dir)) => Ok(Some(XroarReferenceConfig {
            bin: bin.clone(),
            output_dir: output_dir.clone(),
            motoroff: cli.xroar_motoroff,
            settle_seconds: cli.xroar_settle_seconds,
            timeout_seconds: cli.xroar_timeout_seconds,
        })),
        (Some(_), None) => Err("--xroar-reference-dir is required with --xroar-bin".to_owned()),
        (None, Some(_)) => Err("--xroar-bin is required with --xroar-reference-dir".to_owned()),
    }
}

fn collect_snapshot_candidates(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_snapshot_candidate_path(path) {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_snapshot_candidates(&entry.path(), out)?;
    }
    Ok(())
}

fn collect_tape_candidates(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_tape_candidate_path(path) {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_tape_candidates(&entry.path(), out)?;
    }
    Ok(())
}

fn is_snapshot_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["pak", "zip"]))
}

fn is_tape_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["cas", "zip"]))
}

fn scan_snapshot_candidate(
    path: &Path,
    rom: &[u8; ROM_SIZE],
    runtime_smokes: &mut usize,
    smoke: SnapshotSmokeOptions<'_>,
) -> SnapshotSmokeRow {
    let loaded = match read_media_asset(path, MediaKind::Snapshot) {
        Ok(loaded) => loaded,
        Err(err) => {
            return SnapshotSmokeRow {
                path: path.display().to_string(),
                archive_member: None,
                parse_status: "read-error".to_owned(),
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let snapshot = match parse_pcdragon_snapshot(&loaded.bytes) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            return SnapshotSmokeRow {
                path: path.display().to_string(),
                archive_member: loaded.archive_member,
                parse_status: "parse-error".to_owned(),
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let runtime = if *runtime_smokes < smoke.run_limit {
        *runtime_smokes += 1;
        Some(run_snapshot_smoke(rom, &snapshot, smoke))
    } else {
        None
    };

    SnapshotSmokeRow {
        path: path.display().to_string(),
        archive_member: loaded.archive_member,
        parse_status: "ok".to_owned(),
        runtime,
        error: None,
    }
}

fn run_snapshot_smoke(
    rom: &[u8; ROM_SIZE],
    snapshot: &PcDragonSnapshot,
    smoke: SnapshotSmokeOptions<'_>,
) -> SnapshotRuntimeSmoke {
    let report = run_harness_with_keyboard(
        rom,
        DragonKeyboard::new(),
        HarnessRunOptions {
            cartridge: None,
            snapshot: Some(snapshot),
            cycle_limit: smoke.cycle_limit,
            trace_limit: smoke.trace_limit,
            fetch_watch: None,
            write_watch: None,
            dump_text: true,
            dump_text_framebuffer: false,
            capture_framebuffer: true,
            capture_framebuffer_phase: smoke.screenshot_phase,
        },
    );
    let framebuffer = report.framebuffer.as_deref().unwrap_or(&[]);
    let (distinct_colors, non_background_pixels) = framebuffer_stats(framebuffer);
    let screenshot = match (smoke.screenshot_path, report.framebuffer.as_deref()) {
        (Some(path), Some(framebuffer)) => {
            if let Err(err) = write_screenshot_png(
                path,
                framebuffer,
                smoke.screenshot_format,
                report.framebuffer_cycles.unwrap_or(report.cycles),
            ) {
                return failed_snapshot_smoke(
                    snapshot,
                    report,
                    distinct_colors,
                    non_background_pixels,
                    err,
                );
            }
            Some(path.display().to_string())
        }
        _ => None,
    };
    let classification =
        classify_snapshot_smoke(report.stop_reason, distinct_colors, non_background_pixels);
    let comparison_screenshot =
        if matches!(smoke.screenshot_format, SmokeScreenshotFormat::XroarZoomed) {
            screenshot.as_deref().map(Path::new)
        } else {
            None
        };
    let xroar_reference_settle_seconds =
        xroar_snapshot_settle_seconds(report.framebuffer_cycles.unwrap_or(report.cycles));
    let (xroar_reference_screenshot, xroar_reference_error, xroar_reference_comparison) =
        match (smoke.xroar, smoke.xroar_stem) {
            (Some(config), Some(stem)) => match capture_xroar_snapshot_reference(
                config,
                rom,
                snapshot,
                comparison_screenshot,
                stem,
                xroar_reference_settle_seconds,
            ) {
                Ok(reference) => (
                    Some(reference.path.display().to_string()),
                    None,
                    reference.comparison,
                ),
                Err(err) => (None, Some(err), None),
            },
            _ => (None, None, None),
        };

    SnapshotRuntimeSmoke {
        classification,
        stop_reason: format_stop_reason(report.stop_reason),
        cycles: report.cycles,
        instructions: report.instructions,
        pc: report.pc,
        load_address: snapshot.load_address,
        ram_len: snapshot.ram.len(),
        text_screen_base: report.text_screen_base,
        distinct_colors,
        non_background_pixels,
        screen_text: report
            .text_screen
            .as_deref()
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        screenshot,
        screenshot_cycles: report.framebuffer_cycles,
        screenshot_frame_phase_cycles: report
            .framebuffer_cycles
            .map(|cycles| cycles % DRAGON_FRAME_CYCLES),
        xroar_reference_screenshot,
        xroar_reference_settle_seconds: smoke.xroar.map(|_| xroar_reference_settle_seconds),
        xroar_reference_error,
        xroar_reference_comparison,
        xroar_reference_comparison_error: None,
        vdg_trace: vdg_trace_summary(&report),
        error: None,
    }
}

fn failed_snapshot_smoke(
    snapshot: &PcDragonSnapshot,
    report: HarnessReport,
    distinct_colors: usize,
    non_background_pixels: usize,
    error: String,
) -> SnapshotRuntimeSmoke {
    SnapshotRuntimeSmoke {
        classification: SnapshotSmokeClassification::Error,
        stop_reason: format_stop_reason(report.stop_reason),
        cycles: report.cycles,
        instructions: report.instructions,
        pc: report.pc,
        load_address: snapshot.load_address,
        ram_len: snapshot.ram.len(),
        text_screen_base: report.text_screen_base,
        distinct_colors,
        non_background_pixels,
        screen_text: report
            .text_screen
            .as_deref()
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        screenshot: None,
        screenshot_cycles: report.framebuffer_cycles,
        screenshot_frame_phase_cycles: report
            .framebuffer_cycles
            .map(|cycles| cycles % DRAGON_FRAME_CYCLES),
        xroar_reference_screenshot: None,
        xroar_reference_settle_seconds: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
        vdg_trace: vdg_trace_summary(&report),
        error: Some(error),
    }
}

fn vdg_trace_summary(report: &HarnessReport) -> Option<VdgTraceSummary> {
    if report.vdg_samples.is_empty()
        && report.dropped_vdg_samples == 0
        && report.vdg_mode_writes.is_empty()
        && report.dropped_vdg_mode_writes == 0
    {
        return None;
    }

    let mode_writes: Vec<_> = report
        .vdg_mode_writes
        .iter()
        .map(|write| VdgModeWriteSummary {
            cycle: write.cycle,
            frame_master_tick: write.frame_master_tick,
            line: write.line,
            active_y: write.active_y,
            active_x: write.active_x,
            addr: write.addr,
            value: write.value,
            graphics: write.graphics,
            css: write.css,
            int_ext: write.int_ext,
            gm: write.gm,
        })
        .collect();

    Some(VdgTraceSummary {
        dropped_samples: report.dropped_vdg_samples,
        dropped_mode_writes: report.dropped_vdg_mode_writes,
        dropped_device_accesses: report.dropped_device_accesses,
        samples: report
            .vdg_samples
            .iter()
            .map(|sample| VdgSampleSummary {
                cycle: sample.cycle,
                frame_master_tick: sample.frame_master_tick,
                line: sample.line,
                active_y: sample.active_y,
                byte_x: sample.byte_x,
                display_base: sample.display_base,
                sam_video_mode: sample.sam_video_mode,
                sam_display_offset: sample.sam_display_offset,
                pia1_pb: sample.pia1_pb,
                graphics: sample.graphics,
                css: sample.css,
                int_ext: sample.int_ext,
                gm: sample.gm,
            })
            .collect(),
        mode_writes,
    })
}

fn classify_snapshot_smoke(
    stop_reason: StopReason,
    distinct_colors: usize,
    non_background_pixels: usize,
) -> SnapshotSmokeClassification {
    let visible = distinct_colors > 1 && non_background_pixels > 0;
    match (stop_reason, visible) {
        (StopReason::CycleLimit, true) => SnapshotSmokeClassification::RunningVisible,
        (StopReason::CycleLimit, false) => SnapshotSmokeClassification::RunningBlank,
        (StopReason::CpuHalted, true) => SnapshotSmokeClassification::HaltedVisible,
        (StopReason::CpuHalted, false) => SnapshotSmokeClassification::HaltedBlank,
    }
}

fn framebuffer_stats(framebuffer: &[u32]) -> (usize, usize) {
    let Some(&background) = framebuffer.first() else {
        return (0, 0);
    };
    let mut colors = Vec::new();
    let mut non_background_pixels = 0usize;
    for &pixel in framebuffer {
        if pixel != background {
            non_background_pixels += 1;
        }
        if !colors.contains(&pixel) {
            colors.push(pixel);
        }
    }
    (colors.len(), non_background_pixels)
}

fn scan_tape_candidate(
    path: &Path,
    rom: &[u8; ROM_SIZE],
    runtime_smokes: &mut usize,
    smoke: RuntimeSmokeOptions<'_>,
) -> SmokeMatrixRow {
    let loaded = match read_media_asset(path, MediaKind::Tape) {
        Ok(loaded) => loaded,
        Err(err) => {
            return SmokeMatrixRow {
                path: path.display().to_string(),
                archive_member: None,
                parse_status: "read-error".to_owned(),
                blocks: None,
                checksums_valid: None,
                ignored_segments: None,
                ignored_bytes: None,
                header: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let parsed = match parse_cas_tolerant(&loaded.bytes) {
        Ok(image) => image,
        Err(err) => {
            return SmokeMatrixRow {
                path: path.display().to_string(),
                archive_member: loaded.archive_member,
                parse_status: "parse-error".to_owned(),
                blocks: None,
                checksums_valid: None,
                ignored_segments: None,
                ignored_bytes: None,
                header: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let header = parsed.first_header().map(CasHeaderSummary::from);
    let should_smoke = parsed.first_header().is_some_and(|header| {
        matches!(
            header.file_type,
            CasFileType::Basic | CasFileType::MachineCode
        )
    }) && !parsed.has_ignored_bytes()
        && parsed.checksums_valid()
        && *runtime_smokes < smoke.run_limit;
    let runtime = if should_smoke {
        *runtime_smokes += 1;
        Some(run_runtime_smoke(rom, &loaded.bytes, &parsed, smoke))
    } else {
        None
    };

    SmokeMatrixRow {
        path: path.display().to_string(),
        archive_member: loaded.archive_member,
        parse_status: if parsed.has_ignored_bytes() {
            "ok-with-ignored-bytes".to_owned()
        } else {
            "ok".to_owned()
        },
        blocks: Some(parsed.blocks.len()),
        checksums_valid: Some(parsed.checksums_valid()),
        ignored_segments: Some(parsed.ignored_ranges.len()),
        ignored_bytes: Some(parsed.ignored_byte_count()),
        header,
        runtime,
        error: None,
    }
}

fn run_runtime_smoke(
    rom: &[u8; ROM_SIZE],
    tape_bytes: &[u8],
    tape: &CasImage,
    smoke_options: RuntimeSmokeOptions<'_>,
) -> CasRuntimeSmoke {
    let command = tape
        .first_header()
        .map(|header| match header.file_type {
            CasFileType::Basic => "CLOAD",
            CasFileType::MachineCode => "CLOADM",
            CasFileType::Data | CasFileType::Unknown(_) => "",
        })
        .unwrap_or("");
    if command.is_empty() {
        return failed_runtime_smoke("", "unsupported tape file type");
    }

    match run_runtime_smoke_inner(rom, tape_bytes, command, smoke_options) {
        Ok(mut smoke) => {
            if let (Some(config), Some(stem)) = (smoke_options.xroar, smoke_options.xroar_stem) {
                let comparison_screenshot = smoke
                    .start_screenshot
                    .as_ref()
                    .or(smoke.load_screenshot.as_ref());
                match capture_best_xroar_reference(
                    config,
                    rom,
                    tape_bytes,
                    command,
                    xroar_start_command(&smoke),
                    comparison_screenshot.map(Path::new),
                    stem,
                ) {
                    Ok(reference) => {
                        smoke.xroar_reference_screenshot =
                            Some(reference.path.display().to_string());
                        smoke.xroar_reference_motoroff = Some(reference.motoroff);
                        smoke.xroar_reference_comparison = reference.comparison;
                    }
                    Err(err) => smoke.xroar_reference_error = Some(err),
                }
            }
            smoke
        }
        Err(error) => failed_runtime_smoke(command, &error),
    }
}

fn run_runtime_smoke_inner(
    rom: &[u8; ROM_SIZE],
    tape_bytes: &[u8],
    command: &str,
    smoke_options: RuntimeSmokeOptions<'_>,
) -> Result<CasRuntimeSmoke, String> {
    let screenshot_stem = smoke_options.screenshot_stem;
    let screenshot_format = smoke_options.screenshot_format;
    let audio_stem = smoke_options.audio_stem;
    let joystick_steps = smoke_options.joystick;
    let idle_after_start_frames = smoke_options.idle_after_start_frames;
    let mut session = boot_runtime_session(rom)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, tape_bytes));
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load tape into runtime: {err}"))?;
    let tape_length_bits = query_u64(&session, "dragon.tape.length_bits")?;

    session.clear_audio_capture();
    let load_pc_before = query_u16(&session, "dragon.cpu.pc")?;
    type_basic_command(&mut session, command)?;
    let before_load = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture submitted-load frame: {err}"))?;
    let moved_to = wait_for_tape_position_above(&mut session, 0, 180)?;
    if moved_to == 0 {
        return Err("ROM did not start consuming cassette bits".to_owned());
    }
    let load_wait_frames = load_wait_frame_budget(tape_length_bits);
    let load_stop = wait_for_tape_load_stop(&mut session, load_wait_frames)?;
    if load_stop == TapeLoadStop::FrameLimit {
        return Err(format!(
            "cassette load did not stop within {load_wait_frames} frames for {tape_length_bits} tape bits"
        ));
    }
    let lines_after_load = screen_text_lines(&session)?;
    let after_load = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture post-load frame: {err}"))?;
    let load_visible_change = after_load != before_load;
    let load_pc_after = query_u16(&session, "dragon.cpu.pc")?;
    let load_video = video_state(&session)?;
    let load_screenshot = write_smoke_screenshot(
        screenshot_stem,
        "load",
        screenshot_format,
        &session,
        &after_load,
    )?;
    let load_audio = write_smoke_audio(audio_stem, "load", &session)?;
    let load_error = lines_after_load.iter().any(|line| line.contains("ERROR"));
    let load_result = if load_error { "basic-error" } else { "ok" }.to_owned();
    let start_command = if command == "CLOAD" { "RUN" } else { "EXEC" };
    let should_start =
        should_issue_start_command(command, load_error, load_visible_change, load_pc_after);

    let mut start_settle_visible_change = false;
    let (start_result, visible_change_after_start, screen_text, start_audio) = if should_start {
        session.clear_audio_capture();
        type_basic_command(&mut session, start_command)?;
        let changed_frame = wait_for_screenshot_change(&mut session, &after_load, 500)?;
        if let Some(changed_frame) = &changed_frame {
            session
                .run_frames(SMOKE_START_SETTLE_FRAMES)
                .map_err(|err| format!("runtime failed while settling after start: {err}"))?;
            let settled_frame = session
                .screenshot_png_bytes()
                .map_err(|err| format!("failed to capture settled start frame: {err}"))?;
            start_settle_visible_change = settled_frame != *changed_frame;
        }
        let visible_change = changed_frame.is_some();
        let screen_text = screen_text_lines(&session)?;
        let basic_error = screen_text.iter().any(|line| line.contains("ERROR"));
        let start_result = if basic_error {
            "basic-error"
        } else if visible_change {
            "visible-change"
        } else {
            "no-visible-change"
        }
        .to_owned();
        let start_audio = write_smoke_audio(audio_stem, "start", &session)?;
        (start_result, visible_change, screen_text, start_audio)
    } else {
        (
            skipped_start_result(command, load_visible_change, load_pc_after).to_owned(),
            false,
            lines_after_load.clone(),
            None,
        )
    };
    let basic_error = screen_text.iter().any(|line| line.contains("ERROR"));
    let start_pc_after = query_u16(&session, "dragon.cpu.pc")?;
    let start_video = video_state(&session)?;
    let start_screenshot_frame = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture post-start frame: {err}"))?;
    let start_screenshot = write_smoke_screenshot(
        screenshot_stem,
        "start",
        screenshot_format,
        &session,
        &start_screenshot_frame,
    )?;
    let (idle_visible_change, idle_screen_text, idle_screenshot) = if idle_after_start_frames == 0 {
        (false, None, None)
    } else {
        session.run_frames(idle_after_start_frames).map_err(|err| {
            format!("runtime failed while idling after start for {idle_after_start_frames} frames: {err}")
        })?;
        let idle_frame = session
            .screenshot_png_bytes()
            .map_err(|err| format!("failed to capture post-idle frame: {err}"))?;
        let idle_screen_text = screen_text_lines(&session)?;
        let idle_screenshot = write_smoke_screenshot(
            screenshot_stem,
            "idle",
            screenshot_format,
            &session,
            &idle_frame,
        )?;
        (
            idle_frame != start_screenshot_frame,
            Some(idle_screen_text),
            idle_screenshot,
        )
    };
    let (joystick_visible_change, joystick_screen_text, joystick_screenshot) =
        if joystick_steps.is_empty() {
            (false, None, None)
        } else {
            apply_smoke_joystick_steps(&mut session, joystick_steps)?;
            let joystick_frame = session
                .screenshot_png_bytes()
                .map_err(|err| format!("failed to capture post-joystick frame: {err}"))?;
            let joystick_screen_text = screen_text_lines(&session)?;
            let joystick_screenshot = write_smoke_screenshot(
                screenshot_stem,
                "joystick",
                screenshot_format,
                &session,
                &joystick_frame,
            )?;
            (
                joystick_frame != start_screenshot_frame,
                Some(joystick_screen_text),
                joystick_screenshot,
            )
        };
    let start_video_changed = load_video != start_video;
    let classification = classify_runtime_smoke(RuntimeSmokeClassificationInput {
        command,
        load_result: &load_result,
        start_result: &start_result,
        load_visible_change,
        visible_change_after_start,
        start_video_changed,
        start_settle_visible_change,
        basic_error,
        load_video,
        start_video,
    });

    Ok(CasRuntimeSmoke {
        command: command.to_owned(),
        classification,
        load_result,
        start_command: if should_start {
            start_command.to_owned()
        } else {
            String::new()
        },
        start_result,
        load_visible_change,
        load_basic_error: load_error,
        load_pc_before,
        load_pc_after,
        start_pc_after,
        load_video,
        start_video,
        tape_position_bits: query_u64(&session, "dragon.tape.position_bits")?,
        tape_length_bits,
        tape_finished: query_bool(&session, "dragon.tape.finished")?,
        visible_change_after_start,
        start_video_changed,
        start_settle_visible_change,
        basic_error,
        load_screen_text: lines_after_load,
        screen_text,
        error: None,
        load_screenshot,
        load_audio,
        start_screenshot,
        start_audio,
        idle_after_start_frames,
        idle_visible_change,
        idle_screen_text,
        idle_screenshot,
        joystick_steps: joystick_steps.to_vec(),
        joystick_visible_change,
        joystick_screen_text,
        joystick_screenshot,
        xroar_reference_screenshot: None,
        xroar_reference_motoroff: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
    })
}

fn should_issue_start_command(
    command: &str,
    load_error: bool,
    load_visible_change: bool,
    _load_pc_after: u16,
) -> bool {
    !load_error && (matches!(command, "CLOAD" | "CLOADM") || !load_visible_change)
}

fn skipped_start_result(
    command: &str,
    load_visible_change: bool,
    load_pc_after: u16,
) -> &'static str {
    if command == "CLOADM" && load_visible_change && load_pc_after < 0x8000 {
        "already-running-after-load"
    } else if command == "CLOADM" && load_visible_change {
        "start-skipped-load-screen-changed"
    } else {
        "start-skipped"
    }
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSmokeClassificationInput<'a> {
    command: &'a str,
    load_result: &'a str,
    start_result: &'a str,
    load_visible_change: bool,
    visible_change_after_start: bool,
    start_video_changed: bool,
    start_settle_visible_change: bool,
    basic_error: bool,
    load_video: DragonVideoState,
    start_video: DragonVideoState,
}

fn classify_runtime_smoke(
    input: RuntimeSmokeClassificationInput<'_>,
) -> RuntimeSmokeClassification {
    if input.load_result == "error" {
        return RuntimeSmokeClassification::Error;
    }
    if input.load_result == "basic-error" {
        return RuntimeSmokeClassification::LoadBasicError;
    }
    if input.basic_error || input.start_result == "basic-error" {
        return RuntimeSmokeClassification::StartedBasicError;
    }

    let start_uses_graphics = video_state_uses_graphics(input.start_video);
    if input.visible_change_after_start && start_uses_graphics {
        return if input.start_settle_visible_change {
            RuntimeSmokeClassification::StartedGraphicsDrawing
        } else {
            RuntimeSmokeClassification::StartedGraphicsBlank
        };
    }
    if input.start_video_changed {
        return RuntimeSmokeClassification::StartedVideoControlChanged;
    }
    if input.visible_change_after_start {
        return RuntimeSmokeClassification::StartedTextDrawing;
    }
    if input.start_result == "no-visible-change" {
        return RuntimeSmokeClassification::StartedNoVisibleChange;
    }
    if input.command == "CLOADM"
        && input.start_result == "already-running-after-load"
        && input.load_visible_change
    {
        return RuntimeSmokeClassification::MachineCodeRunningAfterLoad;
    }
    if video_state_uses_graphics(input.load_video) {
        return RuntimeSmokeClassification::StartedGraphicsBlank;
    }
    if input.load_visible_change {
        RuntimeSmokeClassification::LoadedVisibleChange
    } else {
        RuntimeSmokeClassification::LoadedNoVisibleChange
    }
}

fn video_state_uses_graphics(video: DragonVideoState) -> bool {
    video.sam_video_mode != 0 || video.pia1_output_b & 0x80 != 0
}

fn video_state(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<DragonVideoState, String> {
    Ok(DragonVideoState {
        sam_video_mode: query_u8(session, "dragon.sam.video_mode")?,
        sam_display_offset: query_u8(session, "dragon.sam.display_offset")?,
        display_base: query_u16(session, "dragon.video.display_base")?,
        pia1_output_b: query_u8(session, "dragon.pia1.output_b")?,
        pia1_ddr_b: query_u8(session, "dragon.pia1.ddr_b")?,
        pia1_control_b: query_u8(session, "dragon.pia1.control_b")?,
        pia1_cb2: query_bool(session, "dragon.pia1.cb2")?,
    })
}

fn write_smoke_screenshot(
    stem: Option<&Path>,
    suffix: &str,
    format: SmokeScreenshotFormat,
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    diagnostic_png: &[u8],
) -> Result<Option<String>, String> {
    let Some(stem) = stem else {
        return Ok(None);
    };
    let path = stem.with_extension(format!("{suffix}.png"));
    let png = match format {
        SmokeScreenshotFormat::Diagnostic => diagnostic_png.to_vec(),
        SmokeScreenshotFormat::XroarZoomed => {
            xroar_zoomed_png_bytes(session.latest_frame().ok_or_else(|| {
                "cannot write xroar-zoomed smoke screenshot before a frame has been captured"
                    .to_owned()
            })?)?
        }
    };
    fs::write(&path, png).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn write_smoke_audio(
    stem: Option<&Path>,
    suffix: &str,
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<Option<String>, String> {
    let Some(stem) = stem else {
        return Ok(None);
    };
    let path = stem.with_extension(format!("{suffix}.wav"));
    session
        .save_audio_capture(&path)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn xroar_zoomed_png_bytes(frame: &CapturedFrame) -> Result<Vec<u8>, String> {
    if frame.format != PixelFormat::Rgba8888
        || frame.width != TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32
        || frame.height != TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32
    {
        return Err(format!(
            "xroar-zoomed smoke screenshots require diagnostic RGBA frames of {}x{}; got {:?} {}x{}",
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
            TEXT_VISIBLE_FRAMEBUFFER_HEIGHT,
            frame.format,
            frame.width,
            frame.height
        ));
    }

    let mut rgba = Vec::with_capacity((XROAR_ZOOMED_WIDTH * XROAR_ZOOMED_HEIGHT * 4) as usize);
    let source_width = TEXT_VISIBLE_FRAMEBUFFER_WIDTH;
    let source_x_origin = motorola_vdg_6847::TEXT_LEFT_BORDER_PIXELS;
    let source_y_origin = motorola_vdg_6847::TEXT_TOP_BORDER_LINES;
    for y in 0..motorola_vdg_6847::TEXT_FRAMEBUFFER_HEIGHT {
        for _ in 0..2 {
            for x in 0..motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH {
                let offset = ((source_y_origin + y) * source_width + source_x_origin + x) * 4;
                let pixel = &frame.pixels[offset..offset + 4];
                rgba.extend_from_slice(pixel);
                rgba.extend_from_slice(pixel);
            }
        }
    }

    encode_rgba_png(XROAR_ZOOMED_WIDTH, XROAR_ZOOMED_HEIGHT, &rgba)
}

fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return Err(format!(
            "RGBA buffer has {} bytes; expected {expected} for {width}x{height}",
            rgba.len()
        ));
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| format!("failed to write PNG header: {err}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|err| format!("failed to write PNG data: {err}"))?;
        writer
            .finish()
            .map_err(|err| format!("failed to finish PNG: {err}"))?;
    }
    Ok(png)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn compare_png_files(emu_path: &Path, reference_path: &Path) -> Result<ImageComparison, String> {
    let emu_bytes = fs::read(emu_path)
        .map_err(|err| format!("failed to read {}: {err}", emu_path.display()))?;
    let reference_bytes = fs::read(reference_path)
        .map_err(|err| format!("failed to read {}: {err}", reference_path.display()))?;
    let emu = decode_png_rgba(&emu_bytes)?;
    let reference = decode_png_rgba(&reference_bytes)?;
    Ok(compare_images(&emu, &reference))
}

fn decode_png_rgba(bytes: &[u8]) -> Result<DecodedImage, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("failed to read PNG header: {err}"))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| format!("failed to read PNG frame: {err}"))?;
    let data = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in data.chunks_exact(3) {
                out.extend_from_slice(chunk);
                out.push(0xFF);
            }
            out
        }
        png::ColorType::Rgba => data.to_vec(),
        other => {
            return Err(format!(
                "unsupported PNG colour type {other:?}; expected RGB or RGBA"
            ));
        }
    };
    Ok(DecodedImage {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn compare_images(emu: &DecodedImage, reference: &DecodedImage) -> ImageComparison {
    let dimensions_match = emu.width == reference.width && emu.height == reference.height;
    let compared_pixels = if dimensions_match {
        u64::from(emu.width) * u64::from(emu.height)
    } else {
        0
    };
    if !dimensions_match {
        return ImageComparison {
            emu_width: emu.width,
            emu_height: emu.height,
            reference_width: reference.width,
            reference_height: reference.height,
            dimensions_match,
            compared_pixels,
            differing_pixels: 0,
            differing_pixel_percent: 0.0,
            max_channel_delta: 0,
            mean_abs_channel_delta: 0.0,
        };
    }

    let mut differing_pixels = 0u64;
    let mut max_channel_delta = 0u8;
    let mut total_abs_channel_delta = 0u64;
    for (emu_pixel, reference_pixel) in emu.rgba.chunks_exact(4).zip(reference.rgba.chunks_exact(4))
    {
        let mut pixel_differs = false;
        for channel in 0..3 {
            let delta = emu_pixel[channel].abs_diff(reference_pixel[channel]);
            pixel_differs |= delta != 0;
            max_channel_delta = max_channel_delta.max(delta);
            total_abs_channel_delta += u64::from(delta);
        }
        if pixel_differs {
            differing_pixels += 1;
        }
    }

    let compared_channels = compared_pixels * 3;
    ImageComparison {
        emu_width: emu.width,
        emu_height: emu.height,
        reference_width: reference.width,
        reference_height: reference.height,
        dimensions_match,
        compared_pixels,
        differing_pixels,
        differing_pixel_percent: (differing_pixels as f64 / compared_pixels as f64) * 100.0,
        max_channel_delta,
        mean_abs_channel_delta: total_abs_channel_delta as f64 / compared_channels as f64,
    }
}

#[derive(Debug)]
struct XroarReferenceCapture {
    path: PathBuf,
    motoroff: usize,
    comparison: Option<ImageComparison>,
}

fn capture_best_xroar_reference(
    config: &XroarReferenceConfig,
    rom: &[u8; ROM_SIZE],
    tape_bytes: &[u8],
    command: &str,
    start_command: Option<&str>,
    comparison_screenshot: Option<&Path>,
    stem: &Path,
) -> Result<XroarReferenceCapture, String> {
    let rom_path = write_temp_bytes("rom", "rom", rom)?;
    let tape_path = write_temp_bytes("tape", "cas", tape_bytes)?;
    let result = capture_best_xroar_reference_inner(
        config,
        command,
        start_command,
        comparison_screenshot,
        stem,
        &rom_path,
        &tape_path,
    );
    let _ = fs::remove_file(&rom_path);
    let _ = fs::remove_file(&tape_path);
    result
}

fn capture_xroar_snapshot_reference(
    config: &XroarReferenceConfig,
    rom: &[u8; ROM_SIZE],
    snapshot: &PcDragonSnapshot,
    comparison_screenshot: Option<&Path>,
    stem: &Path,
    settle_seconds: f32,
) -> Result<XroarReferenceCapture, String> {
    let rom_path = write_temp_bytes("rom", "rom", rom)?;
    let template_path = write_temp_path("snapshot-template", "sna")?;
    let template_result = run_xroar_snapshot_template_command(config, &rom_path, &template_path)
        .and_then(|()| {
            fs::read(&template_path)
                .map_err(|err| format!("failed to read {}: {err}", template_path.display()))
        });
    let snapshot_path = template_result
        .and_then(|template| xroar_v2_snapshot_bytes(&template, snapshot))
        .and_then(|bytes| write_temp_bytes("snapshot", "sna", &bytes));
    let output_path = stem.with_extension("xroar.png");
    let trap_condition = xroar_snapshot_trap_condition(snapshot);
    let result = snapshot_path.and_then(|snapshot_path| {
        let result = run_xroar_snapshot_reference_command(
            config,
            &rom_path,
            &snapshot_path,
            &trap_condition,
            &output_path,
            settle_seconds,
        )
        .and_then(|path| {
            let comparison = comparison_screenshot
                .map(|screenshot| compare_png_files(screenshot, &path))
                .transpose()?;
            Ok(XroarReferenceCapture {
                path,
                motoroff: 0,
                comparison,
            })
        });
        let _ = fs::remove_file(&snapshot_path);
        result
    });
    let _ = fs::remove_file(&rom_path);
    let _ = fs::remove_file(&template_path);
    result
}

fn write_xroar_snapshot_out(
    cli: &Cli,
    snapshot: &PcDragonSnapshot,
    output_path: &Path,
) -> Result<(), String> {
    let xroar_bin = cli
        .xroar_bin
        .as_ref()
        .ok_or_else(|| "--xroar-snapshot-out requires --xroar-bin".to_owned())?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let config = XroarReferenceConfig {
        bin: xroar_bin.clone(),
        output_dir: output_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        motoroff: None,
        settle_seconds: DEFAULT_XROAR_SETTLE_SECONDS,
        timeout_seconds: DEFAULT_XROAR_TIMEOUT_SECONDS,
    };

    let rom_path = output_path.with_extension("rom");
    fs::write(&rom_path, load_rom(&cli.rom)?)
        .map_err(|err| format!("failed to write {}: {err}", rom_path.display()))?;
    let template_path = write_temp_path("snapshot-template", "sna")?;
    let template_result = run_xroar_snapshot_template_command(&config, &rom_path, &template_path)
        .and_then(|()| {
            fs::read(&template_path)
                .map_err(|err| format!("failed to read {}: {err}", template_path.display()))
        });
    let result = template_result
        .and_then(|template| xroar_v2_snapshot_bytes(&template, snapshot))
        .and_then(|bytes| {
            fs::write(output_path, bytes)
                .map_err(|err| format!("failed to write {}: {err}", output_path.display()))
        });
    let _ = fs::remove_file(&template_path);
    result
}

fn capture_best_xroar_reference_inner(
    config: &XroarReferenceConfig,
    command: &str,
    start_command: Option<&str>,
    comparison_screenshot: Option<&Path>,
    stem: &Path,
    rom_path: &Path,
    tape_path: &Path,
) -> Result<XroarReferenceCapture, String> {
    let candidates = xroar_motoroff_candidates(config.motoroff, command, start_command);
    let mut best: Option<XroarReferenceCapture> = None;
    let mut errors = Vec::new();

    for motoroff in candidates {
        let output_path = if config.motoroff.is_some() {
            stem.with_extension("xroar.png")
        } else {
            stem.with_extension(format!("xroar-{motoroff}.png"))
        };
        match run_xroar_reference_command(
            config,
            command,
            start_command,
            motoroff,
            rom_path,
            tape_path,
            &output_path,
        ) {
            Ok(path) => {
                let comparison = comparison_screenshot
                    .map(|screenshot| compare_png_files(screenshot, &path))
                    .transpose()?;
                let candidate = XroarReferenceCapture {
                    path,
                    motoroff,
                    comparison,
                };
                if best
                    .as_ref()
                    .is_none_or(|current| xroar_capture_is_better(&candidate, current))
                {
                    best = Some(candidate);
                }
            }
            Err(err) => errors.push(format!("motoroff {motoroff}: {err}")),
        }
    }

    best.ok_or_else(|| {
        if errors.is_empty() {
            "XRoar reference capture had no motor-off candidates".to_owned()
        } else {
            errors.join("; ")
        }
    })
}

fn run_xroar_snapshot_reference_command(
    config: &XroarReferenceConfig,
    rom_path: &Path,
    snapshot_path: &Path,
    trap_condition: &str,
    output_path: &Path,
    settle_seconds: f32,
) -> Result<PathBuf, String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let _ = fs::remove_file(output_path);

    let mut command = Command::new(&config.bin);
    command
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-vo-picture")
        .arg("zoomed")
        .arg("-machine")
        .arg("dragon32");
    append_xroar_rom_args(&mut command, rom_path)?;
    let output = command
        .arg("-load")
        .arg(snapshot_path)
        .arg("-trap")
        .arg(trap_condition)
        .arg("-trap-timeout")
        .arg(format_seconds(settle_seconds))
        .arg("-trap-timeout-screenshot")
        .arg(output_path)
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write reference screenshot {}",
            output_path.display()
        ));
    }
    Ok(output_path.to_owned())
}

fn run_xroar_snapshot_template_command(
    config: &XroarReferenceConfig,
    rom_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let _ = fs::remove_file(output_path);

    let mut command = Command::new(&config.bin);
    command
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-machine")
        .arg("dragon32");
    append_xroar_rom_args(&mut command, rom_path)?;
    let output = command
        .arg("-trap")
        .arg("immediate")
        .arg("-trap-snap")
        .arg(output_path)
        .arg("-trap-timeout")
        .arg("0.01")
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {} while creating v2 snapshot template: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write v2 snapshot template {}",
            output_path.display()
        ));
    }
    Ok(())
}

fn append_xroar_rom_args(command: &mut Command, rom_path: &Path) -> Result<(), String> {
    let parent = rom_path
        .parent()
        .ok_or_else(|| format!("XRoar ROM path {} has no parent", rom_path.display()))?;
    let filename = rom_path
        .file_name()
        .ok_or_else(|| format!("XRoar ROM path {} has no filename", rom_path.display()))?;
    command
        .arg("-rompath")
        .arg(parent)
        .arg("-extbas")
        .arg(filename);
    Ok(())
}

fn xroar_snapshot_trap_condition(snapshot: &PcDragonSnapshot) -> String {
    format!("pc=0x{:04x}", snapshot.registers.pc)
}

fn run_xroar_reference_command(
    config: &XroarReferenceConfig,
    command: &str,
    start_command: Option<&str>,
    motoroff: usize,
    rom_path: &Path,
    tape_path: &Path,
    output_path: &Path,
) -> Result<PathBuf, String> {
    if motoroff == 0 {
        return Err("--xroar-motoroff must be greater than zero".to_owned());
    }
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let _ = fs::remove_file(output_path);

    let trap_range = format!("{motoroff}-{motoroff}");
    let type_text = xroar_type_text(command, start_command)?;
    let output = Command::new(&config.bin)
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-vo-picture")
        .arg("zoomed")
        .arg("-machine")
        .arg("dragon32")
        .arg("-extbas")
        .arg(rom_path)
        .arg("-load-tape")
        .arg(tape_path)
        .arg("-type")
        .arg(type_text)
        .arg("-trap")
        .arg("tape-motor-off")
        .arg("-trap-range")
        .arg(trap_range)
        .arg("-trap-timeout")
        .arg(format_seconds(config.settle_seconds))
        .arg("-trap-timeout-screenshot")
        .arg(output_path)
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write reference screenshot {}",
            output_path.display()
        ));
    }
    Ok(output_path.to_owned())
}

fn write_temp_bytes(kind: &str, extension: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = write_temp_path(kind, extension)?;
    fs::write(&path, bytes).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

fn write_temp_path(kind: &str, extension: &str) -> Result<PathBuf, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock was before UNIX epoch: {err}"))?
        .as_nanos();
    let path = env::temp_dir().join(format!(
        "emu198x-xroar-{}-{unique}-{kind}.{extension}",
        process::id()
    ));
    Ok(path)
}

fn xroar_v2_snapshot_bytes(
    template: &[u8],
    snapshot: &PcDragonSnapshot,
) -> Result<Vec<u8>, String> {
    let mut bytes = template.to_vec();
    let ram = xroar_snapshot_ram(snapshot);
    patch_xroar_v2_ram(&mut bytes, &ram)?;
    patch_xroar_v2_pias(&mut bytes, snapshot)?;
    patch_xroar_v2_vdg(&mut bytes, snapshot)?;
    patch_xroar_v2_cpu(&mut bytes, snapshot)?;
    patch_xroar_v2_sam(&mut bytes, snapshot)?;
    Ok(bytes)
}

fn patch_xroar_v2_ram(bytes: &mut [u8], ram: &[u8]) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, b"RAM", Some(b"VDG"))?;
    let bank = xroar_v2_find_tag_payload(bytes, range.start, range.end, 7)
        .ok_or_else(|| "XRoar v2 RAM bank tag not found".to_owned())?;
    let data = xroar_v2_find_tag_payload(bytes, bank.payload_start, range.end, 1)
        .ok_or_else(|| "XRoar v2 RAM payload tag not found".to_owned())?;
    if data.payload_end - data.payload_start != ram.len() {
        return Err(format!(
            "XRoar v2 RAM payload size mismatch: template has {}, snapshot has {}",
            data.payload_end - data.payload_start,
            ram.len()
        ));
    }
    bytes[data.payload_start..data.payload_end].copy_from_slice(ram);
    Ok(())
}

fn patch_xroar_v2_pias(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    let peripherals = snapshot.peripherals.unwrap_or(PcDragonPeripherals {
        ff02: 0,
        ff03: 0,
        ff22: 0,
    });

    patch_xroar_v2_pia(
        bytes,
        b"PIA0",
        Some(b"CPU"),
        XroarV2PiaPortB {
            control: peripherals.ff03,
            ddr: 0xff,
            output: peripherals.ff02,
            irq1: peripherals.ff03 & 0x80,
            irq2: peripherals.ff03 & 0x40,
        },
    )?;
    patch_xroar_v2_pia(
        bytes,
        b"PIA1",
        Some(b"PIA0"),
        XroarV2PiaPortB {
            control: 0x04,
            ddr: 0xff,
            output: peripherals.ff22,
            irq1: 0,
            irq2: 0,
        },
    )
}

#[derive(Clone, Copy)]
struct XroarV2PiaPortB {
    control: u8,
    ddr: u8,
    output: u8,
    irq1: u8,
    irq2: u8,
}

fn patch_xroar_v2_pia(
    bytes: &mut Vec<u8>,
    component: &[u8],
    next_component: Option<&[u8]>,
    port_b: XroarV2PiaPortB,
) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, component, next_component)?;
    let part_start = find_bytes_from_until(bytes, b"MC6821", range.start, range.end)
        .map(|offset| offset + b"MC6821".len())
        .ok_or_else(|| {
            format!(
                "XRoar v2 {} MC6821 part payload not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let side_a_marker =
        find_bytes_from_until(bytes, &[1, 0], part_start, range.end).ok_or_else(|| {
            format!(
                "XRoar v2 {} port A tag not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let side_b_marker = find_bytes_from_until(bytes, &[2, 0], side_a_marker + 2, range.end)
        .ok_or_else(|| {
            format!(
                "XRoar v2 {} port B tag not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let side_a = side_a_marker + 2;
    let mut side_a_end = side_b_marker;
    patch_xroar_v2_pia_side_a(bytes, side_a, &mut side_a_end)?;
    let side_b_delta = side_a_end
        .checked_sub(side_b_marker)
        .ok_or_else(|| "XRoar v2 PIA side range underflowed".to_owned())?;
    let side_b = side_b_marker + side_b_delta + 2;
    let mut side_b_end = range.end + side_b_delta;
    patch_xroar_v2_pia_side_b(bytes, side_b, &mut side_b_end, port_b)
}

fn patch_xroar_v2_pia_side_a(
    bytes: &mut Vec<u8>,
    side_a: usize,
    side_end: &mut usize,
) -> Result<(), String> {
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 1, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 2, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 3, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 5, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 13, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 6, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 8, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 9, 0xff)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 10, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 11, 0xff)?;
    Ok(())
}

fn patch_xroar_v2_pia_side_b(
    bytes: &mut Vec<u8>,
    side_b: usize,
    side_end: &mut usize,
    port_b: XroarV2PiaPortB,
) -> Result<(), String> {
    if side_b >= *side_end {
        return Err(format!(
            "XRoar v2 PIA port B range is empty: {side_b}..{}",
            *side_end
        ));
    }
    let out_source = port_b.output & port_b.ddr;
    let out_sink = port_b.output | !port_b.ddr;
    let irq = xroar_v2_pia_irq(port_b.control, port_b.irq1, port_b.irq2);

    patch_xroar_v2_vuint_field_in_range(
        bytes,
        side_b,
        side_end,
        1,
        u32::from(port_b.control & 0x3f),
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 2, u32::from(port_b.ddr))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 3, u32::from(port_b.output))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 5, u32::from(port_b.irq1))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 13, u32::from(port_b.irq2))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 6, u32::from(irq))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 8, u32::from(out_source))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 9, u32::from(out_sink))?;
    Ok(())
}

fn xroar_v2_pia_irq(control: u8, irq1: u8, irq2: u8) -> u8 {
    let irq1_active = if control & 0x01 != 0 { irq1 } else { 0 };
    let irq2_active = if control & 0x28 == 0x08 { irq2 } else { 0 };
    irq1_active | irq2_active
}

fn patch_xroar_v2_vdg(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    const VDG_GREEN: u32 = 0;
    const VDG_WHITE: u32 = 4;
    const VDG_BLACK: u32 = 8;
    const VDG_DARK_GREEN: u32 = 9;
    const VDG_RENDER_CG: u32 = 1;
    const VDG_RENDER_RG: u32 = 2;
    const VDG_TLB: u32 = 120;
    const VDG_TRB: u32 = 112;
    const VDG_LEFT_BORDER_START: u32 = 134;
    const VDG_HS_FALL_DELTA: u32 = 902;
    const GM_NLPR: [u32; 8] = [3, 3, 3, 2, 2, 1, 1, 1];

    let range = xroar_v2_component_range(bytes, b"VDG", Some(b"PIA1"))?;
    let start = find_bytes_from_until(bytes, b"MC6847", range.start, range.end)
        .map(|offset| offset + b"MC6847".len())
        .ok_or_else(|| "XRoar v2 VDG part payload not found".to_owned())?;
    let mut end = range.end;
    let mode = snapshot
        .peripherals
        .map(|peripherals| peripherals.ff22 & 0xf8)
        .unwrap_or(0);
    let gm = u32::from((mode >> 4) & 0x07);
    let gm0 = gm & 1;
    let css = u32::from(mode & 0x08 != 0);
    let graphics = u32::from(mode & 0x80 != 0);
    let is_32byte = u32::from(graphics == 0 || !(gm == 0 || (gm0 != 0 && gm != 7)));
    let cg_colours = if css != 0 { VDG_WHITE } else { VDG_GREEN };
    let fg_colour = if css != 0 { VDG_WHITE } else { VDG_GREEN };
    let bg_colour = if css != 0 { VDG_BLACK } else { VDG_DARK_GREEN };
    let render_mode = if gm0 != 0 {
        VDG_RENDER_RG
    } else {
        VDG_RENDER_CG
    };

    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 6, gm)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 8, graphics)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 9, gm0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 10, css)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 11, css)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 12, css)?;
    upsert_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 14, VDG_HS_FALL_DELTA)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 16, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 17, VDG_LEFT_BORDER_START)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 18, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 21, is_32byte)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 22, gm0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 25, fg_colour)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 26, bg_colour)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 27, cg_colours)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 28, cg_colours)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 30, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 31, render_mode)?;
    patch_xroar_v2_vuint_field_in_range(
        bytes,
        start,
        &mut end,
        33,
        u32::from(!(graphics != 0 && css != 0 && gm0 != 0)),
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 35, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 36, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 37, VDG_TLB)?;
    patch_xroar_v2_vuint_field_in_range(
        bytes,
        start,
        &mut end,
        38,
        if is_32byte != 0 { 32 } else { 16 },
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 39, VDG_TRB)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 40, GM_NLPR[gm as usize])
}

fn patch_xroar_v2_cpu(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, b"CPU", Some(b"SAM"))?;
    let start = find_bytes_from_until(bytes, b"MC6809", range.start, range.end)
        .map(|offset| offset + b"MC6809".len())
        .ok_or_else(|| "XRoar v2 CPU part payload not found".to_owned())?;
    let mut end = range.end;
    let registers = &snapshot.registers;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 1, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 2, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 3, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 4, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 5, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 6, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 7, 1)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 8, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 9, u32::from(registers.cc))?;
    patch_xroar_v2_vuint_field_in_range(
        bytes,
        start,
        &mut end,
        10,
        u32::from(u16::from_be_bytes([registers.a, registers.b])),
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 11, u32::from(registers.dp))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 12, u32::from(registers.x))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 13, u32::from(registers.y))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 14, u32::from(registers.u))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 15, u32::from(registers.s))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 16, u32::from(registers.pc))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 17, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 18, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 19, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 20, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 21, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 22, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 23, 0)
}

fn patch_xroar_v2_sam(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, b"SAM", None)?;
    let start = find_bytes_from_until(bytes, b"SN74LS783", range.start, range.end)
        .map(|offset| offset + b"SN74LS783".len())
        .ok_or_else(|| "XRoar v2 SAM part payload not found".to_owned())?;
    let mut end = range.end;
    let sam = u16::from_be_bytes(xroar_snapshot_sam(snapshot));
    upsert_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 5, u32::from(sam))?;
    let v = u32::from(sam & 0x0007);
    let f = u32::from((sam << 6) & 0xfe00);
    let p = u32::from((sam >> 10) & 0x0001);
    let r = u32::from((sam >> 11) & 0x0003);
    let m = u32::from((sam >> 13) & 0x0003);
    let ty = u32::from((sam >> 15) & 0x0001);
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 6, ty)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 13, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 14, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 15, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 16, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 17, v)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 18, f)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 19, xroar_v2_sam_clr_mode(v))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 28, p)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 29, r)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 30, m)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 36, v)?;
    patch_xroar_v2_sam_vcounters(bytes, start, end, f >> 5)
}

fn patch_xroar_v2_sam_vcounters(
    bytes: &mut Vec<u8>,
    start: usize,
    end: usize,
    b15_5: u32,
) -> Result<(), String> {
    for tag in 20..=27 {
        let value = if tag == 20 { b15_5 } else { 0 };
        patch_xroar_v2_sam_vcounter(bytes, start, end, tag, value)?;
    }
    Ok(())
}

fn patch_xroar_v2_sam_vcounter(
    bytes: &mut Vec<u8>,
    start: usize,
    component_end: usize,
    tag: u32,
    value: u32,
) -> Result<(), String> {
    let vcounter = xroar_v2_find_tag_payload(bytes, start, component_end, tag)
        .ok_or_else(|| format!("XRoar v2 SAM video counter {tag} not found"))?;
    let mut end = xroar_v2_find_tag_payload(bytes, vcounter.payload_end, component_end, tag + 1)
        .map_or(component_end, |next| next.tag_start);
    patch_xroar_v2_vuint_field_in_range(bytes, vcounter.payload_end, &mut end, 1, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, vcounter.payload_end, &mut end, 2, value)?;
    patch_xroar_v2_vuint_field_in_range(bytes, vcounter.payload_end, &mut end, 3, 0)
}

fn xroar_v2_sam_clr_mode(video_mode: u32) -> u32 {
    const CLR_N: u32 = 0;
    const CLR_3: u32 = 1;
    const CLR_4: u32 = 2;
    match video_mode {
        1 | 3 | 5 => CLR_3,
        7 => CLR_N,
        _ => CLR_4,
    }
}

fn patch_xroar_v2_vuint_field_in_range(
    bytes: &mut Vec<u8>,
    start: usize,
    end: &mut usize,
    tag: u32,
    value: u32,
) -> Result<(), String> {
    let delta = patch_xroar_v2_vuint_field(bytes, start, *end, tag, value)?;
    if delta.is_negative() {
        *end = end
            .checked_sub(delta.unsigned_abs())
            .ok_or_else(|| "XRoar v2 component range underflowed while patching".to_owned())?;
    } else {
        *end = end
            .checked_add(delta as usize)
            .ok_or_else(|| "XRoar v2 component range overflowed while patching".to_owned())?;
    }
    Ok(())
}

fn upsert_xroar_v2_vuint_field_in_range(
    bytes: &mut Vec<u8>,
    start: usize,
    end: &mut usize,
    tag: u32,
    value: u32,
) -> Result<(), String> {
    if xroar_v2_find_tag_payload(bytes, start, *end, tag).is_some() {
        patch_xroar_v2_vuint_field_in_range(bytes, start, end, tag, value)
    } else {
        let delta = insert_xroar_v2_vuint_field(bytes, start, tag, value);
        *end = end
            .checked_add(delta)
            .ok_or_else(|| "XRoar v2 component range overflowed while inserting".to_owned())?;
        Ok(())
    }
}

fn insert_xroar_v2_vuint_field(bytes: &mut Vec<u8>, offset: usize, tag: u32, value: u32) -> usize {
    let mut field = xroar_v2_vuint(tag);
    field.extend_from_slice(&xroar_v2_vuint(xroar_v2_vuint(value).len() as u32));
    field.extend_from_slice(&xroar_v2_vuint(value));
    field.push(0);
    let len = field.len();
    bytes.splice(offset..offset, field);
    len
}

fn patch_xroar_v2_vuint_field(
    bytes: &mut Vec<u8>,
    start: usize,
    end: usize,
    tag: u32,
    value: u32,
) -> Result<isize, String> {
    let field = xroar_v2_find_tag_payload(bytes, start, end, tag)
        .ok_or_else(|| format!("XRoar v2 tag {tag} not found"))?;
    let original_len = field.payload_end - field.tag_start;
    let mut replacement = xroar_v2_vuint(tag);
    replacement.extend_from_slice(&xroar_v2_vuint(xroar_v2_vuint(value).len() as u32));
    replacement.extend_from_slice(&xroar_v2_vuint(value));
    let replacement_len = replacement.len();
    bytes.splice(field.tag_start..field.payload_end, replacement);
    Ok(replacement_len as isize - original_len as isize)
}

#[derive(Clone, Copy)]
struct XroarV2Field {
    tag_start: usize,
    payload_start: usize,
    payload_end: usize,
}

fn xroar_v2_find_tag_payload(
    bytes: &[u8],
    start: usize,
    end: usize,
    expected_tag: u32,
) -> Option<XroarV2Field> {
    let mut offset = start;
    while offset < end {
        if bytes[offset] == 0 {
            offset += 1;
            continue;
        }
        let tag_start = offset;
        let (tag, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        let (len, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        let len = usize::try_from(len).ok()?;
        let payload_end = offset.checked_add(len)?;
        if payload_end > end || payload_end > bytes.len() {
            return None;
        }
        if tag == expected_tag {
            return Some(XroarV2Field {
                tag_start,
                payload_start: offset,
                payload_end,
            });
        }
        offset = payload_end;
    }
    None
}

fn xroar_v2_read_vuint(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let mut offset = start;
    let byte0 = *bytes.get(offset)?;
    offset += 1;
    let mut value = u32::from(byte0);
    let mut mask = 0x7f;
    let mut marker = byte0;
    for _ in 1..5 {
        if marker & 0x80 == 0 {
            break;
        }
        marker <<= 1;
        let byte = *bytes.get(offset)?;
        offset += 1;
        mask = (mask << 7) | 0x7f;
        value = (value << 8) | u32::from(byte);
    }
    Some((value & mask, offset))
}

fn xroar_v2_vuint(value: u32) -> Vec<u8> {
    if value <= 0x7f {
        vec![value as u8]
    } else if value <= 0x3fff {
        ((value | 0x8000) as u16).to_be_bytes().to_vec()
    } else if value <= 0x1f_ffff {
        let mut bytes = Vec::with_capacity(3);
        bytes.push(0xc0 | ((value >> 16) as u8));
        bytes.extend_from_slice(&(value as u16).to_be_bytes());
        bytes
    } else if value <= 0x0fff_ffff {
        let mut bytes = Vec::with_capacity(4);
        bytes.extend_from_slice(&((0xe000 | (value >> 16)) as u16).to_be_bytes());
        bytes.extend_from_slice(&(value as u16).to_be_bytes());
        bytes
    } else {
        let mut bytes = Vec::with_capacity(5);
        bytes.push(0xf0);
        bytes.extend_from_slice(&((value >> 16) as u16).to_be_bytes());
        bytes.extend_from_slice(&(value as u16).to_be_bytes());
        bytes
    }
}

fn xroar_v2_component_range(
    bytes: &[u8],
    component: &[u8],
    next_component: Option<&[u8]>,
) -> Result<std::ops::Range<usize>, String> {
    let marker = xroar_v2_component_marker(component);
    let start = find_bytes(bytes, &marker).ok_or_else(|| {
        format!(
            "XRoar v2 component {} not found",
            String::from_utf8_lossy(component)
        )
    })?;
    let structural_end = xroar_v2_open_tag_end(bytes, start + marker.len(), bytes.len())
        .ok_or_else(|| {
            format!(
                "XRoar v2 component {} terminator not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let end = match next_component {
        Some(next) => {
            let next_marker = xroar_v2_component_marker(next);
            let next_start = find_bytes_from(bytes, &next_marker, start + marker.len())
                .ok_or_else(|| {
                    format!(
                        "XRoar v2 component {} terminator {} not found",
                        String::from_utf8_lossy(component),
                        String::from_utf8_lossy(next)
                    )
                })?;
            if next_start > structural_end {
                return Err(format!(
                    "XRoar v2 component {} terminator {} is outside the component tree",
                    String::from_utf8_lossy(component),
                    String::from_utf8_lossy(next)
                ));
            }
            next_start
        }
        None => structural_end,
    };
    Ok(start..end)
}

fn xroar_v2_open_tag_end(bytes: &[u8], mut offset: usize, end: usize) -> Option<usize> {
    let mut open_tags = 1usize;
    let mut tag_open = false;
    while offset < end {
        let (tag, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        if tag == 0 {
            if tag_open {
                tag_open = false;
                continue;
            }
            open_tags = open_tags.checked_sub(1)?;
            if open_tags == 0 {
                return Some(offset);
            }
            continue;
        }
        if tag_open {
            open_tags = open_tags.checked_add(1)?;
        }
        tag_open = true;
        let (len, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        let len = usize::try_from(len).ok()?;
        offset = offset.checked_add(len)?;
        if offset > end || offset > bytes.len() {
            return None;
        }
    }
    None
}

fn xroar_v2_component_marker(component: &[u8]) -> Vec<u8> {
    let mut marker = xroar_v2_vuint(1);
    marker.extend_from_slice(&xroar_v2_vuint(component.len() as u32));
    marker.extend_from_slice(component);
    marker
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    find_bytes_from_until(haystack, needle, start, haystack.len())
}

fn find_bytes_from_until(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
    end: usize,
) -> Option<usize> {
    if needle.is_empty() || start >= end || end > haystack.len() {
        return None;
    }
    haystack[start..end]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

#[cfg(test)]
fn xroar_v1_snapshot_bytes(snapshot: &PcDragonSnapshot) -> Vec<u8> {
    const ID_RAM_PAGE0: u8 = 1;
    const ID_PIA_REGISTERS: u8 = 2;
    const ID_SAM_REGISTERS: u8 = 3;
    const ID_MC6809_STATE: u8 = 4;
    const ID_MACHINECONFIG: u8 = 8;
    const ID_SNAPVERSION: u8 = 9;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"XRoar snapshot.\n\0");
    push_xroar_v1_chunk(&mut bytes, ID_SNAPVERSION, &[1, 0, 8]);
    push_xroar_v1_chunk(&mut bytes, ID_MACHINECONFIG, &[0, 0, 0, 0, 0, 32, 0, 0]);
    push_xroar_v1_chunk(&mut bytes, ID_RAM_PAGE0, &xroar_snapshot_ram(snapshot));
    push_xroar_v1_chunk(&mut bytes, ID_PIA_REGISTERS, &xroar_snapshot_pias(snapshot));
    push_xroar_v1_chunk(&mut bytes, ID_SAM_REGISTERS, &xroar_snapshot_sam(snapshot));
    push_xroar_v1_chunk(&mut bytes, ID_MC6809_STATE, &xroar_snapshot_cpu(snapshot));
    bytes
}

#[cfg(test)]
fn push_xroar_v1_chunk(bytes: &mut Vec<u8>, section: u8, payload: &[u8]) {
    bytes.push(section);
    push_be_u16(bytes, payload.len() as u16);
    bytes.extend_from_slice(payload);
}

fn xroar_snapshot_ram(snapshot: &PcDragonSnapshot) -> Vec<u8> {
    let mut ram = vec![0; 0x8000];
    let start = usize::from(snapshot.load_address);
    if start < ram.len() {
        let len = snapshot.ram.len().min(ram.len() - start);
        ram[start..start + len].copy_from_slice(&snapshot.ram[..len]);
    }
    ram
}

#[cfg(test)]
fn xroar_snapshot_pias(snapshot: &PcDragonSnapshot) -> [u8; 12] {
    let peripherals = snapshot.peripherals.unwrap_or(PcDragonPeripherals {
        ff02: 0,
        ff03: 0,
        ff22: 0,
    });
    [
        0,
        0,
        0,
        0xff,
        peripherals.ff02,
        peripherals.ff03,
        0,
        0,
        0,
        0xff,
        peripherals.ff22,
        0x04,
    ]
}

fn xroar_snapshot_sam(snapshot: &PcDragonSnapshot) -> [u8; 2] {
    const DRAGON32_MEMORY_SIZE_BITS: u16 = 2 << 13;

    let display_register = snapshot
        .display_base
        .map(|base| (base >> 6) & 0x03f8)
        .unwrap_or(0);
    let video_mode = xroar_snapshot_sam_video_mode(
        snapshot
            .peripherals
            .map(|peripherals| peripherals.ff22 & 0xf8)
            .unwrap_or(0),
    );
    let register = DRAGON32_MEMORY_SIZE_BITS | display_register | video_mode;
    register.to_be_bytes()
}

fn xroar_snapshot_sam_video_mode(vdg_mode: u8) -> u16 {
    if vdg_mode & 0x80 == 0 {
        return 0;
    }

    match (vdg_mode >> 4) & 0x07 {
        0 | 1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 | 7 => 6,
        _ => unreachable!("masked VDG GM bits are always 0..=7"),
    }
}

#[cfg(test)]
fn xroar_snapshot_cpu(snapshot: &PcDragonSnapshot) -> Vec<u8> {
    let mut cpu = Vec::with_capacity(20);
    cpu.push(snapshot.registers.cc);
    cpu.push(snapshot.registers.a);
    cpu.push(snapshot.registers.b);
    cpu.push(snapshot.registers.dp);
    push_be_u16(&mut cpu, snapshot.registers.x);
    push_be_u16(&mut cpu, snapshot.registers.y);
    push_be_u16(&mut cpu, snapshot.registers.u);
    push_be_u16(&mut cpu, snapshot.registers.s);
    push_be_u16(&mut cpu, snapshot.registers.pc);
    cpu.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    cpu
}

#[cfg(test)]
fn push_be_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn xroar_start_command(smoke: &CasRuntimeSmoke) -> Option<&str> {
    if smoke.start_command.is_empty() {
        None
    } else {
        Some(smoke.start_command.as_str())
    }
}

fn xroar_type_text(command: &str, start_command: Option<&str>) -> Result<String, String> {
    match (command, start_command) {
        ("CLOAD", Some("RUN")) => Ok("CLOAD\rRUN\r".to_owned()),
        ("CLOAD", None) => Ok("CLOAD\r".to_owned()),
        ("CLOADM", Some("EXEC")) => Ok("CLOADM\rEXEC\r".to_owned()),
        ("CLOADM", None) => Ok("CLOADM\r".to_owned()),
        _ => Err(format!(
            "unsupported XRoar reference command {command:?} with start {start_command:?}"
        )),
    }
}

fn default_xroar_motoroff(command: &str, start_command: Option<&str>) -> usize {
    match (command, start_command) {
        ("CLOAD", None) => 3,
        _ => 2,
    }
}

fn xroar_motoroff_candidates(
    configured: Option<usize>,
    command: &str,
    start_command: Option<&str>,
) -> Vec<usize> {
    if let Some(motoroff) = configured {
        return vec![motoroff];
    }

    let mut candidates = vec![default_xroar_motoroff(command, start_command), 2, 3];
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn xroar_capture_is_better(
    candidate: &XroarReferenceCapture,
    current: &XroarReferenceCapture,
) -> bool {
    match (&candidate.comparison, &current.comparison) {
        (Some(candidate), Some(current)) => {
            (
                candidate.differing_pixels,
                candidate.max_channel_delta,
                candidate.mean_abs_channel_delta.to_bits(),
            ) < (
                current.differing_pixels,
                current.max_channel_delta,
                current.mean_abs_channel_delta.to_bits(),
            )
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => false,
    }
}

fn format_seconds(seconds: f32) -> String {
    if seconds.fract() == 0.0 {
        format!("{seconds:.0}")
    } else {
        seconds.to_string()
    }
}

fn xroar_snapshot_settle_seconds(cycles: u64) -> f32 {
    cycles as f32 / DRAGON_CPU_HZ as f32
}

fn failed_runtime_smoke(command: &str, error: &str) -> CasRuntimeSmoke {
    let start_command = if command == "CLOAD" {
        "RUN"
    } else if command == "CLOADM" {
        "EXEC"
    } else {
        ""
    };
    CasRuntimeSmoke {
        command: command.to_owned(),
        classification: RuntimeSmokeClassification::Error,
        load_result: "error".to_owned(),
        start_command: start_command.to_owned(),
        start_result: "not-run".to_owned(),
        load_visible_change: false,
        load_basic_error: false,
        load_pc_before: 0,
        load_pc_after: 0,
        start_pc_after: 0,
        load_video: DragonVideoState::default(),
        start_video: DragonVideoState::default(),
        tape_position_bits: 0,
        tape_length_bits: 0,
        tape_finished: false,
        visible_change_after_start: false,
        start_video_changed: false,
        start_settle_visible_change: false,
        basic_error: false,
        load_screen_text: Vec::new(),
        screen_text: Vec::new(),
        error: Some(error.to_owned()),
        load_screenshot: None,
        load_audio: None,
        start_screenshot: None,
        start_audio: None,
        idle_after_start_frames: 0,
        idle_visible_change: false,
        idle_screen_text: None,
        idle_screenshot: None,
        joystick_steps: Vec::new(),
        joystick_visible_change: false,
        joystick_screen_text: None,
        joystick_screenshot: None,
        xroar_reference_screenshot: None,
        xroar_reference_motoroff: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
    }
}

fn load_wait_frame_budget(tape_length_bits: u64) -> u32 {
    let scaled = tape_length_bits / 16;
    u32::try_from(scaled.clamp(4_500, 20_000)).map_or(20_000, |frames| frames)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapeLoadStop {
    MotorOff,
    TapeFinished,
    FrameLimit,
}

fn wait_for_tape_load_stop(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    max_frames: u32,
) -> Result<TapeLoadStop, String> {
    for _ in 0..=max_frames {
        if !query_bool(session, "dragon.tape.motor_on")? {
            return Ok(TapeLoadStop::MotorOff);
        }
        if query_bool(session, "dragon.tape.finished")? {
            return Ok(TapeLoadStop::TapeFinished);
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for tape load stop: {err}"))?;
    }
    Ok(TapeLoadStop::FrameLimit)
}

fn boot_runtime_session(
    rom: &[u8; ROM_SIZE],
) -> Result<HeadlessSession<DragonRuntime, DragonSessionQueryProvider>, String> {
    let runtime = DragonRuntime::new(Model::Dragon32Pal, rom)?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );
    let boot = session
        .wait_for_boot(BOOT_FRAME_BUDGET)
        .map_err(|err| format!("Dragon BASIC boot did not complete: {err}"))?;
    if boot.reason != "basic-ok-prompt" {
        return Err(format!("unexpected boot reason {}", boot.reason));
    }
    session
        .run_frames(30)
        .map_err(|err| format!("Dragon runtime did not idle after boot: {err}"))?;
    Ok(session)
}

fn type_basic_command(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    command: &str,
) -> Result<(), String> {
    for ch in command.chars() {
        tap_key(session, &ch.to_ascii_lowercase().to_string())?;
    }
    tap_key(session, "enter")
}

fn tap_key(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
) -> Result<(), String> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames(KEY_EDGE_FRAMES)
        .map_err(|err| format!("key press {name} failed: {err}"))?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session
        .run_frames(KEY_EDGE_FRAMES)
        .map_err(|err| format!("key release {name} failed: {err}"))?;
    Ok(())
}

fn apply_smoke_joystick_steps(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    steps: &[SmokeJoystickStep],
) -> Result<(), String> {
    for step in steps {
        let name = step.control.name();
        if step.control == SmokeJoystickControl::Idle {
            session
                .run_frames(step.frames)
                .map_err(|err| format!("joystick idle for {} frames failed: {err}", step.frames))?;
            continue;
        }
        session.queue_input(InputEvent::Button {
            port: step.port,
            name: name.into(),
            pressed: true,
        });
        session.run_frames(step.frames).map_err(|err| {
            format!(
                "joystick press port {} {name} for {} frames failed: {err}",
                step.port, step.frames
            )
        })?;
        session.queue_input(InputEvent::Button {
            port: step.port,
            name: name.into(),
            pressed: false,
        });
        session
            .run_frames(KEY_EDGE_FRAMES)
            .map_err(|err| format!("joystick release port {} {name} failed: {err}", step.port))?;
    }
    Ok(())
}

fn wait_for_tape_position_above(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    threshold: u64,
    max_frames: u32,
) -> Result<u64, String> {
    for _ in 0..=max_frames {
        let position = query_u64(session, "dragon.tape.position_bits")?;
        if position > threshold {
            return Ok(position);
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for tape movement: {err}"))?;
    }
    query_u64(session, "dragon.tape.position_bits")
}

fn wait_for_screenshot_change(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    before: &[u8],
    max_frames: u32,
) -> Result<Option<Vec<u8>>, String> {
    for _ in 0..=max_frames {
        let current = session
            .screenshot_png_bytes()
            .map_err(|err| format!("failed to capture frame: {err}"))?;
        if current != before {
            return Ok(Some(current));
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for frame change: {err}"))?;
    }
    Ok(None)
}

fn screen_text_lines(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<Vec<String>, String> {
    let result = session
        .query("screen.text.lines")
        .map_err(|err| format!("screen.text.lines query failed: {err}"))?;
    result
        .value
        .as_array()
        .ok_or_else(|| "screen.text.lines was not an array".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "screen.text.lines entry was not a string".to_owned())
        })
        .collect()
}

fn query_u64(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<u64, String> {
    session
        .query(path)
        .map_err(|err| format!("{path} query failed: {err}"))?
        .value
        .as_u64()
        .ok_or_else(|| format!("{path} query was not an unsigned integer"))
}

fn query_u16(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<u16, String> {
    let value = query_u64(session, path)?;
    u16::try_from(value).map_err(|err| format!("{path} query value {value} is not u16: {err}"))
}

fn query_u8(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<u8, String> {
    let value = query_u64(session, path)?;
    u8::try_from(value).map_err(|err| format!("{path} query value {value} is not u8: {err}"))
}

fn query_bool(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<bool, String> {
    session
        .query(path)
        .map_err(|err| format!("{path} query failed: {err}"))?
        .value
        .as_bool()
        .ok_or_else(|| format!("{path} query was not a boolean"))
}

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("tape")
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

fn matches_ignore_ascii_case(value: &str, expected: &[&str]) -> bool {
    expected
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

impl From<&CasHeader> for CasHeaderSummary {
    fn from(header: &CasHeader) -> Self {
        Self {
            name: header.name.clone(),
            file_type: cas_file_type_label(header.file_type).to_owned(),
            ascii_flag: header.ascii_flag,
            gap_flag: header.gap_flag,
            first_address: header.first_address,
            second_address: header.second_address,
        }
    }
}

fn cas_file_type_label(file_type: CasFileType) -> &'static str {
    match file_type {
        CasFileType::Basic => "basic",
        CasFileType::Data => "data",
        CasFileType::MachineCode => "machine-code",
        CasFileType::Unknown(_) => "unknown",
    }
}

fn load_rom(path: &Path) -> Result<[u8; ROM_SIZE], String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return load_rom_from_zip(path);
    }

    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    exact_rom_from_bytes(path, bytes)
}

fn load_rom_from_zip(path: &Path) -> Result<[u8; ROM_SIZE], String> {
    let file =
        fs::File::open(path).map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut candidate = None;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            format!(
                "failed to read zip entry {index} in {}: {err}",
                path.display()
            )
        })?;
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| format!("failed to read {entry_name} from {}: {err}", path.display()))?;

        if bytes.len() == ROM_SIZE {
            if candidate.is_some() {
                return Err(format!(
                    "{} contains multiple {ROM_SIZE}-byte ROM candidates",
                    path.display()
                ));
            }
            candidate = Some(
                bytes
                    .try_into()
                    .map_err(|_| format!("{entry_name} was not exactly {ROM_SIZE} bytes"))?,
            );
        }
    }

    candidate.ok_or_else(|| {
        format!(
            "{} did not contain a {ROM_SIZE}-byte Dragon ROM",
            path.display()
        )
    })
}

fn exact_rom_from_bytes(path: &Path, bytes: Vec<u8>) -> Result<[u8; ROM_SIZE], String> {
    let actual_len = bytes.len();
    bytes.try_into().map_err(|_| {
        format!(
            "{} must be exactly {ROM_SIZE} bytes; got {actual_len}",
            path.display()
        )
    })
}

fn load_cartridge(path: &Path) -> Result<DragonPakImage, String> {
    let loaded = read_media_asset(path, MediaKind::Cartridge)
        .map_err(|err| format!("failed to load Dragon cartridge {}: {err}", path.display()))?;
    parse_dragon_pak(&loaded.bytes)
        .map_err(|err| format!("failed to parse Dragon cartridge {}: {err}", path.display()))
}

fn load_snapshot(path: &Path) -> Result<PcDragonSnapshot, String> {
    let loaded = read_media_asset(path, MediaKind::Snapshot)
        .map_err(|err| format!("failed to load Dragon snapshot {}: {err}", path.display()))?;
    parse_pcdragon_snapshot(&loaded.bytes)
        .map_err(|err| format!("failed to parse Dragon snapshot {}: {err}", path.display()))
}

const fn machine_cartridge_kind(kind: ParsedDragonCartridgeKind) -> DragonCartridgeKind {
    match kind {
        ParsedDragonCartridgeKind::Rom => DragonCartridgeKind::Rom,
        ParsedDragonCartridgeKind::GamesMaster => DragonCartridgeKind::GamesMaster,
    }
}

#[derive(Clone, Copy, Debug)]
struct HarnessRunOptions<'a> {
    cartridge: Option<&'a DragonPakImage>,
    snapshot: Option<&'a PcDragonSnapshot>,
    cycle_limit: u64,
    trace_limit: usize,
    fetch_watch: Option<AddressRange>,
    write_watch: Option<AddressRange>,
    dump_text: bool,
    dump_text_framebuffer: bool,
    capture_framebuffer: bool,
    capture_framebuffer_phase: SmokeScreenshotPhase,
}

fn run_harness_with_keyboard(
    rom: &[u8; ROM_SIZE],
    keyboard: DragonKeyboard,
    options: HarnessRunOptions<'_>,
) -> HarnessReport {
    let mut machine = Dragon32::new_with_keyboard(rom, keyboard);
    if let Some(cartridge) = options.cartridge {
        machine.load_cartridge(machine_cartridge_kind(cartridge.kind), &cartridge.rom, true);
    }
    if let Some(snapshot) = options.snapshot {
        machine.load_pcdragon_snapshot(
            snapshot.load_address,
            &snapshot.ram,
            machine_dragon_32::DragonSnapshotRegisters {
                pc: snapshot.registers.pc,
                x: snapshot.registers.x,
                y: snapshot.registers.y,
                u: snapshot.registers.u,
                s: snapshot.registers.s,
                dp: snapshot.registers.dp,
                b: snapshot.registers.b,
                a: snapshot.registers.a,
                cc: snapshot.registers.cc,
            },
            snapshot
                .peripherals
                .map(|peripherals| machine_dragon_32::DragonSnapshotPeripherals {
                    ff02: peripherals.ff02,
                    ff03: peripherals.ff03,
                    ff22: peripherals.ff22,
                }),
            snapshot.display_base,
        );
    }
    let mut run_options = RunOptions::new(options.trace_limit);
    run_options.fetch_watch = options.fetch_watch;
    run_options.write_watch = options.write_watch;
    let report = machine.run_cycles_with_options(options.cycle_limit, run_options);
    let mut framebuffer_cycles = None;
    if options.capture_framebuffer
        && matches!(report.stop_reason, StopReason::CycleLimit)
        && matches!(
            options.capture_framebuffer_phase,
            SmokeScreenshotPhase::CompletedFrame
        )
    {
        run_to_completed_video_frame(&mut machine);
    }
    let text_screen =
        (options.dump_text || options.dump_text_framebuffer).then(|| machine.capture_text_screen());
    let text_screen_text = text_screen
        .as_ref()
        .filter(|_| options.dump_text)
        .map(|screen| screen.to_plain_text());
    let text_framebuffer = options
        .dump_text_framebuffer
        .then(|| machine.render_visible_text_argb(TextPalette::default()));
    let framebuffer = options.capture_framebuffer.then(|| {
        framebuffer_cycles = Some(machine.cycles());
        machine.beam_visible_argb().to_vec()
    });

    report.into_harness_report(
        text_screen_text,
        text_framebuffer,
        framebuffer,
        framebuffer_cycles,
    )
}

fn run_to_completed_video_frame(machine: &mut Dragon32) {
    let phase = machine.cycles() % DRAGON_FRAME_CYCLES;
    if phase == 0 {
        return;
    }
    let remaining = DRAGON_FRAME_CYCLES - phase;
    let _ = machine.run_cycles(remaining, 0);
}

trait IntoHarnessReport {
    fn into_harness_report(
        self,
        text_screen: Option<String>,
        text_framebuffer: Option<Vec<u32>>,
        framebuffer: Option<Vec<u32>>,
        framebuffer_cycles: Option<u64>,
    ) -> HarnessReport;
}

impl IntoHarnessReport for RunReport {
    fn into_harness_report(
        self,
        text_screen: Option<String>,
        text_framebuffer: Option<Vec<u32>>,
        framebuffer: Option<Vec<u32>>,
        framebuffer_cycles: Option<u64>,
    ) -> HarnessReport {
        HarnessReport {
            stop_reason: self.stop_reason,
            cycles: self.cycles,
            instructions: self.instructions,
            pc: self.pc,
            addr: self.addr,
            rw: self.rw,
            last_fetch: self.last_fetch,
            trace: self.trace,
            dropped_trace: self.dropped_trace,
            device_accesses: self.device_accesses,
            dropped_device_accesses: self.dropped_device_accesses,
            readonly_writes: self.readonly_writes,
            dropped_readonly_writes: self.dropped_readonly_writes,
            watched_fetches: self.watched_fetches,
            dropped_watched_fetches: self.dropped_watched_fetches,
            watched_writes: self.watched_writes,
            dropped_watched_writes: self.dropped_watched_writes,
            pia_signals: self.pia_signals,
            dropped_pia_signals: self.dropped_pia_signals,
            interrupt_lines: self.interrupt_lines,
            dropped_interrupt_lines: self.dropped_interrupt_lines,
            interrupt_accepts: self.interrupt_accepts,
            dropped_interrupt_accepts: self.dropped_interrupt_accepts,
            vdg_samples: self.vdg_samples,
            dropped_vdg_samples: self.dropped_vdg_samples,
            vdg_mode_writes: self.vdg_mode_writes,
            dropped_vdg_mode_writes: self.dropped_vdg_mode_writes,
            text_screen_base: self.text_screen_base,
            text_screen,
            text_framebuffer,
            framebuffer,
            framebuffer_cycles,
        }
    }
}

fn print_report(report: &HarnessReport) {
    println!("dragon harness summary");
    println!("status: {}", format_stop_reason(report.stop_reason));
    println!("cycles: {}", report.cycles);
    println!("instructions: {}", report.instructions);
    println!("pc: ${:04X}", report.pc);
    println!("text screen base: ${:04X}", report.text_screen_base);
    println!(
        "bus: addr=${:04X} rw={}",
        report.addr,
        if report.rw { "read" } else { "write" }
    );
    if let Some(fetch) = report.last_fetch {
        println!(
            "last fetch: cycle={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.pc, fetch.opcode
        );
    }
    if report.dropped_trace != 0 {
        println!("trace dropped: {}", report.dropped_trace);
    }
    if report.dropped_device_accesses != 0 {
        println!(
            "device accesses dropped: {}",
            report.dropped_device_accesses
        );
    }
    println!("device accesses:");
    for access in &report.device_accesses {
        println!(
            "  cycle={} {} {} addr=${:04X} value=${:02X}",
            access.cycle,
            if access.rw { "read" } else { "write" },
            format_device_region(access.device),
            access.addr,
            access.value
        );
    }
    if report.dropped_readonly_writes != 0 {
        println!(
            "readonly writes dropped: {}",
            report.dropped_readonly_writes
        );
    }
    println!("readonly writes:");
    for write in &report.readonly_writes {
        println!(
            "  cycle={} addr=${:04X} value=${:02X}",
            write.cycle, write.addr, write.value
        );
    }
    if report.dropped_watched_fetches != 0 {
        println!(
            "watched fetches dropped: {}",
            report.dropped_watched_fetches
        );
    }
    println!("watched fetches:");
    for fetch in &report.watched_fetches {
        println!(
            "  cycle={} pc=${:04X} opcode=${:02X} {}",
            fetch.cycle,
            fetch.pc,
            fetch.opcode,
            format_cpu_registers(fetch.regs)
        );
    }
    if report.dropped_watched_writes != 0 {
        println!("watched writes dropped: {}", report.dropped_watched_writes);
    }
    println!("watched writes:");
    for write in &report.watched_writes {
        let instruction_pc = write
            .instruction_pc
            .map_or("????".to_owned(), |pc| format!("{pc:04X}"));
        println!(
            "  cycle={} frame_tick={} line={:?} active_y={:?} active_x={:?} instr=${} addr=${:04X} value=${:02X} {}",
            write.cycle,
            write.frame_master_tick,
            write.line,
            write.active_y,
            write.active_x,
            instruction_pc,
            write.addr,
            write.value,
            format_cpu_registers(write.regs)
        );
    }
    if report.dropped_pia_signals != 0 {
        println!("pia signals dropped: {}", report.dropped_pia_signals);
    }
    println!("pia signals:");
    for signal in &report.pia_signals {
        println!(
            "  cycle={} {} {:?} level={} cra=${:02X} crb=${:02X} irq_a={} irq_b={}",
            signal.cycle,
            format_device_region(signal.device),
            signal.signal,
            signal.level,
            signal.control_a,
            signal.control_b,
            signal.irq_a,
            signal.irq_b
        );
    }
    if report.dropped_interrupt_lines != 0 {
        println!(
            "interrupt lines dropped: {}",
            report.dropped_interrupt_lines
        );
    }
    println!("interrupt lines:");
    for line in &report.interrupt_lines {
        println!(
            "  cycle={} {} level={} pc=${:04X} cc=${:02X}",
            line.cycle,
            format_interrupt_kind(line.kind),
            line.level,
            line.pc,
            line.cc
        );
    }
    if report.dropped_interrupt_accepts != 0 {
        println!(
            "interrupt accepts dropped: {}",
            report.dropped_interrupt_accepts
        );
    }
    println!("interrupt accepts:");
    for accept in &report.interrupt_accepts {
        println!(
            "  cycle={} {} pc=${:04X} cc=${:02X}",
            accept.cycle,
            format_interrupt_kind(accept.kind),
            accept.pc,
            accept.cc
        );
    }
    if report.dropped_vdg_samples != 0 {
        println!("vdg samples dropped: {}", report.dropped_vdg_samples);
    }
    println!("vdg samples:");
    for sample in &report.vdg_samples {
        println!(
            "  cycle={} frame_tick={} line={} active_y={} byte={} base=${:04X} sam_mode=${:02X} sam_offset=${:02X} pb=${:02X} ag={} css={} int_ext={} gm={}",
            sample.cycle,
            sample.frame_master_tick,
            sample.line,
            sample.active_y,
            sample.byte_x,
            sample.display_base,
            sample.sam_video_mode,
            sample.sam_display_offset,
            sample.pia1_pb,
            sample.graphics,
            sample.css,
            sample.int_ext,
            sample.gm
        );
    }
    if report.dropped_vdg_mode_writes != 0 {
        println!(
            "vdg mode writes dropped: {}",
            report.dropped_vdg_mode_writes
        );
    }
    println!("vdg mode writes:");
    for write in &report.vdg_mode_writes {
        println!(
            "  cycle={} frame_tick={} line={:?} active_y={:?} active_x={:?} addr=${:04X} value=${:02X} ag={} css={} int_ext={} gm={}",
            write.cycle,
            write.frame_master_tick,
            write.line,
            write.active_y,
            write.active_x,
            write.addr,
            write.value,
            write.graphics,
            write.css,
            write.int_ext,
            write.gm
        );
    }
    if let Some(text_screen) = &report.text_screen {
        println!("text screen:");
        for line in text_screen.lines() {
            println!("  |{line}|");
        }
    }
    if let Some(framebuffer) = &report.text_framebuffer {
        let foreground_pixels = framebuffer
            .iter()
            .filter(|&&pixel| pixel == TextPalette::default().foreground)
            .count();
        println!(
            "text framebuffer: {}x{} foreground-pixels={}",
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, foreground_pixels
        );
    }
    if let Some(framebuffer) = &report.framebuffer {
        println!(
            "framebuffer: {}x{} pixels={}",
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
            TEXT_VISIBLE_FRAMEBUFFER_HEIGHT,
            framebuffer.len()
        );
    }
    println!("trace:");
    for fetch in &report.trace {
        println!(
            "  cycle={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.pc, fetch.opcode
        );
    }
}

fn format_interrupt_kind(kind: CpuInterruptKind) -> &'static str {
    match kind {
        CpuInterruptKind::Irq => "irq",
        CpuInterruptKind::Firq => "firq",
    }
}

fn format_cpu_registers(regs: CpuRegisterTrace) -> String {
    format!(
        "pc=${:04X} a=${:02X} b=${:02X} dp=${:02X} x=${:04X} y=${:04X} u=${:04X} s=${:04X} cc=${:02X}",
        regs.pc, regs.a, regs.b, regs.dp, regs.x, regs.y, regs.u, regs.s, regs.cc
    )
}

fn write_text_png(path: &Path, framebuffer: &[u32]) -> Result<(), String> {
    if framebuffer.len() != TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT {
        return Err(format!(
            "text framebuffer has {} pixels; expected {}",
            framebuffer.len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        ));
    }

    let file = fs::File::create(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(
        writer,
        TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
        TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|err| format!("failed to write PNG header for {}: {err}", path.display()))?;

    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &argb in framebuffer {
        rgba.push(((argb >> 16) & 0xFF) as u8);
        rgba.push(((argb >> 8) & 0xFF) as u8);
        rgba.push((argb & 0xFF) as u8);
        rgba.push(((argb >> 24) & 0xFF) as u8);
    }
    png_writer
        .write_image_data(&rgba)
        .map_err(|err| format!("failed to write PNG data for {}: {err}", path.display()))?;
    png_writer
        .finish()
        .map_err(|err| format!("failed to finish PNG {}: {err}", path.display()))?;
    Ok(())
}

fn write_screenshot_png(
    path: &Path,
    framebuffer: &[u32],
    format: SmokeScreenshotFormat,
    cycles: u64,
) -> Result<(), String> {
    let rgba = argb_to_rgba(framebuffer)?;
    let png = match format {
        SmokeScreenshotFormat::Diagnostic => encode_rgba_png(
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
            TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
            &rgba,
        )?,
        SmokeScreenshotFormat::XroarZoomed => {
            let frame = CapturedFrame {
                timestamp: MachineTime::new(cycles),
                format: PixelFormat::Rgba8888,
                width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
                height: TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
                palette: None,
                pixels: rgba,
            };
            xroar_zoomed_png_bytes(&frame)?
        }
    };
    fs::write(path, png).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn argb_to_rgba(framebuffer: &[u32]) -> Result<Vec<u8>, String> {
    if framebuffer.len() != TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT {
        return Err(format!(
            "framebuffer has {} pixels; expected {}",
            framebuffer.len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        ));
    }

    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &argb in framebuffer {
        rgba.push(((argb >> 16) & 0xFF) as u8);
        rgba.push(((argb >> 8) & 0xFF) as u8);
        rgba.push((argb & 0xFF) as u8);
        rgba.push(((argb >> 24) & 0xFF) as u8);
    }
    Ok(rgba)
}

fn format_stop_reason(reason: StopReason) -> String {
    match reason {
        StopReason::CycleLimit => "cycle-limit".to_owned(),
        StopReason::CpuHalted => "cpu-halted".to_owned(),
    }
}

fn format_device_region(device: DeviceRegion) -> &'static str {
    match device {
        DeviceRegion::Pia0 => "pia0",
        DeviceRegion::Pia1 => "pia1",
        DeviceRegion::Sam => "sam",
        DeviceRegion::Cartridge => "cartridge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    #[test]
    fn harness_can_dump_sam_selected_text_screen() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$01
        rom[0x0001] = 0x01;
        rom[0x0002] = 0xB7; // STA $FFC9: set SAM F1, selecting text base $0400.
        rom[0x0003] = 0xFF;
        rom[0x0004] = 0xC9;
        rom[0x0005] = 0xB7; // STA $0400: MC6847 diagnostic 'A'.
        rom[0x0006] = 0x04;
        rom[0x0007] = 0x00;
        rom[0x0008] = 0x86; // LDA #$02
        rom[0x0009] = 0x02;
        rom[0x000A] = 0xB7; // STA $0401: MC6847 diagnostic 'B'.
        rom[0x000B] = 0x04;
        rom[0x000C] = 0x01;
        rom[0x000D] = 0x01;

        let report = run_harness_with_keyboard(
            &rom,
            DragonKeyboard::new(),
            HarnessRunOptions {
                cartridge: None,
                snapshot: None,
                cycle_limit: 128,
                trace_limit: 8,
                fetch_watch: None,
                write_watch: None,
                dump_text: true,
                dump_text_framebuffer: true,
                capture_framebuffer: true,
                capture_framebuffer_phase: SmokeScreenshotPhase::Immediate,
            },
        );

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.text_screen_base, 0x0400);
        assert_eq!(
            report
                .text_screen
                .as_deref()
                .expect("text dump should be captured")
                .lines()
                .next(),
            Some("AB@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@")
        );
        assert_eq!(
            report
                .text_framebuffer
                .as_ref()
                .expect("text framebuffer should be captured")
                .len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        );
        assert_eq!(
            report
                .framebuffer
                .as_ref()
                .expect("framebuffer should be captured")
                .len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        );
    }

    #[test]
    fn completed_frame_screenshot_phase_advances_to_video_frame_boundary() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x20; // BRA -2.
        rom[0x0001] = 0xFE;

        let report = run_harness_with_keyboard(
            &rom,
            DragonKeyboard::new(),
            HarnessRunOptions {
                cartridge: None,
                snapshot: None,
                cycle_limit: 1,
                trace_limit: 0,
                fetch_watch: None,
                write_watch: None,
                dump_text: false,
                dump_text_framebuffer: false,
                capture_framebuffer: true,
                capture_framebuffer_phase: SmokeScreenshotPhase::CompletedFrame,
            },
        );

        assert_eq!(report.stop_reason, StopReason::CycleLimit);
        assert_eq!(report.cycles, 1);
        assert_eq!(report.framebuffer_cycles, Some(DRAGON_FRAME_CYCLES));
    }

    #[test]
    fn cli_requires_rom_path() {
        let err = parse_cli(Vec::<String>::new()).expect_err("missing ROM should fail");

        assert!(err.contains("missing required --rom"));
    }

    #[test]
    fn cli_parses_hex_cycles_and_trace_limit() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cycles".to_owned(),
            "0x20".to_owned(),
            "--trace-limit".to_owned(),
            "3".to_owned(),
            "--dump-text".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.rom, PathBuf::from("dragon32.rom"));
        assert_eq!(cli.cycles, 32);
        assert_eq!(cli.trace_limit, 3);
        assert_eq!(cli.fetch_watch, None);
        assert_eq!(cli.write_watch, None);
        assert_eq!(cli.pressed_keys, Vec::new());
        assert!(cli.dump_text);
    }

    #[test]
    fn cli_parses_write_watch_range() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--watch-fetch".to_owned(),
            "0x1c00".to_owned(),
            "--watch-write".to_owned(),
            "0x2c00-0x2cff".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.fetch_watch, Some(AddressRange::new(0x1C00, 0x1C00)));
        assert_eq!(cli.write_watch, Some(AddressRange::new(0x2C00, 0x2CFF)));
    }

    #[test]
    fn cli_parses_cartridge_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cart".to_owned(),
            "game.dgn".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.cart, Some(PathBuf::from("game.dgn")));
    }

    #[test]
    fn cli_parses_snapshot_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--snapshot".to_owned(),
            "game.pak".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.snapshot, Some(PathBuf::from("game.pak")));
    }

    #[test]
    fn cli_parses_direct_screenshot_options() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--screenshot".to_owned(),
            "screen.png".to_owned(),
            "--screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
            "--screenshot-phase".to_owned(),
            "completed-frame".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.screenshot, Some(PathBuf::from("screen.png")));
        assert_eq!(cli.screenshot_format, SmokeScreenshotFormat::XroarZoomed);
        assert_eq!(cli.screenshot_phase, SmokeScreenshotPhase::CompletedFrame);
    }

    #[test]
    fn cli_parses_raw_matrix_key_presses() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--press-matrix".to_owned(),
            "2,3".to_owned(),
            "--press-matrix".to_owned(),
            "4,5".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.pressed_keys,
            vec![MatrixKey::new(2, 3), MatrixKey::new(4, 5),]
        );
    }

    #[test]
    fn cli_parses_smoke_matrix_options() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
            "--smoke-run-limit".to_owned(),
            "3".to_owned(),
            "--smoke-report".to_owned(),
            "report.json".to_owned(),
            "--smoke-screenshot-dir".to_owned(),
            "screens".to_owned(),
            "--smoke-screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
            "--smoke-audio-dir".to_owned(),
            "audio".to_owned(),
            "--smoke-joystick".to_owned(),
            "1,fire,20".to_owned(),
            "--smoke-joystick".to_owned(),
            "1,right,30".to_owned(),
            "--smoke-idle-after-start".to_owned(),
            "492".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.smoke_root, Some(PathBuf::from("tapes")));
        assert_eq!(cli.smoke_run_limit, 3);
        assert_eq!(cli.smoke_report, Some(PathBuf::from("report.json")));
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
        assert_eq!(cli.smoke_audio_dir, Some(PathBuf::from("audio")));
        assert_eq!(cli.smoke_idle_after_start, 492);
        assert_eq!(
            cli.smoke_joystick,
            vec![
                SmokeJoystickStep {
                    port: 1,
                    control: SmokeJoystickControl::Fire,
                    frames: 20,
                },
                SmokeJoystickStep {
                    port: 1,
                    control: SmokeJoystickControl::Right,
                    frames: 30,
                },
            ]
        );
        assert_eq!(
            cli.smoke_screenshot_format,
            SmokeScreenshotFormat::XroarZoomed
        );
    }

    #[test]
    fn cli_parses_snapshot_smoke_root() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--snapshot-smoke-root".to_owned(),
            "paks".to_owned(),
            "--smoke-run-limit".to_owned(),
            "5".to_owned(),
            "--smoke-screenshot-dir".to_owned(),
            "screens".to_owned(),
            "--smoke-screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
            "--screenshot-phase".to_owned(),
            "completed-frame".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.snapshot_smoke_root, Some(PathBuf::from("paks")));
        assert_eq!(cli.smoke_run_limit, 5);
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
        assert_eq!(
            cli.smoke_screenshot_format,
            SmokeScreenshotFormat::XroarZoomed
        );
        assert_eq!(cli.screenshot_phase, SmokeScreenshotPhase::CompletedFrame);
    }

    #[test]
    fn cli_rejects_mixed_tape_and_snapshot_smoke_roots() {
        let err = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
            "--snapshot-smoke-root".to_owned(),
            "paks".to_owned(),
        ])
        .expect_err("mixed smoke roots should fail");

        assert!(err.contains("cannot be used together"));
    }

    #[test]
    fn snapshot_smoke_classification_uses_stop_and_visible_pixels() {
        assert_eq!(
            classify_snapshot_smoke(StopReason::CycleLimit, 2, 1),
            SnapshotSmokeClassification::RunningVisible
        );
        assert_eq!(
            classify_snapshot_smoke(StopReason::CycleLimit, 1, 0),
            SnapshotSmokeClassification::RunningBlank
        );
        assert_eq!(
            classify_snapshot_smoke(StopReason::CpuHalted, 2, 1),
            SnapshotSmokeClassification::HaltedVisible
        );
        assert_eq!(
            framebuffer_stats(&[0xff00_0000, 0xff00_0000, 0xffff_ffff]),
            (2, 1)
        );
    }

    #[test]
    fn pcdragon_snapshot_can_be_converted_to_xroar_v1_snapshot() {
        let snapshot = PcDragonSnapshot {
            ram: vec![0xaa, 0xbb].into_boxed_slice(),
            load_address: 0x2000,
            registers: format_dragon_pak::PcDragonRegisters {
                pc: 0x1234,
                x: 0x5678,
                y: 0x9abc,
                u: 0xdef0,
                s: 0x2468,
                dp: 0x12,
                b: 0x34,
                a: 0x56,
                cc: 0x87,
            },
            peripherals: Some(PcDragonPeripherals {
                ff02: 0xde,
                ff03: 0xb5,
                ff22: 0xfc,
            }),
            display_base: Some(0x0600),
        };

        let bytes = xroar_v1_snapshot_bytes(&snapshot);

        assert!(bytes.starts_with(b"XRoar snapshot.\n\0"));
        assert_eq!(xroar_v1_chunk(&bytes, 3), Some(&[0x40, 0x1e][..]));
        assert_eq!(
            xroar_v1_chunk(&bytes, 4).map(|chunk| &chunk[..14]),
            Some(
                &[
                    0x87, 0x56, 0x34, 0x12, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x24, 0x68, 0x12,
                    0x34,
                ][..]
            )
        );
        let ram = xroar_v1_chunk(&bytes, 1).expect("RAM chunk should exist");
        assert_eq!(ram[0x2000], 0xaa);
        assert_eq!(ram[0x2001], 0xbb);
    }

    #[test]
    fn xroar_sam_video_mode_uses_sam_counter_modes_not_vdg_gm_bits() {
        assert_eq!(xroar_snapshot_sam_video_mode(0x00), 0);
        assert_eq!(xroar_snapshot_sam_video_mode(0x80), 1);
        assert_eq!(xroar_snapshot_sam_video_mode(0x90), 1);
        assert_eq!(xroar_snapshot_sam_video_mode(0xa0), 2);
        assert_eq!(xroar_snapshot_sam_video_mode(0xb0), 3);
        assert_eq!(xroar_snapshot_sam_video_mode(0xc0), 4);
        assert_eq!(xroar_snapshot_sam_video_mode(0xd0), 5);
        assert_eq!(xroar_snapshot_sam_video_mode(0xe0), 6);
        assert_eq!(xroar_snapshot_sam_video_mode(0xf0), 6);
    }

    #[test]
    fn xroar_v2_vuint_round_trips_boundary_values() {
        for value in [
            0,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            0x1f_ffff,
            0x20_0000,
            0x0fff_ffff,
            0x1000_0000,
            0x7fff_ffff,
        ] {
            let encoded = xroar_v2_vuint(value);
            assert_eq!(
                xroar_v2_read_vuint(&encoded, 0),
                Some((value, encoded.len()))
            );
        }
    }

    #[test]
    fn xroar_v2_vuint_fields_can_be_patched_and_upserted() {
        let mut bytes = xroar_v2_vuint(1);
        bytes.extend_from_slice(&xroar_v2_vuint(1));
        bytes.extend_from_slice(&xroar_v2_vuint(5));
        let mut end = bytes.len();

        patch_xroar_v2_vuint_field_in_range(&mut bytes, 0, &mut end, 1, 0x4000)
            .expect("existing field should be patchable");
        let field = xroar_v2_find_tag_payload(&bytes, 0, end, 1).expect("field should remain");
        assert_eq!(
            xroar_v2_read_vuint(&bytes, field.payload_start),
            Some((0x4000, field.payload_end))
        );

        upsert_xroar_v2_vuint_field_in_range(&mut bytes, 0, &mut end, 2, 7)
            .expect("missing field should be inserted");
        let field = xroar_v2_find_tag_payload(&bytes, 0, end, 2).expect("field should be added");
        assert_eq!(
            xroar_v2_read_vuint(&bytes, field.payload_start),
            Some((7, field.payload_end))
        );
    }

    #[test]
    fn xroar_v2_component_range_stops_at_structural_close_tag() {
        let mut bytes = xroar_v2_component_marker(b"SAM");
        push_xroar_v2_test_field(&mut bytes, 5, 0x1234);
        bytes.push(0);
        let vdrive_start = bytes.len();
        bytes.extend_from_slice(&xroar_v2_component_marker(b"vdrive"));
        push_xroar_v2_test_field(&mut bytes, 5, 0x56);
        bytes.push(0);

        let range = xroar_v2_component_range(&bytes, b"SAM", None)
            .expect("SAM component should be structurally bounded");
        assert_eq!(range.end, vdrive_start);

        let field = xroar_v2_find_tag_payload(&bytes, range.start, range.end, 5)
            .expect("SAM tag 5 should remain findable");
        assert_eq!(
            xroar_v2_read_vuint(&bytes, field.payload_start),
            Some((0x1234, field.payload_end))
        );
    }

    #[test]
    fn xroar_v2_pia_side_a_is_patched_to_inactive_input_state() {
        let mut bytes = Vec::new();
        for tag in [1, 2, 3, 5, 13, 6, 8, 9, 10, 11] {
            push_xroar_v2_test_field(&mut bytes, tag, 1);
        }
        let mut end = bytes.len();

        patch_xroar_v2_pia_side_a(&mut bytes, 0, &mut end).expect("PIA side A should be patchable");

        for (tag, expected) in [
            (1, 0),
            (2, 0),
            (3, 0),
            (5, 0),
            (13, 0),
            (6, 0),
            (8, 0),
            (9, 0xff),
            (10, 0),
            (11, 0xff),
        ] {
            let field =
                xroar_v2_find_tag_payload(&bytes, 0, end, tag).expect("field should remain");
            assert_eq!(
                xroar_v2_read_vuint(&bytes, field.payload_start),
                Some((expected, field.payload_end))
            );
        }
    }

    fn push_xroar_v2_test_field(bytes: &mut Vec<u8>, tag: u32, value: u32) {
        let payload = xroar_v2_vuint(value);
        bytes.extend_from_slice(&xroar_v2_vuint(tag));
        bytes.extend_from_slice(&xroar_v2_vuint(payload.len() as u32));
        bytes.extend_from_slice(&payload);
        bytes.push(0);
    }

    #[test]
    fn xroar_snapshot_reference_traps_at_snapshot_pc() {
        let snapshot = PcDragonSnapshot {
            ram: Box::new([]),
            load_address: 0,
            registers: format_dragon_pak::PcDragonRegisters {
                pc: 0x1234,
                x: 0,
                y: 0,
                u: 0,
                s: 0,
                dp: 0,
                b: 0,
                a: 0,
                cc: 0,
            },
            peripherals: None,
            display_base: None,
        };

        assert_eq!(xroar_snapshot_trap_condition(&snapshot), "pc=0x1234");
    }

    #[test]
    fn cli_rejects_invalid_smoke_joystick_options() {
        let bad_port = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick".to_owned(),
            "3,fire,20".to_owned(),
        ])
        .expect_err("invalid joystick port should fail");
        assert!(bad_port.contains("expected 1 or 2"));

        let bad_control = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick".to_owned(),
            "1,button,20".to_owned(),
        ])
        .expect_err("invalid joystick control should fail");
        assert!(bad_control.contains("expected up, down, left, right, fire, or idle"));
    }

    #[test]
    fn cli_parses_xroar_reference_options() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
            "--xroar-bin".to_owned(),
            "xroar/src/xroar".to_owned(),
            "--xroar-reference-dir".to_owned(),
            "refs".to_owned(),
            "--xroar-snapshot-out".to_owned(),
            "snapshot.sna".to_owned(),
            "--xroar-motoroff".to_owned(),
            "2".to_owned(),
            "--xroar-settle-seconds".to_owned(),
            "2.5".to_owned(),
            "--xroar-timeout-seconds".to_owned(),
            "30".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.xroar_bin, Some(PathBuf::from("xroar/src/xroar")));
        assert_eq!(cli.xroar_reference_dir, Some(PathBuf::from("refs")));
        assert_eq!(cli.xroar_snapshot_out, Some(PathBuf::from("snapshot.sna")));
        assert_eq!(cli.xroar_motoroff, Some(2));
        assert_eq!(cli.xroar_settle_seconds, 2.5);
        assert_eq!(cli.xroar_timeout_seconds, 30.0);
    }

    #[test]
    fn xroar_reference_config_requires_bin_and_output_dir() {
        let missing_dir = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--xroar-bin".to_owned(),
            "xroar".to_owned(),
        ])
        .expect("CLI parsing should allow validation later");

        assert_eq!(
            xroar_reference_config(&missing_dir),
            Err("--xroar-reference-dir is required with --xroar-bin".to_owned())
        );
    }

    #[test]
    fn xroar_type_text_matches_dragon_load_command() {
        assert_eq!(
            xroar_type_text("CLOAD", Some("RUN")),
            Ok("CLOAD\rRUN\r".to_owned())
        );
        assert_eq!(xroar_type_text("CLOAD", None), Ok("CLOAD\r".to_owned()));
        assert_eq!(
            xroar_type_text("CLOADM", Some("EXEC")),
            Ok("CLOADM\rEXEC\r".to_owned())
        );
        assert_eq!(xroar_type_text("CLOADM", None), Ok("CLOADM\r".to_owned()));
        assert_eq!(default_xroar_motoroff("CLOAD", Some("RUN")), 2);
        assert_eq!(default_xroar_motoroff("CLOAD", None), 3);
        assert_eq!(default_xroar_motoroff("CLOADM", Some("EXEC")), 2);
        assert_eq!(default_xroar_motoroff("CLOADM", None), 2);
    }

    #[test]
    fn xroar_zoomed_screenshot_scales_active_area_without_downscaling() {
        let mut pixels =
            vec![0; TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT * 4];
        let active_offset = (motorola_vdg_6847::TEXT_TOP_BORDER_LINES
            * TEXT_VISIBLE_FRAMEBUFFER_WIDTH
            + motorola_vdg_6847::TEXT_LEFT_BORDER_PIXELS)
            * 4;
        pixels[active_offset..active_offset + 4].copy_from_slice(&[0x12, 0x34, 0x56, 0xFF]);
        let frame = CapturedFrame {
            timestamp: emu198x_shell::MachineTime(0),
            format: PixelFormat::Rgba8888,
            width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
            height: TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
            palette: None,
            pixels,
        };

        let png = xroar_zoomed_png_bytes(&frame).expect("zoomed PNG should encode");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder
            .read_info()
            .expect("zoomed screenshot should decode");
        assert_eq!(reader.info().width, XROAR_ZOOMED_WIDTH);
        assert_eq!(reader.info().height, XROAR_ZOOMED_HEIGHT);
        let mut output = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut output)
            .expect("zoomed screenshot should contain one frame");
        assert_eq!(&output[..4], &[0x12, 0x34, 0x56, 0xFF]);
        assert_eq!(&output[4..8], &[0x12, 0x34, 0x56, 0xFF]);
        assert_eq!(info.color_type, png::ColorType::Rgba);
    }

    #[test]
    fn image_comparison_reports_dimension_and_pixel_differences() {
        let emu = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 0xFF, 10, 20, 30, 0xFF],
        };
        let reference = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 0xFF, 20, 10, 30, 0xFF],
        };

        let comparison = compare_images(&emu, &reference);

        assert!(comparison.dimensions_match);
        assert_eq!(comparison.compared_pixels, 2);
        assert_eq!(comparison.differing_pixels, 1);
        assert_eq!(comparison.differing_pixel_percent, 50.0);
        assert_eq!(comparison.max_channel_delta, 10);
        assert!((comparison.mean_abs_channel_delta - (20.0 / 6.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn classifies_started_graphics_that_draws_after_settle() {
        let text_video = DragonVideoState {
            sam_video_mode: 0,
            sam_display_offset: 2,
            display_base: 0x0400,
            pia1_output_b: 0x07,
            pia1_ddr_b: 0xF8,
            pia1_control_b: 0x37,
            pia1_cb2: false,
        };
        let graphics_video = DragonVideoState {
            sam_video_mode: 6,
            sam_display_offset: 3,
            display_base: 0x0600,
            pia1_output_b: 0xE7,
            pia1_ddr_b: 0xF8,
            pia1_control_b: 0x37,
            pia1_cb2: false,
        };

        let classification = classify_runtime_smoke(RuntimeSmokeClassificationInput {
            command: "CLOAD",
            load_result: "ok",
            start_result: "visible-change",
            load_visible_change: true,
            visible_change_after_start: true,
            start_video_changed: true,
            start_settle_visible_change: true,
            basic_error: false,
            load_video: text_video,
            start_video: graphics_video,
        });

        assert_eq!(
            classification,
            RuntimeSmokeClassification::StartedGraphicsDrawing
        );
    }

    #[test]
    fn classifies_machine_code_autorun_after_load() {
        let video = DragonVideoState {
            sam_video_mode: 0,
            sam_display_offset: 2,
            display_base: 0x0400,
            pia1_output_b: 0x07,
            pia1_ddr_b: 0xF8,
            pia1_control_b: 0x37,
            pia1_cb2: false,
        };

        let classification = classify_runtime_smoke(RuntimeSmokeClassificationInput {
            command: "CLOADM",
            load_result: "ok",
            start_result: "already-running-after-load",
            load_visible_change: true,
            visible_change_after_start: false,
            start_video_changed: false,
            start_settle_visible_change: false,
            basic_error: false,
            load_video: video,
            start_video: video,
        });

        assert_eq!(
            classification,
            RuntimeSmokeClassification::MachineCodeRunningAfterLoad
        );
    }

    #[test]
    fn dragon_key_labels_map_to_confirmed_matrix_positions() {
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("a").expect("A should parse")),
            MatrixKey::new(2, 1)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("A").expect("A should parse")),
            MatrixKey::new(2, 1)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("@").expect("@ should parse")),
            MatrixKey::new(2, 0)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("space").expect("space should parse")),
            MatrixKey::new(5, 7)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("right").expect("right should parse")),
            MatrixKey::new(5, 6)
        );
    }

    #[test]
    fn dragon_key_parser_accepts_control_key_aliases() {
        assert_eq!(parse_dragon_key("return"), Ok(DragonKey::Enter));
        assert_eq!(parse_dragon_key("clr"), Ok(DragonKey::Clear));
        assert_eq!(parse_dragon_key("brk"), Ok(DragonKey::Break));
    }

    #[test]
    fn cli_parses_named_dragon_key_presses_semantically() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--press".to_owned(),
            "A".to_owned(),
            "--press".to_owned(),
            "@".to_owned(),
            "--press".to_owned(),
            "enter".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.pressed_keys,
            vec![
                MatrixKey::new(2, 1),
                MatrixKey::new(2, 0),
                MatrixKey::new(6, 0),
            ]
        );
    }

    #[test]
    fn load_rom_accepts_zip_archives() {
        let rom = rom_with_reset_vector(0x8000);
        let path = env::temp_dir().join(format!(
            "emu198x-dragon-rom-test-{}.zip",
            std::process::id()
        ));

        let file = fs::File::create(&path).expect("test zip should be creatable");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("dragon32.rom", zip::write::SimpleFileOptions::default())
            .expect("zip entry should start");
        zip.write_all(&rom).expect("zip entry should be writable");
        zip.finish().expect("zip should finish");

        let loaded = load_rom(&path).expect("zip ROM should load");
        fs::remove_file(&path).expect("test zip should be removable");

        assert_eq!(loaded, rom);
    }

    fn xroar_v1_chunk(bytes: &[u8], section: u8) -> Option<&[u8]> {
        let mut offset = b"XRoar snapshot.\n\0".len();
        while offset + 3 <= bytes.len() {
            let chunk_section = bytes[offset];
            let len = u16::from_be_bytes([bytes[offset + 1], bytes[offset + 2]]) as usize;
            offset += 3;
            let end = offset.checked_add(len)?;
            if end > bytes.len() {
                return None;
            }
            if chunk_section == section {
                return Some(&bytes[offset..end]);
            }
            offset = end;
        }
        None
    }
}
