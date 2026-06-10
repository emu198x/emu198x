//! MSX1-specific MCP tools.
//!
//! Sits beside the shared `register_common_tools` surface — adds a
//! small set of chip-specific snapshots covering CPU / VDP / PSG /
//! PPI state and raw memory reads. Matches the depth pattern used by
//! `emu198x-nes::mcp_tools` without trying to be exhaustive — extra
//! tools land as concrete debugging needs surface.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_msx::Msx;
use runtime_msx::{MsxRuntime, MsxSessionQueryProvider};
use serde_json::{Value, json};

type MsxSession = HeadlessSession<MsxRuntime, MsxSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut MsxSession) -> Result<Value, ToolError>,
}

impl Tool<MsxSession> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut MsxSession) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

// ════════════════════════════════════════════════════════════════
//  Helpers
// ════════════════════════════════════════════════════════════════

fn msx_ref(s: &MsxSession) -> Result<&Msx, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("BIOS not loaded".into()))
}

// ════════════════════════════════════════════════════════════════
//  Tool bodies — chip-specific snapshots. CPU / memory / disasm /
//  stepping / I/O trace come from `register_base_tools`.
// ════════════════════════════════════════════════════════════════

fn tool_query_vdp(_args: Value, session: &mut MsxSession) -> Result<Value, ToolError> {
    let msx = msx_ref(session)?;
    let vdp = msx.vdp();
    Ok(json!({
        "scanline":      vdp.scanline(),
        "frame_count":   msx.frame_count(),
        "framebuffer_width":  vdp.framebuffer_width(),
        "framebuffer_height": vdp.framebuffer_height(),
    }))
}

fn tool_query_psg(_args: Value, session: &mut MsxSession) -> Result<Value, ToolError> {
    let msx = msx_ref(session)?;
    let psg = msx.psg();
    let regs = psg.registers();
    let mut hex_regs = Vec::with_capacity(16);
    for value in regs {
        hex_regs.push(format!("${:02X}", value));
    }
    Ok(json!({
        "selected_register": psg.selected_register(),
        "registers": hex_regs,
    }))
}

fn tool_query_ppi(_args: Value, session: &mut MsxSession) -> Result<Value, ToolError> {
    let msx = msx_ref(session)?;
    let ppi = msx.ppi();
    Ok(json!({
        "port_a":       format!("${:02X}", ppi.port_a),
        "keyboard_row": ppi.keyboard_row(),
    }))
}

// ════════════════════════════════════════════════════════════════
//  Registration
// ════════════════════════════════════════════════════════════════

/// Register MSX-specific MCP tools on top of the shared shell surface.
/// The shared debug tools (`query_cpu`, `memory_read`, `poke_byte`,
/// `poke_word`, `disasm`, `run_until_pc`, `step`, `io_trace`) come from
/// [`emu198x_shell::mcp_tools::register_base_tools`]; this adds the
/// MSX chip-specific snapshots on top.
pub fn register_msx_tools(registry: &mut ToolRegistry<MsxSession>) {
    fn add(
        registry: &mut ToolRegistry<MsxSession>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut MsxSession) -> Result<Value, ToolError>,
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
        "query_vdp",
        "TMS9918 VDP snapshot (scanline, frame count, framebuffer dimensions).",
        empty(),
        tool_query_vdp,
    );
    add(
        registry,
        "query_psg",
        "AY-3-8910/8912 PSG snapshot — currently selected register + full register file as hex.",
        empty(),
        tool_query_psg,
    );
    add(
        registry,
        "query_ppi",
        "Intel 8255 PPI snapshot — port A (primary slot select per page) and currently selected keyboard row.",
        empty(),
        tool_query_ppi,
    );
}
