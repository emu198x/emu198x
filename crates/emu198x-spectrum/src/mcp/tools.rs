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
    HeadlessSession, ScriptStep,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use runtime_sinclair_zx_spectrum::{Spectrum48kRuntime, SpectrumSessionQueryProvider};
use serde_json::{Value, json};

use crate::script::runner::execute_step;

/// Live-session context every Spectrum MCP tool dispatches against.
pub type SpectrumSession = HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>;

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
        let observation = execute_step(&step, session)
            .map_err(|err| ToolError::Execution(format!("{err}")))?;
        let body = match observation {
            Some(obs) => serde_json::to_string(&obs).map_err(|err| {
                ToolError::Execution(format!("failed to serialize observation: {err}"))
            })?,
            None => String::from("null"),
        };
        Ok(ToolResponse::success_text(body))
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
    fn register_all_publishes_every_script_step_variant() {
        let mut registry: ToolRegistry<SpectrumSession> = ToolRegistry::new();
        register_all(&mut registry);
        let names: Vec<_> = registry
            .iter()
            .map(|tool| tool.name().to_owned())
            .collect();
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
        ];
        for name in expected {
            assert!(names.contains(&name.to_owned()), "missing {name}");
        }
    }
}
