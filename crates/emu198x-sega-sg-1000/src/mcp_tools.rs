//! SG-1000-specific MCP tools.
//!
//! CPU / memory / disasm / stepping / I/O trace come from the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; this adds the
//! SG-1000 chip-specific snapshots on top.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_sega_sg_1000::Sg1000;
use runtime_sega_sg_1000::{Sg1000Runtime, Sg1000SessionQueryProvider};
use serde_json::{Value, json};

type Sg1000Session = HeadlessSession<Sg1000Runtime, Sg1000SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut Sg1000Session) -> Result<Value, ToolError>,
}

impl Tool<Sg1000Session> for InlineTool {
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
        session: &mut Sg1000Session,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn sg_ref(s: &Sg1000Session) -> Result<&Sg1000, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn tool_query_vdp(_args: Value, session: &mut Sg1000Session) -> Result<Value, ToolError> {
    let sg = sg_ref(session)?;
    let vdp = sg.vdp();
    Ok(json!({
        "scanline":    vdp.scanline(),
        "frame_count": sg.frame_count(),
        "framebuffer_width":  vdp.framebuffer_width(),
        "framebuffer_height": vdp.framebuffer_height(),
    }))
}

/// Register SG-1000 MCP tools: the shared debug surface plus the VDP query.
pub fn register_sg1000_tools(registry: &mut ToolRegistry<Sg1000Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);

    registry.register(Box::new(InlineTool {
        name: "query_vdp",
        description: "TMS9918A VDP snapshot — scanline, frame count, framebuffer dimensions.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_vdp,
    }));
}
