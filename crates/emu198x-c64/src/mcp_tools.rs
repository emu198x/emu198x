//! C64-specific MCP tools: `load_basic_program`, `save_disk`, `set_port_drive`.
//!
//! The shared `register_common_tools` covers the machine-agnostic surface
//! (run frames, query, media, capture, reset, …). The BASIC-authoring pair
//! (`load_basic_program` + `save_disk`) carries the flow the Code198x
//! curriculum pipeline depends on, mirroring the Spectrum binary's bespoke
//! tools so the captured output shape is identical across platforms;
//! `set_port_drive` exposes the runtime's per-port IEC drive selector.
//! Each serialises its result as JSON text, like the shell's generic
//! `ScriptStepTool`.

use emu198x_shell::HeadlessSession;
use emu198x_shell::ScriptObservation;
use emu198x_shell::mcp::{Tool, ToolError, ToolRegistry, ToolResponse};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_BASIC_LOADER_BOOT_FRAMES, DriveKind,
    load_basic_source,
};
use serde_json::{Value, json};

/// The C64 MCP session type — the context all C64 tools run against.
pub type C64Session = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

/// The IEC device numbers the C64 serial bus carries.
const IEC_DEVICES: [u8; 4] = [8, 9, 10, 11];

/// The catalogue label for a drive model, used in tool JSON.
const fn drive_kind_label(kind: DriveKind) -> &'static str {
    match kind {
        DriveKind::C1541 => "1541",
        DriveKind::C1571 => "1571",
        DriveKind::C1581 => "1581",
    }
}

/// Parses a drive-model label. `"none"`/`""` map to `None` (empty the port).
fn parse_drive_kind(label: &str) -> Result<Option<DriveKind>, ToolError> {
    match label {
        "1541" => Ok(Some(DriveKind::C1541)),
        "1571" => Ok(Some(DriveKind::C1571)),
        "1581" => Ok(Some(DriveKind::C1581)),
        "none" | "" => Ok(None),
        other => Err(ToolError::InvalidArguments(format!(
            "`kind` must be one of 1541, 1571, 1581, none (got `{other}`)"
        ))),
    }
}

/// The current model on each IEC device, as a JSON object keyed by device
/// number (value `null` for an empty port).
fn port_map(session: &C64Session) -> Value {
    let mut map = serde_json::Map::new();
    for device in IEC_DEVICES {
        let label = session
            .machine()
            .port_drive_kind(device)
            .map(drive_kind_label);
        map.insert(device.to_string(), json!(label));
    }
    Value::Object(map)
}

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

/// `save_disk` — persist drive 8's writable disk to a host `.d64`.
///
/// A SAVE in BASIC lands GCR on the drive's live surface; this decodes the
/// whole surface back to a D64 image and writes it to `path`. The mounted disk
/// must have been loaded with `writable: true`; archive media is never written.
/// See `knowledge/decisions/disk-save-write-back.md`.
struct SaveDiskTool;

impl Tool<C64Session> for SaveDiskTool {
    fn name(&self) -> &str {
        "save_disk"
    }

    fn description(&self) -> &str {
        "Persist drive 8's disk: decode the live 1541 surface back into a .d64 and write it to `path`. The disk must have been mounted with load_media writable=true (archive disks stay read-only)."
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

    fn call(&self, arguments: Value, session: &mut C64Session) -> Result<ToolResponse, ToolError> {
        let path = required_str(&arguments, "path")?;
        let bytes = session.machine().flush_drive8_image().ok_or_else(|| {
            ToolError::Execution("save_disk: no disk mounted in drive 8".to_owned())
        })?;
        let len = bytes.len();
        std::fs::write(path, &bytes).map_err(|err| {
            ToolError::Execution(format!("save_disk: failed to write {path}: {err}"))
        })?;

        let body = json!({ "kind": "save_disk", "path": path, "bytes": len }).to_string();
        Ok(ToolResponse::success_text(body))
    }
}

/// `set_port_drive` — choose the disk-drive model on an IEC device (8–11), or
/// empty the port. The C64 serial bus carries devices 8–11 and each port can
/// hold a 1541, 1571, or 1581; this is the live per-port selector. The chosen
/// model's DOS ROM must have been supplied at startup.
struct SetPortDriveTool;

impl Tool<C64Session> for SetPortDriveTool {
    fn name(&self) -> &str {
        "set_port_drive"
    }

    fn description(&self) -> &str {
        "Choose the disk-drive model on an IEC device (8-11): `kind` is 1541, 1571, 1581, or none (empty the port). The model's DOS ROM must be present. Returns the resulting port map."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "device": {"type": "integer", "minimum": 8, "maximum": 11},
                "kind": {"type": "string", "enum": ["1541", "1571", "1581", "none"]},
            },
            "required": ["device", "kind"],
        })
    }

    fn call(&self, arguments: Value, session: &mut C64Session) -> Result<ToolResponse, ToolError> {
        let device = arguments
            .get("device")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                ToolError::InvalidArguments("`device` (integer 8-11) is required".into())
            })?;
        let device = u8::try_from(device).map_err(|_| {
            ToolError::InvalidArguments(format!("`device` {device} out of range (8-11)"))
        })?;
        let kind = parse_drive_kind(required_str(&arguments, "kind")?)?;

        session
            .machine_mut()
            .set_port_drive(device, kind)
            .map_err(|err| ToolError::Execution(format!("set_port_drive: {err}")))?;

        let body = json!({
            "kind": "set_port_drive",
            "device": device,
            "drive": kind.map(drive_kind_label),
            "ports": port_map(session),
        })
        .to_string();
        Ok(ToolResponse::success_text(body))
    }
}

/// Registers the C64-specific tools on the registry, after the shared
/// `register_common_tools`. `press_key` / `type_string` now come from the
/// shared keyboard tier (`register_keyboard_tools`, registered in `mcp.rs`)
/// over the C64's `KeyboardTarget` impl. RULES.md #30.
pub fn register_c64_tools(registry: &mut ToolRegistry<C64Session>) {
    registry.register(Box::new(LoadBasicProgramTool));
    registry.register(Box::new(SaveDiskTool));
    registry.register(Box::new(SetPortDriveTool));
}

#[cfg(test)]
mod tests {
    use super::{SetPortDriveTool, register_c64_tools};
    use emu198x_shell::HeadlessSession;
    use emu198x_shell::mcp::{JsonRpcId, JsonRpcRequest, Server, ServerInfo, Tool, ToolContent};
    use emu198x_shell::mcp_tools::{register_common_tools, register_keyboard_tools};
    use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};
    use serde_json::{Value, json};

    type C64Session = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

    fn stub_session() -> C64Session {
        let runtime = C64Runtime::blank(Model::C64PalBreadbin);
        HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider)
    }

    /// A session whose runtime has a (stub) 1541 DOS ROM on device 8 — enough
    /// to exercise the port selector's success path without real firmware.
    fn stub_session_with_1541() -> C64Session {
        let runtime = C64Runtime::new(
            Model::C64PalBreadbin,
            vec![0; 0x2000],
            vec![0; 0x2000],
            vec![0; 0x1000],
            Some(vec![0xEA; 0x4000]),
        )
        .expect("stub ROMs (incl. a 1541 DOS ROM) construct a runtime");
        HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider)
    }

    fn response_json(resp: &emu198x_shell::mcp::ToolResponse) -> Value {
        match &resp.content[0] {
            ToolContent::Text { text } => {
                serde_json::from_str(text).expect("tool response is JSON text")
            }
        }
    }

    #[test]
    fn registers_the_basic_authoring_tools() {
        let mut server: Server<C64Session> = Server::new(ServerInfo::new("emu198x-c64", "test"));
        register_common_tools(server.registry_mut());
        register_keyboard_tools(server.registry_mut());
        register_c64_tools(server.registry_mut());

        for name in [
            "load_basic_program",
            "press_key",
            "type_string",
            "save_disk",
            "set_port_drive",
        ] {
            assert!(
                server.registry().get(name).is_some(),
                "C64 tool `{name}` was not registered"
            );
        }
    }

    #[test]
    fn set_port_drive_tool_reports_the_default_layout() {
        let tool = SetPortDriveTool;
        let mut session = stub_session_with_1541();

        // Clearing device 8 empties the only occupied port; the returned map
        // shows every device null.
        let resp = tool
            .call(json!({"device": 8, "kind": "none"}), &mut session)
            .expect("clear device 8");
        let body = response_json(&resp);
        assert_eq!(body["device"], 8);
        assert!(body["drive"].is_null());
        assert!(body["ports"]["8"].is_null());
        assert!(body["ports"]["11"].is_null());

        // Putting the 1541 back on device 8 shows in the map.
        let resp = tool
            .call(json!({"device": 8, "kind": "1541"}), &mut session)
            .expect("restore the 1541 on device 8");
        assert_eq!(response_json(&resp)["ports"]["8"], "1541");
    }

    #[test]
    fn set_port_drive_tool_rejects_bad_input_and_missing_roms() {
        let tool = SetPortDriveTool;
        let mut session = stub_session_with_1541();

        // Out-of-range device.
        assert!(
            tool.call(json!({"device": 12, "kind": "none"}), &mut session)
                .is_err()
        );
        // Unknown model label.
        assert!(
            tool.call(json!({"device": 8, "kind": "9999"}), &mut session)
                .is_err()
        );
        // A model whose DOS ROM was never supplied (only the 1541 ROM exists).
        assert!(
            tool.call(json!({"device": 9, "kind": "1571"}), &mut session)
                .is_err()
        );
    }

    #[test]
    fn tools_list_exposes_the_capture_surface() {
        let mut server: Server<C64Session> = Server::new(ServerInfo::new("emu198x-c64", "test"));
        register_common_tools(server.registry_mut());
        register_keyboard_tools(server.registry_mut());
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
            "set_port_drive",
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
