//! Atari 2600-specific MCP tools.
//!
//! CPU / memory / stepping come from the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set — `memory_read`
//! now works via the machine's side-effect-free `peek` (cartridge ROM +
//! RIOT RAM; the TIA-latch concern that previously blocked it is handled
//! by not reading the chip registers). This adds the TIA snapshot on top.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_atari_2600::Atari2600;
use runtime_atari_2600::{Atari2600Runtime, Atari2600SessionQueryProvider};
use serde_json::{Value, json};

type VcsSession = HeadlessSession<Atari2600Runtime, Atari2600SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut VcsSession) -> Result<Value, ToolError>,
}

impl Tool<VcsSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut VcsSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn vcs_ref(s: &VcsSession) -> Result<&Atari2600, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn tool_query_tia(_args: Value, session: &mut VcsSession) -> Result<Value, ToolError> {
    let vcs = vcs_ref(session)?;
    let tia = vcs.tia();
    Ok(json!({
        "hpos":       tia.hpos(),
        "vpos":       tia.vpos(),
        "frame_count": vcs.frame_count(),
        "framebuffer_width":  tia.framebuffer_width(),
        "framebuffer_height": tia.framebuffer_height(),
    }))
}

/// Register Atari 2600 MCP tools: the shared debug surface plus the TIA query.
pub fn register_vcs_tools(registry: &mut ToolRegistry<VcsSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);

    registry.register(Box::new(InlineTool {
        name: "query_tia",
        description: "TIA snapshot — beam position (hpos/vpos), frame count, framebuffer dimensions.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_tia,
    }));
}
