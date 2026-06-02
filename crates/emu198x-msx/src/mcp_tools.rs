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

fn arg_u16(args: &Value, name: &str) -> Result<u16, ToolError> {
    let v = args.get(name).ok_or_else(|| {
        ToolError::InvalidArguments(format!("missing required argument `{name}`"))
    })?;
    if let Some(n) = v.as_u64() {
        return u16::try_from(n)
            .map_err(|_| ToolError::InvalidArguments(format!("`{name}` out of u16 range: {n}")));
    }
    if let Some(s) = v.as_str() {
        let s = s.trim();
        let (radix, body) = if let Some(rest) = s.strip_prefix('$') {
            (16, rest)
        } else if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            (16, rest)
        } else {
            (10, s)
        };
        return u16::from_str_radix(body, radix)
            .map_err(|err| ToolError::InvalidArguments(format!("`{name}` parse error: {err}")));
    }
    Err(ToolError::InvalidArguments(format!(
        "`{name}` must be an integer or a hex string"
    )))
}

fn arg_u32_or(args: &Value, name: &str, default: u32) -> Result<u32, ToolError> {
    match args.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArguments(format!("`{name}` must be an integer")))
            .and_then(|n| {
                u32::try_from(n)
                    .map_err(|_| ToolError::InvalidArguments(format!("`{name}` out of u32 range")))
            }),
    }
}

// ════════════════════════════════════════════════════════════════
//  Tool bodies
// ════════════════════════════════════════════════════════════════

fn tool_query_cpu(_args: Value, session: &mut MsxSession) -> Result<Value, ToolError> {
    let msx = msx_ref(session)?;
    let cpu = msx.cpu();
    let regs = &cpu.regs;
    Ok(json!({
        "a":  format!("${:02X}", regs.a()),
        "f":  format!("${:02X}", regs.f()),
        "b":  format!("${:02X}", regs.b()),
        "c":  format!("${:02X}", regs.c()),
        "d":  format!("${:02X}", regs.d()),
        "e":  format!("${:02X}", regs.e()),
        "h":  format!("${:02X}", regs.h()),
        "l":  format!("${:02X}", regs.l()),
        "af": format!("${:04X}", regs.af),
        "bc": format!("${:04X}", regs.bc),
        "de": format!("${:04X}", regs.de),
        "hl": format!("${:04X}", regs.hl),
        "ix": format!("${:04X}", regs.ix),
        "iy": format!("${:04X}", regs.iy),
        "sp": format!("${:04X}", regs.sp),
        "pc": format!("${:04X}", regs.pc),
        "i":  format!("${:02X}", regs.i),
        "r":  format!("${:02X}", regs.r),
        "iff1": regs.iff1,
        "iff2": regs.iff2,
        "im":   regs.im,
        "halt": cpu.halt,
        "tstates": msx.cpu_tstates(),
    }))
}

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

fn tool_memory_read(args: Value, session: &mut MsxSession) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let len = arg_u32_or(&args, "len", 16)?.min(4096);
    let msx = msx_ref(session)?;
    let mut hex = String::new();
    let mut ascii = String::new();
    for offset in 0..len {
        let byte = msx.peek(addr.wrapping_add(offset as u16));
        if offset > 0 {
            hex.push(' ');
        }
        hex.push_str(&format!("{:02X}", byte));
        ascii.push(if (0x20..=0x7E).contains(&byte) {
            char::from(byte)
        } else {
            '.'
        });
    }
    Ok(json!({
        "addr":  format!("${:04X}", addr),
        "len":   len,
        "hex":   hex,
        "ascii": ascii,
    }))
}

// ════════════════════════════════════════════════════════════════
//  Registration
// ════════════════════════════════════════════════════════════════

/// Register MSX-specific MCP tools on top of the shared shell surface.
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

    let memory_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "Z80 bus start address (integer or $XXXX / 0xXXXX hex string)."},
            "len":  {"type": "integer", "minimum": 1, "maximum": 4096, "default": 16,
                     "description": "Number of bytes to read."}
        }
    });

    add(
        registry,
        "query_cpu",
        "Full Z80 register snapshot (A/F/BC/DE/HL/IX/IY/SP/PC/I/R, IFF1/IFF2, IM, halted, total T-states).",
        empty(),
        tool_query_cpu,
    );
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
    add(
        registry,
        "memory_read",
        "Read `len` bytes from the Z80 bus starting at `addr` (resolves slots; no side effects).",
        memory_schema,
        tool_memory_read,
    );
}
