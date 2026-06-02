//! Acorn Atom-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_acorn_atom::AcornAtom;
use runtime_acorn_atom::{AtomRuntime, AtomSessionQueryProvider};
use serde_json::{Value, json};

type AtomSession = HeadlessSession<AtomRuntime, AtomSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut AtomSession) -> Result<Value, ToolError>,
}

impl Tool<AtomSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut AtomSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn atom_ref(s: &AtomSession) -> Result<&AcornAtom, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no ROM loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut AtomSession) -> Result<Value, ToolError> {
    let m = atom_ref(session)?;
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

fn tool_memory_read(args: Value, session: &mut AtomSession) -> Result<Value, ToolError> {
    let m = atom_ref(session)?;
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

pub fn register_atom_tools(registry: &mut ToolRegistry<AtomSession>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    registry.register(Box::new(InlineTool {
        name: "query_cpu",
        description: "6502 register snapshot (A/X/Y/SP/PC/P).",
        schema: empty(),
        run: tool_query_cpu,
    }));
    registry.register(Box::new(InlineTool {
        name: "memory_read",
        description: "Read up to 256 bytes from Acorn Atom memory.",
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
