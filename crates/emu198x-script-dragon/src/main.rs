//! Minimal Dragon 32 ROM bring-up harness.
//!
//! This is deliberately smaller than the full machine/runtime path. It gives us
//! an executable ROM/CPU loop while PIA, SAM, and VDG are still being rebuilt.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{
    HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, read_media_asset,
};
use format_dragon_cas::{CasFileType, CasHeader, CasImage, parse_cas_tolerant};
use machine_dragon_32::{
    DeviceAccess, DeviceRegion, Dragon32, DragonKey, DragonKeyboard, FetchTrace, MatrixKey,
    ROM_SIZE, ReadonlyWrite, RunReport, StopReason,
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

Other:
    --help             print this help text
";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cli {
    rom: PathBuf,
    cycles: u64,
    trace_limit: usize,
    pressed_keys: Vec<MatrixKey>,
    dump_text: bool,
    dump_text_png: Option<PathBuf>,
    smoke_root: Option<PathBuf>,
    smoke_run_limit: usize,
    smoke_report: Option<PathBuf>,
    smoke_screenshot_dir: Option<PathBuf>,
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
    basic_error: bool,
    load_screen_text: Vec<String>,
    screen_text: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_screenshot: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct DragonVideoState {
    sam_video_mode: u8,
    sam_display_offset: u8,
    display_base: u16,
    pia1_output_b: u8,
    pia1_ddr_b: u8,
    pia1_control_b: u8,
    pia1_cb2: bool,
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
        cli.cycles,
        cli.trace_limit,
        keyboard,
        cli.dump_text,
        cli.dump_text_png.is_some(),
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
    let mut cycles = DEFAULT_CYCLES;
    let mut trace_limit = DEFAULT_TRACE_LIMIT;
    let mut pressed_keys = Vec::new();
    let mut dump_text = false;
    let mut dump_text_png = None;
    let mut smoke_root = None;
    let mut smoke_run_limit = DEFAULT_SMOKE_RUN_LIMIT;
    let mut smoke_report = None;
    let mut smoke_screenshot_dir = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => {
                rom = Some(PathBuf::from(next_value(&mut iter, "--rom")?));
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
            "--help" | "-h" => return Err(USAGE.to_owned()),
            _ => return Err(format!("unknown argument: {arg}\n\n{USAGE}")),
        }
    }

    Ok(Cli {
        rom: rom.ok_or_else(|| format!("missing required --rom PATH\n\n{USAGE}"))?,
        cycles,
        trace_limit,
        pressed_keys,
        dump_text,
        dump_text_png,
        smoke_root,
        smoke_run_limit,
        smoke_report,
        smoke_screenshot_dir,
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
    let mut tapes = Vec::new();
    collect_tape_candidates(root, &mut tapes)?;
    tapes.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }

    let mut rows = Vec::with_capacity(tapes.len());
    let mut runtime_smokes = 0usize;
    for (index, tape_path) in tapes.iter().enumerate() {
        let screenshot_stem = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}", safe_stem(tape_path))));
        let row = scan_tape_candidate(
            tape_path,
            rom,
            &mut runtime_smokes,
            cli.smoke_run_limit,
            screenshot_stem.as_deref(),
        );
        rows.push(row);
    }

    Ok(SmokeMatrixReport {
        tape_count: rows.len(),
        runtime_smokes,
        rows,
    })
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
    smoke_run_limit: usize,
    screenshot_stem: Option<&Path>,
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
        && *runtime_smokes < smoke_run_limit;
    let runtime = if should_smoke {
        *runtime_smokes += 1;
        Some(run_runtime_smoke(
            rom,
            &loaded.bytes,
            &parsed,
            screenshot_stem,
        ))
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
    screenshot_stem: Option<&Path>,
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

    match run_runtime_smoke_inner(rom, tape_bytes, command, screenshot_stem) {
        Ok(smoke) => smoke,
        Err(error) => failed_runtime_smoke(command, &error),
    }
}

fn run_runtime_smoke_inner(
    rom: &[u8; ROM_SIZE],
    tape_bytes: &[u8],
    command: &str,
    screenshot_stem: Option<&Path>,
) -> Result<CasRuntimeSmoke, String> {
    let mut session = boot_runtime_session(rom)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, tape_bytes));
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load tape into runtime: {err}"))?;
    let tape_length_bits = query_u64(&session, "dragon.tape.length_bits")?;

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
    session
        .wait_for_query_bool("dragon.tape.motor_on", false, load_wait_frames)
        .map_err(|err| {
            format!(
                "cassette motor did not turn off within {load_wait_frames} frames for {tape_length_bits} tape bits: {err}"
            )
        })?;

    let lines_after_load = screen_text_lines(&session)?;
    let after_load = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture post-load frame: {err}"))?;
    let load_visible_change = after_load != before_load;
    let load_pc_after = query_u16(&session, "dragon.cpu.pc")?;
    let load_video = video_state(&session)?;
    let load_screenshot = write_smoke_screenshot(&session, screenshot_stem, "load")?;
    let load_error = lines_after_load.iter().any(|line| line.contains("ERROR"));
    let load_result = if load_error { "basic-error" } else { "ok" }.to_owned();
    let start_command = if command == "CLOAD" { "RUN" } else { "EXEC" };
    let should_start = should_issue_start_command(command, load_error, load_visible_change);

    let (start_result, visible_change_after_start, screen_text) = if should_start {
        type_basic_command(&mut session, start_command)?;
        let visible_change = wait_for_screenshot_change(&mut session, &after_load, 500)?;
        if visible_change {
            session
                .run_frames(SMOKE_START_SETTLE_FRAMES)
                .map_err(|err| format!("runtime failed while settling after start: {err}"))?;
        }
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
        (start_result, visible_change, screen_text)
    } else {
        (
            skipped_start_result(command, load_visible_change, load_pc_after).to_owned(),
            false,
            lines_after_load.clone(),
        )
    };
    let basic_error = screen_text.iter().any(|line| line.contains("ERROR"));
    let start_pc_after = query_u16(&session, "dragon.cpu.pc")?;
    let start_video = video_state(&session)?;
    let start_screenshot = write_smoke_screenshot(&session, screenshot_stem, "start")?;

    Ok(CasRuntimeSmoke {
        command: command.to_owned(),
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
        basic_error,
        load_screen_text: lines_after_load,
        screen_text,
        error: None,
        load_screenshot,
        start_screenshot,
    })
}

fn should_issue_start_command(command: &str, load_error: bool, load_visible_change: bool) -> bool {
    !load_error && (command == "CLOAD" || !load_visible_change)
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
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    stem: Option<&Path>,
    suffix: &str,
) -> Result<Option<String>, String> {
    let Some(stem) = stem else {
        return Ok(None);
    };
    let path = stem.with_extension(format!("{suffix}.png"));
    let png = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture {suffix} screenshot: {err}"))?;
    fs::write(&path, png).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(Some(path.display().to_string()))
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
        basic_error: false,
        load_screen_text: Vec::new(),
        screen_text: Vec::new(),
        error: Some(error.to_owned()),
        load_screenshot: None,
        start_screenshot: None,
    }
}

fn load_wait_frame_budget(tape_length_bits: u64) -> u32 {
    let scaled = tape_length_bits / 16;
    u32::try_from(scaled.clamp(4_500, 20_000)).map_or(20_000, |frames| frames)
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
) -> Result<bool, String> {
    for _ in 0..=max_frames {
        let current = session
            .screenshot_png_bytes()
            .map_err(|err| format!("failed to capture frame: {err}"))?;
        if current != before {
            return Ok(true);
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for frame change: {err}"))?;
    }
    Ok(false)
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

fn run_harness_with_keyboard(
    rom: &[u8; ROM_SIZE],
    cycle_limit: u64,
    trace_limit: usize,
    keyboard: DragonKeyboard,
    dump_text: bool,
    dump_text_framebuffer: bool,
) -> HarnessReport {
    let mut machine = Dragon32::new_with_keyboard(rom, keyboard);
    let report = machine.run_cycles(cycle_limit, trace_limit);
    let text_screen = (dump_text || dump_text_framebuffer).then(|| machine.capture_text_screen());
    let text_screen_text = text_screen
        .as_ref()
        .filter(|_| dump_text)
        .map(|screen| screen.to_plain_text());
    let text_framebuffer =
        dump_text_framebuffer.then(|| machine.render_visible_text_argb(TextPalette::default()));

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

        let report = run_harness_with_keyboard(&rom, 128, 8, DragonKeyboard::new(), true, true);

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
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.smoke_root, Some(PathBuf::from("tapes")));
        assert_eq!(cli.smoke_run_limit, 3);
        assert_eq!(cli.smoke_report, Some(PathBuf::from("report.json")));
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
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
