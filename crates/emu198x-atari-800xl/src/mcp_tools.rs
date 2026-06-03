//! Atari 800XL-specific MCP tools.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
use machine_atari_800xl::Atari800xl;
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider};
use serde_json::{Value, json};

type A800xlSession = HeadlessSession<Atari800xlRuntime, Atari800xlSessionQueryProvider>;

struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut A800xlSession) -> Result<Value, ToolError>,
}

impl Tool<A800xlSession> for InlineTool {
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
        session: &mut A800xlSession,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn a800xl_ref(s: &A800xlSession) -> Result<&Atari800xl, ToolError> {
    s.machine()
        .machine()
        .ok_or_else(|| ToolError::Execution("no OS / cart loaded".into()))
}

fn a800xl_mut(s: &mut A800xlSession) -> Result<&mut Atari800xl, ToolError> {
    s.machine_mut()
        .machine_mut()
        .ok_or_else(|| ToolError::Execution("no OS / cart loaded".into()))
}

/// Parse a numeric JSON argument that may be a number or a `$xx` / `0x` / plain
/// hex/decimal string. Addresses and bytes in this binary accept both forms.
fn parse_num(args: &Value, key: &str) -> Result<u32, ToolError> {
    let v = args
        .get(key)
        .ok_or_else(|| ToolError::InvalidArguments(format!("missing `{key}`")))?;
    if let Some(n) = v.as_u64() {
        return Ok(n as u32);
    }
    let s = v
        .as_str()
        .ok_or_else(|| ToolError::InvalidArguments(format!("`{key}` must be a number or string")))?
        .trim();
    let (radix, digits) = if let Some(h) = s.strip_prefix('$') {
        (16, h)
    } else if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, h)
    } else {
        (10, s)
    };
    u32::from_str_radix(digits, radix)
        .map_err(|_| ToolError::InvalidArguments(format!("`{key}` is not a valid number: {s}")))
}

fn opt_num(args: &Value, key: &str, default: u32) -> Result<u32, ToolError> {
    if args.get(key).is_some() {
        parse_num(args, key)
    } else {
        Ok(default)
    }
}

fn tool_query_cpu(_args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let m = a800xl_ref(session)?;
    let r = &m.cpu().regs;
    Ok(json!({
        "a":  format!("${:02X}", r.a),
        "x":  format!("${:02X}", r.x),
        "y":  format!("${:02X}", r.y),
        "sp": format!("${:02X}", r.sp),
        "pc": format!("${:04X}", r.pc),
        "p":  format!("${:02X}", r.p),
        "halted": m.cpu().halted,
    }))
}

fn tool_memory_read(args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "address")? as u16;
    let len = opt_num(&args, "length", 16)?.clamp(1, 256) as usize;
    let m = a800xl_ref(session)?;
    let bytes: Vec<u8> = (0..len)
        .map(|i| m.peek(addr.wrapping_add(i as u16)))
        .collect();
    let hex = bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(json!({
        "address": format!("${addr:04X}"),
        "length":  len,
        "bytes":   bytes,
        "hex":     hex,
    }))
}

fn tool_poke_byte(args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "address")? as u16;
    let value = parse_num(&args, "value")? as u8;
    a800xl_mut(session)?.poke(addr, value);
    Ok(json!({ "address": format!("${addr:04X}"), "value": format!("${value:02X}") }))
}

fn tool_poke_word(args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "address")? as u16;
    let value = parse_num(&args, "value")? as u16;
    let m = a800xl_mut(session)?;
    m.poke(addr, (value & 0xFF) as u8);
    m.poke(addr.wrapping_add(1), (value >> 8) as u8);
    Ok(json!({ "address": format!("${addr:04X}"), "value": format!("${value:04X}") }))
}

fn hex8(v: u8) -> String {
    format!("${v:02X}")
}
fn hex8s(vs: [u8; 4]) -> Vec<String> {
    vs.iter().map(|&v| hex8(v)).collect()
}

fn tool_query_antic(_args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let a = a800xl_ref(session)?.antic();
    Ok(json!({
        "dmactl": hex8(a.dmactl_value()),
        "nmien":  hex8(a.nmien_value()),
        "dlist":  format!("${:04X}", a.dlist_value()),
        "chbase": hex8(a.chbase_value()),
        "chactl": hex8(a.chactl_value()),
        "hscrol": hex8(a.hscrol_value()),
        "vscrol": hex8(a.vscrol_value()),
        "scan_line": a.scan_line(),
        "vcount":    hex8(a.vcount()),
    }))
}

fn tool_query_gtia(_args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let g = a800xl_ref(session)?.gtia();
    Ok(json!({
        "colbk":  hex8(g.colbk_value()),
        "colpf":  hex8s(g.colpf_values()),
        "colpm":  hex8s(g.colpm_values()),
        "prior":  hex8(g.prior_value()),
        "gractl": hex8(g.gractl_value()),
        "consol": hex8(g.console_switches()),
    }))
}

fn tool_query_pokey(_args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let p = a800xl_ref(session)?.pokey();
    Ok(json!({
        "audf":   hex8s(p.audf()),
        "audc":   hex8s(p.audc()),
        "audctl": hex8(p.audctl()),
        "irqen":  hex8(p.irqen()),
        "irqst":  hex8(p.irqst()),
        "skctl":  hex8(p.skctl()),
        "skstat": hex8(p.skstat()),
        "kbcode": hex8(p.kbcode()),
    }))
}

fn tool_query_pia(_args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let p = a800xl_ref(session)?.pia();
    Ok(json!({
        "porta": hex8(p.port_a_output()),
        "portb": hex8(p.port_b_output()),
        "ddra":  hex8(p.ddr_a()),
        "ddrb":  hex8(p.ddr_b()),
        "cra":   hex8(p.cra()),
        "crb":   hex8(p.crb()),
        "irq_pending": p.irq_pending(),
    }))
}

pub fn register_a800xl_tools(registry: &mut ToolRegistry<A800xlSession>) {
    let empty = || json!({"type": "object", "additionalProperties": false});
    let mut tool = |name, description, schema, run| {
        registry.register(Box::new(InlineTool {
            name,
            description,
            schema,
            run,
        }));
    };

    tool(
        "query_cpu",
        "6502C Sally register snapshot (A/X/Y/SP/PC/P, halted).",
        empty(),
        tool_query_cpu,
    );
    tool(
        "memory_read",
        "Read bytes as the CPU sees them through PORTB banking (RAM/ROM/cart; \
         the $D000-$D7FF register page reads as open bus). Args: address \
         (number or $hex), optional length (1-256, default 16).",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "address": {"type": ["integer", "string"]},
                "length":  {"type": "integer", "minimum": 1, "maximum": 256}
            },
            "required": ["address"]
        }),
        tool_memory_read,
    );
    tool(
        "poke_byte",
        "Write one byte into RAM (beneath any banked ROM). Args: address, value.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "address": {"type": ["integer", "string"]},
                "value":   {"type": ["integer", "string"]}
            },
            "required": ["address", "value"]
        }),
        tool_poke_byte,
    );
    tool(
        "poke_word",
        "Write a little-endian 16-bit word into RAM. Args: address, value.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "address": {"type": ["integer", "string"]},
                "value":   {"type": ["integer", "string"]}
            },
            "required": ["address", "value"]
        }),
        tool_poke_word,
    );
    tool(
        "query_antic",
        "ANTIC display-list processor registers (DMACTL, NMIEN, DLIST, CHBASE, \
         CHACTL, HSCROL, VSCROL, scan line).",
        empty(),
        tool_query_antic,
    );
    tool(
        "query_gtia",
        "GTIA registers (COLBK, COLPF0-3, COLPM0-3, PRIOR, GRACTL, console \
         switches).",
        empty(),
        tool_query_gtia,
    );
    tool(
        "query_pokey",
        "POKEY registers (AUDF/AUDC 0-3, AUDCTL, IRQEN, IRQST, SKCTL, SKSTAT, \
         KBCODE).",
        empty(),
        tool_query_pokey,
    );
    tool(
        "query_pia",
        "PIA 6520 registers (PORTA/PORTB outputs, DDRA/DDRB, CRA/CRB, IRQ).",
        empty(),
        tool_query_pia,
    );
}

#[cfg(test)]
mod tests {
    use super::{A800xlSession, parse_num, register_a800xl_tools};
    use emu198x_shell::mcp::{ToolContent, ToolRegistry, ToolResponse};
    use emu198x_shell::{HeadlessSession, MediaSet};
    use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};
    use serde_json::{Value, json};
    use std::path::PathBuf;

    const FRAME_TICKS_NTSC: u64 = 262 * 228;

    #[test]
    fn parse_num_accepts_dollar_hex_0x_and_decimal() {
        assert_eq!(parse_num(&json!({"a": "$D40E"}), "a").expect("hex"), 0xD40E);
        assert_eq!(parse_num(&json!({"a": "0x20"}), "a").expect("0x"), 0x20);
        assert_eq!(parse_num(&json!({"a": 1536}), "a").expect("int"), 1536);
        assert_eq!(parse_num(&json!({"a": "512"}), "a").expect("dec"), 512);
        assert!(parse_num(&json!({"a": "nope"}), "a").is_err());
        assert!(parse_num(&json!({}), "a").is_err());
    }

    fn rom(name: &str) -> Option<Vec<u8>> {
        let home = std::env::var("HOME").ok()?;
        std::fs::read(
            PathBuf::from(home)
                .join(".emu198x/roms/atari-800xl")
                .join(name),
        )
        .ok()
    }

    fn body(resp: &ToolResponse) -> Value {
        assert!(!resp.is_error, "tool returned error");
        let ToolContent::Text { text } = &resp.content[0];
        serde_json::from_str(text).expect("json body")
    }

    #[test]
    #[ignore = "requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
    fn inspection_tools_report_booted_state() {
        let (Some(os), Some(basic)) = (rom("atarixl.rom"), rom("ataribas.rom")) else {
            eprintln!("skipping: ROMs not present");
            return;
        };
        let runtime = Atari800xlRuntime::new(Model::A800xlNtsc, Some(os), Some(basic), None, true)
            .expect("runtime");
        let mut session: A800xlSession = HeadlessSession::new_with_query_provider(
            runtime,
            FRAME_TICKS_NTSC,
            Atari800xlSessionQueryProvider,
        );
        session.prepare(&MediaSet::new(), &[]).expect("prepare");
        session.run_frames(600).expect("boot");

        let mut reg: ToolRegistry<A800xlSession> = ToolRegistry::new();
        register_a800xl_tools(&mut reg);

        let call = |s: &mut A800xlSession, name: &str, args: Value| -> Value {
            body(&reg.get(name).expect("tool").call(args, s).expect("ok"))
        };

        // ANTIC programmed for display: DMACTL with DL DMA on.
        let antic = call(&mut session, "query_antic", json!({}));
        let dmactl_hex = antic["dmactl"].as_str().expect("dmactl string");
        let dmactl =
            u8::from_str_radix(dmactl_hex.trim_start_matches('$'), 16).expect("dmactl hex");
        assert_ne!(dmactl & 0x20, 0, "DMACTL DL DMA: {antic}");

        // Chip queries all surface their registers.
        assert!(call(&mut session, "query_gtia", json!({}))["colpf"].is_array());
        assert!(call(&mut session, "query_pokey", json!({}))["irqen"].is_string());
        assert!(call(&mut session, "query_pia", json!({}))["portb"].is_string());

        // memory_read default length.
        let mem = call(&mut session, "memory_read", json!({"address": "$0200"}));
        assert_eq!(mem["bytes"].as_array().expect("bytes array").len(), 16);

        // poke/read round-trip.
        call(
            &mut session,
            "poke_byte",
            json!({"address": "$0600", "value": "$5A"}),
        );
        let back = call(
            &mut session,
            "memory_read",
            json!({"address": 0x0600, "length": 1}),
        );
        assert_eq!(back["bytes"][0], 0x5A);

        // poke_word is little-endian.
        call(
            &mut session,
            "poke_word",
            json!({"address": "$0610", "value": "$ABCD"}),
        );
        let w = call(
            &mut session,
            "memory_read",
            json!({"address": "$0610", "length": 2}),
        );
        assert_eq!(w["bytes"][0], 0xCD);
        assert_eq!(w["bytes"][1], 0xAB);
    }
}
