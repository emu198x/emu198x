//! `emu198x-script-c64` — minimal headless C64 runner.
//!
//! This binary is intentionally thin. It resolves ROM paths, optional
//! snapshot/script inputs, and output captures, then hands execution to the
//! shared headless session layer above `runtime-commodore-c64`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::{
    BootArtifacts, ControlCommand, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession,
    MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand,
    ScriptObservation, TraceEvent, TraceSink, boot_machine, read_firmware_asset, read_media_asset,
    read_program_asset,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, Model, autoload_basic_tape,
    file_loader::load_host_file,
};
use serde::Serialize;
use serde_json::Value;

const KERNAL_ID: &str = "commodore-c64-kernal-rom";
const BASIC_ID: &str = "commodore-c64-basic-rom";
const CHARACTER_ID: &str = "commodore-c64-character-rom";
const DEFAULT_IMPORT_BOOT_FRAMES: u32 = 200;
const DEFAULT_TRACE_LIMIT: usize = 512;

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    chargen: Option<PathBuf>,
    load: Option<PathBuf>,
    tape: Option<PathBuf>,
    autoload_tape: bool,
    start_tape: bool,
    load_snapshot: Option<PathBuf>,
    save_snapshot: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    script: Option<PathBuf>,
    wait_for_boot: Option<u32>,
    wait_for_tape_stop: Option<u32>,
    print_queries: Vec<String>,
    print_screen_text: bool,
    trace_vic_colours: bool,
    trace_limit: usize,
    frames: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ModelArg {
    #[default]
    Pal,
    Ntsc,
}

#[derive(Debug)]
struct LoadedFirmware {
    id: &'static str,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct LoadedProgram {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct RunnerReport {
    observations: Vec<ScriptObservation>,
    time: u64,
    boot_detected: bool,
    boot_reason: String,
    loaded_program: Option<String>,
    query_values: Vec<ReportedQuery>,
    screen_text_lines: Option<Vec<String>>,
    trace_lines: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct ReportedQuery {
    path: String,
    value: Value,
}

#[derive(Debug, Default)]
struct TraceCollector {
    lines: Vec<String>,
    limit: usize,
    dropped: usize,
}

impl TraceCollector {
    fn with_limit(limit: usize) -> Self {
        Self {
            lines: Vec::new(),
            limit,
            dropped: 0,
        }
    }

    fn into_lines(mut self) -> Vec<String> {
        if self.dropped != 0 {
            self.lines.push(format!(
                "... truncated {} further trace events",
                self.dropped
            ));
        }
        self.lines
    }
}

impl TraceSink for TraceCollector {
    fn push_trace(&mut self, event: TraceEvent<'_>) -> Result<(), emu198x_shell::MachineError> {
        if self.lines.len() >= self.limit {
            self.dropped = self.dropped.saturating_add(1);
            return Ok(());
        }

        let payload = std::str::from_utf8(event.payload).map_err(|err| {
            emu198x_shell::MachineError::Host {
                reason: format!("trace payload was not utf-8: {err}"),
            }
        })?;
        self.lines.push(format!(
            "{} {} {}",
            event.timestamp.get(),
            event.kind,
            payload
        ));
        Ok(())
    }
}

const USAGE: &str = "\
Usage: emu198x-script-c64 [OPTIONS]

Cold boot:
    --rom-dir DIR             directory containing Commodore ROM images
    --kernal PATH             override KERNAL ROM path
    --basic PATH              override BASIC ROM path
    --chargen PATH            override character ROM path
    --model MODEL             pal or ntsc [default: pal]
    --load PATH               import one .prg/.bas/.t64/.d64 file after boot
    --tape PATH               insert one TAP image into datasette slot
    --autoload-tape           wait for READY., press SHIFT+RUN/STOP, and start tape-1
    --start-tape              press PLAY on the inserted datasette image

State and automation:
    --load-snapshot PATH      restore a runtime snapshot before running
    --save-snapshot PATH      write a runtime snapshot after running
    --script PATH             execute shared JSON session steps after boot
    --wait-for-boot N         run up to N frames until boot.detected is true
    --wait-for-tape-stop N    run up to N frames until c64.tape.playing has started and then stops
    --print-query PATH        resolve one query path after running (repeatable)
    --print-screen-text       print decoded screen-text lines after running
    --trace-vic-colours       trace D020/D021 changes during the explicit --frames run
    --trace-limit N           maximum traced colour-write events to retain [default: 512]
    --screenshot PATH         write the last emitted frame as PNG

Execution:
    --frames N                number of native video frames to run

Other:
    --help, -h                show this help

ROM directory resolution (first match wins):
    1. --rom-dir DIR
    2. EMU198X_C64_ROM_DIR
    3. ~/.emu198x/roms/commodore-c64
    4. ~/.emu198x/roms/c64

Filename resolution inside the ROM directory:
    - kernal.rom or c64-kernal.rom
    - basic.rom or c64-basic.rom
    - chargen.rom or c64-chargen.rom

Examples:
    emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --wait-for-boot 200 --screenshot ready.png
    emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --load demo.bas --save-snapshot demo.c64.pst
    emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --load game.d64 --save-snapshot game.c64.pst
    emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --tape game.tap --autoload-tape --wait-for-tape-stop 12000
    emu198x-script-c64 --load-snapshot ready.c64.pst --frames 25 --save-snapshot later.c64.pst
    emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --tape game.tap --autoload-tape --frames 300 --trace-vic-colours
    emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --script capture.json
";

fn main() {
    let cli = parse_cli(std::env::args().skip(1));
    let script_mode = cli.script.is_some();
    match run(cli) {
        Ok(report) => {
            if script_mode {
                let json = serde_json::to_string(&report).unwrap_or_else(|err| {
                    eprintln!("error: failed to serialize runner report: {err}");
                    process::exit(1);
                });
                println!("{json}");
            } else {
                println!(
                    "C64 runtime: time={} boot_detected={} boot_reason={}",
                    report.time, report.boot_detected, report.boot_reason
                );
                if let Some(message) = &report.loaded_program {
                    println!("{message}");
                }
                for query in &report.query_values {
                    println!("{}={}", query.path, query.value);
                }
                if let Some(lines) = &report.screen_text_lines {
                    println!("screen_text_lines:");
                    for line in lines {
                        println!("{line}");
                    }
                }
                if let Some(lines) = &report.trace_lines {
                    println!("trace_lines:");
                    for line in lines {
                        println!("{line}");
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            process::exit(1);
        }
    }
}

fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli {
        trace_limit: DEFAULT_TRACE_LIMIT,
        ..Cli::default()
    };
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom-dir" => cli.rom_dir = Some(PathBuf::from(next_arg(&mut iter, "--rom-dir"))),
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--chargen" => cli.chargen = Some(PathBuf::from(next_arg(&mut iter, "--chargen"))),
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--load" => cli.load = Some(PathBuf::from(next_arg(&mut iter, "--load"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--autoload-tape" => cli.autoload_tape = true,
            "--start-tape" => cli.start_tape = true,
            "--load-snapshot" => {
                cli.load_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--load-snapshot")));
            }
            "--save-snapshot" => {
                cli.save_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--save-snapshot")));
            }
            "--script" => cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script"))),
            "--wait-for-boot" => {
                cli.wait_for_boot = Some(
                    next_arg(&mut iter, "--wait-for-boot")
                        .parse()
                        .unwrap_or_else(|_| die("--wait-for-boot requires a non-negative integer")),
                );
            }
            "--wait-for-tape-stop" => {
                cli.wait_for_tape_stop = Some(
                    next_arg(&mut iter, "--wait-for-tape-stop")
                        .parse()
                        .unwrap_or_else(|_| {
                            die("--wait-for-tape-stop requires a non-negative integer")
                        }),
                );
            }
            "--print-query" => cli.print_queries.push(next_arg(&mut iter, "--print-query")),
            "--print-screen-text" => cli.print_screen_text = true,
            "--trace-vic-colours" => cli.trace_vic_colours = true,
            "--trace-limit" => {
                cli.trace_limit = next_arg(&mut iter, "--trace-limit")
                    .parse()
                    .unwrap_or_else(|_| die("--trace-limit requires a non-negative integer"));
            }
            "--screenshot" => {
                cli.screenshot = Some(PathBuf::from(next_arg(&mut iter, "--screenshot")));
            }
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames")
                    .parse()
                    .unwrap_or_else(|_| die("--frames requires a non-negative integer"));
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

fn parse_model_arg(value: &str) -> ModelArg {
    match value {
        "pal" => ModelArg::Pal,
        "ntsc" => ModelArg::Ntsc,
        _ => die("--model expects pal or ntsc"),
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

fn run(cli: Cli) -> Result<RunnerReport, String> {
    if cli.autoload_tape && cli.start_tape {
        return Err("--autoload-tape conflicts with --start-tape".into());
    }

    if cli.screenshot.is_some()
        && cli.frames == 0
        && cli.script.is_none()
        && cli.wait_for_boot.is_none()
    {
        return Err(
            "capture requests require --frames, --wait-for-boot, or --script so the machine emits output"
                .into(),
        );
    }

    let machine = boot_runtime(&cli)?;
    let native_frame_ticks = match cli.model {
        ModelArg::Pal => u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        ModelArg::Ntsc => u64::from(TIMING_NTSC_BREADBIN.cycles_per_frame),
    };
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        native_frame_ticks,
        C64SessionQueryProvider,
    );

    let mut observations = Vec::new();
    let needs_boot_before_import = cli.load.is_some();
    if cli.wait_for_boot.is_some() || needs_boot_before_import {
        let explicit_wait = cli.wait_for_boot.is_some();
        let max_frames = cli.wait_for_boot.unwrap_or(DEFAULT_IMPORT_BOOT_FRAMES);
        let result = session
            .wait_for_boot(max_frames)
            .map_err(|err| format!("boot wait failed: {err}"))?;
        if explicit_wait {
            observations.push(ScriptObservation::WaitForBoot {
                frames: result.frames,
                reached: result.reached,
                reason: result.reason,
                row: result.row,
            });
        }
    }

    if let Some(path) = &cli.tape {
        let loaded = read_media_asset(path, MediaKind::Tape)
            .map_err(|err| format!("failed to load tape asset {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
        session
            .load_media(&media)
            .map_err(|err| format!("tape load failed: {err}"))?;
    }

    if cli.autoload_tape {
        if !session.machine().machine().tape_is_loaded() {
            return Err("--autoload-tape requires tape media in slot tape-1".into());
        }

        autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .map_err(|err| format!("tape autoload failed: {err}"))?;
    }

    let mut loaded_program = None;
    if let Some(path) = &cli.load {
        let loaded = load_program_bytes(path)?;
        loaded_program = Some(
            load_host_file(session.machine_mut(), &loaded.name, &loaded.bytes)
                .map_err(|err| format!("program import failed: {err}"))?,
        );
    }

    if let Some(path) = &cli.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        observations.extend(
            script
                .execute_collect(&mut session)
                .map_err(|err| format!("script execution failed: {err}"))?,
        );
    }

    if cli.start_tape {
        session
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .map_err(|err| format!("failed to start tape transport: {err}"))?;
    }

    if let Some(max_frames) = cli.wait_for_tape_stop {
        observations.push(
            wait_for_tape_motion_to_stop(&mut session, max_frames)
                .map_err(|err| format!("tape-stop wait failed: {err}"))?,
        );
    }

    let trace_lines = if cli.trace_vic_colours {
        session.machine_mut().set_trace_vic_colour_writes(true);
        let mut collector = TraceCollector::with_limit(cli.trace_limit);
        if cli.frames > 0 {
            session
                .run_frames_with_trace_sink(cli.frames, &mut collector)
                .map_err(|err| format!("run failed: {err}"))?;
        }
        session.machine_mut().set_trace_vic_colour_writes(false);
        Some(collector.into_lines())
    } else {
        if cli.frames > 0 {
            session
                .run_frames(cli.frames)
                .map_err(|err| format!("run failed: {err}"))?;
        }
        None
    };

    if let Some(path) = &cli.save_snapshot {
        session
            .save_snapshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    if let Some(path) = &cli.screenshot {
        session
            .save_screenshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    let boot_detected = query_bool(&session, "boot.detected")?;
    let boot_reason = query_string(&session, "boot.reason")?
        .unwrap_or_else(|| "boot.detected remained false".to_owned());
    let query_values = collect_queries(&session, &cli.print_queries)?;
    let screen_text_lines = if cli.print_screen_text {
        Some(query_string_list(&session, "screen.text.lines")?)
    } else {
        None
    };

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        boot_detected,
        boot_reason,
        loaded_program,
        query_values,
        screen_text_lines,
        trace_lines,
    })
}

fn boot_runtime(cli: &Cli) -> Result<C64Runtime, String> {
    let firmware_storage = load_firmware_bytes(cli)?;
    let mut firmware = FirmwareSet::new();
    for image in &firmware_storage {
        firmware.push(FirmwareImage::new(image.id, &image.bytes));
    }

    let snapshot_bytes = match &cli.load_snapshot {
        Some(path) => Some(
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        ),
        None => None,
    };

    boot_machine(
        &BootArtifacts {
            firmware,
            snapshot: snapshot_bytes.as_deref(),
        },
        |firmware| C64Runtime::from_firmware(cli.model.to_model(), firmware),
        || C64Runtime::blank(cli.model.to_model()),
    )
    .map_err(|err| format!("boot failed: {err}"))
}

fn load_firmware_bytes(cli: &Cli) -> Result<Vec<LoadedFirmware>, String> {
    let rom_dir = resolve_rom_dir(cli)?;
    let entries = [
        (
            KERNAL_ID,
            resolve_rom_path(
                cli.kernal.as_deref(),
                rom_dir.as_deref(),
                &["kernal.rom", "c64-kernal.rom"],
            )?,
        ),
        (
            BASIC_ID,
            resolve_rom_path(
                cli.basic.as_deref(),
                rom_dir.as_deref(),
                &["basic.rom", "c64-basic.rom"],
            )?,
        ),
        (
            CHARACTER_ID,
            resolve_rom_path(
                cli.chargen.as_deref(),
                rom_dir.as_deref(),
                &["chargen.rom", "c64-chargen.rom"],
            )?,
        ),
    ];

    entries
        .into_iter()
        .filter_map(|(id, path)| path.map(|path| (id, path)))
        .map(|(id, path)| {
            read_firmware_asset(&path)
                .map(|loaded| LoadedFirmware {
                    id,
                    bytes: loaded.bytes,
                })
                .map_err(|err| {
                    format!(
                        "failed to read firmware {id} from {}: {err}",
                        path.display()
                    )
                })
        })
        .collect()
}

fn load_program_bytes(path: &Path) -> Result<LoadedProgram, String> {
    let loaded = read_program_asset(path)
        .map_err(|err| format!("failed to read program {}: {err}", path.display()))?;
    let name = loaded.archive_member.unwrap_or_else(|| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| path.display().to_string())
    });

    Ok(LoadedProgram {
        name,
        bytes: loaded.bytes,
    })
}

fn resolve_rom_dir(cli: &Cli) -> Result<Option<PathBuf>, String> {
    if let Some(dir) = &cli.rom_dir {
        return Ok(Some(dir.clone()));
    }

    if let Ok(dir) = std::env::var("EMU198X_C64_ROM_DIR") {
        return Ok(Some(PathBuf::from(dir)));
    }

    let Some(home) = std::env::var_os("HOME") else {
        return Ok(None);
    };
    let commodore_dir = PathBuf::from(&home).join(".emu198x/roms/commodore-c64");
    if commodore_dir.exists() {
        return Ok(Some(commodore_dir));
    }

    let legacy_dir = PathBuf::from(home).join(".emu198x/roms/c64");
    if legacy_dir.exists() {
        return Ok(Some(legacy_dir));
    }

    if cli.kernal.is_some()
        || cli.basic.is_some()
        || cli.chargen.is_some()
        || cli.load_snapshot.is_some()
    {
        return Ok(None);
    }

    Err(
        "no C64 ROM directory found — pass --rom-dir DIR, set EMU198X_C64_ROM_DIR, or create ~/.emu198x/roms/commodore-c64".into(),
    )
}

fn resolve_rom_path(
    explicit: Option<&Path>,
    rom_dir: Option<&Path>,
    filenames: &[&str],
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = explicit {
        return Ok(Some(path.to_path_buf()));
    }

    let Some(rom_dir) = rom_dir else {
        return Ok(None);
    };

    for filename in filenames {
        let candidate = rom_dir.join(filename);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }

    Err(format!(
        "missing required ROM in {} (looked for {})",
        rom_dir.display(),
        filenames.join(", ")
    ))
}

fn query_bool(
    session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    path: &str,
) -> Result<bool, String> {
    let result = session
        .query(path)
        .map_err(|err| format!("query {path} failed: {err}"))?;
    result
        .value
        .as_bool()
        .ok_or_else(|| format!("query {path} did not return a boolean value"))
}

fn query_value(
    session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    path: &str,
) -> Result<Value, String> {
    session
        .query(path)
        .map(|result| result.value)
        .map_err(|err| format!("query {path} failed: {err}"))
}

fn query_string(
    session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    path: &str,
) -> Result<Option<String>, String> {
    let result = session
        .query(path)
        .map_err(|err| format!("query {path} failed: {err}"))?;
    Ok(result.value.as_str().map(str::to_owned))
}

fn query_string_list(
    session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    path: &str,
) -> Result<Vec<String>, String> {
    let result = session
        .query(path)
        .map_err(|err| format!("query {path} failed: {err}"))?;
    let values = result
        .value
        .as_array()
        .ok_or_else(|| format!("query {path} did not return an array value"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("query {path} contained a non-string entry"))
        })
        .collect()
}

fn collect_queries(
    session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    paths: &[String],
) -> Result<Vec<ReportedQuery>, String> {
    paths
        .iter()
        .map(|path| {
            query_value(session, path).map(|value| ReportedQuery {
                path: path.clone(),
                value,
            })
        })
        .collect()
}

fn wait_for_tape_motion_to_stop(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    max_frames: u32,
) -> Result<ScriptObservation, String> {
    let mut frames = 0;
    let mut saw_motion = false;

    loop {
        let playing = query_bool(session, "c64.tape.playing")?;
        if playing {
            saw_motion = true;
        } else if saw_motion {
            return Ok(ScriptObservation::WaitForQueryBool {
                path: "c64.tape.playing".to_owned(),
                value: false,
                frames,
                reached: session.time(),
            });
        }

        if frames >= max_frames {
            return Ok(ScriptObservation::WaitForQueryBool {
                path: "c64.tape.playing".to_owned(),
                value: false,
                frames,
                reached: session.time(),
            });
        }

        session
            .run_frames(1)
            .map_err(|err| format!("run failed while waiting for tape stop: {err}"))?;
        frames += 1;
    }
}

impl ModelArg {
    const fn to_model(self) -> Model {
        match self {
            Self::Pal => Model::C64PalBreadbin,
            Self::Ntsc => Model::C64NtscBreadbin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_snapshot_boot_and_capture_flags() {
        let cli = parse_cli([
            "--model".to_string(),
            "ntsc".to_string(),
            "--rom-dir".to_string(),
            "roms".to_string(),
            "--load-snapshot".to_string(),
            "in.c64.pst".to_string(),
            "--save-snapshot".to_string(),
            "out.c64.pst".to_string(),
            "--wait-for-boot".to_string(),
            "180".to_string(),
            "--print-screen-text".to_string(),
            "--frames".to_string(),
            "12".to_string(),
            "--screenshot".to_string(),
            "ready.png".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Ntsc,
                rom_dir: Some(PathBuf::from("roms")),
                kernal: None,
                basic: None,
                chargen: None,
                load: None,
                tape: None,
                autoload_tape: false,
                start_tape: false,
                load_snapshot: Some(PathBuf::from("in.c64.pst")),
                save_snapshot: Some(PathBuf::from("out.c64.pst")),
                screenshot: Some(PathBuf::from("ready.png")),
                script: None,
                wait_for_boot: Some(180),
                wait_for_tape_stop: None,
                print_queries: vec![],
                print_screen_text: true,
                trace_vic_colours: false,
                trace_limit: DEFAULT_TRACE_LIMIT,
                frames: 12,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_autoload_flag() {
        let cli = parse_cli([
            "--rom-dir".to_string(),
            "roms".to_string(),
            "--tape".to_string(),
            "game.tap".to_string(),
            "--autoload-tape".to_string(),
            "--wait-for-tape-stop".to_string(),
            "12000".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Pal,
                rom_dir: Some(PathBuf::from("roms")),
                kernal: None,
                basic: None,
                chargen: None,
                load: None,
                tape: Some(PathBuf::from("game.tap")),
                autoload_tape: true,
                start_tape: false,
                load_snapshot: None,
                save_snapshot: None,
                screenshot: None,
                script: None,
                wait_for_boot: None,
                wait_for_tape_stop: Some(12000),
                print_queries: vec![],
                print_screen_text: false,
                trace_vic_colours: false,
                trace_limit: DEFAULT_TRACE_LIMIT,
                frames: 0,
            }
        );
    }

    #[test]
    fn resolve_rom_path_prefers_explicit_override() {
        let resolved = resolve_rom_path(
            Some(Path::new("override/kernal.rom")),
            Some(Path::new("roms")),
            &["kernal.rom", "c64-kernal.rom"],
        )
        .expect("explicit ROM path should resolve");

        assert_eq!(resolved, Some(PathBuf::from("override/kernal.rom")));
    }

    #[test]
    fn parse_cli_accepts_program_import() {
        let cli = parse_cli([
            "--rom-dir".to_string(),
            "roms".to_string(),
            "--load".to_string(),
            "demo.bas".to_string(),
        ]);

        assert_eq!(cli.load, Some(PathBuf::from("demo.bas")));
    }
}
