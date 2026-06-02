//! Atari 7800-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_atari_7800::Atari7800;
use runtime_atari_7800::{Atari7800Runtime, Atari7800SessionQueryProvider};
use serde_json::{Value, json};

type A7800Session = HeadlessSession<Atari7800Runtime, Atari7800SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut A7800Session) -> Result<Value, ToolError>,
}

impl Tool<A7800Session> for InlineTool {
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
        session: &mut A7800Session,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn a7800_ref(s: &A7800Session) -> Result<&Atari7800, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut A7800Session) -> Result<Value, ToolError> {
    let a7 = a7800_ref(session)?;
    let r = &a7.cpu().regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a),
        "x":  format!("${:02X}", r.x),
        "y":  format!("${:02X}", r.y),
        "sp": format!("${:02X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "p":  format!("${:02X}", r.p),
    }))
}

pub fn register_a7800_tools(registry: &mut ToolRegistry<A7800Session>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    registry.register(Box::new(InlineTool {
        name: "query_cpu",
        description: "6502C Sally register snapshot (A/X/Y/SP/PC/P).",
        schema: empty(),
        run: tool_query_cpu,
    }));
}
