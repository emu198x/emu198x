//! Headless C64 runner — `--script` / `--headless` mode.
//!
//! Boots the C64 from firmware (KERNAL/BASIC/chargen/1541), runs native
//! frames, executes shared JSON session steps, imports PRG/BAS/disk/tape
//! media, and captures screenshots / snapshots / screen-text / traces.
//! The non-interactive half of the `emu198x-c64` binary; the shared
//! launcher routes here through `MachineApp::run_script` when a headless
//! flag is present. The launcher parses the flags; this module runs them.
//!
//! The shared script loop is not used because the C64's report carries
//! more than it has a shape for: boot detection, the imported program,
//! printed queries, decoded screen text, and VIC / drive-ROM traces — and
//! prints a plain summary rather than JSON when no `--script` was given.

use std::fs;
use std::path::Path;

use emu198x_shell::launch::{CommonCli, LaunchError, MachineApp};
use emu198x_shell::{
    ControlCommand, HeadlessScript, HeadlessSession, MediaTransportAction, MediaTransportCommand,
    ScriptObservation, ScriptStep, TraceEvent, TraceSink,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_BASIC_LOADER_BOOT_FRAMES,
    DEFAULT_DISK_AUTOLOAD_SLOT, DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT,
    DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, autoload_basic_disk, autoload_basic_disk_with_trace_sink,
    autoload_basic_tape, autoload_basic_tape_with_trace_sink, file_loader::load_host_file,
    load_basic_source,
};
use serde::Serialize;
use serde_json::Value;

use crate::app::{C64, DEFAULT_IMPORT_BOOT_FRAMES, DEFAULT_TAPE_SLOT, load_program_bytes};

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

/// Headless entry point. Runs the session and prints the JSON (script
/// mode) or summary report.
///
/// # Errors
///
/// Returns the failure of the boot, the media workflow, the script, a
/// capture, or a query.
pub fn run(app: &C64, common: &CommonCli) -> Result<(), LaunchError> {
    let script_mode = common.script.is_some();
    let report = run_cli(app, common)?;
    if script_mode {
        let json = serde_json::to_string(&report)
            .map_err(|err| format!("failed to serialize runner report: {err}"))?;
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
    Ok(())
}

fn run_cli(cli: &C64, common: &CommonCli) -> Result<RunnerReport, LaunchError> {
    cli.check_media_flags()?;

    if (common.screenshot.is_some() || common.audio_capture.is_some())
        && common.frames == 0
        && common.script.is_none()
        && cli.wait_for_boot.is_none()
    {
        return Err(LaunchError::Usage(
            "capture requests require --frames, --wait-for-boot, or --script so the machine emits output"
                .to_owned(),
        ));
    }

    let machine = cli.boot_runtime()?;
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        cli.frame_ticks(),
        C64SessionQueryProvider,
    );
    let tracing_enabled = cli.trace_vic_colours || cli.trace_drive_rom_window.is_some();
    let mut trace_collector = None;

    let mut observations = Vec::new();
    let needs_boot_before_import = cli.load.is_some();
    if cli.wait_for_boot.is_some() || needs_boot_before_import {
        let explicit_wait = cli.wait_for_boot.is_some();
        let max_frames = cli.wait_for_boot.unwrap_or(DEFAULT_IMPORT_BOOT_FRAMES);
        let result = if let Some(collector) = trace_collector.as_mut() {
            session
                .wait_for_boot_with_trace_sink(max_frames, collector)
                .map_err(|err| format!("boot wait failed: {err}"))?
        } else {
            session
                .wait_for_boot(max_frames)
                .map_err(|err| format!("boot wait failed: {err}"))?
        };
        if explicit_wait {
            observations.push(ScriptObservation::WaitForBoot {
                frames: result.frames,
                reached: result.reached,
                reason: result.reason,
                row: result.row,
            });
        }
    }

    cli.insert_media(&mut session)?;

    if cli.autoload_tape {
        if trace_collector.is_none() && tracing_enabled {
            session
                .machine_mut()
                .set_trace_vic_colour_writes(cli.trace_vic_colours);
            session
                .machine_mut()
                .set_trace_drive_rom_window(cli.trace_drive_rom_window);
            trace_collector = Some(TraceCollector::with_limit(cli.trace_limit));
        }
        if !session.machine().machine().tape_is_loaded() {
            return Err(LaunchError::Run(
                "--autoload-tape requires tape media in slot tape-1".to_owned(),
            ));
        }

        if let Some(collector) = trace_collector.as_mut() {
            autoload_basic_tape_with_trace_sink(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
                collector,
            )
            .map_err(|err| format!("tape autoload failed: {err}"))?;
        } else {
            autoload_basic_tape(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
            )
            .map_err(|err| format!("tape autoload failed: {err}"))?;
        }
    }

    if cli.autoload_disk {
        if trace_collector.is_none() && tracing_enabled {
            session
                .machine_mut()
                .set_trace_vic_colour_writes(cli.trace_vic_colours);
            session
                .machine_mut()
                .set_trace_drive_rom_window(cli.trace_drive_rom_window);
            trace_collector = Some(TraceCollector::with_limit(cli.trace_limit));
        }
        if let Some(collector) = trace_collector.as_mut() {
            autoload_basic_disk_with_trace_sink(
                &mut session,
                DEFAULT_DISK_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
                collector,
            )
            .map_err(|err| format!("disk autoload failed: {err}"))?;
        } else {
            autoload_basic_disk(
                &mut session,
                DEFAULT_DISK_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
            )
            .map_err(|err| format!("disk autoload failed: {err}"))?;
        }
    }

    let mut loaded_program = None;
    if let Some(path) = &cli.load {
        let loaded = load_program_bytes(path)?;
        loaded_program = Some(
            load_host_file(session.machine_mut(), &loaded.name, &loaded.bytes)
                .map_err(|err| format!("program import failed: {err}"))?,
        );
    }

    if let Some(path) = &common.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        // Step by step rather than `execute_collect` on the whole script, so
        // the C64's own steps can be intercepted before the shared executor
        // sees them. `load_basic_program` is one: the shell has no handler for
        // it and reports `requires a system-specific handler` *mid-run*, after
        // a script has already booted and typed — a late failure on an action
        // the parser had accepted. The C64 has had the loader all along, wired
        // only into its MCP tool. See #914.
        for step in &script.steps {
            let emitted = match step {
                ScriptStep::LoadBasicProgram { path, run } => {
                    Some(execute_load_basic_program(&mut session, path, *run)?)
                }
                other => other
                    .execute_collect(&mut session)
                    .map_err(|err| format!("script execution failed: {err}"))?,
            };
            if let Some(observation) = emitted {
                observations.push(observation);
            }
        }
    }

    if cli.start_tape {
        session
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_SLOT,
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

    if let Some(collector) = trace_collector.as_mut() {
        if common.frames > 0 {
            session
                .run_frames_with_trace_sink(common.frames, collector)
                .map_err(|err| format!("run failed: {err}"))?;
        }
        session.machine_mut().set_trace_vic_colour_writes(false);
        session.machine_mut().set_trace_drive_rom_window(None);
    } else if common.frames > 0 {
        if tracing_enabled {
            session
                .machine_mut()
                .set_trace_vic_colour_writes(cli.trace_vic_colours);
            session
                .machine_mut()
                .set_trace_drive_rom_window(cli.trace_drive_rom_window);
            let mut collector = TraceCollector::with_limit(cli.trace_limit);
            session
                .run_frames_with_trace_sink(common.frames, &mut collector)
                .map_err(|err| format!("run failed: {err}"))?;
            session.machine_mut().set_trace_vic_colour_writes(false);
            session.machine_mut().set_trace_drive_rom_window(None);
            trace_collector = Some(collector);
        } else {
            session
                .run_frames(common.frames)
                .map_err(|err| format!("run failed: {err}"))?;
        }
    }
    let trace_lines = trace_collector.map(TraceCollector::into_lines);

    if let Some(path) = &cli.save_snapshot {
        session
            .save_snapshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    if let Some(path) = &common.screenshot {
        session
            .save_screenshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    if let Some(path) = &common.audio_capture {
        session
            .save_audio_capture(path)
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

/// Installs a plain-text BASIC program, as the MCP tool of the same name does.
///
/// Shares `load_basic_source` with `mcp_tools::LoadBasicProgramTool` rather
/// than reimplementing the poke-and-relink: the tokeniser writes to `$0801`,
/// relinks the line pointers and sets `VARTAB`, and optionally drives the
/// editor to `RUN`.
fn execute_load_basic_program(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    path: &Path,
    run: bool,
) -> Result<ScriptObservation, String> {
    let source = fs::read_to_string(path).map_err(|err| {
        format!(
            "load_basic_program: failed to read {}: {err}",
            path.display()
        )
    })?;
    let result = load_basic_source(session, &source, run, DEFAULT_BASIC_LOADER_BOOT_FRAMES)
        .map_err(|err| {
            format!(
                "load_basic_program: BASIC loader failed for {}: {err}",
                path.display()
            )
        })?;
    Ok(ScriptObservation::LoadBasicProgram {
        program_bytes: result.program_bytes,
        ran: result.ran,
    })
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
        let playing = query_bool(session, "tape.playing")?;
        if playing {
            saw_motion = true;
        } else if saw_motion {
            return Ok(ScriptObservation::WaitForQueryBool {
                path: "tape.playing".to_owned(),
                value: false,
                frames,
                reached: session.time(),
            });
        }

        if frames >= max_frames {
            return Ok(ScriptObservation::WaitForQueryBool {
                path: "tape.playing".to_owned(),
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
