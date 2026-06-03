//! Sord M5-specific MCP tools.

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

fn tool_query_cpu(_args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let m5 = m5_ref(session)?;
    let r = &m5.cpu().regs;
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
        "halt": m5.cpu().halt,
        "tstates": m5.cpu_tstates(),
    }))
}

fn tool_memory_read(args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let len = arg_u32_or(&args, "len", 16)?.min(4096);
    let m5 = m5_ref(session)?;
    let mut hex = String::new();
    let mut ascii = String::new();
    for offset in 0..len {
        let byte = m5.peek(addr.wrapping_add(offset as u16));
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

fn m5_mut(s: &mut M5Session) -> Result<&mut SordM5, ToolError> {
    s.machine_mut()
        .machine_mut()
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

fn tool_disasm(args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let count = arg_u32_or(&args, "count", 16)?.min(256);
    let m5 = m5_ref(session)?;
    let mut lines = Vec::new();
    let mut a = addr;
    for _ in 0..count {
        let (text, len) = zilog_z80::disassemble(a, |x| m5.peek(x));
        lines.push(json!({ "addr": format!("${a:04X}"), "text": text }));
        a = a.wrapping_add(u16::from(len.max(1)));
    }
    Ok(json!({ "lines": lines }))
}

fn tool_run_until_pc(args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let target = arg_u16(&args, "pc")?;
    let max_frames = arg_u32_or(&args, "max_frames", 60)?.min(6000);
    let m5 = m5_mut(session)?;
    let max_tstates = u64::from(max_frames) * m5.tstates_per_frame();
    let (tstates, reached) = m5.run_until_pc(target, max_tstates);
    Ok(json!({
        "pc": format!("${target:04X}"),
        "reached": reached,
        "tstates": tstates,
        "cpu_pc": format!("${:04X}", m5.cpu().regs.pc),
    }))
}

fn tool_io_trace(args: Value, session: &mut M5Session) -> Result<Value, ToolError> {
    let frames = arg_u32_or(&args, "frames", 4)?.min(600);
    let limit = arg_u32_or(&args, "limit", 256)?.min(4096) as usize;
    let m5 = m5_mut(session)?;
    m5.start_io_trace();
    for _ in 0..frames {
        m5.run_frame();
    }
    let events = m5.take_io_trace();
    let total = events.len();

    // Per-port summary: how many reads / writes each port saw.
    let mut ports: std::collections::BTreeMap<u8, (u32, u32)> = std::collections::BTreeMap::new();
    for e in &events {
        let entry = ports.entry(e.port).or_default();
        if e.write {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let summary: Vec<Value> = ports
        .iter()
        .map(|(port, (w, r))| json!({ "port": format!("${port:02X}"), "writes": w, "reads": r }))
        .collect();

    let sample: Vec<Value> = events
        .iter()
        .take(limit)
        .map(|e| {
            json!({
                "pc": format!("${:04X}", e.pc),
                "port": format!("${:02X}", e.port),
                "value": format!("${:02X}", e.value),
                "dir": if e.write { "out" } else { "in" },
            })
        })
        .collect();

    Ok(json!({
        "frames": frames,
        "total_events": total,
        "by_port": summary,
        "events": sample,
        "truncated": total > limit,
    }))
}

pub fn register_m5_tools(registry: &mut ToolRegistry<M5Session>) {
    fn add(
        registry: &mut ToolRegistry<M5Session>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut M5Session) -> Result<Value, ToolError>,
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

    let disasm_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr":  {"description": "Start address (integer or $XXXX / 0xXXXX)."},
            "count": {"type": "integer", "minimum": 1, "maximum": 256, "default": 16}
        }
    });
    let run_until_schema = json!({
        "type": "object",
        "required": ["pc"],
        "properties": {
            "pc":         {"description": "Target PC (integer or $XXXX / 0xXXXX)."},
            "max_frames": {"type": "integer", "minimum": 1, "maximum": 6000, "default": 60}
        }
    });
    let io_trace_schema = json!({
        "type": "object",
        "properties": {
            "frames": {"type": "integer", "minimum": 1, "maximum": 600, "default": 4},
            "limit":  {"type": "integer", "minimum": 1, "maximum": 4096, "default": 256}
        }
    });

    add(
        registry,
        "query_ctc",
        "Z80 CTC state: vector base, INT line, and per-channel mode / counter.",
        empty(),
        tool_query_ctc,
    );
    add(
        registry,
        "query_vdp",
        "TMS9918A register snapshot and current scanline.",
        empty(),
        tool_query_vdp,
    );
    add(
        registry,
        "disasm",
        "Disassemble `count` Z80 instructions from `addr`.",
        disasm_schema,
        tool_disasm,
    );
    add(
        registry,
        "run_until_pc",
        "Run whole instructions until the CPU reaches `pc` or `max_frames` elapse.",
        run_until_schema,
        tool_run_until_pc,
    );
    add(
        registry,
        "io_trace",
        "Run `frames` frames capturing every I/O port access; returns a per-port \
         summary plus a sample of events (PC, port, value, direction).",
        io_trace_schema,
        tool_io_trace,
    );
}
