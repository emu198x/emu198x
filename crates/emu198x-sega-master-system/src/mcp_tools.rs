//! SMS / Game Gear-specific MCP tools.
//!
//! CPU / memory / disasm / stepping / I/O trace come from the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; this adds the
//! SMS VDP and cartridge-mapper snapshots on top.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_sega_master_system::Sms;
use runtime_sega_master_system::{SmsRuntime, SmsSessionQueryProvider};
use serde_json::{Value, json};

type SmsSession = HeadlessSession<SmsRuntime, SmsSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut SmsSession) -> Result<Value, ToolError>,
}

impl Tool<SmsSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut SmsSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn sms_ref(s: &SmsSession) -> Result<&Sms, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn tool_query_vdp(_args: Value, session: &mut SmsSession) -> Result<Value, ToolError> {
    let sms = sms_ref(session)?;
    let vdp = sms.vdp();
    Ok(json!({
        "v_counter":   vdp.read_v_counter(),
        "frame_count": sms.frame_count(),
        "framebuffer_width":  vdp.framebuffer_width(),
        "framebuffer_height": vdp.framebuffer_height(),
    }))
}

fn tool_query_mapper(_args: Value, session: &mut SmsSession) -> Result<Value, ToolError> {
    let sms = sms_ref(session)?;
    let regs = sms.mapper_regs();
    Ok(json!({
        "control": format!("${:02X}", regs[0]),
        "page0":   format!("${:02X}", regs[1]),
        "page1":   format!("${:02X}", regs[2]),
        "page2":   format!("${:02X}", regs[3]),
    }))
}

/// Register SMS MCP tools: the shared debug surface plus VDP / mapper queries.
pub fn register_sms_tools(registry: &mut ToolRegistry<SmsSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);

    registry.register(Box::new(InlineTool {
        name: "query_vdp",
        description: "Sega VDP snapshot — V counter, frame count, framebuffer dimensions.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_vdp,
    }));
    registry.register(Box::new(InlineTool {
        name: "query_mapper",
        description: "Sega mapper registers — control + the three bank-page selects.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_mapper,
    }));
}
