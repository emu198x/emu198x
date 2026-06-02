//! Oric Atmos-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_oric_atmos::OricAtmos;
use runtime_oric_atmos::{OricRuntime, OricSessionQueryProvider};
use serde_json::{Value, json};

type OricSession = HeadlessSession<OricRuntime, OricSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut OricSession) -> Result<Value, ToolError>,
}

impl Tool<OricSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut OricSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn oric_ref(s: &OricSession) -> Result<&OricAtmos, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no ROM loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut OricSession) -> Result<Value, ToolError> {
    let m = oric_ref(session)?;
    let r = &m.cpu().regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a),
        "x":  format!("${:02X}", r.x),
        "y":  format!("${:02X}", r.y),
        "sp": format!("${:02X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "p":  format!("${:02X}", r.p),
    }))
}

pub fn register_oric_tools(registry: &mut ToolRegistry<OricSession>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    registry.register(Box::new(InlineTool {
        name: "query_cpu",
        description: "6502 register snapshot (A/X/Y/SP/PC/P).",
        schema: empty(),
        run: tool_query_cpu,
    }));
}
