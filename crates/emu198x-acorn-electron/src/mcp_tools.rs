//! Electron-specific MCP tools.
//!
//! CPU / memory / stepping come from the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set — `memory_read`
//! now works via the machine's side-effect-free `peek` (the ULA-latch
//! concern that previously blocked it is handled by reading RAM/ROM
//! directly). This adds the Electron ULA snapshot on top.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_acorn_electron::AcornElectron;
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider};
use serde_json::{Value, json};

type ElectronSession = HeadlessSession<ElectronRuntime, ElectronSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut ElectronSession) -> Result<Value, ToolError>,
}

impl Tool<ElectronSession> for InlineTool {
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
        session: &mut ElectronSession,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn el_ref(s: &ElectronSession) -> Result<&AcornElectron, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("OS / BASIC ROMs not loaded".into()))
}

fn tool_query_ula(_args: Value, session: &mut ElectronSession) -> Result<Value, ToolError> {
    let el = el_ref(session)?;
    Ok(json!({
        "display_mode": el.display_mode(),
        "irq":          el.irq_asserted(),
        "frame_count":  el.frame_count(),
        "framebuffer_width":  el.framebuffer_width(),
        "framebuffer_height": el.framebuffer_height(),
    }))
}

/// Register Electron MCP tools: the shared debug surface plus the ULA query.
pub fn register_electron_tools(registry: &mut ToolRegistry<ElectronSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);

    registry.register(Box::new(InlineTool {
        name: "query_ula",
        description: "Electron ULA snapshot — display mode, IRQ line, frame count, framebuffer dimensions.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_ula,
    }));
}
