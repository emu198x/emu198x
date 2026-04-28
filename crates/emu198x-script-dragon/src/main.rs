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
    CapturedFrame, HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, PixelFormat,
    read_media_asset,
};
use format_dragon_cas::{CasFileType, CasHeader, CasImage, parse_cas_tolerant};
use format_dragon_pak::{
    DragonCartridgeKind as ParsedDragonCartridgeKind, DragonPakImage, PcDragonSnapshot,
    parse_dragon_pak, parse_pcdragon_snapshot,
};
use machine_dragon_32::{
    DeviceAccess, DeviceRegion, Dragon32, DragonCartridgeKind, DragonKey, DragonKeyboard,
    FetchTrace, MatrixKey, ROM_SIZE, ReadonlyWrite, RunReport, StopReason,
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
const DRAGON_CPU_HZ: u64 = 894_886;
const DRAGON_FRAME_HZ: u64 = 50;
const DRAGON_FRAME_CYCLES: u64 = DRAGON_CPU_HZ / DRAGON_FRAME_HZ;
const BOOT_FRAME_BUDGET: u32 = 100;
const KEY_EDGE_FRAMES: u32 = 4;
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
    --press KEY        hold a named Dragon key closed; may be repeated
    --press-matrix R,C hold a raw keyboard matrix switch closed; may be repeated
    --dump-text        print the current 32x16 MC6847 text snapshot
    --dump-text-png P  write the current border-inclusive MC6847 text framebuffer as a PNG
    --smoke-root PATH  recursively scan .cas/.zip Dragon tape images
    --smoke-run-limit N
                       run real-ROM CLOAD/CLOADM smoke for first N parsed tapes [default: 8]
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
                       write patched-XRoar reference PNGs for runtime-smoked tapes
    --xroar-motoroff N capture on the Nth tape motor-off [default: auto]
    --xroar-settle-seconds N
                       wait N emulated seconds after the motor-off trap before reference capture [default: 3]
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
    pressed_keys: Vec<MatrixKey>,
    dump_text: bool,
    dump_text_png: Option<PathBuf>,
    smoke_root: Option<PathBuf>,
    smoke_run_limit: usize,
    smoke_report: Option<PathBuf>,
    smoke_screenshot_dir: Option<PathBuf>,
    smoke_screenshot_format: SmokeScreenshotFormat,
    smoke_audio_dir: Option<PathBuf>,
    smoke_joystick: Vec<SmokeJoystickStep>,
    smoke_idle_after_start: u32,
    xroar_bin: Option<PathBuf>,
    xroar_reference_dir: Option<PathBuf>,
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
    text_screen_base: u16,
    text_screen: Option<String>,
    text_framebuffer: Option<Vec<u32>>,
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
            dump_text: cli.dump_text,
            dump_text_framebuffer: cli.dump_text_png.is_some(),
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
    let mut pressed_keys = Vec::new();
    let mut dump_text = false;
    let mut dump_text_png = None;
    let mut smoke_root = None;
    let mut smoke_run_limit = DEFAULT_SMOKE_RUN_LIMIT;
    let mut smoke_report = None;
    let mut smoke_screenshot_dir = None;
    let mut smoke_screenshot_format = SmokeScreenshotFormat::Diagnostic;
    let mut smoke_audio_dir = None;
    let mut smoke_joystick = Vec::new();
    let mut smoke_idle_after_start = 0;
    let mut xroar_bin = None;
    let mut xroar_reference_dir = None;
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
            "--smoke-root" => {
                smoke_root = Some(PathBuf::from(next_value(&mut iter, "--smoke-root")?));
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
                smoke_screenshot_format = parse_smoke_screenshot_format(&next_value(
                    &mut iter,
                    "--smoke-screenshot-format",
                )?)?;
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

    Ok(Cli {
        rom: rom.ok_or_else(|| format!("missing required --rom PATH\n\n{USAGE}"))?,
        cart,
        snapshot,
        cycles,
        trace_limit,
        pressed_keys,
        dump_text,
        dump_text_png,
        smoke_root,
        smoke_run_limit,
        smoke_report,
        smoke_screenshot_dir,
        smoke_screenshot_format,
        smoke_audio_dir,
        smoke_joystick,
        smoke_idle_after_start,
        xroar_bin,
        xroar_reference_dir,
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

fn parse_u32(value: &str, flag: &str) -> Result<u32, String> {
    let parsed = parse_u64(value, flag)?;
    u32::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
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

fn parse_smoke_screenshot_format(value: &str) -> Result<SmokeScreenshotFormat, String> {
    match value {
        "diagnostic" => Ok(SmokeScreenshotFormat::Diagnostic),
        "xroar-zoomed" => Ok(SmokeScreenshotFormat::XroarZoomed),
        _ => Err(format!(
            "invalid --smoke-screenshot-format value {value}; expected diagnostic or xroar-zoomed"
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

fn is_tape_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["cas", "zip"]))
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
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock was before UNIX epoch: {err}"))?
        .as_nanos();
    let path = env::temp_dir().join(format!(
        "emu198x-xroar-{}-{unique}-{kind}.{extension}",
        process::id()
    ));
    fs::write(&path, bytes).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
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
    dump_text: bool,
    dump_text_framebuffer: bool,
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
    let report = machine.run_cycles(options.cycle_limit, options.trace_limit);
    let text_screen =
        (options.dump_text || options.dump_text_framebuffer).then(|| machine.capture_text_screen());
    let text_screen_text = text_screen
        .as_ref()
        .filter(|_| options.dump_text)
        .map(|screen| screen.to_plain_text());
    let text_framebuffer = options
        .dump_text_framebuffer
        .then(|| machine.render_visible_text_argb(TextPalette::default()));

    report.into_harness_report(text_screen_text, text_framebuffer)
}

trait IntoHarnessReport {
    fn into_harness_report(
        self,
        text_screen: Option<String>,
        text_framebuffer: Option<Vec<u32>>,
    ) -> HarnessReport;
}

impl IntoHarnessReport for RunReport {
    fn into_harness_report(
        self,
        text_screen: Option<String>,
        text_framebuffer: Option<Vec<u32>>,
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
            text_screen_base: self.text_screen_base,
            text_screen,
            text_framebuffer,
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
    println!("trace:");
    for fetch in &report.trace {
        println!(
            "  cycle={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.pc, fetch.opcode
        );
    }
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
                dump_text: true,
                dump_text_framebuffer: true,
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
        assert_eq!(cli.pressed_keys, Vec::new());
        assert!(cli.dump_text);
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
}
