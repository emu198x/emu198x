//! ColecoVision-specific MCP tools.
//!
//! CPU / memory / disasm / stepping / I/O trace come from the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; this adds the
//! ColecoVision VDP snapshot on top.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_coleco_colecovision::ColecoVision;
use runtime_coleco_colecovision::{CvRuntime, CvSessionQueryProvider};
use serde_json::{Value, json};

type CvSession = HeadlessSession<CvRuntime, CvSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut CvSession) -> Result<Value, ToolError>,
}

impl Tool<CvSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut CvSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn cv_ref(s: &CvSession) -> Result<&ColecoVision, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn tool_query_vdp(_args: Value, session: &mut CvSession) -> Result<Value, ToolError> {
    let cv = cv_ref(session)?;
    let vdp = cv.vdp();
    Ok(json!({
        "scanline":    vdp.scanline(),
        "frame_count": cv.frame_count(),
        "framebuffer_width":  vdp.framebuffer_width(),
        "framebuffer_height": vdp.framebuffer_height(),
    }))
}

/// Register ColecoVision MCP tools: the shared debug surface plus the VDP query.
pub fn register_cv_tools(registry: &mut ToolRegistry<CvSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);

    registry.register(Box::new(InlineTool {
        name: "query_vdp",
        description: "TMS9918A VDP snapshot — scanline, frame count, framebuffer dimensions.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_vdp,
    }));
}
