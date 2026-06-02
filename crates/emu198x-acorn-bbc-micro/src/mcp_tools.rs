//! BBC Micro-specific MCP tools.
//!
//! `memory_read` omitted because the BBC bus decode is `&mut self` (VIA
//! and ULA reads can latch state). Add once machine grows a peek path.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_acorn_bbc_micro::BbcMicro;
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider};
use serde_json::{Value, json};

type BbcSession = HeadlessSession<BbcMicroRuntime, BbcMicroSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut BbcSession) -> Result<Value, ToolError>,
}

impl Tool<BbcSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut BbcSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn bbc_ref(s: &BbcSession) -> Result<&BbcMicro, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("MOS ROM not loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut BbcSession) -> Result<Value, ToolError> {
    let bbc = bbc_ref(session)?;
    let r = &bbc.cpu().regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a),
        "x":  format!("${:02X}", r.x),
        "y":  format!("${:02X}", r.y),
        "sp": format!("${:02X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "p":  format!("${:02X}", r.p),
        "cycles": bbc.cpu_cycles(),
    }))
}

pub fn register_bbc_tools(registry: &mut ToolRegistry<BbcSession>) {
    fn add(
        registry: &mut ToolRegistry<BbcSession>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut BbcSession) -> Result<Value, ToolError>,
    ) {
        registry.register(Box::new(InlineTool {
            name,
            description,
            schema,
            run,
        }));
    }

    let empty = || json!({"type": "object", "additionalProperties": false});

    add(
        registry,
        "query_cpu",
        "6502 register snapshot (A/X/Y/SP/PC/P, cycles).",
        empty(),
        tool_query_cpu,
    );
}
