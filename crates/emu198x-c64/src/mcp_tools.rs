//! C64-specific MCP tools: `load_basic_program`, `press_key`,
//! `type_string`.
//!
//! The shared `register_common_tools` covers the machine-agnostic surface
//! (run frames, query, media, capture, reset, …). These three carry the
//! BASIC-authoring flow the Code198x curriculum pipeline depends on, and
//! mirror the Spectrum binary's bespoke tools so the captured output shape
//! is identical across platforms. Each builds the shared
//! [`ScriptObservation`] variant and serialises it as the tool's JSON-text
//! result, exactly as the shell's generic `ScriptStepTool` does.

use emu198x_shell::HeadlessSession;
use emu198x_shell::ScriptObservation;
use emu198x_shell::mcp::{Tool, ToolError, ToolRegistry, ToolResponse};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_BASIC_LOADER_BOOT_FRAMES, DEFAULT_KEY_HOLD_FRAMES,
    DEFAULT_TYPE_SETTLE_FRAMES, key_name_is_valid, load_basic_source, press_key, type_string,
};
use serde_json::{Value, json};

/// The C64 MCP session type — the context all C64 tools run against.
pub type C64Session = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

fn observation_response(observation: &ScriptObservation) -> Result<ToolResponse, ToolError> {
    let body = serde_json::to_string(observation)
        .map_err(|err| ToolError::Execution(format!("failed to serialize observation: {err}")))?;
    Ok(ToolResponse::success_text(body))
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments(format!("`{key}` (string) is required")))
}

fn optional_u32(arguments: &Value, key: &str) -> Option<u32> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

/// `load_basic_program` — tokenise a plain-text `.bas` file, install it at
/// `$0801`, and optionally `RUN` it.
struct LoadBasicProgramTool;

impl Tool<C64Session> for LoadBasicProgramTool {
    fn name(&self) -> &str {
        "load_basic_program"
    }

    fn description(&self) -> &str {
        "Tokenise a plain-text .bas file and install it as the live C64 BASIC program (optionally RUN it)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "run": {"type": "boolean"},
            },
            "required": ["path"],
        })
    }

    fn call(&self, arguments: Value, session: &mut C64Session) -> Result<ToolResponse, ToolError> {
        let path = required_str(&arguments, "path")?;
        let run = arguments
            .get("run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let source = std::fs::read_to_string(path).map_err(|err| {
            ToolError::Execution(format!("load_basic_program: failed to read {path}: {err}"))
        })?;
        let result = load_basic_source(session, &source, run, DEFAULT_BASIC_LOADER_BOOT_FRAMES)
            .map_err(|err| {
                ToolError::Execution(format!(
                    "load_basic_program: BASIC loader failed for {path}: {err}"
                ))
            })?;

        observation_response(&ScriptObservation::LoadBasicProgram {
            program_bytes: result.program_bytes,
            ran: result.ran,
        })
    }
}

/// `press_key` — press one named C64 key, hold it, and release it.
struct PressKeyTool;

impl Tool<C64Session> for PressKeyTool {
    fn name(&self) -> &str {
        "press_key"
    }

    fn description(&self) -> &str {
        "Press a single named C64 key, hold for `hold_frames` (default 3), then release. Names: A-Z, 0-9, Space, Return, Delete, F1/F3/F5/F7, cursor Up/Down/Left/Right, LShift, RShift, Ctrl, Commodore, RunStop (case-insensitive)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "key": {"type": "string"},
                "hold_frames": {"type": "integer", "minimum": 0},
            },
            "required": ["key"],
        })
    }

    fn call(&self, arguments: Value, session: &mut C64Session) -> Result<ToolResponse, ToolError> {
        let key = required_str(&arguments, "key")?;
        if !key_name_is_valid(key) {
            return Err(ToolError::InvalidArguments(format!(
                "press_key: unknown key `{key}` — valid names: A-Z, 0-9, Space, Return, \
                 Delete, F1/F3/F5/F7, Up/Down/Left/Right, LShift, RShift, Ctrl, \
                 Commodore, RunStop (case-insensitive)"
            )));
        }
        let hold = optional_u32(&arguments, "hold_frames").unwrap_or(DEFAULT_KEY_HOLD_FRAMES);

        let reached = press_key(session, key, hold)
            .map_err(|err| ToolError::Execution(format!("press_key: {err}")))?;

        observation_response(&ScriptObservation::PressKey {
            key: key.to_owned(),
            hold_frames: hold.clamp(1, 600),
            reached,
        })
    }
}

/// `type_string` — type a string through the C64 keyboard, one character
/// at a time.
struct TypeStringTool;

impl Tool<C64Session> for TypeStringTool {
    fn name(&self) -> &str {
        "type_string"
    }

    fn description(&self) -> &str {
        "Type a string through the C64 keyboard with per-key hold/release timing. Letters use the unshifted keycap (the default charset is upper case). Newlines press RETURN. Characters with no single C64 keystroke are skipped."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "hold_frames": {"type": "integer", "minimum": 0},
                "settle_frames": {"type": "integer", "minimum": 0},
            },
            "required": ["text"],
        })
    }

    fn call(&self, arguments: Value, session: &mut C64Session) -> Result<ToolResponse, ToolError> {
        let text = required_str(&arguments, "text")?;
        let hold = optional_u32(&arguments, "hold_frames").unwrap_or(DEFAULT_KEY_HOLD_FRAMES);
        let settle =
            optional_u32(&arguments, "settle_frames").unwrap_or(DEFAULT_TYPE_SETTLE_FRAMES);

        let chars_typed = type_string(session, text, hold, settle)
            .map_err(|err| ToolError::Execution(format!("type_string: {err}")))?;

        observation_response(&ScriptObservation::TypeString {
            chars_typed,
            reached: session.time(),
        })
    }
}

/// Registers the C64-specific BASIC-authoring tools on the registry, after
/// the shared `register_common_tools`.
pub fn register_c64_tools(registry: &mut ToolRegistry<C64Session>) {
    registry.register(Box::new(LoadBasicProgramTool));
    registry.register(Box::new(PressKeyTool));
    registry.register(Box::new(TypeStringTool));
}

#[cfg(test)]
mod tests {
    use super::register_c64_tools;
    use emu198x_shell::HeadlessSession;
    use emu198x_shell::mcp::{JsonRpcId, JsonRpcRequest, Server, ServerInfo};
    use emu198x_shell::mcp_tools::register_common_tools;
    use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};
    use serde_json::{Value, json};

    type C64Session = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

    fn stub_session() -> C64Session {
        let runtime = C64Runtime::blank(Model::C64PalBreadbin);
        HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider)
    }

    #[test]
    fn registers_the_basic_authoring_tools() {
        let mut server: Server<C64Session> = Server::new(ServerInfo::new("emu198x-c64", "test"));
        register_common_tools(server.registry_mut());
        register_c64_tools(server.registry_mut());

        for name in ["load_basic_program", "press_key", "type_string"] {
            assert!(
                server.registry().get(name).is_some(),
                "C64 tool `{name}` was not registered"
            );
        }
    }

    #[test]
    fn tools_list_exposes_the_capture_surface() {
        let mut server: Server<C64Session> = Server::new(ServerInfo::new("emu198x-c64", "test"));
        register_common_tools(server.registry_mut());
        register_c64_tools(server.registry_mut());
        let mut session = stub_session();

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonRpcId::Number(1)),
            method: "tools/list".to_string(),
            params: Some(json!({})),
        };
        let resp = server
            .handle(req, &mut session)
            .expect("tools/list request carries an id");
        let result = resp.result.expect("tools/list returns a result");
        let names: Vec<&str> = result
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();

        for name in [
            "load_basic_program",
            "press_key",
            "type_string",
            "run_frames",
            "query",
            "save_screenshot",
            "reset",
            "wait_for_boot",
        ] {
            assert!(names.contains(&name), "tools/list missing `{name}`");
        }
    }
}
