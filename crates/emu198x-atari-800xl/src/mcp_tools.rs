//! Atari 800XL-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_atari_800xl::Atari800xl;
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider};
use serde_json::{Value, json};

type A800xlSession = HeadlessSession<Atari800xlRuntime, Atari800xlSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut A800xlSession) -> Result<Value, ToolError>,
}

impl Tool<A800xlSession> for InlineTool {
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
        session: &mut A800xlSession,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn a800xl_ref(s: &A800xlSession) -> Result<&Atari800xl, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no OS / cart loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let m = a800xl_ref(session)?;
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

pub fn register_a800xl_tools(registry: &mut ToolRegistry<A800xlSession>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    registry.register(Box::new(InlineTool {
        name: "query_cpu",
        description: "6502C Sally register snapshot (A/X/Y/SP/PC/P).",
        schema: empty(),
        run: tool_query_cpu,
    }));
}
