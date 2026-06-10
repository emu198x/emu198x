//! NES-specific MCP tools.
//!
//! Action-and-dump tools for the NES debugging tasks that come up in
//! practice: raw memory / palette / OAM / nametable dumps, instruction
//! stepping, and `run_until_pc` / `run_until_mem_change` breakpoint
//! primitives. The chip-register snapshots (`cpu` / `ppu` / `apu` /
//! `mapper`) now live on the generic `query` surface as folded query
//! paths — see `runtime_nintendo_nes::queries` (#456).
//!
//! Tool bodies return a `Value` that the shared
//! [`InlineTool`](emu198x_shell::mcp::InlineTool) wrapper serialises to a
//! JSON text content block (the client parses). Tools are listed at the
//! bottom of this file in the same order they appear in `tools/list`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{InlineTool, ToolError, ToolRegistry},
};
use machine_nintendo_nes::Nes;
use runtime_nintendo_nes::{NesRuntime, NesSessionQueryProvider};
use serde_json::{Value, json};

type NesSession = HeadlessSession<NesRuntime, NesSessionQueryProvider>;

// ════════════════════════════════════════════════════════════════
//  Helpers — argument parsing + machine access
// ════════════════════════════════════════════════════════════════

fn nes_mut(s: &mut NesSession) -> Result<&mut Nes, ToolError> {
    s.machine_mut()
        .machine_mut()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

fn nes_ref(s: &NesSession) -> Result<&Nes, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no cartridge loaded".into()))
}

/// Parse an address argument that accepts either a decimal integer
/// or a hex string with `$XXX` or `0xXXX` prefix.
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

fn arg_u64_or(args: &Value, name: &str, default: u64) -> Result<u64, ToolError> {
    match args.get(name) {
        None => Ok(default),
        Some(v) => v.as_u64().ok_or_else(|| {
            ToolError::InvalidArguments(format!("`{name}` must be a non-negative integer"))
        }),
    }
}

fn arg_u16_or(args: &Value, name: &str, default: u16) -> Result<u16, ToolError> {
    if args.get(name).is_none() {
        return Ok(default);
    }
    arg_u16(args, name)
}

// ════════════════════════════════════════════════════════════════
//  Tools
// ════════════════════════════════════════════════════════════════

fn tool_memory_read(args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let len = arg_u64_or(&args, "len", 16)?.min(4096);
    let nes = nes_ref(s)?;
    let bytes: Vec<u8> = (0..len as u16)
        .map(|i| nes.peek(addr.wrapping_add(i)))
        .collect();
    Ok(json!({
        "addr":  format!("${:04X}", addr),
        "len":   bytes.len(),
        "bytes": bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
        "ascii": bytes
            .iter()
            .map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' })
            .collect::<String>(),
    }))
}

fn tool_dump_palette(_args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    let nes = nes_ref(s)?;
    let pal = nes.ppu.palette_ram();
    let bg: Vec<String> = (0..16).map(|i| format!("{:02X}", pal[i])).collect();
    let sp: Vec<String> = (16..32).map(|i| format!("{:02X}", pal[i])).collect();
    Ok(json!({
        "background_palette": bg.join(" "),
        "sprite_palette":     sp.join(" "),
    }))
}

fn tool_dump_oam(args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    // OAM is 64 sprites × 4 bytes. Default: dump all 64; allow
    // `start` / `count` to focus on a slice.
    let start = arg_u16_or(&args, "start", 0)?.min(63);
    let count = arg_u64_or(&args, "count", 64)?
        .min(64 - u64::from(start))
        .max(1);
    let nes = nes_ref(s)?;
    let oam = nes.ppu.oam();
    let sprites: Vec<Value> = (start..start + count as u16)
        .map(|i| {
            let base = (i as usize) * 4;
            let y = oam[base];
            let tile = oam[base + 1];
            let attr = oam[base + 2];
            let x = oam[base + 3];
            json!({
                "index":   i,
                "y":       y,
                "tile":    format!("${:02X}", tile),
                "attr":    format!("${:02X}", attr),
                "x":       x,
                "palette": attr & 0x03,
                "priority_behind_bg": (attr & 0x20) != 0,
                "flip_h":  (attr & 0x40) != 0,
                "flip_v":  (attr & 0x80) != 0,
            })
        })
        .collect();
    Ok(json!({
        "start":   start,
        "count":   sprites.len(),
        "sprites": sprites,
    }))
}

fn tool_dump_nametable(args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    // Default: dump both 1K nametables as a hexdump-style block.
    // `which`: 0/1 to pick one nametable (1 KiB); omit for both.
    let which = args.get("which").and_then(Value::as_u64);
    let nes = nes_ref(s)?;
    let nt = nes.ppu.nametable_ram();
    let (start, end) = match which {
        Some(0) => (0, 1024),
        Some(1) => (1024, 2048),
        _ => (0, 2048),
    };
    let slice = &nt[start..end];
    let hex = slice
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    let ascii: String = slice
        .iter()
        .map(|&b| {
            if (0x20..=0x7E).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    Ok(json!({
        "start": start,
        "len":   slice.len(),
        "hex":   hex,
        "ascii": ascii,
    }))
}

fn tool_step(args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    let count = arg_u64_or(&args, "count", 1)?.max(1);
    let max_master_ticks = arg_u64_or(&args, "max_master_ticks", 100_000_000)?;
    let mut pc_trace: Vec<String> = Vec::new();
    let mut master_ticks_used: u64 = 0;
    for _ in 0..count {
        // Step one instruction: capture the PC at the current
        // instruction boundary, then advance master ticks until
        // we land at a NEW boundary with a different PC. The CPU
        // ticks every 3rd master tick; `instruction_complete()`
        // returns true at the boundary the *current* opcode fetch
        // is about to happen at, so we need both "moved off the
        // start boundary" and "arrived at a new boundary" before
        // we count the step.
        let start_pc = nes_ref(s)?.cpu.regs.pc;
        let mut left_boundary = false;
        let step_start = master_ticks_used;
        loop {
            let nes = nes_mut(s)?;
            nes.tick();
            master_ticks_used += 1;
            if master_ticks_used - step_start > max_master_ticks {
                return Err(ToolError::Execution(format!(
                    "step exceeded max_master_ticks ({max_master_ticks}) without completing an instruction"
                )));
            }
            let nes_r = nes_ref(s)?;
            let complete = nes_r.cpu.instruction_complete();
            if !complete {
                left_boundary = true;
            }
            if left_boundary && complete && nes_r.cpu.regs.pc != start_pc {
                break;
            }
        }
        let pc = nes_ref(s)?.cpu.regs.pc;
        pc_trace.push(format!("${pc:04X}"));
    }
    Ok(json!({
        "count":             count,
        "master_ticks_used": master_ticks_used,
        "pc_trace":          pc_trace,
        "pc":                pc_trace.last().cloned().unwrap_or_else(|| "$????".into()),
    }))
}

fn tool_run_until_pc(args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    let target = arg_u16(&args, "target")?;
    let max_master_ticks = arg_u64_or(&args, "max_master_ticks", 50_000_000)?;
    let mut master_ticks_used: u64 = 0;
    let mut hit = false;
    while master_ticks_used < max_master_ticks {
        let nes = nes_mut(s)?;
        nes.tick();
        master_ticks_used += 1;
        let pc_match = {
            let nes = nes_ref(s)?;
            nes.cpu.instruction_complete() && nes.cpu.regs.pc == target
        };
        if pc_match {
            hit = true;
            break;
        }
    }
    let final_pc = nes_ref(s)?.cpu.regs.pc;
    Ok(json!({
        "hit":               hit,
        "master_ticks_used": master_ticks_used,
        "target":            format!("${:04X}", target),
        "pc":                format!("${:04X}", final_pc),
    }))
}

fn tool_run_until_mem_change(args: Value, s: &mut NesSession) -> Result<Value, ToolError> {
    let addr = arg_u16(&args, "addr")?;
    let max_master_ticks = arg_u64_or(&args, "max_master_ticks", 50_000_000)?;
    let initial = nes_ref(s)?.peek(addr);
    let mut master_ticks_used: u64 = 0;
    let mut hit = false;
    let mut current = initial;
    while master_ticks_used < max_master_ticks {
        let nes = nes_mut(s)?;
        nes.tick();
        master_ticks_used += 1;
        current = nes_ref(s)?.peek(addr);
        if current != initial {
            hit = true;
            break;
        }
    }
    Ok(json!({
        "hit":               hit,
        "master_ticks_used": master_ticks_used,
        "addr":              format!("${:04X}", addr),
        "initial":           format!("${:02X}", initial),
        "current":           format!("${:02X}", current),
    }))
}

// ════════════════════════════════════════════════════════════════
//  Registration
// ════════════════════════════════════════════════════════════════

pub fn register_nes_tools(registry: &mut ToolRegistry<NesSession>) {
    fn add(
        registry: &mut ToolRegistry<NesSession>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut NesSession) -> Result<Value, ToolError>,
    ) {
        registry.register(Box::new(InlineTool {
            name,
            description,
            schema,
            run,
        }));
    }

    let empty = || json!({"type": "object", "additionalProperties": false});

    let addr_only = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "CPU bus address — integer or hex string ($XXXX / 0xXXXX)."},
        }
    });

    let memory_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "CPU bus start address — integer or hex string."},
            "len":  {"type": "integer", "minimum": 1, "maximum": 4096, "default": 16,
                     "description": "Number of bytes to read."}
        }
    });

    let oam_schema = json!({
        "type": "object",
        "properties": {
            "start": {"type": "integer", "minimum": 0, "maximum": 63,
                      "description": "Starting sprite index (0-63). Default 0."},
            "count": {"type": "integer", "minimum": 1, "maximum": 64,
                      "description": "Number of sprites. Default 64."}
        }
    });

    let nametable_schema = json!({
        "type": "object",
        "properties": {
            "which": {"type": "integer", "minimum": 0, "maximum": 1,
                      "description": "Pick one nametable (0 or 1, 1 KiB each). Omit to dump both."}
        }
    });

    let step_schema = json!({
        "type": "object",
        "properties": {
            "count":            {"type": "integer", "minimum": 1, "default": 1,
                                 "description": "Number of CPU instructions to step."},
            "max_master_ticks": {"type": "integer", "minimum": 1, "default": 100_000_000,
                                 "description": "Per-instruction master-tick ceiling — guards against KIL / halted CPU."}
        }
    });

    let until_pc_schema = json!({
        "type": "object",
        "required": ["target"],
        "properties": {
            "target":           {"description": "PC target — integer or hex string."},
            "max_master_ticks": {"type": "integer", "minimum": 1, "default": 50_000_000,
                                 "description": "Master-tick ceiling before giving up."}
        }
    });

    let until_mem_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr":             {"description": "CPU bus address to watch — integer or hex string."},
            "max_master_ticks": {"type": "integer", "minimum": 1, "default": 50_000_000,
                                 "description": "Master-tick ceiling before giving up."}
        }
    });

    add(
        registry,
        "memory_read",
        "Read `len` bytes from CPU-visible memory starting at `addr` (no side effects). Returns hex + ASCII.",
        memory_schema,
        tool_memory_read,
    );
    add(
        registry,
        "dump_palette",
        "PPU palette RAM (32 bytes — 16 background + 16 sprite).",
        empty(),
        tool_dump_palette,
    );
    add(
        registry,
        "dump_oam",
        "PPU OAM as decoded sprite entries (y, tile, attr, x + flip/priority/palette bits). Default dumps all 64 sprites.",
        oam_schema,
        tool_dump_oam,
    );
    add(
        registry,
        "dump_nametable",
        "PPU nametable RAM (2 KiB) as hex + ASCII. Pick `which: 0|1` for a single 1 KiB nametable.",
        nametable_schema,
        tool_dump_nametable,
    );
    add(
        registry,
        "step",
        "Step `count` CPU instructions, returning a PC trace. Drives the machine at master-tick granularity until each instruction boundary; the PPU advances accordingly.",
        step_schema,
        tool_step,
    );
    add(
        registry,
        "run_until_pc",
        "Run until CPU PC == target at an instruction boundary, or master_tick ceiling reached.",
        until_pc_schema,
        tool_run_until_pc,
    );
    add(
        registry,
        "run_until_mem_change",
        "Run until the byte at `addr` changes from its current value, or master_tick ceiling reached.",
        until_mem_schema,
        tool_run_until_mem_change,
    );

    let _ = addr_only; // kept for future tools that need just an address
}
