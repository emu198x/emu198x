//! Sord M5-specific MCP tools.
//!
//! CPU / memory / disasm / stepping / I/O trace come from the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; this adds the
//! M5 chip-specific CTC and VDP snapshots on top.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_sord_m5::SordM5;
use runtime_sord_m5::{M5Runtime, M5SessionQueryProvider};
use serde_json::{Value, json};

type M5Session = HeadlessSession<M5Runtime, M5SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut M5Session) -> Result<Value, ToolError>,
}

impl Tool<M5Session> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut M5Session) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn m5_ref(s: &M5Session) -> Result<&SordM5, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("ROM not loaded".into()))
}

fn tool_query_ctc(_args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let m5 = m5_ref(session)?;
    let ctc = m5.ctc();
    let channels: Vec<Value> = (0u8..4)
        .map(|ch| {
            json!({
                "channel": ch,
                "running": ctc.running(ch),
                "counter_mode": ctc.counter_mode(ch),
                "int_enabled": ctc.int_enabled(ch),
                "counter": ctc.counter(ch),
            })
        })
        .collect();
    Ok(json!({
        "vector_base": format!("${:02X}", ctc.vector_base()),
        "interrupt": ctc.interrupt(),
        "channels": channels,
    }))
}

fn tool_query_vdp(_args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let m5 = m5_ref(session)?;
    let vdp = m5.vdp();
    let regs: Vec<String> = vdp
        .registers()
        .iter()
        .map(|r| format!("${r:02X}"))
        .collect();
    Ok(json!({
        "registers": regs,
        "scanline": vdp.scanline(),
    }))
}

/// Register Sord M5 MCP tools: the shared debug surface plus the CTC and
/// VDP chip-specific snapshots.
pub fn register_m5_tools(registry: &mut ToolRegistry<M5Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);

    registry.register(Box::new(InlineTool {
        name: "query_ctc",
        description: "Z80 CTC state: vector base, INT line, and per-channel mode / counter.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_ctc,
    }));
    registry.register(Box::new(InlineTool {
        name: "query_vdp",
        description: "TMS9918A register snapshot and current scanline.",
        schema: json!({"type": "object", "additionalProperties": false}),
        run: tool_query_vdp,
    }));
}
