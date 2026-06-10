//! Atari 800XL-specific MCP tools.
//!
//! CPU / memory / poke / disasm / stepping come from the shared
//! [`emu198x_shell::mcp_tools::register_base_tools`] set (6502 `disasm` is
//! wired via the Asm198x `isa-disasm` decoder). This adds the ANTIC / GTIA /
//! POKEY / PIA chip snapshots and the keyboard input tools (`press_key`,
//! `type_string`) on top.

use emu198x_shell::{
    HeadlessSession, InputEvent,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
};
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

/// Parse a numeric JSON argument that may be a number or a `$xx` / `0x` /
/// plain hex/decimal string.
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

/// Frames a key is held / settled between presses.
const KEY_HOLD_FRAMES: u32 = 3;
const KEY_SETTLE_FRAMES: u32 = 6;

fn press_release(
    session: &mut A800xlSession,
    name: &str,
    hold: u32,
    settle: u32,
) -> Result<(), ToolError> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames(hold)
        .map_err(|e| ToolError::Execution(format!("press hold: {e}")))?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session
        .run_frames(settle)
        .map_err(|e| ToolError::Execution(format!("release settle: {e}")))?;
    Ok(())
}

fn tool_press_key(args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let name = args
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("missing `key` (string)".into()))?
        .to_owned();
    let hold = opt_num(&args, "hold_frames", KEY_HOLD_FRAMES)?.clamp(1, 600);
    press_release(session, &name, hold, KEY_SETTLE_FRAMES)?;
    Ok(json!({ "key": name, "hold_frames": hold }))
}

fn tool_type_string(args: Value, session: &mut A800xlSession) -> Result<Value, ToolError> {
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("missing `text` (string)".into()))?
        .to_owned();
    let hold = opt_num(&args, "hold_frames", KEY_HOLD_FRAMES)?.clamp(1, 600);

    let mut typed = 0u32;
    let mut prev: Option<String> = None;
    for ch in text.chars() {
        let name = if ch == '\n' || ch == '\r' {
            "Return".to_owned()
        } else {
            ch.to_string()
        };
        if prev.as_deref() == Some(name.as_str()) {
            session
                .run_frames(KEY_SETTLE_FRAMES)
                .map_err(|e| ToolError::Execution(format!("repeat settle: {e}")))?;
        }
        press_release(session, &name, hold, KEY_SETTLE_FRAMES)?;
        prev = Some(name);
        typed += 1;
    }
    Ok(json!({ "text": text, "chars_typed": typed }))
}

/// Register the 800XL-specific MCP tools: the keyboard input tools. The
/// CPU / memory / debug surface comes from
/// [`register_base_tools`](emu198x_shell::mcp_tools::register_base_tools),
/// and the ANTIC / GTIA / POKEY / PIA chip state is read through the generic
/// `query` tool as query paths (`antic`, `gtia`, `pokey`, `pia`, and their
/// leaves) — both registered by the server before this.
pub fn register_a800xl_tools(registry: &mut ToolRegistry<A800xlSession>) {
    let mut tool = |name, description, schema, run| {
        registry.register(Box::new(InlineTool {
            name,
            description,
            schema,
            run,
        }));
    };

    tool(
        "press_key",
        "Press, hold, and release one key. `key` is a single character (case \
         set by caps lock) or a name (Return, Space, Esc, Tab, Delete). \
         Optional hold_frames (default 3).",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "key":         {"type": "string"},
                "hold_frames": {"type": "integer", "minimum": 1, "maximum": 600}
            },
            "required": ["key"]
        }),
        tool_press_key,
    );
    tool(
        "type_string",
        "Type a string by pressing each character in turn (newline → RETURN). \
         Letters arrive uppercase under the power-on caps lock, as on real \
         hardware. Optional hold_frames (default 3).",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "text":        {"type": "string"},
                "hold_frames": {"type": "integer", "minimum": 1, "maximum": 600}
            },
            "required": ["text"]
        }),
        tool_type_string,
    );
}

#[cfg(test)]
mod tests {
    use super::{A800xlSession, parse_num, register_a800xl_tools};
    use emu198x_shell::mcp::{ToolContent, ToolRegistry, ToolResponse};
    use emu198x_shell::mcp_tools::register_base_tools;
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
        register_base_tools(&mut reg);
        register_a800xl_tools(&mut reg);

        let call = |s: &mut A800xlSession, name: &str, args: Value| -> Value {
            body(&reg.get(name).expect("tool").call(args, s).expect("ok"))
        };

        // Chip state is read via query paths now. ANTIC programmed for
        // display: DMACTL with DL DMA on.
        let chip = |s: &mut A800xlSession, path: &str| -> Value {
            call(s, "query", json!({ "path": path }))["result"]["value"].clone()
        };
        let antic = chip(&mut session, "antic");
        let dmactl_hex = antic["dmactl"].as_str().expect("dmactl string");
        let dmactl =
            u8::from_str_radix(dmactl_hex.trim_start_matches('$'), 16).expect("dmactl hex");
        assert_ne!(dmactl & 0x20, 0, "DMACTL DL DMA: {antic}");

        // The other chip snapshots all surface their registers.
        assert!(chip(&mut session, "gtia")["colpf"].is_array());
        assert!(chip(&mut session, "pokey")["irqen"].is_string());
        assert!(chip(&mut session, "pia")["portb"].is_string());

        // Shared memory_read: 16 space-separated hex bytes by default.
        let mem = call(&mut session, "memory_read", json!({"addr": "$0200"}));
        let hex = mem["hex"].as_str().expect("hex string");
        assert_eq!(hex.split_whitespace().count(), 16);

        // poke/read round-trip via the shared tools.
        call(
            &mut session,
            "poke_byte",
            json!({"addr": "$0600", "value": "$5A"}),
        );
        let back = call(
            &mut session,
            "memory_read",
            json!({"addr": 0x0600, "len": 1}),
        );
        assert_eq!(back["hex"].as_str().expect("hex"), "5A");

        // poke_word is little-endian.
        call(
            &mut session,
            "poke_word",
            json!({"addr": "$0610", "value": "$ABCD"}),
        );
        let w = call(
            &mut session,
            "memory_read",
            json!({"addr": "$0610", "len": 2}),
        );
        assert_eq!(w["hex"].as_str().expect("hex"), "CD AB");
    }

    fn screen_addr(ram: &[u8]) -> usize {
        let dlist = u16::from(ram[0x0230]) | (u16::from(ram[0x0231]) << 8);
        let mut p = dlist as usize;
        for _ in 0..64 {
            let b = ram[p];
            if b & 0x40 != 0 && (b & 0x0F) >= 2 {
                return usize::from(ram[p + 1]) | (usize::from(ram[p + 2]) << 8);
            }
            if b & 0x0F == 0x01 {
                break;
            }
            p += if b & 0x40 != 0 { 3 } else { 1 };
        }
        0
    }

    #[test]
    #[ignore = "requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
    fn run_and_input_tools_drive_basic() {
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
        register_base_tools(&mut reg);
        register_a800xl_tools(&mut reg);
        let call = |s: &mut A800xlSession, name: &str, args: Value| -> Value {
            body(&reg.get(name).expect("tool").call(args, s).expect("ok"))
        };

        // Shared run_until_pc: the idle BASIC prompt sits at its current PC,
        // so running to that PC is reached immediately.
        let pc = call(&mut session, "query_cpu", json!({}))["pc"]
            .as_str()
            .expect("pc")
            .to_owned();
        let ran = call(&mut session, "run_until_pc", json!({"pc": pc}));
        assert_eq!(ran["reached"], true, "run_until_pc revisits idle PC: {ran}");

        // type_string drives BASIC: `PRINT 6*7` then RETURN evaluates to 42.
        let typed = call(&mut session, "type_string", json!({"text": "PRINT 6*7\n"}));
        assert_eq!(typed["chars_typed"], 10);
        session.run_frames(30).expect("evaluate");

        let m = session.machine().machine().expect("machine");
        let ram = m.ram();
        let scr = screen_addr(ram);
        let found = (0..40 * 24 - 1).any(|j| ram[scr + j] == 0x14 && ram[scr + j + 1] == 0x12);
        assert!(
            found,
            "type_string `PRINT 6*7` did not yield `42` on screen"
        );
    }
}
