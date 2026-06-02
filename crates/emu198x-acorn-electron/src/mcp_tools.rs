//! Electron-specific MCP tools.
//!
//! `memory_read` is omitted because the Electron's bus decode is
//! `&mut self` (ULA reads can latch state). Add once
//! machine-acorn-electron grows a side-effect-free peek path.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_acorn_electron::AcornElectron;
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider};
use serde_json::{Value, json};

type ElectronSession = HeadlessSession<ElectronRuntime, ElectronSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut ElectronSession) -> Result<Value, ToolError>,
}

impl Tool<ElectronSession> for InlineTool {
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
        session: &mut ElectronSession,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn el_ref(s: &ElectronSession) -> Result<&AcornElectron, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("OS / BASIC ROMs not loaded".into()))
}

fn tool_query_cpu(_args: Value, session: &mut ElectronSession) -> Result<Value, ToolError> {
    let el = el_ref(session)?;
    let r = &el.cpu().regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a),
        "x":  format!("${:02X}", r.x),
        "y":  format!("${:02X}", r.y),
        "sp": format!("${:02X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "p":  format!("${:02X}", r.p),
        "cycles": el.cpu_cycles(),
    }))
}

fn tool_query_ula(_args: Value, session: &mut ElectronSession) -> Result<Value, ToolError> {
    let el = el_ref(session)?;
    Ok(json!({
        "display_mode": el.display_mode(),
        "irq":          el.irq_asserted(),
        "frame_count":  el.frame_count(),
        "framebuffer_width":  el.framebuffer_width(),
        "framebuffer_height": el.framebuffer_height(),
    }))
}

pub fn register_electron_tools(registry: &mut ToolRegistry<ElectronSession>) {
    fn add(
        registry: &mut ToolRegistry<ElectronSession>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut ElectronSession) -> Result<Value, ToolError>,
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
    add(
        registry,
        "query_ula",
        "Electron ULA snapshot — display mode, IRQ line, frame count, framebuffer dimensions.",
        empty(),
        tool_query_ula,
    );
}
