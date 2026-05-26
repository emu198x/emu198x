//! MCP tool registrations for the Spectrum binary.
//!
//! One tool per `ScriptStep` variant (≈18 tools). Each tool's `call`
//! lifts the supplied JSON arguments into a `ScriptStep` (by injecting
//! the `action` discriminator and re-deserializing), dispatches it
//! through the same `execute_step` interceptor that script mode uses,
//! and returns the resulting `ScriptObservation` as a JSON-text content
//! block.
//!
//! Schemas are hand-written. The crate's existing JSON-round-trip tests
//! freeze the wire shape of each `ScriptStep` variant; if those tests
//! break, a tool's schema here probably also needs an update.

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, MachineCore, ScriptObservation, ScriptStep,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
    read_firmware_asset,
};
use format_sinclair_zx_spectrum_bas::tokenise;
use runtime_sinclair_zx_spectrum::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, SpectrumRuntimeKind,
    SpectrumSessionQueryProvider, autoload_basic_tape, load_basic_program,
};
use serde_json::{Value, json};

use crate::machine::{MachineKind, rom_root, variant_rom_bundle};

/// Live-session context every Spectrum MCP tool dispatches against.
///
/// Family-level: the inner runtime is one of the SOLID-8 Spectrum
/// variants, chosen at boot time and swappable mid-session via the
/// `set_machine` tool.
pub type SpectrumSession = HeadlessSession<SpectrumRuntimeKind, SpectrumSessionQueryProvider>;

/// One MCP tool that maps directly onto a `ScriptStep` variant.
struct ScriptStepTool {
    /// Stable tool name; matches the variant's serde `action` tag.
    name: &'static str,
    /// Human-readable description shown by MCP clients.
    description: &'static str,
    /// JSON Schema for the tool's input arguments.
    schema: Value,
}

impl Tool<SpectrumSession> for ScriptStepTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    fn call(
        &self,
        arguments: Value,
        session: &mut SpectrumSession,
    ) -> Result<ToolResponse, ToolError> {
        let step = parse_step(self.name, arguments)?;
        let observation = mcp_execute_step(&step, session)?;
        let body = match observation {
            Some(obs) => serde_json::to_string(&obs).map_err(|err| {
                ToolError::Execution(format!("failed to serialize observation: {err}"))
            })?,
            None => String::from("null"),
        };
        Ok(ToolResponse::success_text(body))
    }
}

/// Family-MCP dispatch for one `ScriptStep`.
///
/// - `SetMachine`: rebuilds the inner runtime to the requested
///   variant. The session-side state (queued input, latest frame,
///   captured audio, last run result) is cleared via
///   [`HeadlessSession::reset`] so the new variant starts from a
///   clean session.
/// - `AutoloadTape` / `LoadBasicProgram`: 48K-only on the runtime
///   side today. We downcast through
///   [`SpectrumRuntimeKind::as_48k_mut`]; if the active variant is
///   not 48K we return [`ToolError::Execution`] with a clear message.
///   (Generalising these helpers to the 128K family is its own
///   commit on the runtime crate.)
/// - Everything else delegates to [`ScriptStep::execute_collect`],
///   which works generically over `MachineCore`.
fn mcp_execute_step(
    step: &ScriptStep,
    session: &mut SpectrumSession,
) -> Result<Option<ScriptObservation>, ToolError> {
    match step {
        ScriptStep::SetMachine { machine } => execute_set_machine(machine, session).map(Some),
        ScriptStep::QueryAy => execute_query_ay(session).map(Some),
        ScriptStep::AutoloadTape {
            slot,
            max_boot_frames,
        } => {
            let frames = if *max_boot_frames == 0 {
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES
            } else {
                *max_boot_frames
            };
            execute_autoload_tape(session, slot, frames).map(Some)
        }
        ScriptStep::LoadBasicProgram { path, run } => {
            execute_load_basic_program(session, path, *run).map(Some)
        }
        other => other
            .execute_collect(session)
            .map_err(|err| ToolError::Execution(format!("{err}"))),
    }
}

fn execute_set_machine(
    requested: &str,
    session: &mut SpectrumSession,
) -> Result<ScriptObservation, ToolError> {
    let kind = MachineKind::from_script_id(requested).ok_or_else(|| {
        ToolError::InvalidArguments(format!(
            "set_machine: unknown machine id `{requested}`; expected one of \
             spectrum_16k, spectrum_48k, spectrum_plus, spectrum_128k, \
             spectrum_plus2, spectrum_plus2a, spectrum_plus2b, spectrum_plus3"
        ))
    })?;
    let model = kind_to_model(kind);
    let rom_root_dir = rom_root().ok_or_else(|| {
        ToolError::Execution(
            "set_machine: $HOME is unset; cannot locate ROM bundle root \
             (~/.emu198x/roms)"
                .to_owned(),
        )
    })?;

    // Two-pass firmware load: read all ROM bytes into an owned vec,
    // then borrow them into the `FirmwareSet`. Mirrors the pattern
    // used by `script::runner::boot_eager_variant`.
    let bundle = variant_rom_bundle(kind, &rom_root_dir);
    let mut rom_bytes: Vec<(String, Vec<u8>)> = Vec::with_capacity(bundle.len());
    for (id, path) in bundle {
        if !path.is_file() {
            return Err(ToolError::Execution(format!(
                "set_machine: ROM not found at {}",
                path.display()
            )));
        }
        let loaded = read_firmware_asset(&path).map_err(|err| {
            ToolError::Execution(format!(
                "set_machine: failed to read {}: {err}",
                path.display()
            ))
        })?;
        rom_bytes.push((id.to_string(), loaded.bytes.to_vec()));
    }
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &rom_bytes {
        firmware.push(FirmwareImage::new(id.clone(), bytes));
    }
    let new_runtime = SpectrumRuntimeKind::from_firmware(model, &firmware).map_err(|err| {
        ToolError::Execution(format!("set_machine: build runtime: {err}"))
    })?;
    let profile = new_runtime.profile().clone();

    // Swap the inner machine + clear session-side state, and re-pace
    // the session to the new variant's frame budget so `run_frames`
    // emits one native frame per call.
    let new_frame_ticks = u64::from(new_runtime.frame_halfcycles());
    *session.machine_mut() = new_runtime;
    session.set_native_frame_ticks(new_frame_ticks);
    session
        .reset(emu198x_shell::ResetKind::Hard)
        .map_err(|err| ToolError::Execution(format!("set_machine: clear session: {err}")))?;

    Ok(ScriptObservation::SetMachine {
        machine: requested.to_owned(),
        profile_id: profile.profile_id.as_str().to_owned(),
        display_name: profile.display_name.to_string(),
    })
}

fn execute_query_ay(session: &mut SpectrumSession) -> Result<ScriptObservation, ToolError> {
    // Look up the two low-level AY paths through the existing
    // session query provider; on AY-bearing variants both resolve,
    // on 48K-class variants `spectrum.ay.registers` is not in
    // `variant_query_paths()` and the provider returns `Ok(None)` →
    // QueryError::UnknownPath. We surface that as a clear "active
    // variant has no AY" error rather than a generic UnknownPath.
    let regs = session
        .query("spectrum.ay.registers")
        .map_err(|err| ay_unsupported_error(&err))?;
    let raw: Vec<u8> = serde_json::from_value(regs.value).map_err(|err| {
        ToolError::Execution(format!("query_ay: malformed spectrum.ay.registers value: {err}"))
    })?;
    if raw.len() != 16 {
        return Err(ToolError::Execution(format!(
            "query_ay: expected 16 AY registers, got {}",
            raw.len()
        )));
    }
    let selected = session
        .query("spectrum.ay.selected_register")
        .map_err(|err| ay_unsupported_error(&err))?;
    let selected_register: u8 = serde_json::from_value(selected.value).map_err(|err| {
        ToolError::Execution(format!(
            "query_ay: malformed spectrum.ay.selected_register value: {err}"
        ))
    })?;

    let tone_period_a = u16::from(raw[0]) | (u16::from(raw[1] & 0x0F) << 8);
    let tone_period_b = u16::from(raw[2]) | (u16::from(raw[3] & 0x0F) << 8);
    let tone_period_c = u16::from(raw[4]) | (u16::from(raw[5] & 0x0F) << 8);
    let envelope_period = u16::from(raw[11]) | (u16::from(raw[12]) << 8);

    Ok(ScriptObservation::QueryAy {
        selected_register,
        raw: raw.clone(),
        tone_period_a,
        tone_period_b,
        tone_period_c,
        noise_period: raw[6] & 0x1F,
        mixer: raw[7],
        amplitude_a: raw[8] & 0x1F,
        amplitude_b: raw[9] & 0x1F,
        amplitude_c: raw[10] & 0x1F,
        envelope_period,
        envelope_shape: raw[13] & 0x0F,
    })
}

fn ay_unsupported_error(err: &emu198x_shell::QueryError) -> ToolError {
    ToolError::Execution(format!(
        "query_ay: active Spectrum variant does not have an AY-3-8912 chip \
         (only 128K, +2, +2A, +2B, +3, Pentagon, Scorpion, and Timex TC2068 / \
         TS2068 expose AY state). Switch to one of those variants via the \
         `set_machine` tool first. Underlying error: {err}"
    ))
}

fn execute_autoload_tape(
    session: &mut SpectrumSession,
    slot: &str,
    max_boot_frames: u32,
) -> Result<ScriptObservation, ToolError> {
    let result = autoload_basic_tape(session, slot, max_boot_frames)
        .map_err(|err| ToolError::Execution(format!("autoload_tape: {err}")))?;
    Ok(ScriptObservation::AutoloadTape {
        slot: result.slot,
        boot_frames: result.boot.frames,
    })
}

fn execute_load_basic_program(
    session: &mut SpectrumSession,
    path: &std::path::Path,
    run: bool,
) -> Result<ScriptObservation, ToolError> {
    let source = std::fs::read_to_string(path).map_err(|err| {
        ToolError::Execution(format!(
            "load_basic_program: failed to read {}: {err}",
            path.display()
        ))
    })?;
    let program = tokenise(&source).map_err(|err| {
        ToolError::Execution(format!(
            "load_basic_program: failed to tokenise {}: {err}",
            path.display()
        ))
    })?;
    let result =
        load_basic_program(session, &program, run, DEFAULT_BASIC_LOADER_BOOT_FRAMES).map_err(
            |err| {
                ToolError::Execution(format!(
                    "load_basic_program: BASIC loader failed for {}: {err}",
                    path.display()
                ))
            },
        )?;
    Ok(ScriptObservation::LoadBasicProgram {
        program_bytes: result.program_bytes,
        ran: result.ran,
    })
}

fn kind_to_model(kind: MachineKind) -> runtime_sinclair_zx_spectrum::Model {
    use runtime_sinclair_zx_spectrum::Model;
    match kind {
        MachineKind::Spectrum16K => Model::Spectrum16KPal,
        MachineKind::Spectrum48K => Model::Spectrum48KPal,
        MachineKind::SpectrumPlus => Model::SpectrumPlus,
        MachineKind::Spectrum128K => Model::Spectrum128KPal,
        MachineKind::SpectrumPlus2 => Model::SpectrumPlus2,
        MachineKind::SpectrumPlus2A => Model::SpectrumPlus2A,
        MachineKind::SpectrumPlus2B => Model::SpectrumPlus2B,
        MachineKind::SpectrumPlus3 => Model::SpectrumPlus3,
    }
}

/// Re-deserializes a `ScriptStep` by injecting the `action` tag into
/// the supplied arguments object. Mirrors the shell crate's serde
/// shape, so any field rename / addition shows up here as a parse
/// error rather than a silent shape mismatch.
fn parse_step(action: &str, arguments: Value) -> Result<ScriptStep, ToolError> {
    let mut object = match arguments {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => {
            return Err(ToolError::InvalidArguments(
                "arguments must be a JSON object".to_owned(),
            ));
        }
    };
    object.insert("action".to_owned(), Value::String(action.to_owned()));
    serde_json::from_value(Value::Object(object)).map_err(|err| {
        ToolError::InvalidArguments(format!("could not parse {action} arguments: {err}"))
    })
}

/// Registers every Spectrum tool on the supplied registry. Order is the
/// order shown by `tools/list`.
pub fn register_all(registry: &mut ToolRegistry<SpectrumSession>) {
    let object = || json!({"type": "object"});
    let string_field = || json!({"type": "string"});
    let integer_field = || json!({"type": "integer", "minimum": 0});
    let boolean_field = || json!({"type": "boolean"});

    let media_kind = json!({
        "type": "string",
        "enum": ["tape", "disk", "cartridge", "optical", "snapshot", "program"],
    });
    let media_transport = json!({
        "type": "string",
        "enum": ["start", "stop"],
    });

    registry.register(Box::new(ScriptStepTool {
        name: "load_media",
        description: "Load one media image into a named slot (tape, disk, cartridge, etc.).",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": string_field(),
                "kind": media_kind,
                "path": string_field(),
            },
            "required": ["slot", "kind", "path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "media_transport",
        description: "Start or stop media transport on the named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": string_field(),
                "transport": media_transport,
            },
            "required": ["slot", "transport"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "input",
        description: "Queue host input events (key presses / releases) for the next run step.",
        schema: json!({
            "type": "object",
            "properties": {
                "events": {
                    "type": "array",
                    "items": object(),
                },
            },
            "required": ["events"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "run_frames",
        description: "Run the machine for one number of native video frames.",
        schema: json!({
            "type": "object",
            "properties": {"frames": integer_field()},
            "required": ["frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_boot",
        description: "Run frames until the machine reports `boot.detected = true`.",
        schema: json!({
            "type": "object",
            "properties": {"max_frames": integer_field()},
            "required": ["max_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_contains",
        description: "Run frames until one text-bearing query contains the requested substring.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "needle": string_field(),
                "max_frames": integer_field(),
            },
            "required": ["path", "needle", "max_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_bool",
        description: "Run frames until one boolean query path reaches the requested value.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "value": boolean_field(),
                "max_frames": integer_field(),
            },
            "required": ["path", "value", "max_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query",
        description: "Resolve one shared query path against the live session.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query_paths",
        description: "List supported query paths, optionally filtered by prefix.",
        schema: json!({
            "type": "object",
            "properties": {
                "prefix": {"type": ["string", "null"]},
            },
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "load_snapshot",
        description: "Restore a snapshot file into the live machine. \
            Accepts the runtime's own postcard save state, plus portable \
            .sna / .z80 snapshots (the format is picked from the file \
            extension). .zip archives wrapping a single .sna or .z80 \
            are auto-extracted.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "save_snapshot",
        description: "Save the current machine snapshot to disk.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "save_screenshot",
        description: "Save the latest emitted frame as a PNG file.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "save_audio_capture",
        description: "Save the captured audio stream as a WAV file.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "reset_after": boolean_field(),
            },
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "set_machine",
        description: "Switch the live machine to the named variant (currently errors with `not yet supported`).",
        schema: json!({
            "type": "object",
            "properties": {"machine": string_field()},
            "required": ["machine"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "autoload_tape",
        description: "Wait for boot, type LOAD \"\", and start tape transport on the named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": string_field(),
                "max_boot_frames": integer_field(),
            },
            "required": ["slot", "max_boot_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "load_basic_program",
        description: "Tokenise a plain-text .bas file and install it as the live BASIC program (optionally RUN it).",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "run": boolean_field(),
            },
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "start_video_recording",
        description: "Begin recording the live framebuffer + audio to one MP4 file.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "stop_video_recording",
        description: "Finalise the in-flight video recording and return the summary.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "reset",
        description: "Reset the running machine. `kind: hard` is a power-cycle equivalent; `kind: soft` is a machine-local soft reset (today both behave identically on Spectrum). Clears queued input, captured frame, captured audio. Rejected while a video recording is active.",
        schema: json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["hard", "soft"],
                },
            },
            "required": ["kind"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "start_audio_recording",
        description: "Begin recording emitted audio to a 16-bit PCM WAV file. Mirrors start_video_recording for audio-only capture: subsequent run_frames tee audio into the session's buffer; the WAV is written when stop_audio_recording is called. Prefer this over save_audio_capture when the recording window is bounded by script steps.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "stop_audio_recording",
        description: "Finalise the in-flight audio recording. Slices the audio buffer from the start_audio_recording offset to the current end, encodes 16-bit PCM WAV, and writes it to disk.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query_ay",
        description: "Query the AY-3-8912 sound chip's full register state in one call. Returns the 16 raw registers plus decoded tone periods (A/B/C), noise period, mixer, amplitudes, envelope period, and envelope shape. Errors when the active variant has no AY (16K / 48K / Spectrum+); call set_machine first to switch to a 128K-class variant.",
        schema: json!({"type": "object"}),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(step, ScriptStep::Reset { kind: ResetKind::Hard });
        let step = parse_step("reset", json!({"kind": "soft"})).expect("valid step");
        assert_eq!(step, ScriptStep::Reset { kind: ResetKind::Soft });
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_query_ay() {
        let step = parse_step("query_ay", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::QueryAy);
    }

    #[test]
    fn register_all_publishes_every_script_step_variant() {
        let mut registry: ToolRegistry<SpectrumSession> = ToolRegistry::new();
        register_all(&mut registry);
        let names: Vec<_> = registry.iter().map(|tool| tool.name().to_owned()).collect();
        let expected = [
            "load_media",
            "media_transport",
            "input",
            "run_frames",
            "wait_for_boot",
            "wait_for_query_contains",
            "wait_for_query_bool",
            "query",
            "query_paths",
            "load_snapshot",
            "save_snapshot",
            "save_screenshot",
            "save_audio_capture",
            "set_machine",
            "autoload_tape",
            "load_basic_program",
            "start_video_recording",
            "stop_video_recording",
            "reset",
            "start_audio_recording",
            "stop_audio_recording",
            "query_ay",
        ];
        for name in expected {
            assert!(names.contains(&name.to_owned()), "missing {name}");
        }
    }
}
