//! Shared MCP tools that wrap the machine-agnostic [`ScriptStep`]
//! variants.
//!
//! Every per-system binary's MCP server needs the same core surface —
//! run frames, query state, read snapshots, capture screenshots. Those
//! steps already execute generically through
//! [`ScriptStep::execute_collect`], so the tool wrappers can live here
//! once instead of being copied into each binary. A system registers
//! these with [`register_common_tools`] and then adds any
//! system-specific tools (CPU disassembly, chip-specific queries) on
//! top.
//!
//! The Spectrum binary predates this helper and keeps its own bespoke
//! registrations; new systems (NES, Game Boy, Dragon, C64, Amiga) build
//! on the shared set.

use serde_json::{Value, json};

use crate::machine::MachineCore;
use crate::mcp::{Tool, ToolError, ToolRegistry, ToolResponse};
use crate::query::SessionQueryProvider;
use crate::script::ScriptStep;
use crate::session::HeadlessSession;

/// One MCP tool mapping directly onto a machine-agnostic [`ScriptStep`]
/// variant. The tool name is the variant's serde `action` tag; the
/// arguments are the variant's remaining fields.
struct ScriptStepTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
}

impl<M, Q> Tool<HeadlessSession<M, Q>> for ScriptStepTool
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
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
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<ToolResponse, ToolError> {
        let step = build_step(self.name, arguments)?;
        let observation = step
            .execute_collect(session)
            .map_err(|err| ToolError::Execution(err.to_string()))?;
        let body = match observation {
            Some(obs) => serde_json::to_string(&obs).map_err(|err| {
                ToolError::Execution(format!("failed to serialize observation: {err}"))
            })?,
            None => String::from("null"),
        };
        Ok(ToolResponse::success_text(body))
    }
}

/// Build a [`ScriptStep`] from a tool name (the serde `action` tag) and
/// its JSON arguments by merging the tag into the argument object and
/// deserializing.
fn build_step(action: &str, arguments: Value) -> Result<ScriptStep, ToolError> {
    let mut obj = match arguments {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => {
            return Err(ToolError::InvalidArguments(
                "arguments must be a JSON object".to_owned(),
            ));
        }
    };
    obj.insert("action".to_owned(), Value::String(action.to_owned()));
    serde_json::from_value(Value::Object(obj)).map_err(|err| {
        ToolError::InvalidArguments(format!("invalid arguments for `{action}`: {err}"))
    })
}

/// Register the machine-agnostic MCP tools onto a per-system registry.
///
/// Covers media loading + transport, input, running frames, the boot /
/// query waiters, query resolution, snapshot save/restore, and
/// screenshot / audio capture. These delegate to
/// [`ScriptStep::execute_collect`], which is generic over any
/// [`MachineCore`] + [`SessionQueryProvider`].
pub fn register_common_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    registry.register(Box::new(ScriptStepTool {
        name: "run_frames",
        description: "Run the machine for a number of native video frames.",
        schema: json!({
            "type": "object",
            "properties": { "frames": { "type": "integer", "minimum": 0 } },
            "required": ["frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_boot",
        description: "Run frames until the machine reports it has booted (or the frame budget is exhausted).",
        schema: json!({
            "type": "object",
            "properties": { "max_frames": { "type": "integer", "minimum": 0 } },
            "required": ["max_frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_contains",
        description: "Run frames until a text-bearing query path contains a substring.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "needle": { "type": "string" },
                "max_frames": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "needle", "max_frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_bool",
        description: "Run frames until a boolean query path reaches a target value.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "value": { "type": "boolean" },
                "max_frames": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "value", "max_frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "query",
        description: "Resolve one shared query path against the live session.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "query_paths",
        description: "List supported query paths, optionally filtered by prefix.",
        schema: json!({
            "type": "object",
            "properties": { "prefix": { "type": ["string", "null"] } }
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "input",
        description: "Queue generic input events (keys / buttons / axes) for the next run step.",
        schema: json!({
            "type": "object",
            "properties": { "events": { "type": "array" } },
            "required": ["events"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "load_media",
        description: "Load a media image (cartridge / disk / tape / program) into a named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": { "type": "string" },
                "kind": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["slot", "kind", "path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "media_transport",
        description: "Start or stop media transport on a named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": { "type": "string" },
                "transport": { "type": "string" }
            },
            "required": ["slot", "transport"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "load_snapshot",
        description: "Restore a snapshot file into the live machine.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "save_snapshot",
        description: "Save the current machine snapshot to disk.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "save_screenshot",
        description: "Save the latest emitted frame as a PNG file.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "save_audio_capture",
        description: "Save the captured audio stream as a WAV file.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "reset_after": { "type": "boolean" }
            },
            "required": ["path"]
        }),
    }));
}
