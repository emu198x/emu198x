//! Spectravideo SVI-328-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_spectravideo_svi_328::Svi328;
use runtime_spectravideo_svi_328::{Svi328Runtime, Svi328SessionQueryProvider};
use serde_json::{Value, json};

type SviSession = HeadlessSession<Svi328Runtime, Svi328SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut SviSession) -> Result<Value, ToolError>,
}

impl Tool<SviSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut SviSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn svi_ref(s: &SviSession) -> Result<&Svi328, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no ROM loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut SviSession) -> Result<Value, ToolError> {
    let m = svi_ref(session)?;
    let r = &m.cpu().regs;
    Ok(json!({
        "af": format!("${:04X}", r.af),
        "bc": format!("${:04X}", r.bc),
        "de": format!("${:04X}", r.de),
        "hl": format!("${:04X}", r.hl),
        "ix": format!("${:04X}", r.ix),
        "iy": format!("${:04X}", r.iy),
        "sp": format!("${:04X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "i":  format!("${:02X}", r.i),
        "r":  format!("${:02X}", r.r),
    }))
}

pub fn register_svi_tools(registry: &mut ToolRegistry<SviSession>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    registry.register(Box::new(InlineTool {
        name: "query_cpu",
        description: "Z80 register snapshot.",
        schema: empty(),
        run: tool_query_cpu,
    }));
}
