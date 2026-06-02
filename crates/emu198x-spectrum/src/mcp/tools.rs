//! MCP tool registrations for the Spectrum binary.
//!
//! One tool per `ScriptStep` variant (≈18 tools). Each tool's `call`
//! lifts the supplied JSON arguments into a `ScriptStep` (by injecting
//! the `action` discriminator and re-deserializing), dispatches it
//! through the same `execute_step` interceptor that script mode uses,
//! and returns the resulting `ScriptObservation` as a JSON-text content
//! block.
//!
//! Schemas are hand-written. The crate's existing JSON-round-trip tests
//! freeze the wire shape of each `ScriptStep` variant; if those tests
//! break, a tool's schema here probably also needs an update.

use emu198x_shell::{
    AyWriteEntry, DisasmInstruction, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore,
    MemoryWriteEntry, ScriptObservation, ScriptStep,
    mcp::{Tool, ToolError, ToolRegistry, ToolResponse},
    read_firmware_asset,
};
use format_sinclair_zx_spectrum_bas::tokenise;
use runtime_sinclair_zx_spectrum::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, SpectrumLiveAccess,
    SpectrumRuntimeKind, SpectrumSessionQueryProvider, autoload_basic_tape, load_basic_program,
};
use serde_json::{Value, json};

use crate::machine::{MachineKind, rom_root, variant_rom_bundle};
use crate::portable_snapshot::{is_portable_snapshot_path, parse_portable_snapshot_at};

/// Live-session context every Spectrum MCP tool dispatches against.
///
/// Family-level: the inner runtime is one of the SOLID-8 Spectrum
/// variants, chosen at boot time and swappable mid-session via the
/// `set_machine` tool.
pub type SpectrumSession = HeadlessSession<SpectrumRuntimeKind, SpectrumSessionQueryProvider>;

/// One MCP tool that maps directly onto a `ScriptStep` variant.
struct ScriptStepTool {
    /// Stable tool name; matches the variant's serde `action` tag.
    name: &'static str,
    /// Human-readable description shown by MCP clients.
    description: &'static str,
    /// JSON Schema for the tool's input arguments.
    schema: Value,
}

impl Tool<SpectrumSession> for ScriptStepTool {
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
        session: &mut SpectrumSession,
    ) -> Result<ToolResponse, ToolError> {
        let step = parse_step(self.name, arguments)?;
        let observation = mcp_execute_step(&step, session)?;
        let body = match observation {
            Some(obs) => serde_json::to_string(&obs).map_err(|err| {
                ToolError::Execution(format!("failed to serialize observation: {err}"))
            })?,
            None => String::from("null"),
        };
        Ok(ToolResponse::success_text(body))
    }
}

/// Family-MCP dispatch for one `ScriptStep`.
///
/// - `SetMachine`: rebuilds the inner runtime to the requested
///   variant. The session-side state (queued input, latest frame,
///   captured audio, last run result) is cleared via
///   [`HeadlessSession::reset`] so the new variant starts from a
///   clean session.
/// - `AutoloadTape` / `LoadBasicProgram`: 48K-only on the runtime
///   side today. We downcast through
///   [`SpectrumRuntimeKind::as_48k_mut`]; if the active variant is
///   not 48K we return [`ToolError::Execution`] with a clear message.
///   (Generalising these helpers to the 128K family is its own
///   commit on the runtime crate.)
/// - Everything else delegates to [`ScriptStep::execute_collect`],
///   which works generically over `MachineCore`.
fn mcp_execute_step(
    step: &ScriptStep,
    session: &mut SpectrumSession,
) -> Result<Option<ScriptObservation>, ToolError> {
    match step {
        ScriptStep::SetMachine { machine } => execute_set_machine(machine, session).map(Some),
        ScriptStep::QueryAy => execute_query_ay(session).map(Some),
        ScriptStep::QueryCpu => Ok(Some(execute_query_cpu(session))),
        ScriptStep::Step { instructions } => Ok(Some(execute_step(session, *instructions))),
        ScriptStep::RunUntilPc {
            addr,
            max_halfcycles,
        } => Ok(Some(execute_run_until_pc(session, *addr, *max_halfcycles))),
        ScriptStep::Disasm { addr, instructions } => {
            Ok(Some(execute_disasm(session, *addr, *instructions)))
        }
        ScriptStep::PortRead { port } => Ok(Some(execute_port_read(session, *port))),
        ScriptStep::PortWrite { port, value } => {
            execute_port_write(session, *port, *value);
            Ok(None)
        }
        ScriptStep::WatchAyStart => execute_watch_ay_start(session).map(Some),
        ScriptStep::WatchAyClear => Ok(Some(execute_watch_ay_clear(session))),
        ScriptStep::WatchAyLog { limit, unique } => {
            Ok(Some(execute_watch_ay_log(session, *limit, *unique)))
        }
        ScriptStep::PressKey { key, hold_frames } => {
            execute_press_key(session, key, *hold_frames).map(Some)
        }
        ScriptStep::TypeString {
            text,
            hold_frames,
            settle_frames,
        } => execute_type_string(session, text, *hold_frames, *settle_frames).map(Some),
        ScriptStep::AutoloadTape {
            slot,
            max_boot_frames,
        } => {
            let frames = if *max_boot_frames == 0 {
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES
            } else {
                *max_boot_frames
            };
            execute_autoload_tape(session, slot, frames).map(Some)
        }
        ScriptStep::LoadBasicProgram { path, run } => {
            execute_load_basic_program(session, path, *run).map(Some)
        }
        ScriptStep::MemoryRead { addr, len } => execute_memory_read(session, *addr, *len).map(Some),
        ScriptStep::PokeByte { addr, value } => {
            execute_poke_byte(session, *addr, *value)?;
            Ok(None)
        }
        ScriptStep::PokeWord { addr, value } => {
            execute_poke_word(session, *addr, *value)?;
            Ok(None)
        }
        ScriptStep::WatchMemoryStart { addr, len } => {
            execute_watch_memory_start(session, *addr, *len).map(Some)
        }
        ScriptStep::WatchMemoryClear => execute_watch_memory_clear(session).map(Some),
        ScriptStep::WatchMemoryLog { limit, unique } => {
            execute_watch_memory_log(session, *limit, *unique).map(Some)
        }
        ScriptStep::LoadSnapshot { path } if is_portable_snapshot_path(path) => {
            execute_load_portable_snapshot(session, path).map(|()| None)
        }
        other => other
            .execute_collect(session)
            .map_err(|err| ToolError::Execution(format!("{err}"))),
    }
}

/// MCP-side equivalent of
/// `crate::script::runner::execute_load_portable_snapshot`. Shares the
/// classifier + parser through [`crate::portable_snapshot`] and applies
/// the result through [`SpectrumLiveAccess::apply_snapshot`] so every
/// runtime kind in `SpectrumRuntimeKind` is reachable.
fn execute_load_portable_snapshot(
    session: &mut SpectrumSession,
    path: &std::path::Path,
) -> Result<(), ToolError> {
    if session.is_recording() {
        return Err(ToolError::Execution(format!(
            "cannot load portable snapshot {} while a video recording is in flight; \
             stop the recording first",
            path.display()
        )));
    }
    let snapshot = parse_portable_snapshot_at(path)
        .map_err(|err| ToolError::Execution(format!("{err}")))?;
    SpectrumLiveAccess::apply_snapshot(session.machine_mut(), &snapshot);
    Ok(())
}

/// Validate that a u32 address fits in the Z80's u16 space.
fn addr_to_u16(addr: u32, label: &str) -> Result<u16, ToolError> {
    u16::try_from(addr).map_err(|_| {
        ToolError::InvalidArguments(format!(
            "{label}: address ${addr:08X} is outside the Z80 0000-FFFF address space"
        ))
    })
}

/// Cap a requested read length so the response stays bounded.
const MEMORY_READ_MAX: u32 = 256;

fn execute_memory_read(
    session: &mut SpectrumSession,
    addr: u32,
    len: u32,
) -> Result<ScriptObservation, ToolError> {
    if len == 0 {
        return Err(ToolError::InvalidArguments(
            "memory_read: `len` must be at least 1".to_owned(),
        ));
    }
    let start = addr_to_u16(addr, "memory_read")?;
    let capped = len.min(MEMORY_READ_MAX);
    let machine = session.machine();
    let mut bytes = Vec::with_capacity(capped as usize);
    for offset in 0..capped {
        let a = start.wrapping_add(offset as u16);
        bytes.push(machine.read_byte(a));
    }
    Ok(ScriptObservation::MemoryRead {
        addr,
        len: capped,
        bytes,
    })
}

fn execute_poke_byte(session: &mut SpectrumSession, addr: u32, value: u8) -> Result<(), ToolError> {
    let a = addr_to_u16(addr, "poke_byte")?;
    session.machine_mut().write_byte(a, value);
    Ok(())
}

fn execute_poke_word(
    session: &mut SpectrumSession,
    addr: u32,
    value: u16,
) -> Result<(), ToolError> {
    // Z80 stores 16-bit values little-endian: low byte at addr+0,
    // high byte at addr+1. The high byte may wrap around at $FFFF —
    // mirror that behaviour rather than rejecting the call.
    let low_addr = addr_to_u16(addr, "poke_word")?;
    let high_addr = low_addr.wrapping_add(1);
    let machine = session.machine_mut();
    machine.write_byte(low_addr, (value & 0xFF) as u8);
    machine.write_byte(high_addr, (value >> 8) as u8);
    Ok(())
}

fn execute_watch_memory_start(
    session: &mut SpectrumSession,
    addr: u32,
    len: u32,
) -> Result<ScriptObservation, ToolError> {
    if len == 0 {
        return Err(ToolError::InvalidArguments(
            "watch_memory_start: `len` must be at least 1".to_owned(),
        ));
    }
    let start = addr_to_u16(addr, "watch_memory_start")?;
    let len_u16 = u16::try_from(len).map_err(|_| {
        ToolError::InvalidArguments(format!(
            "watch_memory_start: `len` {len} exceeds the Z80 64 KiB address space"
        ))
    })?;
    session
        .machine_mut()
        .start_memory_write_watch(start, len_u16)
        .map_err(|err| ToolError::Execution(format!("watch_memory_start: {err}")))?;
    Ok(ScriptObservation::WatchMemoryStart {
        addr,
        len,
        capacity: common_sinclair_zx_spectrum::DEFAULT_WATCH_CAP as u32,
    })
}

fn execute_watch_memory_clear(
    session: &mut SpectrumSession,
) -> Result<ScriptObservation, ToolError> {
    let captured = session
        .machine()
        .memory_write_watch_records()
        .map(|r| r.len() as u32)
        .unwrap_or(0);
    let had_watch = session.machine().memory_write_watch_records().is_some();
    session.machine_mut().stop_memory_write_watch();
    Ok(ScriptObservation::WatchMemoryClear {
        had_watch,
        captured,
    })
}

fn execute_watch_memory_log(
    session: &mut SpectrumSession,
    limit: Option<u32>,
    unique: bool,
) -> Result<ScriptObservation, ToolError> {
    let limit = limit.unwrap_or(64) as usize;
    let machine = session.machine();
    let range = machine.memory_write_watch_range();
    let Some(records) = machine.memory_write_watch_records() else {
        return Ok(ScriptObservation::WatchMemoryLog {
            addr: None,
            len: None,
            total_writes: 0,
            returned: 0,
            entries: Vec::new(),
        });
    };
    let total_writes = records.len() as u32;
    let mut filtered: Vec<&common_sinclair_zx_spectrum::MemoryWriteRecord> =
        records.iter().collect();
    if unique {
        let mut seen = std::collections::HashSet::new();
        filtered.retain(|r| seen.insert((r.pc, r.addr, r.value)));
    }
    // Take the most-recent `limit` entries, then restore oldest-first
    // order for readability.
    let total_filtered = filtered.len();
    let start = total_filtered.saturating_sub(limit);
    let entries: Vec<MemoryWriteEntry> = filtered[start..]
        .iter()
        .map(|r| MemoryWriteEntry {
            pc: u32::from(r.pc),
            addr: u32::from(r.addr),
            value: u32::from(r.value),
        })
        .collect();
    Ok(ScriptObservation::WatchMemoryLog {
        addr: range.map(|(lo, _)| u32::from(lo)),
        len: range.map(|(_, len)| u32::from(len)),
        total_writes,
        returned: entries.len() as u32,
        entries,
    })
}

fn execute_set_machine(
    requested: &str,
    session: &mut SpectrumSession,
) -> Result<ScriptObservation, ToolError> {
    let kind = MachineKind::from_script_id(requested).ok_or_else(|| {
        ToolError::InvalidArguments(format!(
            "set_machine: unknown machine id `{requested}`; expected one of \
             spectrum_16k, spectrum_48k, spectrum_plus, spectrum_128k, \
             spectrum_plus2, spectrum_plus2a, spectrum_plus2b, spectrum_plus3"
        ))
    })?;
    let model = kind_to_model(kind);
    let rom_root_dir = rom_root().ok_or_else(|| {
        ToolError::Execution(
            "set_machine: $HOME is unset; cannot locate ROM bundle root \
             (~/.emu198x/roms)"
                .to_owned(),
        )
    })?;

    // Two-pass firmware load: read all ROM bytes into an owned vec,
    // then borrow them into the `FirmwareSet`. Mirrors the pattern
    // used by `script::runner::boot_eager_variant`.
    let bundle = variant_rom_bundle(kind, &rom_root_dir);
    let mut rom_bytes: Vec<(String, Vec<u8>)> = Vec::with_capacity(bundle.len());
    for (id, path) in bundle {
        if !path.is_file() {
            return Err(ToolError::Execution(format!(
                "set_machine: ROM not found at {}",
                path.display()
            )));
        }
        let loaded = read_firmware_asset(&path).map_err(|err| {
            ToolError::Execution(format!(
                "set_machine: failed to read {}: {err}",
                path.display()
            ))
        })?;
        rom_bytes.push((id.to_string(), loaded.bytes.to_vec()));
    }
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &rom_bytes {
        firmware.push(FirmwareImage::new(id.clone(), bytes));
    }
    let new_runtime = SpectrumRuntimeKind::from_firmware(model, &firmware)
        .map_err(|err| ToolError::Execution(format!("set_machine: build runtime: {err}")))?;
    let profile = new_runtime.profile().clone();

    // Swap the inner machine + clear session-side state, and re-pace
    // the session to the new variant's frame budget so `run_frames`
    // emits one native frame per call.
    let new_frame_ticks = u64::from(new_runtime.frame_halfcycles());
    *session.machine_mut() = new_runtime;
    session.set_native_frame_ticks(new_frame_ticks);
    session
        .reset(emu198x_shell::ResetKind::Hard)
        .map_err(|err| ToolError::Execution(format!("set_machine: clear session: {err}")))?;

    Ok(ScriptObservation::SetMachine {
        machine: requested.to_owned(),
        profile_id: profile.profile_id.as_str().to_owned(),
        display_name: profile.display_name.to_string(),
    })
}

fn execute_query_ay(session: &mut SpectrumSession) -> Result<ScriptObservation, ToolError> {
    // Look up the two low-level AY paths through the existing
    // session query provider; on AY-bearing variants both resolve,
    // on 48K-class variants `spectrum.ay.registers` is not in
    // `variant_query_paths()` and the provider returns `Ok(None)` →
    // QueryError::UnknownPath. We surface that as a clear "active
    // variant has no AY" error rather than a generic UnknownPath.
    let regs = session
        .query("spectrum.ay.registers")
        .map_err(|err| ay_unsupported_error(&err))?;
    let raw: Vec<u8> = serde_json::from_value(regs.value).map_err(|err| {
        ToolError::Execution(format!(
            "query_ay: malformed spectrum.ay.registers value: {err}"
        ))
    })?;
    if raw.len() != 16 {
        return Err(ToolError::Execution(format!(
            "query_ay: expected 16 AY registers, got {}",
            raw.len()
        )));
    }
    let selected = session
        .query("spectrum.ay.selected_register")
        .map_err(|err| ay_unsupported_error(&err))?;
    let selected_register: u8 = serde_json::from_value(selected.value).map_err(|err| {
        ToolError::Execution(format!(
            "query_ay: malformed spectrum.ay.selected_register value: {err}"
        ))
    })?;

    let tone_period_a = u16::from(raw[0]) | (u16::from(raw[1] & 0x0F) << 8);
    let tone_period_b = u16::from(raw[2]) | (u16::from(raw[3] & 0x0F) << 8);
    let tone_period_c = u16::from(raw[4]) | (u16::from(raw[5] & 0x0F) << 8);
    let envelope_period = u16::from(raw[11]) | (u16::from(raw[12]) << 8);

    Ok(ScriptObservation::QueryAy {
        selected_register,
        raw: raw.clone(),
        tone_period_a,
        tone_period_b,
        tone_period_c,
        noise_period: raw[6] & 0x1F,
        mixer: raw[7],
        amplitude_a: raw[8] & 0x1F,
        amplitude_b: raw[9] & 0x1F,
        amplitude_c: raw[10] & 0x1F,
        envelope_period,
        envelope_shape: raw[13] & 0x0F,
    })
}

fn execute_query_cpu(session: &SpectrumSession) -> ScriptObservation {
    let regs = session.machine().z80_registers();
    let halt = session.machine().z80_halted();
    let f = regs.f();
    ScriptObservation::QueryCpu {
        pc: regs.pc,
        sp: regs.sp,
        i: regs.i,
        r: regs.r,
        af: regs.af,
        a: regs.a(),
        f,
        bc: regs.bc,
        b: regs.b(),
        c: regs.c(),
        de: regs.de,
        d: regs.d(),
        e: regs.e(),
        hl: regs.hl,
        h: regs.h(),
        l: regs.l(),
        af_alt: regs.af_alt,
        bc_alt: regs.bc_alt,
        de_alt: regs.de_alt,
        hl_alt: regs.hl_alt,
        ix: regs.ix,
        iy: regs.iy,
        im: regs.im,
        iff1: regs.iff1,
        iff2: regs.iff2,
        flag_s: f & 0x80 != 0,
        flag_z: f & 0x40 != 0,
        flag_5: f & 0x20 != 0,
        flag_h: f & 0x10 != 0,
        flag_3: f & 0x08 != 0,
        flag_pv: f & 0x04 != 0,
        flag_n: f & 0x02 != 0,
        flag_c: f & 0x01 != 0,
        halt,
    }
}

/// Default half-cycle budget for `run_until_pc` when the caller leaves
/// it unset. Roughly ten 48K frames (69888 hc/frame × 10) — long
/// enough to cover ROM-routine probes, short enough that a runaway
/// loop returns control in a fraction of a second.
const DEFAULT_RUN_UNTIL_PC_BUDGET: u32 = 700_000;
const MAX_RUN_UNTIL_PC_BUDGET: u32 = 50_000_000;
const DEFAULT_DISASM_COUNT: u32 = 16;
const MAX_DISASM_COUNT: u32 = 64;
const MAX_STEP_INSTRUCTIONS: u32 = 16_384;

fn execute_step(session: &mut SpectrumSession, instructions: Option<u32>) -> ScriptObservation {
    let n = instructions.unwrap_or(1).min(MAX_STEP_INSTRUCTIONS);
    let halfcycles = session.machine_mut().step_instructions(n);
    let regs = session.machine().z80_registers();
    let halt = session.machine().z80_halted();
    ScriptObservation::Step {
        instructions: n,
        halfcycles,
        pc: regs.pc,
        halt,
    }
}

fn execute_run_until_pc(
    session: &mut SpectrumSession,
    addr: u16,
    max_halfcycles: Option<u32>,
) -> ScriptObservation {
    let budget = max_halfcycles
        .unwrap_or(DEFAULT_RUN_UNTIL_PC_BUDGET)
        .min(MAX_RUN_UNTIL_PC_BUDGET);
    let (reached, halfcycles, instructions) = session.machine_mut().run_until_pc(addr, budget);
    let pc = session.machine().z80_registers().pc;
    ScriptObservation::RunUntilPc {
        reached,
        pc,
        halfcycles,
        instructions,
    }
}

fn execute_disasm(
    session: &SpectrumSession,
    addr: u16,
    instructions: Option<u32>,
) -> ScriptObservation {
    let count = instructions
        .unwrap_or(DEFAULT_DISASM_COUNT)
        .min(MAX_DISASM_COUNT);
    let machine = session.machine();
    let read = |a: u16| machine.read_byte(a);

    let mut decoded = Vec::with_capacity(count as usize);
    let mut cursor = addr;
    for _ in 0..count {
        let (mnemonic, len) = zilog_z80::disassemble(cursor, read);
        let mut raw = Vec::with_capacity(len as usize);
        for off in 0..len {
            raw.push(machine.read_byte(cursor.wrapping_add(u16::from(off))));
        }
        decoded.push(DisasmInstruction {
            addr: u32::from(cursor),
            bytes: len,
            raw,
            mnemonic,
        });
        cursor = cursor.wrapping_add(u16::from(len));
    }

    ScriptObservation::Disasm {
        addr: u32::from(addr),
        count,
        instructions: decoded,
    }
}

fn execute_port_read(session: &mut SpectrumSession, port: u16) -> ScriptObservation {
    let value = session.machine_mut().port_read(port);
    ScriptObservation::PortRead { port, value }
}

fn execute_port_write(session: &mut SpectrumSession, port: u16, value: u8) {
    session.machine_mut().port_write(port, value);
}

fn execute_watch_ay_start(session: &mut SpectrumSession) -> Result<ScriptObservation, ToolError> {
    session
        .machine_mut()
        .start_ay_write_watch()
        .map_err(|err| ToolError::Execution(format!("watch_ay_start: {err}")))?;
    Ok(ScriptObservation::WatchAyStart {
        capacity: common_sinclair_zx_spectrum::DEFAULT_AY_WATCH_CAP as u32,
    })
}

fn execute_watch_ay_clear(session: &mut SpectrumSession) -> ScriptObservation {
    let machine = session.machine();
    let captured = machine
        .ay_write_watch_records()
        .map(|r| r.len() as u32)
        .unwrap_or(0);
    let had_watch = machine.ay_write_watch_records().is_some();
    session.machine_mut().stop_ay_write_watch();
    ScriptObservation::WatchAyClear {
        had_watch,
        captured,
    }
}

/// Default frames to hold a key down for `press_key`. Three frames
/// of a 50 Hz PAL refresh is 60 ms — well above the ROM keyboard
/// scan interval (one frame) but short enough that a script doesn't
/// stall noticeably.
const DEFAULT_PRESS_KEY_HOLD_FRAMES: u32 = 3;
const MAX_PRESS_KEY_HOLD_FRAMES: u32 = 600;

fn execute_press_key(
    session: &mut SpectrumSession,
    key: &str,
    hold_frames: Option<u32>,
) -> Result<ScriptObservation, ToolError> {
    // Validate the key name through SpectrumKey::from_name so a
    // typo yields a clean error rather than silently doing nothing.
    if common_sinclair_zx_spectrum::keyboard::SpectrumKey::from_name(key).is_none() {
        return Err(ToolError::InvalidArguments(format!(
            "press_key: unknown key `{key}` — valid names: A-Z, 0-9, Space, \
             Enter, CapsShift, SymbolShift (case-insensitive)"
        )));
    }

    let hold = hold_frames
        .unwrap_or(DEFAULT_PRESS_KEY_HOLD_FRAMES)
        .clamp(1, MAX_PRESS_KEY_HOLD_FRAMES);

    // Press.
    session.queue_input(emu198x_shell::InputEvent::Key {
        name: key.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames(hold)
        .map_err(|err| ToolError::Execution(format!("press_key: hold run failed: {err}")))?;

    // Release.
    session.queue_input(emu198x_shell::InputEvent::Key {
        name: key.to_owned().into(),
        pressed: false,
    });
    // One settle frame so the released state is visible to the next
    // step (otherwise the next run_frames would start with the key
    // still drawn as pressed).
    session
        .run_frames(1)
        .map_err(|err| ToolError::Execution(format!("press_key: settle run failed: {err}")))?;

    Ok(ScriptObservation::PressKey {
        key: key.to_owned(),
        hold_frames: hold,
        reached: session.time(),
    })
}

const DEFAULT_TYPE_STRING_SETTLE_FRAMES: u32 = 10;

fn execute_type_string(
    session: &mut SpectrumSession,
    text: &str,
    hold_frames: Option<u32>,
    settle_frames: Option<u32>,
) -> Result<ScriptObservation, ToolError> {
    let hold = hold_frames
        .unwrap_or(DEFAULT_PRESS_KEY_HOLD_FRAMES)
        .clamp(1, MAX_PRESS_KEY_HOLD_FRAMES);
    let settle = settle_frames.unwrap_or(DEFAULT_TYPE_STRING_SETTLE_FRAMES);
    let mut chars_typed: u32 = 0;

    let mut prev_key: Option<String> = None;

    for ch in text.chars() {
        let (key_name, needs_caps_shift) = match ch {
            'a'..='z' => (ch.to_ascii_uppercase().to_string(), false),
            'A'..='Z' => (ch.to_string(), true),
            '0'..='9' => (ch.to_string(), false),
            ' ' => ("Space".to_owned(), false),
            '\n' => ("Enter".to_owned(), false),
            _ => continue,
        };

        if common_sinclair_zx_spectrum::keyboard::SpectrumKey::from_name(&key_name).is_none() {
            continue;
        }

        // Extra settle before a repeated key so the ROM keyboard
        // scan sees the release before the next press.
        if prev_key.as_deref() == Some(&key_name) {
            session.run_frames(3).map_err(|err| {
                ToolError::Execution(format!("type_string: repeat settle failed: {err}"))
            })?;
        }

        // Press CapsShift if needed for uppercase.
        if needs_caps_shift {
            session.queue_input(emu198x_shell::InputEvent::Key {
                name: "CapsShift".to_owned().into(),
                pressed: true,
            });
        }

        // Press the key.
        session.queue_input(emu198x_shell::InputEvent::Key {
            name: key_name.clone().into(),
            pressed: true,
        });
        session
            .run_frames(hold)
            .map_err(|err| ToolError::Execution(format!("type_string: hold failed: {err}")))?;

        // Release the key.
        session.queue_input(emu198x_shell::InputEvent::Key {
            name: key_name.clone().into(),
            pressed: false,
        });
        if needs_caps_shift {
            session.queue_input(emu198x_shell::InputEvent::Key {
                name: "CapsShift".to_owned().into(),
                pressed: false,
            });
        }

        // Settle frame between keystrokes.
        session
            .run_frames(1)
            .map_err(|err| ToolError::Execution(format!("type_string: settle failed: {err}")))?;

        prev_key = Some(key_name);
        chars_typed += 1;
    }

    // Extra settle after the last key.
    if settle > 0 {
        session.run_frames(settle).map_err(|err| {
            ToolError::Execution(format!("type_string: final settle failed: {err}"))
        })?;
    }

    Ok(ScriptObservation::TypeString {
        chars_typed,
        reached: session.time(),
    })
}

fn execute_watch_ay_log(
    session: &SpectrumSession,
    limit: Option<u32>,
    unique: bool,
) -> ScriptObservation {
    let limit = limit.unwrap_or(64) as usize;
    let machine = session.machine();
    let Some(records) = machine.ay_write_watch_records() else {
        return ScriptObservation::WatchAyLog {
            total_writes: 0,
            returned: 0,
            entries: Vec::new(),
        };
    };
    let total_writes = records.len() as u32;
    let mut filtered: Vec<&common_sinclair_zx_spectrum::AyWriteRecord> = records.iter().collect();
    if unique {
        let mut seen = std::collections::HashSet::new();
        filtered.retain(|r| seen.insert((r.pc, r.register, r.value)));
    }
    let total_filtered = filtered.len();
    let start = total_filtered.saturating_sub(limit);
    let entries: Vec<AyWriteEntry> = filtered[start..]
        .iter()
        .map(|r| AyWriteEntry {
            pc: u32::from(r.pc),
            register: r.register,
            value: r.value,
        })
        .collect();
    ScriptObservation::WatchAyLog {
        total_writes,
        returned: entries.len() as u32,
        entries,
    }
}

fn ay_unsupported_error(err: &emu198x_shell::QueryError) -> ToolError {
    ToolError::Execution(format!(
        "query_ay: active Spectrum variant does not have an AY-3-8912 chip \
         (only 128K, +2, +2A, +2B, +3, Pentagon, Scorpion, and Timex TC2068 / \
         TS2068 expose AY state). Switch to one of those variants via the \
         `set_machine` tool first. Underlying error: {err}"
    ))
}

fn execute_autoload_tape(
    session: &mut SpectrumSession,
    slot: &str,
    max_boot_frames: u32,
) -> Result<ScriptObservation, ToolError> {
    let result = autoload_basic_tape(session, slot, max_boot_frames)
        .map_err(|err| ToolError::Execution(format!("autoload_tape: {err}")))?;
    Ok(ScriptObservation::AutoloadTape {
        slot: result.slot,
        boot_frames: result.boot.frames,
    })
}

fn execute_load_basic_program(
    session: &mut SpectrumSession,
    path: &std::path::Path,
    run: bool,
) -> Result<ScriptObservation, ToolError> {
    let source = std::fs::read_to_string(path).map_err(|err| {
        ToolError::Execution(format!(
            "load_basic_program: failed to read {}: {err}",
            path.display()
        ))
    })?;
    let program = tokenise(&source).map_err(|err| {
        ToolError::Execution(format!(
            "load_basic_program: failed to tokenise {}: {err}",
            path.display()
        ))
    })?;
    let result = load_basic_program(session, &program, run, DEFAULT_BASIC_LOADER_BOOT_FRAMES)
        .map_err(|err| {
            ToolError::Execution(format!(
                "load_basic_program: BASIC loader failed for {}: {err}",
                path.display()
            ))
        })?;
    Ok(ScriptObservation::LoadBasicProgram {
        program_bytes: result.program_bytes,
        ran: result.ran,
    })
}

fn kind_to_model(kind: MachineKind) -> runtime_sinclair_zx_spectrum::Model {
    use runtime_sinclair_zx_spectrum::Model;
    match kind {
        MachineKind::Spectrum16K => Model::Spectrum16KPal,
        MachineKind::Spectrum48K => Model::Spectrum48KPal,
        MachineKind::SpectrumPlus => Model::SpectrumPlus,
        MachineKind::Spectrum128K => Model::Spectrum128KPal,
        MachineKind::SpectrumPlus2 => Model::SpectrumPlus2,
        MachineKind::SpectrumPlus2A => Model::SpectrumPlus2A,
        MachineKind::SpectrumPlus2B => Model::SpectrumPlus2B,
        MachineKind::SpectrumPlus3 => Model::SpectrumPlus3,
        MachineKind::Pentagon128 => Model::Pentagon128,
        MachineKind::ScorpionZS256 => Model::ScorpionZS256,
        MachineKind::TimexTC2048 => Model::TimexTC2048,
        MachineKind::TimexTC2068 => Model::TimexTC2068,
        MachineKind::TimexTS2068 => Model::TimexTS2068,
    }
}

/// Re-deserializes a `ScriptStep` by injecting the `action` tag into
/// the supplied arguments object. Mirrors the shell crate's serde
/// shape, so any field rename / addition shows up here as a parse
/// error rather than a silent shape mismatch.
fn parse_step(action: &str, arguments: Value) -> Result<ScriptStep, ToolError> {
    let mut object = match arguments {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => {
            return Err(ToolError::InvalidArguments(
                "arguments must be a JSON object".to_owned(),
            ));
        }
    };
    object.insert("action".to_owned(), Value::String(action.to_owned()));
    serde_json::from_value(Value::Object(object)).map_err(|err| {
        ToolError::InvalidArguments(format!("could not parse {action} arguments: {err}"))
    })
}

/// Registers every Spectrum tool on the supplied registry. Order is the
/// order shown by `tools/list`.
pub fn register_all(registry: &mut ToolRegistry<SpectrumSession>) {
    let object = || json!({"type": "object"});
    let string_field = || json!({"type": "string"});
    let integer_field = || json!({"type": "integer", "minimum": 0});
    let boolean_field = || json!({"type": "boolean"});

    let media_kind = json!({
        "type": "string",
        "enum": ["tape", "disk", "cartridge", "optical", "snapshot", "program"],
    });
    let media_transport = json!({
        "type": "string",
        "enum": ["start", "stop"],
    });

    registry.register(Box::new(ScriptStepTool {
        name: "load_media",
        description: "Load one media image into a named slot (tape, disk, cartridge, etc.).",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": string_field(),
                "kind": media_kind,
                "path": string_field(),
            },
            "required": ["slot", "kind", "path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "media_transport",
        description: "Start or stop media transport on the named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": string_field(),
                "transport": media_transport,
            },
            "required": ["slot", "transport"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "input",
        description: "Queue host input events (key presses / releases) for the next run step.",
        schema: json!({
            "type": "object",
            "properties": {
                "events": {
                    "type": "array",
                    "items": object(),
                },
            },
            "required": ["events"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "run_frames",
        description: "Run the machine for one number of native video frames.",
        schema: json!({
            "type": "object",
            "properties": {"frames": integer_field()},
            "required": ["frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_boot",
        description: "Run frames until the machine reports `boot.detected = true`.",
        schema: json!({
            "type": "object",
            "properties": {"max_frames": integer_field()},
            "required": ["max_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_contains",
        description: "Run frames until one text-bearing query contains the requested substring.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "needle": string_field(),
                "max_frames": integer_field(),
            },
            "required": ["path", "needle", "max_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "wait_for_query_bool",
        description: "Run frames until one boolean query path reaches the requested value.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "value": boolean_field(),
                "max_frames": integer_field(),
            },
            "required": ["path", "value", "max_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query",
        description: "Resolve one shared query path against the live session.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query_paths",
        description: "List supported query paths, optionally filtered by prefix.",
        schema: json!({
            "type": "object",
            "properties": {
                "prefix": {"type": ["string", "null"]},
            },
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "load_snapshot",
        description: "Restore a snapshot file into the live machine. \
            Accepts the runtime's own postcard save state, plus portable \
            .sna / .z80 snapshots (the format is picked from the file \
            extension). .zip archives wrapping a single .sna or .z80 \
            are auto-extracted.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "save_snapshot",
        description: "Save the current machine snapshot to disk.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "save_screenshot",
        description: "Save the latest emitted frame as a PNG file.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "save_audio_capture",
        description: "Legacy. Save the entire session capture buffer as WAV. Prefer start_audio_recording / stop_audio_recording for new scripts — the start/stop pair captures a clean window bounded by script steps. Without `reset_after`, this dumps everything since session start including silence from before the chapter began.",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "reset_after": boolean_field(),
            },
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "clear_audio_capture",
        description: "Drop the session capture buffer without writing it to disk. Pair with save_audio_capture when you want save + buffer-reset in two explicit steps rather than the `reset_after` boolean. No effect on the start_audio_recording / stop_audio_recording path — that uses its own per-recording offset.",
        schema: json!({
            "type": "object",
            "properties": {},
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "set_machine",
        description: "Switch the live machine to the named variant (currently errors with `not yet supported`).",
        schema: json!({
            "type": "object",
            "properties": {"machine": string_field()},
            "required": ["machine"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "autoload_tape",
        description: "Wait for boot, type LOAD \"\", and start tape transport on the named slot.",
        schema: json!({
            "type": "object",
            "properties": {
                "slot": string_field(),
                "max_boot_frames": integer_field(),
            },
            "required": ["slot", "max_boot_frames"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "load_basic_program",
        description: "Tokenise a plain-text .bas file and install it as the live BASIC program (optionally RUN it).",
        schema: json!({
            "type": "object",
            "properties": {
                "path": string_field(),
                "run": boolean_field(),
            },
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "start_video_recording",
        description: "Begin recording the live framebuffer + audio to one MP4 file.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "stop_video_recording",
        description: "Finalise the in-flight video recording and return the summary.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "reset",
        description: "Reset the running machine. `kind: hard` is a power-cycle equivalent; `kind: soft` is a machine-local soft reset (today both behave identically on Spectrum). Clears queued input, captured frame, captured audio. Rejected while a video recording is active.",
        schema: json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["hard", "soft"],
                },
            },
            "required": ["kind"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "start_audio_recording",
        description: "Begin recording emitted audio to a 16-bit PCM WAV file. Mirrors start_video_recording for audio-only capture: subsequent run_frames tee audio into the session's buffer; the WAV is written when stop_audio_recording is called. Prefer this over save_audio_capture when the recording window is bounded by script steps.",
        schema: json!({
            "type": "object",
            "properties": {"path": string_field()},
            "required": ["path"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "stop_audio_recording",
        description: "Finalise the in-flight audio recording. Slices the audio buffer from the start_audio_recording offset to the current end, encodes 16-bit PCM WAV, and writes it to disk.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query_ay",
        description: "Query the AY-3-8912 sound chip's full register state in one call. Returns the 16 raw registers plus decoded tone periods (A/B/C), noise period, mixer, amplitudes, envelope period, and envelope shape. Errors when the active variant has no AY (16K / 48K / Spectrum+); call set_machine first to switch to a 128K-class variant.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "query_cpu",
        description: "Read every Z80 register in one call: PC, SP, I, R, the main bank (AF/BC/DE/HL + a/f/b/c/d/e/h/l), the alternate bank (AF'/BC'/DE'/HL'), index registers (IX/IY), interrupt state (IM/IFF1/IFF2), the decoded F flags (S/Z/5/H/3/P-V/N/C), and the halt pin.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "step",
        description: "Single-step the Z80. Runs cycles until one instruction completes (or `instructions` instructions, default 1, max 16384). Returns the post-step PC, halt state, and total half-cycles consumed.",
        schema: json!({
            "type": "object",
            "properties": {
                "instructions": integer_field(),
            },
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "run_until_pc",
        description: "Run the Z80 until PC reaches `addr` at an instruction boundary, or `max_halfcycles` master-clock half-cycles elapse (default 700000 ≈ ten 48K frames, max 50000000). Useful for 'run to here' debugging.",
        schema: json!({
            "type": "object",
            "properties": {
                "addr": integer_field(),
                "max_halfcycles": integer_field(),
            },
            "required": ["addr"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "disasm",
        description: "Disassemble `instructions` Z80 opcodes starting at `addr` (default 16, max 64). Reads through the CPU memory bus so paging is honoured. Returns the mnemonic, raw bytes, and length of each instruction.",
        schema: json!({
            "type": "object",
            "properties": {
                "addr": integer_field(),
                "instructions": integer_field(),
            },
            "required": ["addr"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "port_read",
        description: "Read one Z80 I/O port through the bus-level handler. Same value an IN A,(C) would observe (ULA $FE, Kempston $1F, AY $FFFD, …) without driving the CPU through the synthetic instruction.",
        schema: json!({
            "type": "object",
            "properties": {
                "port": integer_field(),
            },
            "required": ["port"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "port_write",
        description: "Write one Z80 I/O port through the bus-level handler. Side-effects mirror OUT (C),A — border colour ($FE bits 0-2), beeper (bit 4), 128K paging ($7FFD), AY register select ($FFFD) and data ($BFFD). Silent.",
        schema: json!({
            "type": "object",
            "properties": {
                "port": integer_field(),
                "value": integer_field(),
            },
            "required": ["port", "value"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "watch_ay_start",
        description: "Begin recording every OUT ($BFFD), data — the Z80 → AY data port — capturing (pc, register, value). Curriculum-focused: lets a script show how a music driver or sound-effect routine programs the AY across a frame/scene/bar. Errors when the active variant has no AY (16K / 48K / Spectrum+ / TC2048).",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "watch_ay_clear",
        description: "Stop the AY register tracer and drop the captured log. Reports how many records were held at the moment of clear.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "watch_ay_log",
        description: "Fetch the captured AY writes. Returns up to `limit` most-recent entries (default 64), oldest-first. Set `unique = true` to deduplicate by (pc, register, value) before applying the limit.",
        schema: json!({
            "type": "object",
            "properties": {
                "limit": integer_field(),
                "unique": boolean_field(),
            },
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "press_key",
        description: "Press a single named Spectrum key, hold for `hold_frames` native frames (default 3), then release. One step replaces the press / run_frames / release dance. Valid key names: A-Z, 0-9, Space, Enter, CapsShift, SymbolShift (case-insensitive).",
        schema: json!({
            "type": "object",
            "properties": {
                "key": string_field(),
                "hold_frames": integer_field(),
            },
            "required": ["key"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "type_string",
        description: "Type a string of characters with proper per-key hold/release timing. Each character is pressed individually with `hold_frames` (default 3) hold time and a 1-frame settle gap between keystrokes. Uppercase letters automatically use CapsShift. Newlines press Enter. `settle_frames` (default 10) extra frames run after the last keystroke. Much faster than calling press_key per character.",
        schema: json!({
            "type": "object",
            "properties": {
                "text": string_field(),
                "hold_frames": integer_field(),
                "settle_frames": integer_field(),
            },
            "required": ["text"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "memory_read",
        description: "Read a contiguous span of CPU-visible memory (Z80 address space, 0x0000-0xFFFF). Returns raw bytes in memory order. `len` is capped at 256 bytes per call.",
        schema: json!({
            "type": "object",
            "properties": {
                "addr": integer_field(),
                "len": integer_field(),
            },
            "required": ["addr", "len"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "poke_byte",
        description: "Write one byte to CPU-visible memory at the given address. Silent — no observation is emitted.",
        schema: json!({
            "type": "object",
            "properties": {
                "addr": integer_field(),
                "value": integer_field(),
            },
            "required": ["addr", "value"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "poke_word",
        description: "Write a 16-bit value to CPU-visible memory, little-endian (low byte at addr+0, high byte at addr+1). Silent.",
        schema: json!({
            "type": "object",
            "properties": {
                "addr": integer_field(),
                "value": integer_field(),
            },
            "required": ["addr", "value"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "watch_memory_start",
        description: "Begin recording every Z80 CPU write that lands inside [addr, addr+len). Each capture stores (pc, addr, value). Replaces any prior watch and clears the log. Capture cap is 8192 entries; further writes are dropped silently. Pair with watch_memory_log / watch_memory_clear.",
        schema: json!({
            "type": "object",
            "properties": {
                "addr": integer_field(),
                "len": integer_field(),
            },
            "required": ["addr", "len"],
        }),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "watch_memory_clear",
        description: "Stop watching CPU writes and drop the captured log. Reports how many records were held at the moment of clear.",
        schema: json!({"type": "object"}),
    }));

    registry.register(Box::new(ScriptStepTool {
        name: "watch_memory_log",
        description: "Fetch the captured CPU writes. Returns up to `limit` most-recent entries (default 64), in oldest-first order. Set `unique = true` to deduplicate by (pc, addr, value) before applying the limit.",
        schema: json!({
            "type": "object",
            "properties": {
                "limit": integer_field(),
                "unique": boolean_field(),
            },
        }),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_step_round_trips_run_frames_arguments() {
        let step = parse_step("run_frames", json!({"frames": 25})).expect("valid step");
        assert_eq!(step, ScriptStep::RunFrames { frames: 25 });
    }

    #[test]
    fn parse_step_round_trips_load_basic_program_with_default_run() {
        let step =
            parse_step("load_basic_program", json!({"path": "hello.bas"})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::LoadBasicProgram {
                path: "hello.bas".into(),
                run: true,
            }
        );
    }

    #[test]
    fn parse_step_rejects_non_object_arguments() {
        let err = parse_step("run_frames", json!(42)).expect_err("non-object");
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_zero_field_steps() {
        let step = parse_step("stop_video_recording", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::StopVideoRecording);
    }

    #[test]
    fn parse_step_round_trips_reset_with_kind() {
        use emu198x_shell::ResetKind;
        let step = parse_step("reset", json!({"kind": "hard"})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::Reset {
                kind: ResetKind::Hard
            }
        );
        let step = parse_step("reset", json!({"kind": "soft"})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::Reset {
                kind: ResetKind::Soft
            }
        );
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_query_ay() {
        let step = parse_step("query_ay", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::QueryAy);
    }

    #[test]
    fn parse_step_accepts_null_arguments_for_query_cpu() {
        let step = parse_step("query_cpu", Value::Null).expect("valid step");
        assert_eq!(step, ScriptStep::QueryCpu);
    }

    #[test]
    fn parse_step_round_trips_step_with_default_count() {
        let step = parse_step("step", json!({})).expect("valid step");
        assert_eq!(step, ScriptStep::Step { instructions: None });
        let step = parse_step("step", json!({"instructions": 5})).expect("valid step");
        assert_eq!(
            step,
            ScriptStep::Step {
                instructions: Some(5),
            }
        );
    }

    #[test]
    fn parse_step_round_trips_run_until_pc() {
        let step = parse_step("run_until_pc", json!({"addr": 0x1234})).expect("valid run_until_pc");
        assert_eq!(
            step,
            ScriptStep::RunUntilPc {
                addr: 0x1234,
                max_halfcycles: None,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_disasm() {
        let step =
            parse_step("disasm", json!({"addr": 0x4000, "instructions": 8})).expect("valid disasm");
        assert_eq!(
            step,
            ScriptStep::Disasm {
                addr: 0x4000,
                instructions: Some(8),
            }
        );
    }

    #[test]
    fn parse_step_round_trips_port_read_and_write() {
        let r = parse_step("port_read", json!({"port": 0x00FE})).expect("valid port_read");
        assert_eq!(r, ScriptStep::PortRead { port: 0x00FE });
        let w = parse_step("port_write", json!({"port": 0x00FE, "value": 5}))
            .expect("valid port_write");
        assert_eq!(
            w,
            ScriptStep::PortWrite {
                port: 0x00FE,
                value: 5,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_press_key_default_and_explicit_hold() {
        let s = parse_step("press_key", json!({"key": "Space"})).expect("valid press_key");
        assert_eq!(
            s,
            ScriptStep::PressKey {
                key: "Space".into(),
                hold_frames: None,
            }
        );
        let s = parse_step("press_key", json!({"key": "Enter", "hold_frames": 8}))
            .expect("valid press_key with hold");
        assert_eq!(
            s,
            ScriptStep::PressKey {
                key: "Enter".into(),
                hold_frames: Some(8),
            }
        );
    }

    #[test]
    fn parse_step_round_trips_watch_ay_variants() {
        let start = parse_step("watch_ay_start", Value::Null).expect("valid watch_ay_start");
        assert_eq!(start, ScriptStep::WatchAyStart);
        let clear = parse_step("watch_ay_clear", Value::Null).expect("valid watch_ay_clear");
        assert_eq!(clear, ScriptStep::WatchAyClear);
        let log = parse_step("watch_ay_log", json!({})).expect("valid watch_ay_log");
        assert_eq!(
            log,
            ScriptStep::WatchAyLog {
                limit: None,
                unique: false,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_memory_read_arguments() {
        let step = parse_step("memory_read", json!({"addr": 0x4000, "len": 32}))
            .expect("valid memory_read");
        assert_eq!(
            step,
            ScriptStep::MemoryRead {
                addr: 0x4000,
                len: 32,
            }
        );
    }

    #[test]
    fn parse_step_round_trips_watch_memory_start_arguments() {
        let step = parse_step("watch_memory_start", json!({"addr": 0x5800, "len": 0x300}))
            .expect("valid watch_memory_start");
        assert_eq!(
            step,
            ScriptStep::WatchMemoryStart {
                addr: 0x5800,
                len: 0x300,
            }
        );
    }

    #[test]
    fn parse_step_accepts_empty_object_for_watch_memory_log() {
        let step = parse_step("watch_memory_log", json!({})).expect("valid watch_memory_log");
        assert_eq!(
            step,
            ScriptStep::WatchMemoryLog {
                limit: None,
                unique: false,
            }
        );
    }

    #[test]
    fn register_all_publishes_every_script_step_variant() {
        let mut registry: ToolRegistry<SpectrumSession> = ToolRegistry::new();
        register_all(&mut registry);
        let names: Vec<_> = registry.iter().map(|tool| tool.name().to_owned()).collect();
        let expected = [
            "load_media",
            "media_transport",
            "input",
            "run_frames",
            "wait_for_boot",
            "wait_for_query_contains",
            "wait_for_query_bool",
            "query",
            "query_paths",
            "load_snapshot",
            "save_snapshot",
            "save_screenshot",
            "save_audio_capture",
            "clear_audio_capture",
            "set_machine",
            "autoload_tape",
            "load_basic_program",
            "start_video_recording",
            "stop_video_recording",
            "reset",
            "start_audio_recording",
            "stop_audio_recording",
            "query_ay",
            "memory_read",
            "poke_byte",
            "poke_word",
            "watch_memory_start",
            "watch_memory_clear",
            "watch_memory_log",
            "query_cpu",
            "step",
            "run_until_pc",
            "disasm",
            "port_read",
            "port_write",
            "watch_ay_start",
            "watch_ay_clear",
            "watch_ay_log",
            "press_key",
        ];
        for name in expected {
            assert!(names.contains(&name.to_owned()), "missing {name}");
        }
    }
}
