//! Jupiter Ace-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_jupiter_ace::JupiterAce;
use runtime_jupiter_ace::{JupiterAceRuntime, JupiterAceSessionQueryProvider};
use serde_json::{Value, json};

type AceSession = HeadlessSession<JupiterAceRuntime, JupiterAceSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut AceSession) -> Result<Value, ToolError>,
}

impl Tool<AceSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut AceSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn ace_ref(s: &AceSession) -> Result<&JupiterAce, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no ROM loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut AceSession) -> Result<Value, ToolError> {
    let m = ace_ref(session)?;
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

fn tool_memory_read(args: Value, session: &mut AceSession) -> Result<Value, ToolError> {
    let m = ace_ref(session)?;
    let address = args
        .get("address")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::Execution("address required (u16)".into()))?;
    let length = args
        .get("length")
        .and_then(Value::as_u64)
        .unwrap_or(16)
        .min(256);
    let mut bytes = Vec::with_capacity(length as usize);
    for i in 0..length {
        let addr = ((address + i) & 0xFFFF) as u16;
        bytes.push(m.peek_memory(addr));
    }
    Ok(json!({
        "address": format!("${:04X}", address & 0xFFFF),
        "length": length,
        "bytes": bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
    }))
}

pub fn register_ace_tools(registry: &mut ToolRegistry<AceSession>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    registry.register(Box::new(InlineTool {
        name: "query_cpu",
        description: "Z80 register snapshot.",
        schema: empty(),
        run: tool_query_cpu,
    }));
    registry.register(Box::new(InlineTool {
        name: "memory_read",
        description: "Read up to 256 bytes from Jupiter Ace memory.",
        schema: json!({
            "type": "object",
            "properties": {
                "address": {"type": "integer", "minimum": 0, "maximum": 65535},
                "length":  {"type": "integer", "minimum": 1, "maximum": 256}
            },
            "required": ["address"],
            "additionalProperties": false
        }),
        run: tool_memory_read,
    }));
}
