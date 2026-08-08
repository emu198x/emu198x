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
use crate::mcp::{InlineTool, Tool, ToolError, ToolRegistry, ToolResponse};
use crate::query::SessionQueryProvider;
use crate::script::{ScriptObservation, ScriptStep};
use crate::session::HeadlessSession;

/// A pluggable step dispatcher: turns a parsed [`ScriptStep`] into an
/// optional [`ScriptObservation`]. The default
/// [`dispatch_via_execute_collect`] runs the step generically through
/// [`ScriptStep::execute_collect`]; a system whose MCP surface needs
/// richer, machine-specific handling (the Spectrum's live-access steps,
/// `set_machine`, `type_string`, …) supplies its own.
pub type StepDispatch<M, Q> =
    fn(&ScriptStep, &mut HeadlessSession<M, Q>) -> Result<Option<ScriptObservation>, ToolError>;

/// The default dispatcher: run the step generically over any
/// [`MachineCore`] + [`SessionQueryProvider`].
pub fn dispatch_via_execute_collect<M, Q>(
    step: &ScriptStep,
    session: &mut HeadlessSession<M, Q>,
) -> Result<Option<ScriptObservation>, ToolError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    step.execute_collect(session)
        .map_err(|err| ToolError::Execution(err.to_string()))
}

/// One MCP tool mapping directly onto a machine-agnostic [`ScriptStep`]
/// variant. The tool name is the variant's serde `action` tag; the
/// arguments are the variant's remaining fields. The `dispatch` field
/// selects how the parsed step is executed — `common` uses the generic
/// executor, `with_dispatch` injects a system-specific one.
pub struct ScriptStepTool<M, Q> {
    name: &'static str,
    description: &'static str,
    schema: Value,
    dispatch: StepDispatch<M, Q>,
}

impl<M, Q> ScriptStepTool<M, Q>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    /// A common tool whose step runs through the generic executor.
    pub fn common(name: &'static str, description: &'static str, schema: Value) -> Self {
        Self {
            name,
            description,
            schema,
            dispatch: dispatch_via_execute_collect,
        }
    }

    /// A tool that runs its step through a system-specific dispatcher
    /// (e.g. the Spectrum's `mcp_execute_step`).
    pub fn with_dispatch(
        name: &'static str,
        description: &'static str,
        schema: Value,
        dispatch: StepDispatch<M, Q>,
    ) -> Self {
        Self {
            name,
            description,
            schema,
            dispatch,
        }
    }
}

impl<M, Q> Tool<HeadlessSession<M, Q>> for ScriptStepTool<M, Q>
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
        let observation = (self.dispatch)(&step, session)?;
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
pub fn build_step(action: &str, arguments: Value) -> Result<ScriptStep, ToolError> {
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
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    registry.register(Box::new(ScriptStepTool::common(
        "run_frames",
        "Run the machine for a number of native video frames.",
        json!({
            "type": "object",
            "properties": { "frames": { "type": "integer", "minimum": 0 } },
            "required": ["frames"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "run_ticks",
        "Run the machine for an exact number of sub-frame ticks \
                      (one authoritative-clock unit each, e.g. one PPU dot on \
                      the NES) for cycle-exact debugging. Errors if the system \
                      does not support sub-frame stepping.",
        json!({
            "type": "object",
            "properties": { "ticks": { "type": "integer", "minimum": 0 } },
            "required": ["ticks"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "wait_for_boot",
        "Run frames until the machine reports it has booted (or the frame budget is exhausted).",
        json!({
            "type": "object",
            "properties": { "max_frames": { "type": "integer", "minimum": 0 } },
            "required": ["max_frames"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "wait_for_query_contains",
        "Run frames until a text-bearing query path contains a substring.",
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "needle": { "type": "string" },
                "max_frames": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "needle", "max_frames"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "wait_for_query_bool",
        "Run frames until a boolean query path reaches a target value.",
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "value": { "type": "boolean" },
                "max_frames": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "value", "max_frames"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "query",
        "Resolve one shared query path against the live session.",
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "query_paths",
        "List supported query paths, optionally filtered by prefix.",
        json!({
            "type": "object",
            "properties": { "prefix": { "type": ["string", "null"] } }
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "input",
        "Queue generic input events (keys / buttons / axes) for the next run step.",
        json!({
            "type": "object",
            "properties": { "events": { "type": "array" } },
            "required": ["events"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "load_media",
        "Load a media image (cartridge / disk / tape / program) into a named slot. Set `writable` to allow the machine to persist a SAVE to this image (archive media must stay read-only; default false).",
        json!({
            "type": "object",
            "properties": {
                "slot": { "type": "string" },
                "kind": { "type": "string" },
                "path": { "type": "string" },
                "writable": { "type": "boolean" }
            },
            "required": ["slot", "kind", "path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "media_transport",
        "Start or stop media transport on a named slot.",
        json!({
            "type": "object",
            "properties": {
                "slot": { "type": "string" },
                "transport": { "type": "string" }
            },
            "required": ["slot", "transport"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "load_snapshot",
        "Restore a snapshot file into the live machine.",
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "save_snapshot",
        "Save the current machine snapshot to disk.",
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "save_screenshot",
        "Save the latest emitted frame as a PNG file.",
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "save_audio_capture",
        "Save the captured audio stream as a WAV file.",
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "reset_after": { "type": "boolean" }
            },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "start_audio_recording",
        "Begin recording emitted audio to a 16-bit PCM WAV file. \
                      Subsequent run_frames tee audio into the session buffer; \
                      the WAV is written when stop_audio_recording is called.",
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "stop_audio_recording",
        "Finalise the in-flight audio recording and return the summary.",
        json!({ "type": "object" }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "start_video_recording",
        "Begin recording the live framebuffer + audio to one MP4 file. \
                      The file is written when stop_video_recording is called.",
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "stop_video_recording",
        "Finalise the in-flight video recording and return the summary.",
        json!({ "type": "object" }),
    )));
    registry.register(Box::new(ScriptStepTool::common(
        "reset",
        "Reset the machine. `kind` is \"hard\" (power-cycle, the \
                      default) or \"soft\" (machine-local). Clears queued input, \
                      the latest frame, captured audio, and the last run result; \
                      rejected while a video recording is in flight.",
        json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["hard", "soft"], "default": "hard" }
            }
        }),
    )));
}

// ---------------------------------------------------------------------------
// Shared debug tools (CPU / memory / disassembly / stepping / I/O trace).
//
// These operate through the machine-agnostic `DebugTarget` trait, surfaced
// by `MachineCore::debug_target[_mut]`, so the whole set is written once and
// registered onto any machine that implements the trait — instead of being
// copy-pasted into each binary's `mcp_tools.rs`.
// ---------------------------------------------------------------------------

use crate::debug::DebugTarget;

// The CPU / memory / disassembly / stepping debug verbs are registered as
// `ScriptStepTool`s over the shared `ScriptStep` arms (see `register_debug_tools`
// below), so MCP and `--script` run one implementation. Only `io_trace` — which
// runs frames and is gated on `supports_io_trace` — stays a bespoke
// `InlineTool`; its helpers (`debug_mut` / `parse_num` / `parse_opt`) live here.

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

/// Register the shared debug verbs onto a per-system registry, as
/// [`ScriptStepTool`]s over the machine-agnostic [`ScriptStep`] debug arms —
/// so the MCP tool and the matching `--script` step run the *same*
/// implementation through the machine's [`DebugTarget`]. Tools error cleanly
/// when no debug target is available.
///
/// Registers: `query_cpu`, `memory_read`, `poke_byte`, `poke_word`, `disasm`,
/// `step`, `run_until_pc`, `run_until_any_pc`, `run_until_mem_change` (all
/// `ScriptStep`-backed), plus `io_trace` (a bespoke [`InlineTool`]: it runs
/// frames and is gated on port-mapped I/O, so it has no `ScriptStep`).
pub fn register_debug_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    let mut common = |name, description, schema| {
        registry.register(Box::new(ScriptStepTool::<M, Q>::common(
            name,
            description,
            schema,
        )));
    };

    common(
        "query_cpu",
        "CPU register snapshot (machine-specific fields under `registers`).",
        json!({ "type": "object", "additionalProperties": false }),
    );
    common(
        "memory_read",
        "Read `len` bytes from the CPU bus at `addr` (no side effects).",
        json!({
            "type": "object",
            "required": ["addr"],
            "properties": {
                "addr": { "type": "integer", "description": "Start address." },
                "len":  { "type": "integer", "minimum": 1, "maximum": 256, "default": 16 }
            }
        }),
    );
    common(
        "poke_byte",
        "Write one byte to writable memory at `addr`.",
        json!({
            "type": "object",
            "required": ["addr", "value"],
            "properties": {
                "addr":  { "type": "integer" },
                "value": { "type": "integer", "minimum": 0, "maximum": 255 }
            }
        }),
    );
    common(
        "poke_word",
        "Write a 16-bit little-endian value at `addr` (big-endian CPUs override).",
        json!({
            "type": "object",
            "required": ["addr", "value"],
            "properties": {
                "addr":  { "type": "integer" },
                "value": { "type": "integer", "minimum": 0, "maximum": 65535 }
            }
        }),
    );
    common(
        "disasm",
        "Disassemble `instructions` opcodes from `addr`; each line carries text + raw bytes.",
        json!({
            "type": "object",
            "required": ["addr"],
            "properties": {
                "addr":         { "type": "integer" },
                "instructions": { "type": "integer", "minimum": 1, "maximum": 256, "default": 16 }
            }
        }),
    );
    common(
        "step",
        "Single-step `instructions` whole CPU instructions; returns the PC trace + next instruction.",
        json!({
            "type": "object",
            "properties": { "instructions": { "type": "integer", "minimum": 1, "default": 1 } }
        }),
    );
    common(
        "run_until_pc",
        "Step whole instructions until PC reaches `addr`, or `max_steps` elapse.",
        json!({
            "type": "object",
            "required": ["addr"],
            "properties": {
                "addr":      { "type": "integer" },
                "max_steps": { "type": "integer", "minimum": 1 }
            }
        }),
    );
    common(
        "run_until_any_pc",
        "Step whole instructions until PC matches any entry in `targets`, or `max_steps` elapse.",
        json!({
            "type": "object",
            "required": ["targets"],
            "properties": {
                "targets":   { "type": "array", "minItems": 1, "items": { "type": "integer" } },
                "max_steps": { "type": "integer", "minimum": 1 }
            }
        }),
    );
    common(
        "run_until_mem_change",
        "Step whole instructions until any watched byte in `addrs` changes, or `max_steps` elapse.",
        json!({
            "type": "object",
            "required": ["addrs"],
            "properties": {
                "addrs":     { "type": "array", "minItems": 1, "items": { "type": "integer" } },
                "max_steps": { "type": "integer", "minimum": 1 }
            }
        }),
    );
    registry.register(Box::new(InlineTool {
        name: "io_trace",
        description: "Run `frames` frames capturing every I/O port access (port-mapped \
                      Z80 / 6502 machines only); returns a per-port summary plus a sample \
                      of events.",
        schema: json!({
            "type": "object",
            "properties": {
                "frames": { "type": "integer", "minimum": 1, "maximum": 600, "default": 4 },
                "limit":  { "type": "integer", "minimum": 1, "maximum": 4096, "default": 256 }
            }
        }),
        run: run_io_trace,
    }));
}

/// (Amiga, NES) still shadows the generic one.
pub fn register_base_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    register_common_tools(registry);
    register_debug_tools(registry);
}

/// Register the memory-write watch verbs (`watch_memory_start`,
/// `watch_memory_clear`, `watch_memory_log`) as `ScriptStepTool` wrappers over
/// the shared [`ScriptStep`] arms (so MCP and `--script` run one body, via
/// each machine's [`MachineCore::watch_target`]).
///
/// Opt-in — call from a binary whose machine implements the memory-watch
/// surface of [`crate::watch::WatchTarget`] (the Spectrum + Amiga families
/// today). Not folded into [`register_base_tools`]: most cores have not wired
/// write-capture, and exposing a non-functional tool would mislead.
pub fn register_memory_watch_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    let mut common = |name, description, schema| {
        registry.register(Box::new(ScriptStepTool::<M, Q>::common(
            name,
            description,
            schema,
        )));
    };

    common(
        "watch_memory_start",
        "Begin recording memory writes inside `[addr, addr + len)`; replaces \
         any prior range and clears the log. Amiga records identify CPU, \
         blitter D-channel, and disk read-DMA sources.",
        json!({
            "type": "object",
            "required": ["addr", "len"],
            "properties": {
                "addr": { "type": "integer", "description": "Watch range start." },
                "len":  { "type": "integer", "minimum": 1, "description": "Range length in bytes." }
            }
        }),
    );
    common(
        "watch_memory_clear",
        "Stop watching memory writes and drop the captured log.",
        json!({ "type": "object", "additionalProperties": false }),
    );
    common(
        "watch_memory_log",
        "Fetch the captured memory-write log (most-recent `limit`, oldest \
         first). Source-aware machines report a typed `source` per entry and \
         can filter by source and inclusive CCK bounds.",
        json!({
            "type": "object",
            "properties": {
                "limit":  { "type": "integer", "minimum": 1, "default": 64 },
                "unique": { "type": "boolean", "default": false,
                            "description": "Deduplicate identical (pc, addr, value, source) tuples." },
                "source": { "type": "string", "enum": ["cpu", "blitter", "disk_dma"],
                            "description": "Return writes explicitly attributed to this hardware agent." },
                "cck_min": { "type": "integer", "minimum": 0,
                             "description": "Inclusive lower CCK bound; excludes unstamped writes." },
                "cck_max": { "type": "integer", "minimum": 0,
                             "description": "Inclusive upper CCK bound; excludes unstamped writes." }
            }
        }),
    );
}

/// Register the keyboard verbs (`press_key`, `type_string`) as
/// `ScriptStepTool` wrappers over the shared [`ScriptStep`] arms (so MCP and
/// `--script` run one body, via each machine's [`MachineCore::keyboard_target`]).
///
/// Opt-in — call from a binary whose machine implements
/// [`crate::keyboard::KeyboardTarget`] (any machine with a keyboard).
pub fn register_keyboard_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    let mut common = |name, description, schema| {
        registry.register(Box::new(ScriptStepTool::<M, Q>::common(
            name,
            description,
            schema,
        )));
    };

    common(
        "press_key",
        "Press one named key, hold it for `hold_frames` native frames (default \
         3), then release. Valid key names depend on the machine's layout.",
        json!({
            "type": "object",
            "required": ["key"],
            "properties": {
                "key":         { "type": "string", "description": "Named key for this machine's layout." },
                "hold_frames": { "type": "integer", "minimum": 1 }
            }
        }),
    );
    common(
        "press_keys",
        "Press several named keys as a chord — held together for `hold_frames`, \
         then released in reverse. For combos no single key covers: the Amiga's \
         Ctrl-Amiga-Amiga reset, the C64's RunStop+Restore, the Spectrum's \
         CapsShift compounds, any modifier+key.",
        json!({
            "type": "object",
            "required": ["keys"],
            "properties": {
                "keys": { "type": "array", "minItems": 1, "items": { "type": "string" },
                          "description": "Keys to hold together, in press order (modifiers first)." },
                "hold_frames": { "type": "integer", "minimum": 1 }
            }
        }),
    );
    common(
        "type_string",
        "Type a string through the keyboard with per-key hold/release timing. \
         Characters with no single keystroke on this machine are skipped.",
        json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text":          { "type": "string" },
                "hold_frames":   { "type": "integer", "minimum": 1 },
                "settle_frames": { "type": "integer", "minimum": 0,
                                   "description": "Extra frames after the last keystroke." }
            }
        }),
    );
}

/// Register the AY register-write watch verbs (`watch_ay_start`,
/// `watch_ay_clear`, `watch_ay_log`) as `ScriptStepTool` wrappers over the
/// shared [`ScriptStep`] arms.
///
/// Opt-in — call from a binary whose machine implements the AY-watch surface
/// of [`crate::watch::WatchTarget`] (the Spectrum family today; the wider AY
/// fleet — MSX, Oric, SVI-328, … — once their cores wire AY write-capture).
pub fn register_ay_watch_tools<M, Q>(registry: &mut ToolRegistry<HeadlessSession<M, Q>>)
where
    M: MachineCore + 'static,
    Q: SessionQueryProvider<M> + 'static,
{
    let mut common = |name, description, schema| {
        registry.register(Box::new(ScriptStepTool::<M, Q>::common(
            name,
            description,
            schema,
        )));
    };

    common(
        "watch_ay_start",
        "Begin recording every AY register write as (pc, register, value); \
         clears any prior log.",
        json!({ "type": "object", "additionalProperties": false }),
    );
    common(
        "watch_ay_clear",
        "Stop watching AY writes and drop the captured log.",
        json!({ "type": "object", "additionalProperties": false }),
    );
    common(
        "watch_ay_log",
        "Fetch the captured AY-write log (most-recent `limit`, oldest first).",
        json!({
            "type": "object",
            "properties": {
                "limit":  { "type": "integer", "minimum": 1, "default": 64 },
                "unique": { "type": "boolean", "default": false,
                            "description": "Deduplicate identical (pc, register, value) triples." }
            }
        }),
    );
}
