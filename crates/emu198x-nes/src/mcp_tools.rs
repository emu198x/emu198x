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

    // memory_read / step / run_until_pc / run_until_mem_change are served by
    // the shared `register_debug_tools` tier (registered earlier via
    // `register_base_tools`). The NES used to shadow them with master-tick
    // variants; now that the shared `step_instruction` reports master ticks
    // and the shared tier carries `pc_trace` + `run_until_mem_change`, the
    // shadows are redundant — converged up per RULES.md #30. Only the
    // PPU-specific dumps stay bespoke.
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
}
