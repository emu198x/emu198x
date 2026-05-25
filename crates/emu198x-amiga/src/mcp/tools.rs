//! MCP tool registrations for the Amiga MCP server.
//!
//! The Stage Q tool set is intentionally minimal — just what's needed
//! to investigate the STRAP wedge interactively without a recompile:
//!
//!   run_frames / run_ticks   advance machine time
//!   run_until_pc             advance until PC hits a target (or limit)
//!   reset                    re-load ROM, fresh boot
//!   query_cpu                full CPU register snapshot
//!   query_chipset            BPLCON0 / DMACON / vpos / hpos / copper / IRQ state
//!   query_cia                CIA-A and CIA-B timer + control state
//!   memory_read              raw bytes from any address (chip RAM or ROM)
//!   disasm                   m68k disassembly at an address
//!
//! Each tool returns a JSON object (or array) inside the
//! `ToolResponse::success_text` body — the client parses the JSON.

use emu198x_shell::mcp::{Tool, ToolError, ToolRegistry, ToolResponse};
use motorola_68000::disasm::disassemble;
use serde_json::{Value, json};

use super::session::AmigaA1200Session;

/// Wrap a closure as a `Tool` impl. The closure receives parsed
/// arguments and a mutable session reference and returns the JSON
/// response body. Lets us define tools inline without a struct per
/// tool.
struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut AmigaA1200Session) -> Result<Value, ToolError>,
}

impl Tool<AmigaA1200Session> for InlineTool {
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
        session: &mut AmigaA1200Session,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

/// Helper: pull an unsigned 64-bit integer out of an `arguments`
/// object. Accepts both JSON-number and decimal-or-hex JSON strings
/// (`"0xF80000"` / `"16252928"` / `16252928`). Hex prefix `$` is
/// accepted too — it's the m68k convention.
fn arg_u64(args: &Value, key: &str) -> Result<u64, ToolError> {
    let v = args
        .get(key)
        .ok_or_else(|| ToolError::InvalidArguments(format!("missing argument `{key}`")))?;
    if let Some(n) = v.as_u64() {
        return Ok(n);
    }
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        let (body, radix) = if let Some(rest) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
            (rest, 16)
        } else if let Some(rest) = trimmed.strip_prefix('$') {
            (rest, 16)
        } else {
            (trimmed, 10)
        };
        return u64::from_str_radix(body, radix).map_err(|err| {
            ToolError::InvalidArguments(format!("could not parse `{key}` as integer: {err}"))
        });
    }
    Err(ToolError::InvalidArguments(format!(
        "argument `{key}` must be a number or string"
    )))
}

/// As `arg_u64` but returns `default` when the key is absent.
fn arg_u64_or(args: &Value, key: &str, default: u64) -> Result<u64, ToolError> {
    if args.get(key).is_none() {
        Ok(default)
    } else {
        arg_u64(args, key)
    }
}

fn arg_u32(args: &Value, key: &str) -> Result<u32, ToolError> {
    let v = arg_u64(args, key)?;
    u32::try_from(v).map_err(|_| {
        ToolError::InvalidArguments(format!("`{key}` value {v} doesn't fit in u32"))
    })
}

/// Read a longword from chip RAM (or wherever the machine routes
/// the address) using the machine's existing memory backdoor. Falls
/// back to assembling from bytes for non-chip-RAM addresses so we
/// can dump ROM too.
fn read_long(session: &AmigaA1200Session, addr: u32) -> u32 {
    session.machine.read_long(addr)
}

fn read_byte(session: &AmigaA1200Session, addr: u32) -> u8 {
    let aligned = addr & !1;
    let long = session.machine.read_long(aligned & !2);
    let shift = (3 - (addr & 3)) * 8;
    ((long >> shift) & 0xFF) as u8
}

// ─── Tool implementations ─────────────────────────────────────────────

fn tool_run_frames(args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    use machine_commodore_amiga_a1200::PAL_FRAME_TICKS;
    let n = arg_u64_or(&args, "frames", 1)?;
    for _ in 0..n {
        for _ in 0..PAL_FRAME_TICKS {
            s.machine.tick();
        }
    }
    Ok(json!({
        "frames_run": n,
        "pc": format!("${:08X}", s.machine.cpu().regs.pc),
    }))
}

fn tool_run_ticks(args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let n = arg_u64_or(&args, "ticks", 1)?;
    for _ in 0..n {
        s.machine.tick();
    }
    Ok(json!({
        "ticks_run": n,
        "pc": format!("${:08X}", s.machine.cpu().regs.pc),
    }))
}

fn tool_run_until_pc(args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let target = arg_u32(&args, "target")?;
    let max_ticks = arg_u64_or(&args, "max_ticks", 100_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit = false;
    while ticks_taken < max_ticks {
        s.machine.tick();
        ticks_taken += 1;
        if s.machine.cpu().regs.pc == target {
            hit = true;
            break;
        }
    }
    Ok(json!({
        "hit": hit,
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.machine.cpu().regs.pc),
        "target": format!("${:08X}", target),
    }))
}

fn tool_reset(_args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    s.reset()
        .map_err(|err| ToolError::Execution(format!("reset: {err}")))?;
    Ok(json!({
        "reset": true,
        "rom_path": s.rom_path.display().to_string(),
        "pc": format!("${:08X}", s.machine.cpu().regs.pc),
    }))
}

fn tool_query_cpu(_args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let cpu = s.machine.cpu();
    let regs = &cpu.regs;
    Ok(json!({
        "pc":  format!("${:08X}", regs.pc),
        "instr_start_pc": format!("${:08X}", cpu.instr_start_pc),
        "sr":  format!("${:04X}", regs.sr),
        "supervisor": regs.is_supervisor(),
        "interrupt_mask": regs.interrupt_mask(),
        "ssp": format!("${:08X}", regs.ssp),
        "usp": format!("${:08X}", regs.usp),
        "vbr": format!("${:08X}", regs.vbr),
        "d": (0..8).map(|i| format!("${:08X}", regs.d[i])).collect::<Vec<_>>(),
        "a": (0..8).map(|i| format!("${:08X}", regs.a(i))).collect::<Vec<_>>(),
        "ipl_pin": cpu.ipl,
        "interrupts_taken": cpu.interrupts_taken,
        "exc_vector": cpu.exc_vector,
        "in_followup": cpu.in_followup,
        "followup_tag": cpu.followup_tag,
        "instruction_starts": cpu.instruction_starts,
    }))
}

fn tool_query_chipset(_args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let m = &s.machine;
    Ok(json!({
        "bplcon0": format!("${:04X}", m.bplcon0()),
        "dmacon":  format!("${:04X}", m.dmacon()),
        "adkcon":  format!("${:04X}", m.adkcon()),
        "color00": format!("${:04X}", m.color(0)),
        "cop1lc":  format!("${:08X}", m.copper().cop1lc),
        "copper_pc": format!("${:08X}", m.copper().pc),
        "overlay": m.memory().overlay(),
    }))
}

fn tool_query_paula(_args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let intena = s.machine.intena();
    let intreq = s.machine.intreq();
    let master = (intena & 0x4000) != 0;
    Ok(json!({
        "intena": format!("${:04X}", intena),
        "intreq": format!("${:04X}", intreq),
        "master_enable": master,
        "intena_bits": decode_int_bits(intena),
        "intreq_bits": decode_int_bits(intreq),
    }))
}

/// Decode the Paula INTENA/INTREQ bit layout into a readable map.
/// Bit 14 = master enable; bits 13..0 are individual interrupt sources.
fn decode_int_bits(val: u16) -> Value {
    const NAMES: [&str; 15] = [
        "TBE", "DSKBLK", "SOFT", "PORTS", "COPER", "VERTB", "BLIT", "AUD0",
        "AUD1", "AUD2", "AUD3", "RBF", "DSKSYN", "EXTER", "INTEN",
    ];
    let mut out = serde_json::Map::new();
    for (bit, name) in NAMES.iter().enumerate() {
        if val & (1 << bit) != 0 {
            out.insert((*name).to_string(), Value::Bool(true));
        }
    }
    Value::Object(out)
}

fn tool_query_cia(_args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let cia_a = s.machine.cia_a();
    Ok(json!({
        "cia_a": {
            "cra": format!("${:02X}", cia_a.cra()),
            "crb": format!("${:02X}", cia_a.crb()),
            "timer_a": format!("${:04X}", cia_a.timer_a()),
            "timer_b": format!("${:04X}", cia_a.timer_b()),
            "timer_a_running": cia_a.timer_a_running(),
            "timer_b_running": cia_a.timer_b_running(),
        }
    }))
}

fn tool_memory_read(args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let len = arg_u64_or(&args, "len", 16)?;
    let len = u32::try_from(len)
        .map_err(|_| ToolError::InvalidArguments("len exceeds u32".into()))?;
    if len == 0 || len > 4096 {
        return Err(ToolError::InvalidArguments(
            "len must be 1..=4096".into(),
        ));
    }
    let bytes: Vec<String> = (0..len)
        .map(|i| format!("{:02X}", read_byte(s, addr.wrapping_add(i))))
        .collect();
    Ok(json!({
        "addr": format!("${:08X}", addr),
        "len": len,
        "bytes_hex": bytes.join(" "),
    }))
}

fn tool_memory_read_long(args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    Ok(json!({
        "addr": format!("${:08X}", addr),
        "value": format!("${:08X}", read_long(s, addr)),
    }))
}

fn tool_disasm(args: Value, s: &mut AmigaA1200Session) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let count = arg_u64_or(&args, "count", 8)? as u32;
    if count == 0 || count > 128 {
        return Err(ToolError::InvalidArguments(
            "count must be 1..=128".into(),
        ));
    }
    let mut pc = addr;
    let mut lines: Vec<Value> = Vec::new();
    for _ in 0..count {
        let machine = &s.machine;
        let read = |a: u32| -> u8 {
            let aligned = a & !3;
            let long = machine.read_long(aligned);
            let shift = (3 - (a & 3)) * 8;
            ((long >> shift) & 0xFF) as u8
        };
        let (mnemonic, instr_len) = disassemble(pc, read);
        let bytes_hex: String = (0..instr_len)
            .map(|i| {
                let a = pc.wrapping_add(u32::from(i));
                let aligned = a & !3;
                let long = s.machine.read_long(aligned);
                let shift = (3 - (a & 3)) * 8;
                format!("{:02X}", ((long >> shift) & 0xFF) as u8)
            })
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(json!({
            "addr": format!("${:08X}", pc),
            "bytes": bytes_hex,
            "disasm": mnemonic,
        }));
        pc = pc.wrapping_add(u32::from(instr_len));
    }
    Ok(json!(lines))
}

// ─── Registration ─────────────────────────────────────────────────────

/// Registers every Amiga MCP tool on the supplied registry. The order
/// is the order shown by `tools/list`.
pub fn register_all(registry: &mut ToolRegistry<AmigaA1200Session>) {
    fn add(
        registry: &mut ToolRegistry<AmigaA1200Session>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut AmigaA1200Session) -> Result<Value, ToolError>,
    ) {
        registry.register(Box::new(InlineTool {
            name,
            description,
            schema,
            run,
        }));
    }

    let empty = || json!({"type": "object", "additionalProperties": false});
    let frames_schema = json!({
        "type": "object",
        "properties": {
            "frames": {"type": "integer", "minimum": 1, "default": 1}
        }
    });
    let ticks_schema = json!({
        "type": "object",
        "properties": {
            "ticks": {"type": "integer", "minimum": 1, "default": 1}
        }
    });
    let until_pc_schema = json!({
        "type": "object",
        "required": ["target"],
        "properties": {
            "target": {"description": "PC target — decimal int or hex string ($XXX / 0xXXX)"},
            "max_ticks": {"type": "integer", "minimum": 1, "default": 100000000}
        }
    });
    let addr_only = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "Address — decimal int or hex string ($XXX / 0xXXX)"}
        }
    });
    let memory_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "Address — decimal int or hex string"},
            "len": {"type": "integer", "minimum": 1, "maximum": 4096, "default": 16}
        }
    });
    let disasm_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "Address — decimal int or hex string"},
            "count": {"type": "integer", "minimum": 1, "maximum": 128, "default": 8}
        }
    });

    add(registry, "run_frames",  "Advance the machine by N PAL frames.", frames_schema, tool_run_frames);
    add(registry, "run_ticks",   "Advance the machine by N master/4 ticks.", ticks_schema, tool_run_ticks);
    add(registry, "run_until_pc","Run until PC == target or max_ticks reached.", until_pc_schema, tool_run_until_pc);
    add(registry, "reset",       "Reload the ROM and re-create the A1200 (fresh boot).", empty(), tool_reset);
    add(registry, "query_cpu",   "Full CPU register snapshot (D0-D7, A0-A7, PC, SR, SSP, USP, VBR, IPL pin, exception state).", empty(), tool_query_cpu);
    add(registry, "query_chipset","BPLCON0 / DMACON / ADKCON / COLOR00 / COP1LC / copper PC / overlay state.", empty(), tool_query_chipset);
    add(registry, "query_paula", "Paula INTENA / INTREQ with bit names decoded.", empty(), tool_query_paula);
    add(registry, "query_cia",   "CIA-A timer + control register snapshot.", empty(), tool_query_cia);
    add(registry, "memory_read", "Read raw bytes from any address (chip RAM / ROM / chipset).", memory_schema, tool_memory_read);
    add(registry, "memory_read_long", "Read a 32-bit longword from an address.", addr_only, tool_memory_read_long);
    add(registry, "disasm",      "Disassemble `count` m68k instructions starting at `addr`.", disasm_schema, tool_disasm);
}
