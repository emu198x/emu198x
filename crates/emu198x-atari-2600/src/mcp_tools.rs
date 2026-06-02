//! Atari 2600-specific MCP tools — 6507 CPU + TIA + RIOT snapshots.
//!
//! `memory_read` is intentionally omitted on the 2600 because the chip
//! decode is `&mut self` (reading TIA registers can latch internal
//! state). Add it once machine-atari-2600 grows a side-effect-free
//! peek path.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_atari_2600::Atari2600;
use runtime_atari_2600::{Atari2600Runtime, Atari2600SessionQueryProvider};
use serde_json::{Value, json};

type VcsSession = HeadlessSession<Atari2600Runtime, Atari2600SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut VcsSession) -> Result<Value, ToolError>,
}

impl Tool<VcsSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut VcsSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn vcs_ref(s: &VcsSession) -> Result<&Atari2600, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut VcsSession) -> Result<Value, ToolError> {
    let vcs = vcs_ref(session)?;
    let r = &vcs.cpu().regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a),
        "x":  format!("${:02X}", r.x),
        "y":  format!("${:02X}", r.y),
        "sp": format!("${:02X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "p":  format!("${:02X}", r.p),
        "master_clock": vcs.master_clock(),
    }))
}

fn tool_query_tia(_args: Value, session: &mut VcsSession) -> Result<Value, ToolError> {
    let vcs = vcs_ref(session)?;
    let tia = vcs.tia();
    Ok(json!({
        "hpos":       tia.hpos(),
        "vpos":       tia.vpos(),
        "frame_count": vcs.frame_count(),
        "framebuffer_width":  tia.framebuffer_width(),
        "framebuffer_height": tia.framebuffer_height(),
    }))
}

pub fn register_vcs_tools(registry: &mut ToolRegistry<VcsSession>) {
    fn add(
        registry: &mut ToolRegistry<VcsSession>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut VcsSession) -> Result<Value, ToolError>,
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
        "6507 register snapshot (A/X/Y/SP/PC/P, master clock).",
        empty(),
        tool_query_cpu,
    );
    add(
        registry,
        "query_tia",
        "TIA snapshot — beam position (hpos/vpos), frame count, framebuffer dimensions.",
        empty(),
        tool_query_tia,
    );
}
