//! MCP tool registrations for the Spectrum binary.
//!
//! Only the genuinely Spectrum-specific tools live here — Z80 port I/O, the
//! AY register query, tape/BASIC loaders, keyboard helpers, `set_machine`,
//! and the `load_snapshot` override. The generic CPU/memory/disassembly
//! verbs come from the shared `register_debug_tools` tier, and the
//! `watch_memory_*` / `watch_ay_*` verbs from the shared watch tier
//! (`register_memory_watch_tools` / `register_ay_watch_tools`). Each tool's
//! `call` lifts the JSON arguments into a `ScriptStep` (by injecting the
//! `action` discriminator and re-deserializing), dispatches it through the
//! same `mcp_execute_step` interceptor script mode uses, and returns the
//! resulting `ScriptObservation` as a JSON-text content block.
//!
//! Schemas are hand-written. The crate's existing JSON-round-trip tests
//! freeze the wire shape of each `ScriptStep` variant; if those tests
//! break, a tool's schema here probably also needs an update.

use emu198x_shell::{
    HeadlessSession, ScriptObservation, ScriptStep,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
    mcp_tools::ScriptStepTool,
};
use runtime_sinclair_zx_spectrum::{
    SpectrumLiveAccess, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
};
use serde_json::{Value, json};

use crate::portable_snapshot::{is_portable_snapshot_path, parse_portable_snapshot_at};

/// Live-session context every Spectrum MCP tool dispatches against.
///
/// Family-level: the inner runtime is one of the SOLID-8 Spectrum
/// variants, chosen at boot time and swappable mid-session via the
/// `set_machine` tool.
pub type SpectrumSession = HeadlessSession<SpectrumRuntimeKind, SpectrumSessionQueryProvider>;

/// Register one Spectrum step tool: the shared `ScriptStepTool` wrapper
/// (from the shell) carrying the Spectrum's richer `mcp_execute_step`
/// dispatcher. Replaces the bespoke wrapper that used to live here — the
/// tool plumbing is now shared fleet-wide, only the dispatcher is
/// Spectrum-specific (#456).
fn add_step(
    registry: &mut ToolRegistry<SpectrumSession>,
    name: &'static str,
    description: &'static str,
    schema: Value,
) {
    registry.register(Box::new(ScriptStepTool::with_dispatch(
        name,
        description,
        schema,
        mcp_execute_step,
    )));
}

/// Family-MCP dispatch for one `ScriptStep`.
///
/// - `LoadSnapshot` of a portable `.sna` / `.z80`: routed through the
///   Spectrum parser rather than the runtime save-state decoder.
/// - Everything else delegates to [`ScriptStep::execute_collect`]: the
///   keyboard verbs run through the shared `KeyboardTarget`, the tape and
///   BASIC loaders through the `MachineCore` loader hooks (RULES.md #30).
fn mcp_execute_step(
    step: &ScriptStep,
    session: &mut SpectrumSession,
) -> Result<Option<ScriptObservation>, ToolError> {
    match step {
        ScriptStep::LoadSnapshot { path } if is_portable_snapshot_path(path) => {
            execute_load_portable_snapshot(session, path).map(|()| None)
        }
        other => other
            .execute_collect(session)
            .map_err(|err| ToolError::Execution(format!("{err}"))),
    }
}

/// MCP-side equivalent of
/// `crate::script::runner::execute_load_portable_snapshot`. Shares the
/// classifier + parser through [`crate::portable_snapshot`] and applies
/// the result through [`SpectrumLiveAccess::apply_snapshot`] so every
/// runtime kind in `SpectrumRuntimeKind` is reachable. Shared with the
/// `--script` runner (#456) — both modes hold the family enum session.
pub(crate) fn execute_load_portable_snapshot(
    session: &mut SpectrumSession,
    path: &std::path::Path,
) -> Result<(), ToolError> {
    if session.is_recording() {
        return Err(ToolError::Execution(format!(
            "cannot load portable snapshot {} while a video recording is in flight; \
             stop the recording first",
            path.display()
        )));
    }
    let snapshot =
        parse_portable_snapshot_at(path).map_err(|err| ToolError::Execution(format!("{err}")))?;
    SpectrumLiveAccess::apply_snapshot(session.machine_mut(), &snapshot);
    Ok(())
}

/// Registers the Spectrum-specific MCP tools on the supplied registry:
/// the bespoke surface (`set_machine`, `autoload_tape`,
/// `load_basic_program`, `port_read`/`write`, `type_string`,
/// `press_key`, `watch_ay_*`, `watch_memory_*`, `clear_audio_capture`)
/// plus the Z80 debug tools that aren't on the shared surface
/// (`memory_read`, `disasm`, `step`, `run_until_pc`,
/// `poke_byte`/`poke_word`).
///
/// Two reads are NOT here, both folded onto the generic surface (#456):
/// `query_cpu` comes from the shared `register_debug_tools` via the
/// enriched `DebugTarget::dbg_cpu_state`, and `query_ay` became the
/// grouped `ay` object + decoded `ay.*` query paths. The other generic
/// tools (`run_frames`, `input`, `query`, media, capture, `reset`, …)
/// likewise come from `register_common_tools`. Order is the order shown
/// by `tools/list`.
/// `save_tape` — persist a tape `SAVE` to a host `.tap`.
///
/// During a BASIC `SAVE` the ROM toggles the MIC line; the recorder captures
/// that signal and this decodes it back into standard-speed blocks, writing a
/// reloadable `.tap` to `path`. Captures every `SAVE` performed since the tape
/// session started (authentic multi-file tape behaviour). Mirrors the C64
/// `save_disk` tool. See `docs/systems/sinclair/zx-spectrum/index.md`.
struct SaveTapeTool;

impl Tool<SpectrumSession> for SaveTapeTool {
    fn name(&self) -> &str {
        "save_tape"
    }

    fn description(&self) -> &str {
        "Persist a tape SAVE: decode the MIC signal the ROM laid down during BASIC SAVE into a standard .tap and write it to `path`. Captures every SAVE since the tape session started. Errors if nothing has been saved yet."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
            },
            "required": ["path"],
        })
    }

    fn call(
        &self,
        arguments: Value,
        session: &mut SpectrumSession,
    ) -> Result<ToolResponse, ToolError> {
        let path = arguments
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::Execution("save_tape: missing required string 'path'".to_owned())
            })?;
        let bytes = session.machine().flush_tape_image().ok_or_else(|| {
            ToolError::Execution("save_tape: no tape SAVE has been recorded".to_owned())
        })?;
        let len = bytes.len();
        std::fs::write(path, &bytes).map_err(|err| {
            ToolError::Execution(format!("save_tape: failed to write {path}: {err}"))
        })?;

        let body = json!({ "kind": "save_tape", "path": path, "bytes": len }).to_string();
        Ok(ToolResponse::success_text(body))
    }
}

pub fn register_spectrum_tools(registry: &mut ToolRegistry<SpectrumSession>) {
    let string_field = || json!({"type": "string"});

    // `set_machine` comes from the shared variant-switch tier over the
    // family runtime's `MachineCore::set_machine` hook; the profiles
    // declare `variant-switch`.

    // `autoload_tape` and `load_basic_program` come from the shared
    // loader tiers over the family runtime's `MachineCore` hooks; the
    // profile declares `tape-autoload` / `basic-program-load`.

    // Override the shared `load_snapshot` tool (register_common_tools)
    // with the Spectrum one so it dispatches through `mcp_execute_step`,
    // which routes portable `.sna` / `.z80` (and `.zip`) files to the
    // shared snapshot parser. The shared tool postcard-decodes the path
    // as the runtime's own save state, which fails on a portable
    // snapshot ("Found a bool that wasn't 0 or 1"). Registered last so it
    // wins by name. gap #6.
    add_step(
        registry,
        "load_snapshot",
        "Restore a snapshot into the live machine. Portable `.sna` / `.z80` files (optionally inside a `.zip`) are parsed and applied; a runtime save-state blob is restored directly.",
        json!({
            "type": "object",
            "properties": { "path": string_field() },
            "required": ["path"],
        }),
    );

    // `query_ay` folded into the generic `query` surface (#456): the AY
    // snapshot is the grouped `ay` object plus decoded `ay.*` leaves
    // (tone/noise periods, mixer, amplitudes, envelope), resolved by the
    // runtime's `resolve_ay_path`. No bespoke MCP tool any more.

    // `query_cpu` is served by the shared `register_debug_tools` via the
    // enriched `DebugTarget::dbg_cpu_state` (full Z80 file + decoded
    // flags), so there is no bespoke override here any more (#456).

    // press_key / type_string now come from the shared keyboard tier
    // (`register_keyboard_tools`, registered in `mcp/mod.rs`) over the
    // Spectrum's `KeyboardTarget` impl — one body for MCP + `--script`,
    // fleet-wide. RULES.md #30.

    registry.register(Box::new(SaveTapeTool));
}

#[cfg(test)]
mod tests {
    use super::*;
    // The step-parsing round-trip tests below validate ScriptStep serde
    // for Spectrum-relevant actions; they exercise the shared shell
    // builder (the bespoke `parse_step` folded into it, #456).
    use emu198x_shell::mcp_tools::build_step as parse_step;

    #[test]
    fn parse_step_round_trips_run_frames_arguments() {
        let step = parse_step("run_frames", json!({"frames": 25})).expect("valid step");
        assert_eq!(step, ScriptStep::RunFrames { frames: 25 });
    }

    #[test]
    fn parse_step_round_trips_load_basic_program_with_default_run() {
        let step =
            parse_step("load_basic_program", json!({"path": "hello.bas"})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::LoadBasicProgram {
                path: "hello.bas".into(),
                run: true,
            }
        );
    }

    #[test]
    fn parse_step_rejects_non_object_arguments() {
        let err = parse_step("run_frames", json!(42)).expect_err("non-object");
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_zero_field_steps() {
        let step = parse_step("stop_video_recording", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::StopVideoRecording);
    }

    #[test]
    fn parse_step_round_trips_reset_with_kind() {
        use emu198x_shell::ResetKind;
        let step = parse_step("reset", json!({"kind": "hard"})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::Reset {
                kind: ResetKind::Hard
            }
        );
        let step = parse_step("reset", json!({"kind": "soft"})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::Reset {
                kind: ResetKind::Soft
            }
        );
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_query_ay() {
        let step = parse_step("query_ay", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::QueryAy);
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_query_cpu() {
        let step = parse_step("query_cpu", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::QueryCpu);
    }

    #[test]
    fn parse_step_round_trips_step_with_default_count() {
        let step = parse_step("step", json!({})).expect("valid step");
        assert_eq!(step, ScriptStep::Step { instructions: None });
        let step = parse_step("step", json!({"instructions": 5})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::Step {
                instructions: Some(5),
            }
        );
    }

    #[test]
    fn parse_step_round_trips_run_until_pc() {
        let step = parse_step("run_until_pc", json!({"addr": 0x1234})).expect("valid run_until_pc");
        assert_eq!(
            step,
            ScriptStep::RunUntilPc {
                addr: 0x1234,
                max_steps: None,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_disasm() {
        let step =
            parse_step("disasm", json!({"addr": 0x4000, "instructions": 8})).expect("valid disasm");
        assert_eq!(
            step,
            ScriptStep::Disasm {
                addr: 0x4000,
                instructions: Some(8),
            }
        );
    }

    #[test]
    fn parse_step_round_trips_port_read_and_write() {
        let r = parse_step("port_read", json!({"port": 0x00FE})).expect("valid port_read");
        assert_eq!(r, ScriptStep::PortRead { port: 0x00FE });
        let w = parse_step("port_write", json!({"port": 0x00FE, "value": 5}))
            .expect("valid port_write");
        assert_eq!(
            w,
            ScriptStep::PortWrite {
                port: 0x00FE,
                value: 5,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_press_key_default_and_explicit_hold() {
        let s = parse_step("press_key", json!({"key": "Space"})).expect("valid press_key");
        assert_eq!(
            s,
            ScriptStep::PressKey {
                key: "Space".into(),
                hold_frames: None,
            }
        );
        let s = parse_step("press_key", json!({"key": "Enter", "hold_frames": 8}))
            .expect("valid press_key with hold");
        assert_eq!(
            s,
            ScriptStep::PressKey {
                key: "Enter".into(),
                hold_frames: Some(8),
            }
        );
    }

    #[test]
    fn parse_step_round_trips_watch_ay_variants() {
        let start = parse_step("watch_ay_start", Value::Null).expect("valid watch_ay_start");
        assert_eq!(start, ScriptStep::WatchAyStart);
        let clear = parse_step("watch_ay_clear", Value::Null).expect("valid watch_ay_clear");
        assert_eq!(clear, ScriptStep::WatchAyClear);
        let log = parse_step("watch_ay_log", json!({})).expect("valid watch_ay_log");
        assert_eq!(
            log,
            ScriptStep::WatchAyLog {
                limit: None,
                unique: false,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_memory_read_arguments() {
        let step = parse_step("memory_read", json!({"addr": 0x4000, "len": 32}))
            .expect("valid memory_read");
        assert_eq!(
            step,
            ScriptStep::MemoryRead {
                addr: 0x4000,
                len: 32,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_watch_memory_start_arguments() {
        let step = parse_step("watch_memory_start", json!({"addr": 0x5800, "len": 0x300}))
            .expect("valid watch_memory_start");
        assert_eq!(
            step,
            ScriptStep::WatchMemoryStart {
                addr: 0x5800,
                len: 0x300,
            }
        );
    }

    #[test]
    fn parse_step_accepts_empty_object_for_watch_memory_log() {
        let step = parse_step("watch_memory_log", json!({})).expect("valid watch_memory_log");
        assert_eq!(
            step,
            ScriptStep::WatchMemoryLog {
                limit: None,
                unique: false,
                source: None,
                cck_min: None,
                cck_max: None,
            }
        );
    }

    #[test]
    fn register_spectrum_tools_holds_the_bespoke_surface_only() {
        let mut registry: ToolRegistry<SpectrumSession> = ToolRegistry::new();
        register_spectrum_tools(&mut registry);
        let names: Vec<_> = registry.iter().map(|tool| tool.name().to_owned()).collect();

        // Only genuinely Spectrum-specific tools live here —
        // `save_tape`, and `load_snapshot` (an
        // intentional override so portable `.sna` / `.z80` route through
        // the Spectrum parser, gap #6). The generic CPU/memory/disassembly
        // verbs — `query_cpu`, `memory_read`, `disasm`, `step`, `poke_byte`,
        // `poke_word`, `run_until_pc` — are NOT here: they come from the
        // shared `register_debug_tools` tier; the `watch_memory_*` /
        // `watch_ay_*` verbs from the shared watch tier; `port_read` /
        // `port_write` from the shared port-I/O tier behind `PortIoTarget`;
        // `autoload_tape` / `load_basic_program` from the loader tiers over
        // the `MachineCore` hooks; and `clear_audio_capture` / `query_ay`
        // from the common set. MCP
        // and `--script` run one implementation (RULES.md #30, #456,
        // knowledge/decisions/tools-follow-the-machine-spec.md).
        let expected = ["load_snapshot", "save_tape"];
        for name in expected {
            assert!(names.contains(&name.to_owned()), "missing {name}");
        }
        assert_eq!(names.len(), expected.len(), "unexpected extra tool");

        // The generic + watch + keyboard verbs come from the shared tiers
        // (`register_common_tools` / `register_debug_tools` /
        // `register_memory_watch_tools` / `register_ay_watch_tools` /
        // `register_keyboard_tools`), NOT from here — they must be absent so
        // the fold doesn't double up.
        for shared in [
            "run_frames",
            "load_media",
            "input",
            "query",
            "reset",
            "watch_memory_start",
            "watch_memory_clear",
            "watch_memory_log",
            "watch_ay_start",
            "watch_ay_clear",
            "watch_ay_log",
            "press_key",
            "type_string",
            "port_read",
            "port_write",
            "clear_audio_capture",
            "query_ay",
            "autoload_tape",
            "load_basic_program",
            "set_machine",
        ] {
            assert!(
                !names.contains(&shared.to_owned()),
                "`{shared}` should come from a shared tier, not register_spectrum_tools"
            );
        }
    }
}
