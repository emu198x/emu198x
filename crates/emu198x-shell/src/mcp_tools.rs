//! Shared MCP tools that wrap the machine-agnostic [`ScriptStep`]
//! variants.
//!
//! Every per-system binary's MCP server needs the same core surface —
//! run frames, query state, read snapshots, capture screenshots. Those
//! steps already execute generically through
//! [`ScriptStep::execute_collect`], so the tool wrappers can live here
//! once instead of being copied into each binary. A system registers
//! these with [`register_common_tools`] and then adds any
//! system-specific tools (CPU disassembly, chip-specific queries) on
//! top.
//!
//! The Spectrum binary predates this helper and keeps its own bespoke
//! registrations; new systems (NES, Game Boy, Dragon, C64, Amiga) build
//! on the shared set.

use serde_json::{Value, json};

use crate::machine::MachineCore;
use crate::mcp::{Tool, ToolError, ToolRegistry, ToolResponse};
use crate::query::SessionQueryProvider;
use crate::script::ScriptStep;
use crate::session::HeadlessSession;

/// One MCP tool mapping directly onto a machine-agnostic [`ScriptStep`]
/// variant. The tool name is the variant's serde `action` tag; the
/// arguments are the variant's remaining fields.
struct ScriptStepTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
}

impl<M, Q> Tool<HeadlessSession<M, Q>> for ScriptStepTool
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
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
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<ToolResponse, ToolError> {
        let step = build_step(self.name, arguments)?;
        let observation = step
            .execute_collect(session)
            .map_err(|err| ToolError::Execution(err.to_string()))?;
        let body = match observation {
            Some(obs) => serde_json::to_string(&obs).map_err(|err| {
                ToolError::Execution(format!("failed to serialize observation: {err}"))
            })?,
            None => String::from("null"),
        };
        Ok(ToolResponse::success_text(body))
    }
}

/// Build a [`ScriptStep`] from a tool name (the serde `action` tag) and
/// its JSON arguments by merging the tag into the argument object and
/// deserializing.
fn build_step(action: &str, arguments: Value) -> Result<ScriptStep, ToolError> {
    let mut obj = match arguments {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => {
            return Err(ToolError::InvalidArguments(
                "arguments must be a JSON object".to_owned(),
            ));
        }
    };
    obj.insert("action".to_owned(), Value::String(action.to_owned()));
    serde_json::from_value(Value::Object(obj)).map_err(|err| {
        ToolError::InvalidArguments(format!("invalid arguments for `{action}`: {err}"))
    })
}

/// Register the machine-agnostic MCP tools onto a per-system registry.
///
/// Covers media loading + transport, input, running frames, the boot /
/// query waiters, query resolution, snapshot save/restore, and
/// screenshot / audio capture. These delegate to
/// [`ScriptStep::execute_collect`], which is generic over any
/// [`MachineCore`] + [`SessionQueryProvider`].
pub fn register_common_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    registry.register(Box::new(ScriptStepTool {
        name: "run_frames",
        description: "Run the machine for a number of native video frames.",
        schema: json!({
            "type": "object",
            "properties": { "frames": { "type": "integer", "minimum": 0 } },
            "required": ["frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "run_ticks",
        description: "Run the machine for an exact number of sub-frame ticks \
                      (one authoritative-clock unit each, e.g. one PPU dot on \
                      the NES) for cycle-exact debugging. Errors if the system \
                      does not support sub-frame stepping.",
        schema: json!({
            "type": "object",
            "properties": { "ticks": { "type": "integer", "minimum": 0 } },
            "required": ["ticks"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_boot",
        description: "Run frames until the machine reports it has booted (or the frame budget is exhausted).",
        schema: json!({
            "type": "object",
            "properties": { "max_frames": { "type": "integer", "minimum": 0 } },
            "required": ["max_frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_contains",
        description: "Run frames until a text-bearing query path contains a substring.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "needle": { "type": "string" },
                "max_frames": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "needle", "max_frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_bool",
        description: "Run frames until a boolean query path reaches a target value.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "value": { "type": "boolean" },
                "max_frames": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "value", "max_frames"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "query",
        description: "Resolve one shared query path against the live session.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "query_paths",
        description: "List supported query paths, optionally filtered by prefix.",
        schema: json!({
            "type": "object",
            "properties": { "prefix": { "type": ["string", "null"] } }
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "input",
        description: "Queue generic input events (keys / buttons / axes) for the next run step.",
        schema: json!({
            "type": "object",
            "properties": { "events": { "type": "array" } },
            "required": ["events"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "load_media",
        description: "Load a media image (cartridge / disk / tape / program) into a named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": { "type": "string" },
                "kind": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["slot", "kind", "path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "media_transport",
        description: "Start or stop media transport on a named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": { "type": "string" },
                "transport": { "type": "string" }
            },
            "required": ["slot", "transport"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "load_snapshot",
        description: "Restore a snapshot file into the live machine.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "save_snapshot",
        description: "Save the current machine snapshot to disk.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "save_screenshot",
        description: "Save the latest emitted frame as a PNG file.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "save_audio_capture",
        description: "Save the captured audio stream as a WAV file.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "reset_after": { "type": "boolean" }
            },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "start_audio_recording",
        description: "Begin recording emitted audio to a 16-bit PCM WAV file. \
                      Subsequent run_frames tee audio into the session buffer; \
                      the WAV is written when stop_audio_recording is called.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "stop_audio_recording",
        description: "Finalise the in-flight audio recording and return the summary.",
        schema: json!({ "type": "object" }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "start_video_recording",
        description: "Begin recording the live framebuffer + audio to one MP4 file. \
                      The file is written when stop_video_recording is called.",
        schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    }));
    registry.register(Box::new(ScriptStepTool {
        name: "stop_video_recording",
        description: "Finalise the in-flight video recording and return the summary.",
        schema: json!({ "type": "object" }),
    }));
}

// ---------------------------------------------------------------------------
// Shared debug tools (CPU / memory / disassembly / stepping / I/O trace).
//
// These operate through the machine-agnostic `DebugTarget` trait, surfaced
// by `MachineCore::debug_target[_mut]`, so the whole set is written once and
// registered onto any machine that implements the trait — instead of being
// copy-pasted into each binary's `mcp_tools.rs`.
// ---------------------------------------------------------------------------

use std::marker::PhantomData;

use crate::debug::DebugTarget;

/// A debug tool whose body is a monomorphized function pointer over a
/// specific `(M, Q)`. The generic [`register_debug_tools`] instantiates one
/// per machine with the matching run functions.
struct DebugTool<M, Q> {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut HeadlessSession<M, Q>) -> Result<Value, ToolError>,
    _pd: PhantomData<fn() -> (M, Q)>,
}

impl<M, Q> Tool<HeadlessSession<M, Q>> for DebugTool<M, Q>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
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
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<ToolResponse, ToolError> {
        let body = (self.run)(arguments, session)?;
        let text = serde_json::to_string(&body)
            .map_err(|err| ToolError::Execution(format!("serialize: {err}")))?;
        Ok(ToolResponse::success_text(text))
    }
}

fn debug_ref<M: MachineCore, Q>(
    session: &HeadlessSession<M, Q>,
) -> Result<&dyn DebugTarget, ToolError> {
    session
        .machine()
        .debug_target()
        .ok_or_else(|| ToolError::Execution("debug target unavailable (machine not loaded)".into()))
}

fn debug_mut<M: MachineCore, Q>(
    session: &mut HeadlessSession<M, Q>,
) -> Result<&mut dyn DebugTarget, ToolError> {
    session
        .machine_mut()
        .debug_target_mut()
        .ok_or_else(|| ToolError::Execution("debug target unavailable (machine not loaded)".into()))
}

/// Parse a required address/number argument: a JSON integer, or a string
/// in `$XXXX` / `0xXXXX` / decimal form.
fn parse_num(args: &Value, name: &str) -> Result<u32, ToolError> {
    let v = args
        .get(name)
        .ok_or_else(|| ToolError::InvalidArguments(format!("missing argument `{name}`")))?;
    if let Some(n) = v.as_u64() {
        return u32::try_from(n)
            .map_err(|_| ToolError::InvalidArguments(format!("`{name}` out of range: {n}")));
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
        return u32::from_str_radix(body, radix)
            .map_err(|err| ToolError::InvalidArguments(format!("`{name}` parse: {err}")));
    }
    Err(ToolError::InvalidArguments(format!(
        "`{name}` must be an integer or hex string"
    )))
}

fn parse_opt(args: &Value, name: &str, default: u32) -> Result<u32, ToolError> {
    if args.get(name).is_some() {
        parse_num(args, name)
    } else {
        Ok(default)
    }
}

fn run_query_cpu<M: MachineCore, Q>(
    _args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    Ok(debug_ref(session)?.cpu_state())
}

fn run_memory_read<M: MachineCore, Q>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "addr")?;
    let len = parse_opt(&args, "len", 16)?.min(4096);
    let target = debug_ref(session)?;
    let mut hex = String::new();
    let mut ascii = String::new();
    for offset in 0..len {
        let byte = target.peek(addr.wrapping_add(offset));
        if offset > 0 {
            hex.push(' ');
        }
        hex.push_str(&format!("{byte:02X}"));
        ascii.push(if (0x20..=0x7E).contains(&byte) {
            char::from(byte)
        } else {
            '.'
        });
    }
    Ok(json!({ "addr": format!("${addr:04X}"), "len": len, "hex": hex, "ascii": ascii }))
}

fn run_poke_byte<M: MachineCore, Q>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "addr")?;
    let value = parse_num(&args, "value")? as u8;
    debug_mut(session)?.poke(addr, value);
    Ok(json!({ "addr": format!("${addr:04X}"), "value": format!("${value:02X}") }))
}

fn run_poke_word<M: MachineCore, Q>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "addr")?;
    let value = parse_num(&args, "value")? as u16;
    let target = debug_mut(session)?;
    let [lo, hi] = value.to_le_bytes();
    target.poke(addr, lo);
    target.poke(addr.wrapping_add(1), hi);
    Ok(json!({ "addr": format!("${addr:04X}"), "value": format!("${value:04X}") }))
}

fn run_disasm<M: MachineCore, Q>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    let addr = parse_num(&args, "addr")?;
    let count = parse_opt(&args, "count", 16)?.min(256);
    let target = debug_ref(session)?;
    let mut lines = Vec::new();
    let mut a = addr;
    for _ in 0..count {
        let Some((text, len)) = target.disassemble(a) else {
            return Err(ToolError::Execution(
                "no disassembler wired for this CPU (e.g. the 6809 family has no disassemble hook yet)"
                    .into(),
            ));
        };
        lines.push(json!({ "addr": format!("${a:04X}"), "text": text }));
        a = a.wrapping_add(u32::from(len.max(1)));
    }
    Ok(json!({ "lines": lines }))
}

fn run_run_until_pc<M: MachineCore, Q>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    let target_pc = parse_num(&args, "pc")?;
    let max_steps = u64::from(parse_opt(&args, "max_steps", 2_000_000)?);
    let target = debug_mut(session)?;
    let mut ticks = 0u64;
    let mut reached = false;
    let mut steps = 0u64;
    while steps < max_steps {
        if target.pc() == target_pc {
            reached = true;
            break;
        }
        ticks += target.step_instruction();
        steps += 1;
    }
    reached |= target.pc() == target_pc;
    Ok(json!({
        "pc": format!("${target_pc:04X}"),
        "reached": reached,
        "steps": steps,
        "ticks": ticks,
        "cpu_pc": format!("${:04X}", target.pc()),
    }))
}

fn run_step<M: MachineCore, Q>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    let count = parse_opt(&args, "count", 1)?.min(100_000);
    let target = debug_mut(session)?;
    let mut ticks = 0u64;
    for _ in 0..count {
        ticks += target.step_instruction();
    }
    let pc = target.pc();
    let next = target.disassemble(pc).map(|(text, _)| text);
    Ok(json!({
        "count": count,
        "ticks": ticks,
        "cpu_pc": format!("${pc:04X}"),
        "next": next,
    }))
}

fn run_io_trace<M: MachineCore, Q: SessionQueryProvider<M>>(
    args: Value,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Value, ToolError> {
    if !debug_mut(session)?.supports_io_trace() {
        return Err(ToolError::Execution(
            "this machine does not support I/O port tracing (memory-mapped CPU); \
             use memory_read / disasm / run_until_pc instead"
                .into(),
        ));
    }
    let frames = parse_opt(&args, "frames", 4)?.min(600);
    let limit = parse_opt(&args, "limit", 256)?.min(4096) as usize;

    debug_mut(session)?.start_io_trace();
    let step = build_step("run_frames", json!({ "frames": frames }))?;
    step.execute_collect(session)
        .map_err(|err| ToolError::Execution(err.to_string()))?;
    let events = debug_mut(session)?.take_io_trace();
    let total = events.len();

    let mut ports: std::collections::BTreeMap<u8, (u32, u32)> = std::collections::BTreeMap::new();
    for e in &events {
        let entry = ports.entry(e.port).or_default();
        if e.write {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let by_port: Vec<Value> = ports
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
        "by_port": by_port,
        "events": sample,
        "truncated": total > limit,
    }))
}

/// Register the shared debug tools onto a per-system registry. Requires the
/// machine's runtime to implement [`MachineCore::debug_target`]; tools error
/// cleanly at call time when no debug target is available.
///
/// Registers: `query_cpu`, `memory_read`, `poke_byte`, `poke_word`,
/// `disasm`, `run_until_pc`, `step`, `io_trace`.
pub fn register_debug_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    fn add<M, Q>(
        registry: &mut ToolRegistry<HeadlessSession<M, Q>>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut HeadlessSession<M, Q>) -> Result<Value, ToolError>,
    ) where
        M: MachineCore + 'static,
        Q: SessionQueryProvider<M> + 'static,
    {
        registry.register(Box::new(DebugTool {
            name,
            description,
            schema,
            run,
            _pd: PhantomData,
        }));
    }

    let empty = || json!({ "type": "object", "additionalProperties": false });
    let addr_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": { "description": "Address (integer or $XXXX / 0xXXXX)." },
            "len":  { "type": "integer", "minimum": 1, "maximum": 4096, "default": 16 }
        }
    });
    let poke_byte_schema = json!({
        "type": "object",
        "required": ["addr", "value"],
        "properties": {
            "addr":  { "description": "Address (integer or $XXXX / 0xXXXX)." },
            "value": { "description": "Byte value (integer or $XX / 0xXX)." }
        }
    });
    let poke_word_schema = json!({
        "type": "object",
        "required": ["addr", "value"],
        "properties": {
            "addr":  { "description": "Address (integer or $XXXX / 0xXXXX)." },
            "value": { "description": "16-bit value, written little-endian." }
        }
    });
    let disasm_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr":  { "description": "Start address (integer or $XXXX / 0xXXXX)." },
            "count": { "type": "integer", "minimum": 1, "maximum": 256, "default": 16 }
        }
    });
    let run_until_schema = json!({
        "type": "object",
        "required": ["pc"],
        "properties": {
            "pc":        { "description": "Target PC (integer or $XXXX / 0xXXXX)." },
            "max_steps": { "type": "integer", "minimum": 1, "default": 2000000 }
        }
    });
    let step_schema = json!({
        "type": "object",
        "properties": { "count": { "type": "integer", "minimum": 1, "maximum": 100000, "default": 1 } }
    });
    let io_trace_schema = json!({
        "type": "object",
        "properties": {
            "frames": { "type": "integer", "minimum": 1, "maximum": 600, "default": 4 },
            "limit":  { "type": "integer", "minimum": 1, "maximum": 4096, "default": 256 }
        }
    });

    add(
        registry,
        "query_cpu",
        "CPU register snapshot.",
        empty(),
        run_query_cpu,
    );
    add(
        registry,
        "memory_read",
        "Read `len` bytes from the CPU bus at `addr` (no side effects).",
        addr_schema,
        run_memory_read,
    );
    add(
        registry,
        "poke_byte",
        "Write one byte to writable memory at `addr`.",
        poke_byte_schema,
        run_poke_byte,
    );
    add(
        registry,
        "poke_word",
        "Write a 16-bit little-endian value to memory at `addr`.",
        poke_word_schema,
        run_poke_word,
    );
    add(
        registry,
        "disasm",
        "Disassemble `count` instructions from `addr` (CPU-dependent; \
         6502 disassembly pending the Asm198x crate).",
        disasm_schema,
        run_disasm,
    );
    add(
        registry,
        "run_until_pc",
        "Run whole instructions until the CPU reaches `pc` or `max_steps` elapse.",
        run_until_schema,
        run_run_until_pc,
    );
    add(
        registry,
        "step",
        "Single-step `count` whole CPU instructions; returns the new PC and next instruction.",
        step_schema,
        run_step,
    );
    add(
        registry,
        "io_trace",
        "Run `frames` frames capturing every I/O port access (Z80-family \
         machines only); returns a per-port summary plus a sample of events.",
        io_trace_schema,
        run_io_trace,
    );
}
