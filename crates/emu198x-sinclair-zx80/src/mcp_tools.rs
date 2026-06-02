//! ZX80-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_sinclair_zx80::Zx80;
use runtime_sinclair_zx80::{Zx80Runtime, Zx80SessionQueryProvider};
use serde_json::{Value, json};

type Zx80Session = HeadlessSession<Zx80Runtime, Zx80SessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut Zx80Session) -> Result<Value, ToolError>,
}

impl Tool<Zx80Session> for InlineTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut Zx80Session) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn zx_ref(s: &Zx80Session) -> Result<&Zx80, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("ROM not loaded".into()))
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

fn tool_query_cpu(_args: Value, session: &mut Zx80Session) -> Result<Value, ToolError> {
    let zx = zx_ref(session)?;
    let r = &zx.cpu().regs;
    Ok(json!({
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
        "halt": zx.cpu().halt,
    }))
}

fn tool_memory_read(args: Value, session: &mut Zx80Session) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let len = arg_u32_or(&args, "len", 16)?.min(4096);
    let zx = zx_ref(session)?;
    let mut hex = String::new();
    let mut ascii = String::new();
    for offset in 0..len {
        let byte = zx.peek_memory(addr.wrapping_add(offset as u16));
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

pub fn register_zx80_tools(registry: &mut ToolRegistry<Zx80Session>) {
    fn add(
        registry: &mut ToolRegistry<Zx80Session>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut Zx80Session) -> Result<Value, ToolError>,
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
            "addr": {"description": "Z80 bus address (integer or $XXXX / 0xXXXX)."},
            "len":  {"type": "integer", "minimum": 1, "maximum": 4096, "default": 16}
        }
    });

    add(
        registry,
        "query_cpu",
        "Z80 register snapshot.",
        empty(),
        tool_query_cpu,
    );
    add(
        registry,
        "memory_read",
        "Read `len` bytes from the Z80 bus starting at `addr` (no side effects).",
        memory_schema,
        tool_memory_read,
    );
}
