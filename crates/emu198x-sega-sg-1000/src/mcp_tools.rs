//! SG-1000-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_sega_sg_1000::Sg1000;
use runtime_sega_sg_1000::{Sg1000Runtime, Sg1000SessionQueryProvider};
use serde_json::{Value, json};

type Sg1000Session = HeadlessSession<Sg1000Runtime, Sg1000SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut Sg1000Session) -> Result<Value, ToolError>,
}

impl Tool<Sg1000Session> for InlineTool {
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
        session: &mut Sg1000Session,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn sg_ref(s: &Sg1000Session) -> Result<&Sg1000, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn arg_u16(args: &Value, name: &str) -> Result<u16, ToolError> {
    let v = args
        .get(name)
        .ok_or_else(|| ToolError::InvalidArguments(format!("missing argument `{name}`")))?;
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
            .map_err(|err| ToolError::InvalidArguments(format!("`{name}` parse: {err}")));
    }
    Err(ToolError::InvalidArguments(format!(
        "`{name}` must be int or hex string"
    )))
}

fn arg_u32_or(args: &Value, name: &str, default: u32) -> Result<u32, ToolError> {
    match args.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArguments(format!("`{name}` must be int")))
            .and_then(|n| {
                u32::try_from(n)
                    .map_err(|_| ToolError::InvalidArguments(format!("`{name}` out of u32 range")))
            }),
    }
}

fn tool_query_cpu(_args: Value, session: &mut Sg1000Session) -> Result<Value, ToolError> {
    let sg = sg_ref(session)?;
    let cpu = sg.cpu();
    let r = &cpu.regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a()),
        "b":  format!("${:02X}", r.b()),
        "c":  format!("${:02X}", r.c()),
        "d":  format!("${:02X}", r.d()),
        "e":  format!("${:02X}", r.e()),
        "h":  format!("${:02X}", r.h()),
        "l":  format!("${:02X}", r.l()),
        "af": format!("${:04X}", r.af),
        "bc": format!("${:04X}", r.bc),
        "de": format!("${:04X}", r.de),
        "hl": format!("${:04X}", r.hl),
        "ix": format!("${:04X}", r.ix),
        "iy": format!("${:04X}", r.iy),
        "sp": format!("${:04X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "iff1": r.iff1,
        "iff2": r.iff2,
        "im":   r.im,
        "halt": cpu.halt,
        "tstates": sg.cpu_tstates(),
    }))
}

fn tool_query_vdp(_args: Value, session: &mut Sg1000Session) -> Result<Value, ToolError> {
    let sg = sg_ref(session)?;
    let vdp = sg.vdp();
    Ok(json!({
        "scanline":    vdp.scanline(),
        "frame_count": sg.frame_count(),
        "framebuffer_width":  vdp.framebuffer_width(),
        "framebuffer_height": vdp.framebuffer_height(),
    }))
}

fn tool_memory_read(args: Value, session: &mut Sg1000Session) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let len = arg_u32_or(&args, "len", 16)?.min(4096);
    let sg = sg_ref(session)?;
    let mut hex = String::new();
    let mut ascii = String::new();
    for offset in 0..len {
        let byte = sg.peek(addr.wrapping_add(offset as u16));
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

pub fn register_sg1000_tools(registry: &mut ToolRegistry<Sg1000Session>) {
    fn add(
        registry: &mut ToolRegistry<Sg1000Session>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut Sg1000Session) -> Result<Value, ToolError>,
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
            "addr": {"description": "Z80 bus start address (integer or $XXXX / 0xXXXX)."},
            "len":  {"type": "integer", "minimum": 1, "maximum": 4096, "default": 16}
        }
    });

    add(
        registry,
        "query_cpu",
        "Z80 register snapshot (A/F/BC/DE/HL/IX/IY/SP/PC, IFF1/IFF2, IM, halt, total T-states).",
        empty(),
        tool_query_cpu,
    );
    add(
        registry,
        "query_vdp",
        "TMS9918A VDP snapshot — scanline, frame count, framebuffer dimensions.",
        empty(),
        tool_query_vdp,
    );
    add(
        registry,
        "memory_read",
        "Read `len` bytes from the Z80 bus starting at `addr` (no side effects).",
        memory_schema,
        tool_memory_read,
    );
}
