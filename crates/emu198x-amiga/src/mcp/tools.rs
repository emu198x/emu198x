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

use std::path::PathBuf;

use emu198x_shell::mcp::{Tool, ToolError, ToolRegistry, ToolResponse};
use emu198x_shell::{CapturedFrame, MachineTime, PixelFormat, VideoRecorder};
use machine_commodore_amiga_a1200::{Adf, FB_HEIGHT, FB_WIDTH, PAL_FRAME_TICKS};
use motorola_68000::disasm::disassemble;
use serde_json::{Value, json};

use super::session::AmigaSession;

/// Wrap a closure as a `Tool` impl. The closure receives parsed
/// arguments and a mutable session reference and returns the JSON
/// response body. Lets us define tools inline without a struct per
/// tool.
struct InlineTool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut AmigaSession) -> Result<Value, ToolError>,
}

impl Tool<AmigaSession> for InlineTool {
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
        session: &mut AmigaSession,
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
/// can dump ROM too. Routes through [`AmigaLiveAccess`] so it works
/// against any chipset variant the session may be hosting.
fn read_long(session: &AmigaSession, addr: u32) -> u32 {
    session.access().read_long(addr)
}

fn read_byte(session: &AmigaSession, addr: u32) -> u8 {
    let aligned = addr & !1;
    let long = session.access().read_long(aligned & !2);
    let shift = (3 - (addr & 3)) * 8;
    ((long >> shift) & 0xFF) as u8
}

// ─── Tool implementations ─────────────────────────────────────────────

/// Convert the Denise ARGB framebuffer into an Rgba8888 `CapturedFrame`
/// for the active recorder. Returns `Err` only if the ffmpeg write
/// pipe fails; that's surfaced to the calling tool so the JSON-RPC
/// client sees the recording fault.
fn push_recorder_frame(s: &mut AmigaSession) -> Result<(), ToolError> {
    // Eagerly extract everything we need from the machine before we
    // take the recorder borrow, so the borrow checker sees the two
    // mutable accesses to `s` as disjoint. Pre-migration the
    // `machine` field allowed simultaneous field-level borrows
    // (`s.machine.X` and `s.recorder` were independent); the
    // `machine_mut()` downcast helper now reborrows all of `s`, so
    // the order matters.
    if s.recorder.is_none() {
        return Ok(());
    }
    let (rgba, tick_count) = {
        let access = s.access();
        let tick_count = access.tick_count();
        let fb = access.framebuffer();
        let mut rgba: Vec<u8> = Vec::with_capacity(fb.len() * 4);
        for &p in fb {
            rgba.push(((p >> 16) & 0xFF) as u8);
            rgba.push(((p >> 8) & 0xFF) as u8);
            rgba.push((p & 0xFF) as u8);
            rgba.push(((p >> 24) & 0xFF) as u8);
        }
        (rgba, tick_count)
    };
    let frame = CapturedFrame {
        timestamp: MachineTime::new(tick_count),
        format: PixelFormat::Rgba8888,
        width: FB_WIDTH,
        height: FB_HEIGHT,
        palette: None,
        pixels: rgba,
    };
    let recorder = s.recorder.as_mut().expect("checked above");
    recorder
        .push_frame(&frame)
        .map_err(|err| ToolError::Execution(format!("record frame: {err}")))?;
    s.last_recorded_tick = tick_count;
    Ok(())
}

/// Advance the machine by `tick_count` ticks. While a recording is
/// active, pushes one frame to the recorder every `PAL_FRAME_TICKS`
/// ticks crossed — so a 1000-frame run records 1000 frames regardless
/// of whether the caller used `run_frames` or `run_ticks`.
fn tick_for(s: &mut AmigaSession, ticks: u64) -> Result<(), ToolError> {
    // Drives ticks through `tick_with_trace` so the cpu_trace tool
    // sees every instruction boundary when armed. Overhead when
    // disarmed is one bool check per tick.
    if s.recorder.is_none() {
        for _ in 0..ticks {
            s.tick_with_trace();
        }
        return Ok(());
    }
    for _ in 0..ticks {
        s.tick_with_trace();
        let now = s.access().tick_count();
        if now.saturating_sub(s.last_recorded_tick) >= PAL_FRAME_TICKS {
            push_recorder_frame(s)?;
        }
    }
    Ok(())
}

fn tool_run_frames(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let n = arg_u64_or(&args, "frames", 1)?;
    tick_for(s, n.saturating_mul(PAL_FRAME_TICKS))?;
    Ok(json!({
        "frames_run": n,
        "pc": format!("${:08X}", s.access().cpu_pc()),
        "recording_frames": s.recorder.as_ref().map(|r| r.frames_written()),
    }))
}

fn tool_run_ticks(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let n = arg_u64_or(&args, "ticks", 1)?;
    tick_for(s, n)?;
    Ok(json!({
        "ticks_run": n,
        "pc": format!("${:08X}", s.access().cpu_pc()),
        "recording_frames": s.recorder.as_ref().map(|r| r.frames_written()),
    }))
}

fn tool_run_until_pc(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let target = arg_u32(&args, "target")?;
    let max_ticks = arg_u64_or(&args, "max_ticks", 100_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit = false;
    while ticks_taken < max_ticks {
        s.tick_with_trace();
        ticks_taken += 1;
        if s.access().cpu_pc() == target {
            hit = true;
            break;
        }
    }
    Ok(json!({
        "hit": hit,
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.access().cpu_pc()),
        "target": format!("${:08X}", target),
    }))
}

fn tool_reset(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Default to "hard" since that's what `reset` did before the
    // `kind` argument existed.
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("hard")
        .to_ascii_lowercase();
    let kind = match kind.as_str() {
        "hard" => "hard",
        "soft" => "soft",
        other => {
            return Err(ToolError::InvalidArguments(format!(
                "reset: unknown kind `{other}`; expected \"hard\" or \"soft\""
            )));
        }
    };

    // Today both kinds rebuild the A1200 from the ROM image (hard
    // reset). The A1200's `MachineCore::reset(ResetKind)` impl
    // currently ignores the kind, so plumbing soft / hard through
    // would not change the observable result. We accept the
    // argument so the wire format matches the shared shell layer's
    // ScriptStep::Reset { kind } and so scripts written against
    // either system look the same; differentiating soft vs hard on
    // the A1200 is a separate per-chip job (CIA reset behaviour,
    // ResetHandlers preservation, etc.).
    s.reset()
        .map_err(|err| ToolError::Execution(format!("reset: {err}")))?;
    Ok(json!({
        "reset": true,
        "kind": kind,
        "kind_differentiated": false,
        "rom_path": s.rom_path.display().to_string(),
        "pc": format!("${:08X}", s.access().cpu_pc()),
    }))
}

fn tool_query_cpu(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let cpu = s.access().cpu_snapshot();
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

fn tool_query_chipset(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let m = s.access();
    Ok(json!({
        "bplcon0": format!("${:04X}", m.bplcon0()),
        "dmacon":  format!("${:04X}", m.dmacon()),
        "adkcon":  format!("${:04X}", m.adkcon()),
        "color00": format!("${:04X}", m.color(0)),
        "cop1lc":  format!("${:08X}", m.copper_cop1lc()),
        "cop2lc":  format!("${:08X}", m.copper_cop2lc()),
        "copper_pc": format!("${:08X}", m.copper_pc()),
        "overlay": m.overlay(),
    }))
}

fn tool_query_paula(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let access = s.access();
    let intena = access.intena();
    let intreq = access.intreq();
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

fn tool_query_cia(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    fn snapshot(c: &mos_cia_8520::Cia8520) -> Value {
        json!({
            "cra": format!("${:02X}", c.cra()),
            "crb": format!("${:02X}", c.crb()),
            "timer_a": format!("${:04X}", c.timer_a()),
            "timer_b": format!("${:04X}", c.timer_b()),
            "timer_a_running": c.timer_a_running(),
            "timer_b_running": c.timer_b_running(),
            "icr_status": format!("${:02X}", c.icr_status()),
            "icr_mask":   format!("${:02X}", c.icr_mask()),
            "irq_active": c.irq_active(),
            "ddr_a": format!("${:02X}", c.ddr_a()),
            "ddr_b": format!("${:02X}", c.ddr_b()),
            "port_a_output": format!("${:02X}", c.port_a_output()),
            "port_b_output": format!("${:02X}", c.port_b_output()),
            "tod_counter": format!("${:06X}", c.tod_counter()),
            "tod_alarm":   format!("${:06X}", c.tod_alarm()),
            "tod_halted":  c.tod_halted(),
        })
    }
    let access = s.access();
    Ok(json!({
        "cia_a": snapshot(access.cia_a()),
        "cia_b": snapshot(access.cia_b()),
    }))
}

fn tool_query_agnus(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let a = s.access().agnus();
    Ok(json!({
        "vpos": a.vpos,
        "hpos_cck": a.hpos,
        "blitter_busy": a.blitter_busy,
        "blitter_exec_pending": a.blitter_exec_pending,
        "blitter_ccks_remaining": a.blitter_ccks_remaining,
        "bpl_pt": (0..8).map(|i| format!("${:08X}", a.bpl_pt[i])).collect::<Vec<_>>(),
        "blt_apt": format!("${:08X}", a.blt_apt),
        "blt_bpt": format!("${:08X}", a.blt_bpt),
        "blt_cpt": format!("${:08X}", a.blt_cpt),
        "blt_dpt": format!("${:08X}", a.blt_dpt),
        "fmode": format!("${:04X}", a.fmode),
        "bpl_fetch_width": a.bpl_fetch_width(),
        "spr_fetch_width": a.spr_fetch_width(),
        "diwstrt": format!("${:04X}", a.diwstrt),
        "diwstop": format!("${:04X}", a.diwstop),
        "ddfstrt": format!("${:04X}", a.ddfstrt),
        "ddfstop": format!("${:04X}", a.ddfstop),
        "bpl1mod": a.bpl1mod,
        "bpl2mod": a.bpl2mod,
        "bplcon0": format!("${:04X}", a.bplcon0),
        "num_bitplanes": a.num_bitplanes(),
    }))
}

/// Read a NUL-terminated C string from chip RAM via the trait's
/// `read_word` accessor. The Amiga ROM / chip RAM is big-endian
/// word-addressed; we walk word-by-word and decode each high/low
/// byte until either NUL or `max_len` bytes have been collected.
/// `addr == 0` returns an empty string (Amiga lists often carry
/// null `ln_Name` for anonymous nodes).
fn read_amiga_cstring(
    access: &dyn runtime_commodore_amiga::AmigaLiveAccess,
    addr: u32,
    max_len: usize,
) -> String {
    if addr == 0 {
        return String::new();
    }
    let mut bytes = Vec::with_capacity(max_len);
    let mut a = addr & !1; // word-align — Amiga task names are always word-aligned
    // If the original addr was odd, skip the high byte of the first word.
    let skip_first_byte = addr & 1 != 0;
    let mut first = true;
    while bytes.len() < max_len {
        let word = access.read_word(a);
        let high = (word >> 8) as u8;
        let low = (word & 0xFF) as u8;
        if !(first && skip_first_byte) {
            if high == 0 {
                break;
            }
            bytes.push(high);
        }
        if bytes.len() >= max_len {
            break;
        }
        if low == 0 {
            break;
        }
        bytes.push(low);
        a = a.wrapping_add(2);
        first = false;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Decode one Exec `Task` struct (with embedded `Node` at offset 0)
/// into JSON. Field offsets follow the AmigaOS RKM (Exec/Tasks).
fn read_exec_task(
    access: &dyn runtime_commodore_amiga::AmigaLiveAccess,
    addr: u32,
) -> Value {
    // Node (14 bytes): ln_Succ, ln_Pred, ln_Type, ln_Pri, ln_Name
    let ln_succ = access.read_long(addr);
    let ln_pred = access.read_long(addr.wrapping_add(4));
    let type_pri = access.read_word(addr.wrapping_add(8));
    let ln_type = (type_pri >> 8) as u8;
    // ln_Pri is a signed byte — interpret as i8.
    let ln_pri = (type_pri & 0xFF) as i8;
    let ln_name = access.read_long(addr.wrapping_add(10));
    let name = read_amiga_cstring(access, ln_name, 64);
    // Task (struct, +14 = tc_Flags, +15 = tc_State, +18 = tc_SigAlloc,
    // +22 = tc_SigWait, +26 = tc_SigRecvd, +30 = tc_SigExcept,
    // +54 = tc_SPReg, +88 = tc_UserData).
    let flags_state = access.read_word(addr.wrapping_add(14));
    let tc_flags = (flags_state >> 8) as u8;
    let tc_state = (flags_state & 0xFF) as u8;
    let tc_sig_alloc = access.read_long(addr.wrapping_add(18));
    let tc_sig_wait = access.read_long(addr.wrapping_add(22));
    let tc_sig_recvd = access.read_long(addr.wrapping_add(26));
    let tc_sig_except = access.read_long(addr.wrapping_add(30));
    let tc_sp_reg = access.read_long(addr.wrapping_add(54));
    let tc_user_data = access.read_long(addr.wrapping_add(88));
    let state_label = match tc_state {
        0 => "INVALID",
        1 => "ADDED",
        2 => "RUN",
        3 => "READY",
        4 => "WAIT",
        5 => "EXCEPT",
        6 => "REMOVED",
        _ => "?",
    };
    json!({
        "addr": format!("${:08X}", addr),
        "ln_name": name,
        "ln_succ": format!("${:08X}", ln_succ),
        "ln_pred": format!("${:08X}", ln_pred),
        "ln_type": ln_type,
        "ln_pri":  ln_pri,
        "tc_flags": format!("${:02X}", tc_flags),
        "tc_state": tc_state,
        "tc_state_label": state_label,
        "tc_sig_alloc":  format!("${:08X}", tc_sig_alloc),
        "tc_sig_wait":   format!("${:08X}", tc_sig_wait),
        "tc_sig_recvd":  format!("${:08X}", tc_sig_recvd),
        "tc_sig_except": format!("${:08X}", tc_sig_except),
        "tc_sp_reg":    format!("${:08X}", tc_sp_reg),
        "tc_user_data": format!("${:08X}", tc_user_data),
    })
}

/// Walk one Exec `List` (struct at `list_addr`: ln_Head, ln_Tail,
/// ln_TailPred + 2 bytes). Returns each node decoded as a task.
/// The standard Exec convention is that the tail-sentinel is the
/// list struct itself + 4 — walking stops when `ln_Succ` either
/// points there or is 0.
fn walk_exec_list(
    access: &dyn runtime_commodore_amiga::AmigaLiveAccess,
    list_addr: u32,
    max_nodes: usize,
) -> Vec<Value> {
    let mut out = Vec::new();
    let sentinel = list_addr.wrapping_add(4);
    let mut cur = access.read_long(list_addr);
    let mut seen = std::collections::HashSet::new();
    while out.len() < max_nodes {
        if cur == 0 || cur == sentinel {
            break;
        }
        if !seen.insert(cur) {
            break; // cycle guard — list got corrupted
        }
        out.push(read_exec_task(access, cur));
        cur = access.read_long(cur);
    }
    out
}

fn tool_query_exec_tasks(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // ExecBase pointer lives at $00000004 — set by Exec during boot.
    // Until KS has booted past the very early stages, this read will
    // return $00000000 (chip RAM is zero) and the rest degrades
    // gracefully (empty lists, null this_task).
    let access = s.access();
    let exec_base = access.read_long(0x0000_0004);
    if exec_base == 0 {
        return Ok(json!({
            "exec_base": "$00000000",
            "note": "ExecBase not yet initialised — run a few hundred frames after boot before querying.",
        }));
    }
    // Exec field offsets (RKM 3rd ed, Exec/Tasks):
    //   +276 ThisTask           — currently-running task
    //   +406 TaskReady   (14B)  — list of ready-to-run tasks
    //   +420 TaskWait    (14B)  — list of waiting tasks
    let this_task = access.read_long(exec_base.wrapping_add(276));
    let this_task_info = if this_task != 0 {
        Some(read_exec_task(access, this_task))
    } else {
        None
    };
    let ready = walk_exec_list(access, exec_base.wrapping_add(406), 64);
    let waiting = walk_exec_list(access, exec_base.wrapping_add(420), 64);
    Ok(json!({
        "exec_base":      format!("${:08X}", exec_base),
        "this_task":      format!("${:08X}", this_task),
        "this_task_info": this_task_info,
        "task_ready":     ready,
        "task_wait":      waiting,
    }))
}

/// Decode one Exec `MsgPort` struct + count queued messages.
/// Layout (Exec/Ports RKM):
///   Node (14B) + mp_Flags (B) + mp_SigBit (B) + mp_SigTask (4B) +
///   mp_MsgList (List, 14B)
fn read_exec_port(
    access: &dyn runtime_commodore_amiga::AmigaLiveAccess,
    addr: u32,
) -> Value {
    let ln_succ = access.read_long(addr);
    let type_pri = access.read_word(addr.wrapping_add(8));
    let ln_type = (type_pri >> 8) as u8;
    let ln_pri = (type_pri & 0xFF) as i8;
    let ln_name = access.read_long(addr.wrapping_add(10));
    let name = read_amiga_cstring(access, ln_name, 64);
    let flags_sigbit = access.read_word(addr.wrapping_add(14));
    let mp_flags = (flags_sigbit >> 8) as u8;
    let mp_sigbit = (flags_sigbit & 0xFF) as u8;
    let mp_sigtask = access.read_long(addr.wrapping_add(16));
    // Walk mp_MsgList to count pending messages. The list lives at
    // port+20; its tail sentinel is port+24 (= &mp_MsgList.ln_Tail).
    let list_addr = addr.wrapping_add(20);
    let sentinel = list_addr.wrapping_add(4);
    let mut msg_count = 0u32;
    let mut cur = access.read_long(list_addr);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1024 {
        if cur == 0 || cur == sentinel {
            break;
        }
        if !seen.insert(cur) {
            break;
        }
        msg_count += 1;
        cur = access.read_long(cur);
    }
    // mp_Flags bit layout (RKM): low 2 bits = PA_* action
    //   PA_SIGNAL (0) — signal mp_SigTask when message arrives
    //   PA_SOFTINT (1) — soft-interrupt mp_SigTask
    //   PA_IGNORE (2) — no notification (used during port migration)
    let action = mp_flags & 3;
    let action_label = match action {
        0 => "PA_SIGNAL",
        1 => "PA_SOFTINT",
        2 => "PA_IGNORE",
        _ => "?",
    };
    let _ = ln_succ; // currently unused but cheap to keep for symmetry
    json!({
        "addr": format!("${:08X}", addr),
        "ln_name": name,
        "ln_type": ln_type,
        "ln_pri":  ln_pri,
        "mp_flags": format!("${:02X}", mp_flags),
        "mp_action": action_label,
        "mp_sigbit": mp_sigbit,
        "mp_sigbit_mask": format!("${:08X}", 1u32 << mp_sigbit),
        "mp_sigtask": format!("${:08X}", mp_sigtask),
        "msg_count": msg_count,
    })
}

fn tool_query_exec_ports(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // ExecBase->PortList lives at SysBase + 392.
    let access = s.access();
    let exec_base = access.read_long(0x0000_0004);
    if exec_base == 0 {
        return Ok(json!({
            "exec_base": "$00000000",
            "note": "ExecBase not yet initialised — run a few hundred frames after boot before querying.",
        }));
    }
    let port_list = exec_base.wrapping_add(392);
    let sentinel = port_list.wrapping_add(4);
    let mut ports: Vec<Value> = Vec::new();
    let mut cur = access.read_long(port_list);
    let mut seen = std::collections::HashSet::new();
    while ports.len() < 256 {
        if cur == 0 || cur == sentinel {
            break;
        }
        if !seen.insert(cur) {
            break;
        }
        ports.push(read_exec_port(access, cur));
        cur = access.read_long(cur);
    }
    Ok(json!({
        "exec_base": format!("${:08X}", exec_base),
        "port_list_addr": format!("${:08X}", port_list),
        "ports": ports,
    }))
}

fn tool_start_video_recording(
    args: Value,
    s: &mut AmigaSession,
) -> Result<Value, ToolError> {
    if s.recorder.is_some() {
        return Err(ToolError::Execution(
            "a recording is already in flight — stop it before starting another".into(),
        ));
    }
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("missing string `path`".into()))?;
    // PAL Amiga = 50 fps. (NTSC would be 60, but the A1200 machine
    // currently boots PAL by default; an explicit `fps` override is
    // accepted for completeness.)
    let fps = args.get("fps").and_then(Value::as_u64).unwrap_or(50) as u32;
    let started_at = MachineTime::new(s.access().tick_count());
    let recorder = VideoRecorder::start(PathBuf::from(path), FB_WIDTH, FB_HEIGHT, fps, started_at)
        .map_err(|err| ToolError::Execution(format!("start recording: {err}")))?;
    s.recorder = Some(recorder);
    s.last_recorded_tick = s.access().tick_count();
    // Push one frame immediately so the recording begins with the
    // current screen state, not a missing first frame.
    push_recorder_frame(s)?;
    Ok(json!({
        "started": true,
        "path": path,
        "width": FB_WIDTH,
        "height": FB_HEIGHT,
        "fps": fps,
        "started_at_tick": s.access().tick_count(),
    }))
}

fn tool_stop_video_recording(
    _args: Value,
    s: &mut AmigaSession,
) -> Result<Value, ToolError> {
    let recorder = s
        .recorder
        .take()
        .ok_or_else(|| ToolError::Execution("no recording is in flight".into()))?;
    let summary = recorder
        .finish(None)
        .map_err(|err| ToolError::Execution(format!("finish recording: {err}")))?;
    Ok(json!({
        "stopped": true,
        "path": summary.path.display().to_string(),
        "frames": summary.frames,
        "duration_ms": summary.duration_ms,
        "has_audio": summary.has_audio,
    }))
}

fn tool_dump_framebuffer(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    use machine_commodore_amiga_a1200::{FB_HEIGHT, FB_WIDTH};
    let fb = s.access().framebuffer();
    let total_pixels = (FB_WIDTH * FB_HEIGHT) as usize;

    // Histogram top colours so the caller can see "what's on screen" without
    // saving anything to disk — useful when running headlessly.
    let mut hist: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for &p in fb {
        *hist.entry(p).or_insert(0) += 1;
    }
    let mut by_count: Vec<(u32, u32)> = hist.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<Value> = by_count
        .iter()
        .take(8)
        .map(|(argb, count)| {
            json!({
                "argb": format!("${:08X}", argb),
                "pixels": count,
                "pct": (*count as f64 / total_pixels as f64 * 100.0).round() / 1.0,
            })
        })
        .collect();
    let unique = by_count.len();

    // A coarse hash so the caller can tell two snapshots apart.
    let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a 64
    for &p in fb {
        hash ^= u64::from(p);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    // Optional PNG write — give the user a real image when they pass
    // `path`. Skipped when omitted so headless callers stay headless.
    let png_path = args.get("path").and_then(Value::as_str);
    let mut png_written: Option<String> = None;
    if let Some(p) = png_path {
        let path_buf = std::path::PathBuf::from(p);
        if let Some(parent) = path_buf.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| ToolError::Execution(format!("mkdir: {err}")))?;
            }
        }
        let file = std::fs::File::create(&path_buf)
            .map_err(|err| ToolError::Execution(format!("create png: {err}")))?;
        let mut encoder = png::Encoder::new(file, FB_WIDTH, FB_HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| ToolError::Execution(format!("png header: {err}")))?;
        // ARGB → RGBA bytes.
        let mut bytes: Vec<u8> = Vec::with_capacity(total_pixels * 4);
        for &p in fb {
            let r = ((p >> 16) & 0xFF) as u8;
            let g = ((p >> 8) & 0xFF) as u8;
            let b = (p & 0xFF) as u8;
            let a = ((p >> 24) & 0xFF) as u8;
            bytes.extend_from_slice(&[r, g, b, a]);
        }
        writer
            .write_image_data(&bytes)
            .map_err(|err| ToolError::Execution(format!("png write: {err}")))?;
        png_written = Some(path_buf.display().to_string());
    }

    Ok(json!({
        "width": FB_WIDTH,
        "height": FB_HEIGHT,
        "unique_colors": unique,
        "top_colors": top,
        "hash_fnv1a64": format!("${:016X}", hash),
        "png_written": png_written,
    }))
}

fn tool_bplcon0_log(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // BPLCON0 write tracing is mirrored across OCS / ECS / AGA — the
    // trait returns the live slice for whichever chipset variant the
    // session is hosting.
    let log = s.access().bplcon0_log();
    let unique_only = args
        .get("unique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;

    let mut entries: Vec<&(u64, u32, u16)> = log.iter().collect();
    if unique_only {
        // De-dupe by value: keep first occurrence of each distinct
        // BPLCON0 value. Surfaces "which different settings did KS
        // actually try?" without drowning in copper-per-line writes.
        let mut seen = std::collections::HashSet::new();
        entries.retain(|(_, _, v)| seen.insert(*v));
    }
    let total = entries.len();
    let shown: Vec<Value> = entries
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|(cck, pc, val)| {
            let bpu = (val >> 12) & 0x07;
            let bpu4 = (val >> 4) & 0x01;
            json!({
                "cck": cck,
                "pc": format!("${:08X}", pc),
                "val": format!("${:04X}", val),
                "bpu": bpu,
                "bpu_bit4": bpu4 != 0,
                "hires": (val & 0x8000) != 0,
                "ham":   (val & 0x0800) != 0,
                "dblpf": (val & 0x0400) != 0,
                "color": (val & 0x0200) != 0,
                "lace":  (val & 0x0004) != 0,
            })
        })
        .collect();

    // Always summarize the BPU values seen so the caller sees the
    // answer to "does BPU>0 ever happen?" without paging through
    // entries.
    let mut bpu_counts: [u64; 16] = [0; 16];
    for &(_, _, v) in log {
        let bpu = ((v >> 12) & 0x07) as usize;
        let bpu4 = ((v >> 4) & 0x01) as usize;
        let total_bpu = bpu | (bpu4 << 3);
        if total_bpu < 16 {
            bpu_counts[total_bpu] += 1;
        }
    }

    Ok(json!({
        "total_writes": log.len(),
        "returned": shown.len(),
        "filtered_total": total,
        "unique": unique_only,
        "bpu_histogram": bpu_counts,
        "entries": shown,
    }))
}

fn tool_query_aga(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Pull the OCS 12-bit palette first via the trait so we don't
    // hold the A1200 borrow while later code needs it for AGA-only
    // `denise_aga()` access.
    let ocs_palette: Vec<String> = (0..32)
        .map(|i| format!("${:03X}", s.access().color(i)))
        .collect();
    let aga = s.aga_machine_mut().denise_aga();
    let bplcon3 = aga.bplcon3;
    let bank = (bplcon3 >> 13) & 7;
    let loct = (bplcon3 & 0x0200) != 0;
    // Count non-zero entries per bank — surfaces whether KS has
    // populated any AGA-specific palette banks beyond bank 0.
    let mut bank_nonzero: [u32; 8] = [0; 8];
    for (i, &c) in aga.palette_24.iter().enumerate() {
        if c != 0 {
            bank_nonzero[i / 32] += 1;
        }
    }
    let bank0: Vec<String> = aga.palette_24[0..32]
        .iter()
        .map(|c| format!("${:06X}", c))
        .collect();
    let mut full_palette: Option<Vec<String>> = None;
    if args.get("all_banks").and_then(Value::as_bool).unwrap_or(false) {
        full_palette = Some(
            aga.palette_24
                .iter()
                .map(|c| format!("${:06X}", c))
                .collect(),
        );
    }
    Ok(json!({
        "deniseid": format!("${:04X}", aga.deniseid()),
        "bplcon3": format!("${:04X}", bplcon3),
        "bplcon3_bank": bank,
        "bplcon3_loct": loct,
        "bplcon4": format!("${:04X}", aga.bplcon4),
        "spr_width": aga.spr_width,
        "ham_prev_rgb24": format!("${:06X}", aga.ham_prev_rgb24),
        "palette_24_nonzero_per_bank": bank_nonzero,
        "palette_24_bank0": bank0,
        "ocs_palette_12bit": ocs_palette,
        "palette_24_full": full_palette,
    }))
}

fn tool_chipset_read_log(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Chipset-register read tracing is mirrored across OCS / ECS /
    // AGA. The trait hands back a slice directly.
    let log = s.access().reg_read_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let unique = args.get("unique").and_then(Value::as_bool).unwrap_or(false);
    let offset_filter = args
        .get("offset")
        .and_then(|v| {
            if v.is_null() {
                None
            } else {
                let one = json!({ "x": v });
                arg_u32(&one, "x").ok().map(|n| n as u16)
            }
        });

    let mut filtered: Vec<&(u64, u32, u16, u16)> = log
        .iter()
        .filter(|(_, _, off, _)| offset_filter.map_or(true, |want| *off == want))
        .collect();
    let dedupe_mode = args
        .get("dedupe")
        .and_then(Value::as_str)
        .unwrap_or(if unique { "pc_off_val" } else { "none" });
    let cck_lo = args.get("cck_min").and_then(Value::as_u64);
    let cck_hi = args.get("cck_max").and_then(Value::as_u64);
    if let Some(lo) = cck_lo {
        filtered.retain(|(cck, _, _, _)| *cck >= lo);
    }
    if let Some(hi) = cck_hi {
        filtered.retain(|(cck, _, _, _)| *cck <= hi);
    }
    if dedupe_mode != "none" {
        let mut seen = std::collections::HashSet::new();
        filtered.retain(|(_, pc, off, val)| {
            let key: u64 = match dedupe_mode {
                "pc_off" => ((*pc as u64) << 16) | (*off as u64),
                "off"    => *off as u64,
                _        => ((*pc as u64) << 32) | ((*off as u64) << 16) | (*val as u64),
            };
            seen.insert(key)
        });
    }
    let total = filtered.len();
    let entries: Vec<Value> = filtered
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|(cck, pc, off, val)| {
            json!({
                "cck": cck,
                "pc":     format!("${:08X}", pc),
                "offset": format!("${:04X}", off),
                "value":  format!("${:04X}", val),
            })
        })
        .collect();
    // Also include a per-offset summary so callers can see at a
    // glance which registers KS even touches.
    let mut off_counts: std::collections::BTreeMap<u16, u64> = std::collections::BTreeMap::new();
    for &(_, _, off, _) in log {
        *off_counts.entry(off).or_insert(0) += 1;
    }
    let summary: Vec<Value> = off_counts
        .iter()
        .map(|(off, count)| json!({ "offset": format!("${:04X}", off), "reads": count }))
        .collect();
    Ok(json!({
        "total_logged": log.len(),
        "filtered_total": total,
        "returned": entries.len(),
        "offset_summary": summary,
        "entries": entries,
    }))
}

fn tool_chipset_write_log(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Every CPU write that lands in `dispatch_custom_register`'s write
    // arm. Lets callers answer "when did COP2LC change?" or "what
    // were all the writes to $DFF0xx during boot?" without polling
    // `query_chipset` every N frames. Mirrors the shape and filters
    // of `chipset_read_log`. Cross-cutting across OCS / ECS / AGA.
    let log = s.access().custom_write_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let offset_filter = args
        .get("offset")
        .and_then(|v| {
            if v.is_null() {
                None
            } else {
                let one = json!({ "x": v });
                arg_u32(&one, "x").ok().map(|n| n as u16)
            }
        });
    let offset_min = args.get("offset_min").and_then(Value::as_u64).map(|n| n as u16);
    let offset_max = args.get("offset_max").and_then(Value::as_u64).map(|n| n as u16);
    let cck_lo = args.get("cck_min").and_then(Value::as_u64);
    let cck_hi = args.get("cck_max").and_then(Value::as_u64);
    let dedupe_mode = args
        .get("dedupe")
        .and_then(Value::as_str)
        .unwrap_or("none");

    let mut filtered: Vec<&(u64, u32, u32, u16, u16, bool)> = log
        .iter()
        .filter(|(_, _, _, off, _, _)| offset_filter.map_or(true, |want| *off == want))
        .filter(|(_, _, _, off, _, _)| offset_min.map_or(true, |lo| *off >= lo))
        .filter(|(_, _, _, off, _, _)| offset_max.map_or(true, |hi| *off <= hi))
        .collect();
    if let Some(lo) = cck_lo {
        filtered.retain(|(cck, _, _, _, _, _)| *cck >= lo);
    }
    if let Some(hi) = cck_hi {
        filtered.retain(|(cck, _, _, _, _, _)| *cck <= hi);
    }
    if dedupe_mode != "none" {
        let mut seen = std::collections::HashSet::new();
        filtered.retain(|(_, pc, _, off, val, _)| {
            let key: u64 = match dedupe_mode {
                "pc_off" => ((*pc as u64) << 16) | (*off as u64),
                "off"    => *off as u64,
                _        => ((*pc as u64) << 32) | ((*off as u64) << 16) | (*val as u64),
            };
            seen.insert(key)
        });
    }
    let total = filtered.len();
    let entries: Vec<Value> = filtered
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|(cck, pc, addr, off, val, is_word)| {
            json!({
                "cck": cck,
                "pc":     format!("${:08X}", pc),
                "addr":   format!("${:08X}", addr),
                "offset": format!("${:04X}", off),
                "value":  format!("${:04X}", val),
                "size":   if *is_word { "word" } else { "byte" },
            })
        })
        .collect();
    // Per-offset write count so callers can see at a glance which
    // registers KS / app actually touches.
    let mut off_counts: std::collections::BTreeMap<u16, u64> = std::collections::BTreeMap::new();
    for &(_, _, _, off, _, _) in log {
        *off_counts.entry(off).or_insert(0) += 1;
    }
    let summary: Vec<Value> = off_counts
        .iter()
        .map(|(off, count)| json!({ "offset": format!("${:04X}", off), "writes": count }))
        .collect();
    Ok(json!({
        "total_logged": log.len(),
        "filtered_total": total,
        "returned": entries.len(),
        "offset_summary": summary,
        "entries": entries,
    }))
}

fn tool_poke_word(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let val = arg_u32(&args, "val")?;
    let val_u16 = u16::try_from(val & 0xFFFF).unwrap_or(0);
    s.access_mut().poke_word(addr, val_u16);
    Ok(json!({
        "poked": true,
        "addr": format!("${:08X}", addr),
        "val":  format!("${:04X}", val_u16),
    }))
}

fn tool_watch_memory(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let lo = arg_u32(&args, "addr")?;
    let len = arg_u32(&args, "len")?;
    if len == 0 {
        return Err(ToolError::InvalidArguments("`len` must be ≥ 1".into()));
    }
    s.access_mut().set_watch(Some((lo, len)));
    Ok(json!({
        "watching": {
            "lo":  format!("${:08X}", lo),
            "len": len,
            "hi_exclusive": format!("${:08X}", lo.wrapping_add(len)),
        },
        "log_cleared": true,
    }))
}

fn tool_watch_memory_clear(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let prior = s.access().watch_range();
    let count = s.access().watch_log().len();
    s.access_mut().set_watch(None);
    Ok(json!({
        "had_watch": prior.is_some(),
        "writes_captured_before_clear": count,
    }))
}

fn tool_watch_memory_log(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let access = s.access();
    let log = access.watch_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let unique = args
        .get("unique")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut entries: Vec<&(u64, u32, u32, u16, bool)> = log.iter().collect();
    if unique {
        let mut seen = std::collections::HashSet::new();
        entries.retain(|(_, pc, addr, val, _)| seen.insert((*pc, *addr, *val)));
    }
    let total = entries.len();
    let returned: Vec<Value> = entries
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|(cck, pc, addr, val, is_word)| {
            json!({
                "cck": cck,
                "pc":   format!("${:08X}", pc),
                "addr": format!("${:08X}", addr),
                "val":  format!("${:04X}", val),
                "size": if *is_word { "word" } else { "byte" },
            })
        })
        .collect();
    Ok(json!({
        "total_writes": log.len(),
        "filtered_total": total,
        "returned": returned.len(),
        "watch_range": access.watch_range().map(|(lo, len)| json!({
            "lo": format!("${:08X}", lo),
            "len": len,
        })),
        "entries": returned,
    }))
}

fn tool_restart(args: Value, _s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Tear down a live recording cleanly so the temp file doesn't
    // leak — `VideoRecorder::Drop` would handle this too, but doing
    // it inline makes the surface explicit. (Session is dropped on
    // exit anyway.)
    let exit_code = args.get("exit_code").and_then(Value::as_i64).unwrap_or(0) as i32;
    let response = json!({
        "restart": true,
        "exit_code": exit_code,
        "note": "Process exits AFTER this response is flushed. Hosts that auto-respawn (Claude Desktop, most IDE MCP plugins) will reconnect to the fresh binary on the next request.",
    });
    // Spawn a watchdog: serde flushes the response, then we
    // synchronously exit. The 50ms grace is excessive but cheap and
    // guarantees the line is on the wire on slow terminals.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::process::exit(exit_code);
    });
    Ok(response)
}

fn tool_palette_log(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Palette-write tracing is mirrored across OCS / ECS / AGA. The
    // fifth field of each entry is `Option<u16>` — `Some(bplcon3)` on
    // ECS / AGA where BPLCON3 is a real register, `None` on OCS where
    // $0106 isn't backed by any chip state.
    let log = s.access().palette_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let only_color = args
        .get("only_color")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let only_bplcon3 = args
        .get("only_bplcon3")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let unique = args
        .get("unique")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let idx_range: Option<(u16, u16)> = args
        .get("color_idx_range")
        .and_then(Value::as_array)
        .and_then(|a| {
            let lo = a.get(0)?.as_u64()? as u16;
            let hi = a.get(1)?.as_u64()? as u16;
            Some((lo, hi))
        });
    let mut filtered: Vec<&(u64, u32, u16, u16, Option<u16>)> = log
        .iter()
        .filter(|(_, _, off, _, _)| {
            let is_color = *off >= 0x180 && *off <= 0x1BE;
            let is_bplcon3 = *off == 0x0106;
            if only_color && !is_color {
                return false;
            }
            if only_bplcon3 && !is_bplcon3 {
                return false;
            }
            if let Some((lo, hi)) = idx_range {
                if !is_color {
                    return false;
                }
                let idx = (*off - 0x180) / 2;
                if idx < lo || idx > hi {
                    return false;
                }
            }
            true
        })
        .collect();
    if unique {
        let mut seen = std::collections::HashSet::new();
        filtered.retain(|(_, _, off, val, b3)| {
            // `bank` / `loct` are decoded from BPLCON3 when it's
            // present; on OCS the entry has no BPLCON3 so we de-dupe
            // by (offset, val) alone.
            let bank = b3.map(|b| (b >> 13) & 7).unwrap_or(0);
            let loct = b3.map(|b| (b & 0x0200) != 0).unwrap_or(false);
            seen.insert((*off, *val, bank, loct))
        });
    }
    let total = filtered.len();
    let entries: Vec<Value> = filtered
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|(cck, pc, off, val, b3)| {
            // BPLCON3 bank / loct only meaningful when the chipset
            // has BPLCON3 (ECS / AGA). On OCS the writes are still
            // captured but the bank/loct fields stay null in JSON
            // so callers can distinguish "no BPLCON3 register" from
            // "BPLCON3 = 0".
            let bank = b3.map(|b| (b >> 13) & 7);
            let loct = b3.map(|b| (b & 0x0200) != 0);
            let kind = match *off {
                0x0106 => "BPLCON3",
                0x010C => "BPLCON4",
                _ => "COLOR",
            };
            let idx = if *off >= 0x180 && *off <= 0x1BE {
                Some(((*off - 0x180) / 2) as usize)
            } else {
                None
            };
            json!({
                "cck": cck,
                "pc": format!("${:08X}", pc),
                "kind": kind,
                "offset": format!("${:04X}", off),
                "val": format!("${:04X}", val),
                "color_idx": idx,
                "bplcon3_at_write": b3.map(|b| format!("${:04X}", b)),
                "bank": bank,
                "loct": loct,
            })
        })
        .collect();
    Ok(json!({
        "total_logged": log.len(),
        "filtered_total": total,
        "returned": entries.len(),
        "entries": entries,
    }))
}

fn tool_query_blitter(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Cross-cutting: every field below is on the shared OCS Agnus base
    // type that the ECS / AGA wrappers Deref to, so the trait surface
    // suffices.
    let a = s.access().agnus();
    Ok(json!({
        "busy": a.blitter_busy,
        "exec_pending": a.blitter_exec_pending,
        "ccks_remaining": a.blitter_ccks_remaining,
        "apt": format!("${:08X}", a.blt_apt),
        "bpt": format!("${:08X}", a.blt_bpt),
        "cpt": format!("${:08X}", a.blt_cpt),
        "dpt": format!("${:08X}", a.blt_dpt),
    }))
}

fn tool_query_copper_list(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // The copper instruction encoding (MOVE / WAIT / SKIP) is identical
    // across OCS / ECS / AGA — only the host `Copper` struct type
    // differs between chipsets. So the disassembly uses the trait's
    // `copper_cop1lc()` for the start address and `read_word()` for
    // the per-word fetch, sidestepping the typed struct entirely.
    let access = s.access();
    let default_start = access.copper_cop1lc();
    let start = if let Some(v) = args.get("addr") {
        if v.is_null() { default_start } else { arg_u32(&args, "addr")? }
    } else {
        default_start
    };
    let count = arg_u64_or(&args, "count", 32)?;
    if count == 0 || count > 256 {
        return Err(ToolError::InvalidArguments("count must be 1..=256".into()));
    }
    let mut pc = start;
    let mut out: Vec<Value> = Vec::new();
    for _ in 0..count {
        let w1 = access.read_word(pc);
        let w2 = access.read_word(pc.wrapping_add(2));
        let line = if (w1 & 1) == 0 {
            // MOVE: w1 = register offset (lower 9 bits), w2 = value
            let reg = w1 & 0x1FE;
            json!({
                "addr": format!("${:08X}", pc),
                "op": "MOVE",
                "reg": format!("${:04X}", reg),
                "value": format!("${:04X}", w2),
                "raw": [format!("${:04X}", w1), format!("${:04X}", w2)],
            })
        } else if (w2 & 1) == 0 {
            let vp = (w1 >> 8) & 0xFF;
            let hp = (w1 >> 1) & 0x7F;
            let ve = (w2 >> 8) & 0x7F;
            let he = (w2 >> 1) & 0x7F;
            json!({
                "addr": format!("${:08X}", pc),
                "op": "WAIT",
                "vp": format!("{:02X}", vp),
                "hp": format!("{:02X}", hp),
                "ve_mask": format!("{:02X}", ve),
                "he_mask": format!("{:02X}", he),
                "raw": [format!("${:04X}", w1), format!("${:04X}", w2)],
            })
        } else {
            json!({
                "addr": format!("${:08X}", pc),
                "op": "SKIP",
                "raw": [format!("${:04X}", w1), format!("${:04X}", w2)],
            })
        };
        // CMOVE ENDOFLIST sentinel ($FFFF,$FFFE) ends a copper list.
        let is_end = w1 == 0xFFFF && w2 == 0xFFFE;
        out.push(line);
        pc = pc.wrapping_add(4);
        if is_end {
            break;
        }
    }
    Ok(json!({
        "start": format!("${:08X}", start),
        "entries": out,
    }))
}

fn tool_query_stack(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let usp = args.get("usp").and_then(Value::as_bool).unwrap_or(false);
    let count = arg_u64_or(&args, "count", 16)?;
    if count == 0 || count > 256 {
        return Err(ToolError::InvalidArguments("count must be 1..=256".into()));
    }
    let regs = s.access().cpu_snapshot().regs;
    let base = if usp { regs.usp } else { regs.ssp };
    let entries: Vec<Value> = (0..count)
        .map(|i| {
            let addr = base.wrapping_add((i as u32) * 4);
            json!({
                "addr": format!("${:08X}", addr),
                "value": format!("${:08X}", s.access().read_long(addr)),
            })
        })
        .collect();
    Ok(json!({
        "stack": if usp { "USP" } else { "SSP" },
        "base": format!("${:08X}", base),
        "entries": entries,
    }))
}

fn tool_cpu_trace_arm(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Enable the instruction-boundary trace. Clears any previously
    // captured entries so the new run starts fresh; the PC filter
    // and max_entries from the previous arm are replaced if the
    // caller supplies new values.
    let pc_min = args.get("pc_min").and_then(|v| {
        if v.is_null() { None } else {
            let one = json!({ "x": v });
            arg_u32(&one, "x").ok()
        }
    });
    let pc_max = args.get("pc_max").and_then(|v| {
        if v.is_null() { None } else {
            let one = json!({ "x": v });
            arg_u32(&one, "x").ok()
        }
    });
    let pc_filter = match (pc_min, pc_max) {
        (Some(lo), Some(hi)) if lo <= hi => Some((lo, hi)),
        (Some(_), Some(_)) => {
            return Err(ToolError::InvalidArguments(
                "pc_min must be <= pc_max".into(),
            ));
        }
        _ => None,
    };
    let max_entries = args
        .get("max_entries")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(100_000);
    if max_entries == 0 || max_entries > 10_000_000 {
        return Err(ToolError::InvalidArguments(
            "max_entries must be 1..=10_000_000".into(),
        ));
    }
    s.cpu_trace.armed = true;
    s.cpu_trace.pc_filter = pc_filter;
    s.cpu_trace.max_entries = max_entries;
    s.cpu_trace.entries.clear();
    s.cpu_trace.last_seen_instr_starts = s.access().cpu_instruction_starts();
    Ok(json!({
        "armed": true,
        "pc_filter": pc_filter.map(|(lo, hi)| json!({
            "min": format!("${:08X}", lo),
            "max": format!("${:08X}", hi),
        })),
        "max_entries": max_entries,
    }))
}

fn tool_cpu_trace_disarm(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let captured = s.cpu_trace.entries.len();
    s.cpu_trace.armed = false;
    Ok(json!({
        "armed": false,
        "captured": captured,
    }))
}

fn tool_cpu_trace_clear(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let dropped = s.cpu_trace.entries.len();
    s.cpu_trace.entries.clear();
    s.cpu_trace.last_seen_instr_starts = s.access().cpu_instruction_starts();
    Ok(json!({
        "dropped": dropped,
        "armed": s.cpu_trace.armed,
    }))
}

fn tool_cpu_trace_log(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    // Dump captured trace entries. Default returns the most-recent
    // 256 entries (tail) so the response stays compact even when
    // running long; callers wanting the full trace pass a higher
    // `limit`. `from_start: true` switches to "first N entries"
    // instead of "last N" for inspecting the beginning of a region.
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(256) as usize;
    let from_start = args
        .get("from_start")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cck_lo = args.get("cck_min").and_then(Value::as_u64);
    let cck_hi = args.get("cck_max").and_then(Value::as_u64);

    let mut filtered: Vec<&(u64, u32, u16, u16)> = s
        .cpu_trace
        .entries
        .iter()
        .filter(|(cck, _, _, _)| cck_lo.map_or(true, |lo| *cck >= lo))
        .filter(|(cck, _, _, _)| cck_hi.map_or(true, |hi| *cck <= hi))
        .collect();
    let total = filtered.len();
    if !from_start {
        // Tail mode: keep the trailing `limit` entries.
        if filtered.len() > limit {
            let drop = filtered.len() - limit;
            filtered.drain(0..drop);
        }
    } else if filtered.len() > limit {
        filtered.truncate(limit);
    }
    let entries: Vec<Value> = filtered
        .iter()
        .map(|(cck, pc, sr, opcode)| {
            json!({
                "cck": cck,
                "pc":     format!("${:08X}", pc),
                "sr":     format!("${:04X}", sr),
                "opcode": format!("${:04X}", opcode),
            })
        })
        .collect();
    Ok(json!({
        "armed": s.cpu_trace.armed,
        "captured": s.cpu_trace.entries.len(),
        "filtered_total": total,
        "returned": entries.len(),
        "at_limit": s.cpu_trace.entries.len() >= s.cpu_trace.max_entries,
        "max_entries": s.cpu_trace.max_entries,
        "entries": entries,
    }))
}

fn tool_step(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let n = arg_u64_or(&args, "count", 1)?;
    let max_ticks = arg_u64_or(&args, "max_ticks", 1_000_000)?;
    let start = s.access().cpu_instruction_starts();
    let target = start.wrapping_add(n);
    let mut ticks_taken: u64 = 0;
    let mut trace: Vec<Value> = Vec::new();
    let mut last_seen = start;
    while s.access().cpu_instruction_starts() != target && ticks_taken < max_ticks {
        s.tick_with_trace();
        ticks_taken += 1;
        let now = s.access().cpu_instruction_starts();
        if now != last_seen && !s.access().cpu_in_followup() {
            last_seen = now;
            trace.push(json!({
                "step": now.wrapping_sub(start),
                "pc": format!("${:08X}", s.access().cpu_pc()),
            }));
            if trace.len() as u64 >= n {
                break;
            }
        }
    }
    Ok(json!({
        "requested": n,
        "completed": s.access().cpu_instruction_starts().wrapping_sub(start),
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.access().cpu_pc()),
        "trace": trace,
    }))
}

fn tool_run_until_any_pc(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let targets = args
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::InvalidArguments("missing array `targets`".into()))?;
    let mut wanted: Vec<u32> = Vec::with_capacity(targets.len());
    for t in targets {
        let one = json!({ "x": t });
        wanted.push(arg_u32(&one, "x")?);
    }
    if wanted.is_empty() {
        return Err(ToolError::InvalidArguments("`targets` must be non-empty".into()));
    }
    let max_ticks = arg_u64_or(&args, "max_ticks", 100_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit: Option<u32> = None;
    while ticks_taken < max_ticks {
        s.tick_with_trace();
        ticks_taken += 1;
        let pc = s.access().cpu_pc();
        if wanted.iter().any(|t| *t == pc) {
            hit = Some(pc);
            break;
        }
    }
    Ok(json!({
        "hit": hit.map(|p| format!("${:08X}", p)),
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.access().cpu_pc()),
    }))
}

fn tool_insert_media(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("missing string `path`".into()))?;
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("adf");
    let change_pending = args
        .get("change_pending")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let entry_hint = args.get("entry").and_then(Value::as_str);
    let path_buf = PathBuf::from(path);
    let (bytes, source_label) = load_media_bytes(&path_buf, entry_hint)?;
    match kind {
        "adf" => {
            let adf = Adf::from_bytes(bytes)
                .map_err(|err| ToolError::Execution(format!("parse ADF: {err:?}")))?;
            s.access_mut().insert_floppy0(adf, change_pending);
            Ok(json!({
                "inserted": true,
                "kind": "adf",
                "path": path_buf.display().to_string(),
                "source": source_label,
                "change_pending": change_pending,
                "has_disk": s.access().drive().has_disk(),
            }))
        }
        other => Err(ToolError::InvalidArguments(format!(
            "unsupported media kind `{other}` (only `adf` is wired today)"
        ))),
    }
}

/// Load the raw bytes of a media image from disk. If `path` has a
/// `.zip` extension, opens the archive and reads either a single
/// `.adf` member (auto-detected when there's exactly one) or the
/// member whose filename matches `entry_hint`. Otherwise reads the
/// file verbatim.
///
/// Returns the bytes plus a human-readable label of where they came
/// from, so the response surfaces which archive entry was used.
fn load_media_bytes(
    path: &std::path::Path,
    entry_hint: Option<&str>,
) -> Result<(Vec<u8>, String), ToolError> {
    let is_zip = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false);
    if !is_zip {
        let bytes = std::fs::read(path)
            .map_err(|err| ToolError::Execution(format!("read {}: {err}", path.display())))?;
        return Ok((bytes, path.display().to_string()));
    }
    let file = std::fs::File::open(path)
        .map_err(|err| ToolError::Execution(format!("open zip {}: {err}", path.display())))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| ToolError::Execution(format!("read zip {}: {err}", path.display())))?;
    // Decide which entry to extract.
    let mut chosen_index: Option<usize> = None;
    let mut adf_entries: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|err| ToolError::Execution(format!("zip entry {i}: {err}")))?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        if let Some(want) = entry_hint {
            if name == want {
                chosen_index = Some(i);
                break;
            }
        }
        if name.to_lowercase().ends_with(".adf") {
            adf_entries.push((i, name));
        }
    }
    let chosen_index = match chosen_index {
        Some(i) => i,
        None => match adf_entries.len() {
            0 => {
                return Err(ToolError::Execution(format!(
                    "no .adf entry found in {}",
                    path.display()
                )));
            }
            1 => adf_entries[0].0,
            _ => {
                let names: Vec<&str> = adf_entries.iter().map(|(_, n)| n.as_str()).collect();
                return Err(ToolError::InvalidArguments(format!(
                    "zip {} has {} .adf entries; pass `entry` to pick one: {:?}",
                    path.display(),
                    names.len(),
                    names
                )));
            }
        },
    };
    let mut entry = archive
        .by_index(chosen_index)
        .map_err(|err| ToolError::Execution(format!("re-open zip entry: {err}")))?;
    let entry_name = entry.name().to_string();
    let mut buf = Vec::with_capacity(entry.size() as usize);
    std::io::copy(&mut entry, &mut buf)
        .map_err(|err| ToolError::Execution(format!("read zip entry: {err}")))?;
    Ok((buf, format!("{}#{}", path.display(), entry_name)))
}

fn tool_eject_media(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let had_disk = s.access().drive().has_disk();
    s.access_mut().eject_floppy0();
    Ok(json!({
        "ejected": had_disk,
        "has_disk": s.access().drive().has_disk(),
    }))
}

fn tool_query_disk(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let drive = s.access().drive();
    let status = drive.status();
    Ok(json!({
        "has_disk": drive.has_disk(),
        "selected": drive.selected(),
        "cylinder": drive.cylinder(),
        "head": drive.head(),
        "motor_on": drive.motor_on(),
        "motor_spinning": drive.motor_spinning(),
        "status": {
            "disk_change_low": status.disk_change,
            "write_protect_low": status.write_protect,
            "track0_low": status.track0,
            "ready_low": status.ready,
        },
    }))
}

fn tool_run_until_mem_change(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let addrs = args
        .get("addrs")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::InvalidArguments("missing array `addrs`".into()))?;
    let mut watch: Vec<(u32, u32)> = Vec::with_capacity(addrs.len());
    {
        let access = s.access();
        for a in addrs {
            let one = json!({ "x": a });
            let addr = arg_u32(&one, "x")?;
            watch.push((addr, access.read_long(addr)));
        }
    }
    if watch.is_empty() {
        return Err(ToolError::InvalidArguments("`addrs` must be non-empty".into()));
    }
    let max_ticks = arg_u64_or(&args, "max_ticks", 50_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit: Option<(u32, u32, u32)> = None;
    while ticks_taken < max_ticks {
        s.tick_with_trace();
        ticks_taken += 1;
        for (addr, old) in &watch {
            let now = s.access().read_long(*addr);
            if now != *old {
                hit = Some((*addr, *old, now));
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let result = hit.map(|(a, o, n)| json!({
        "addr": format!("${:08X}", a),
        "old": format!("${:08X}", o),
        "new": format!("${:08X}", n),
    }));
    Ok(json!({
        "hit": result,
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.access().cpu_pc()),
    }))
}

fn tool_memory_read(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
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

fn tool_memory_read_long(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    Ok(json!({
        "addr": format!("${:08X}", addr),
        "value": format!("${:08X}", read_long(s, addr)),
    }))
}

fn tool_disasm(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let count = arg_u64_or(&args, "count", 8)? as u32;
    if count == 0 || count > 128 {
        return Err(ToolError::InvalidArguments(
            "count must be 1..=128".into(),
        ));
    }
    let mut pc = addr;
    let mut lines: Vec<Value> = Vec::new();
    let access = s.access();
    for _ in 0..count {
        let read = |a: u32| -> u8 {
            let aligned = a & !3;
            let long = access.read_long(aligned);
            let shift = (3 - (a & 3)) * 8;
            ((long >> shift) & 0xFF) as u8
        };
        let (mnemonic, instr_len) = disassemble(pc, read);
        let bytes_hex: String = (0..instr_len)
            .map(|i| {
                let a = pc.wrapping_add(u32::from(i));
                let aligned = a & !3;
                let long = access.read_long(aligned);
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
pub fn register_all(registry: &mut ToolRegistry<AmigaSession>) {
    fn add(
        registry: &mut ToolRegistry<AmigaSession>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut AmigaSession) -> Result<Value, ToolError>,
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

    let step_schema = json!({
        "type": "object",
        "properties": {
            "count": {"type": "integer", "minimum": 1, "default": 1},
            "max_ticks": {"type": "integer", "minimum": 1, "default": 1000000}
        }
    });
    let any_pc_schema = json!({
        "type": "object",
        "required": ["targets"],
        "properties": {
            "targets": {"type": "array", "items": {"description": "PC — decimal int or hex string"}, "minItems": 1},
            "max_ticks": {"type": "integer", "minimum": 1, "default": 100000000}
        }
    });
    let mem_change_schema = json!({
        "type": "object",
        "required": ["addrs"],
        "properties": {
            "addrs": {"type": "array", "items": {"description": "Address (longword) — decimal int or hex string"}, "minItems": 1},
            "max_ticks": {"type": "integer", "minimum": 1, "default": 50000000}
        }
    });
    let copper_list_schema = json!({
        "type": "object",
        "properties": {
            "addr": {"description": "Start address (default = COP1LC)"},
            "count": {"type": "integer", "minimum": 1, "maximum": 256, "default": 32}
        }
    });
    let stack_schema = json!({
        "type": "object",
        "properties": {
            "usp": {"type": "boolean", "default": false},
            "count": {"type": "integer", "minimum": 1, "maximum": 256, "default": 16}
        }
    });
    let insert_media_schema = json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string", "description": "Filesystem path to media image. .zip is supported — single .adf is auto-detected; pass `entry` to pick one when multiple exist."},
            "entry": {"type": "string", "description": "Optional zip entry filename when `path` is a .zip with more than one .adf inside."},
            "kind": {"type": "string", "enum": ["adf"], "default": "adf",
                     "description": "Media kind. Only `adf` is wired today; `hdf`/`ipf` reserved."},
            "change_pending": {"type": "boolean", "default": true,
                               "description": "Use insert_adf_with_change_pending so KS sees a disk-change event."}
        }
    });

    add(registry, "run_frames",  "Advance the machine by N PAL frames.", frames_schema, tool_run_frames);
    add(registry, "run_ticks",   "Advance the machine by N master/4 ticks.", ticks_schema, tool_run_ticks);
    add(registry, "run_until_pc","Run until PC == target or max_ticks reached.", until_pc_schema, tool_run_until_pc);
    add(registry, "run_until_any_pc", "Run until PC matches any address in `targets` or max_ticks reached.", any_pc_schema, tool_run_until_any_pc);
    add(registry, "run_until_mem_change", "Run until any longword in `addrs` changes value, or max_ticks reached.", mem_change_schema, tool_run_until_mem_change);
    add(registry, "step",        "Step one or more CPU instructions, returning a PC trace.", step_schema, tool_step);
    let cpu_trace_arm_schema = json!({
        "type": "object",
        "properties": {
            "pc_min":      {"description": "Optional inclusive PC lower bound; entries outside the range are dropped before capture (hex/decimal)."},
            "pc_max":      {"description": "Optional inclusive PC upper bound."},
            "max_entries": {"type": "integer", "minimum": 1, "maximum": 10_000_000, "default": 100_000,
                            "description": "Hard cap on captured entries; further pushes are silently dropped past this point."}
        }
    });
    add(registry, "cpu_trace_arm",
        "Start recording an instruction-boundary CPU trace. Captures (cck, instr_start_pc, sr, opcode_word) at every instruction completion that subsequent `run_*` / `step` calls cross. Clears any prior trace; replaces filter + max_entries. Use `pc_min`/`pc_max` to capture only inside a region of interest (e.g. KS palette init).",
        cpu_trace_arm_schema, tool_cpu_trace_arm);
    add(registry, "cpu_trace_disarm",
        "Stop recording. The captured trace is kept; `cpu_trace_log` still reads it. Re-arming clears.",
        empty(), tool_cpu_trace_disarm);
    add(registry, "cpu_trace_clear",
        "Discard captured entries without disarming. Lets you focus on a fresh window without re-arming.",
        empty(), tool_cpu_trace_clear);
    let cpu_trace_log_schema = json!({
        "type": "object",
        "properties": {
            "limit":      {"type": "integer", "minimum": 1, "maximum": 10_000_000, "default": 256,
                           "description": "Maximum entries to return. Default 256 to keep responses compact."},
            "from_start": {"type": "boolean", "default": false,
                           "description": "If false (default), return the trailing `limit` entries. If true, return the leading `limit` entries."},
            "cck_min":    {"type": "integer", "description": "Only include entries at or after this cck."},
            "cck_max":    {"type": "integer", "description": "Only include entries at or before this cck."}
        }
    });
    add(registry, "cpu_trace_log",
        "Dump captured CPU trace entries. Tail-window by default (most recent `limit`); pass `from_start:true` for the leading window. Filter by cck range for a specific time slice.",
        cpu_trace_log_schema, tool_cpu_trace_log);
    let reset_schema = json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["hard", "soft"],
                "default": "hard",
                "description": "Hard = power-cycle (reload ROM, rebuild machine). Soft = machine-local soft reset (intended to preserve RAM). Today both rebuild from the ROM; the kind is accepted so MCP scripts can use the same wire format as the shared shell layer."
            }
        }
    });
    add(registry, "reset",       "Reload the ROM and re-create the A1200 (fresh boot). Accepts an optional `kind` (\"hard\" / \"soft\"; both currently behave as hard).", reset_schema, tool_reset);
    add(registry, "query_cpu",   "Full CPU register snapshot (D0-D7, A0-A7, PC, SR, SSP, USP, VBR, IPL pin, exception state).", empty(), tool_query_cpu);
    add(registry, "query_chipset","BPLCON0 / DMACON / ADKCON / COLOR00 / COP1LC / copper PC / overlay state.", empty(), tool_query_chipset);
    add(registry, "query_paula", "Paula INTENA / INTREQ with bit names decoded.", empty(), tool_query_paula);
    add(registry, "query_cia",   "CIA-A + CIA-B timer / ICR / port / TOD snapshot.", empty(), tool_query_cia);
    add(registry, "query_agnus", "Agnus snapshot (vpos / hpos / bitplane pointers / blitter pointers).", empty(), tool_query_agnus);
    add(registry, "query_blitter","Blitter snapshot (busy, exec_pending, ccks_remaining, APT/BPT/CPT/DPT).", empty(), tool_query_blitter);
    add(registry, "query_exec_tasks",
        "Walk ExecBase (at $00000004) and dump ThisTask, TaskReady, TaskWait. Each entry decodes the Exec Node (name, type, priority) + Task (state, tc_SigWait, tc_SigRecvd, SP, user data). Use to find what WB.Workbench is blocked on — a non-zero `tc_sig_wait` on the WB task with `tc_state=WAIT` shows the signal it's waiting for.",
        empty(), tool_query_exec_tasks);
    add(registry, "query_exec_ports",
        "Walk ExecBase->PortList (SysBase+392) and dump every public MsgPort: name, mp_SigBit (which signal bit notifies the owner), mp_SigTask (owning task address), mp_Flags (PA_SIGNAL / PA_SOFTINT / PA_IGNORE), and queued-message count. Use to find which port WB.Workbench is blocked on — cross-reference `mp_sigtask` against `query_exec_tasks` Workbench addr, look for the port with the matching `mp_sigbit_mask`.",
        empty(), tool_query_exec_ports);
    let query_aga_schema = json!({
        "type": "object",
        "properties": {
            "all_banks": {"type": "boolean", "default": false,
                          "description": "Include the full 256-entry palette_24 dump in the response."}
        }
    });
    add(registry, "query_aga",    "AGA Lisa state. DENISEID, BPLCON3 bank+LOCT, BPLCON4, palette_24 bank 0 + non-zero counts per bank, OCS 12-bit palette side-by-side. Pass `all_banks:true` for the full 256-entry dump.", query_aga_schema, tool_query_aga);
    let palette_log_schema = json!({
        "type": "object",
        "properties": {
            "limit":         {"type": "integer", "minimum": 1, "maximum": 4096, "default": 64},
            "only_color":    {"type": "boolean", "default": false},
            "only_bplcon3":  {"type": "boolean", "default": false},
            "unique":        {"type": "boolean", "default": false,
                              "description": "De-dupe by (offset, val, bank, LOCT). Drops repeated copper-list rewrites."},
            "color_idx_range": {"type": "array", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2,
                                "description": "Filter to COLOR writes whose index is in [lo, hi] inclusive."}
        }
    });
    add(registry, "palette_log", "Every COLOR / BPLCON3 write captured during the run, with BPLCON3 BANK + LOCT decoded for each write. Use to reconstruct the AGA palette-programming sequence KS uses.", palette_log_schema, tool_palette_log);
    let restart_schema = json!({
        "type": "object",
        "properties": {
            "exit_code": {"type": "integer", "default": 0,
                          "description": "Process exit code. Non-zero useful for hosts that only respawn on crash."}
        }
    });
    add(registry, "restart",
        "Exit the MCP server process so the host re-spawns the freshly built binary on the next call. Response is flushed before exit.",
        restart_schema, tool_restart);
    let watch_set_schema = json!({
        "type": "object",
        "required": ["addr", "len"],
        "properties": {
            "addr": {"description": "Watch range low address (inclusive). Hex/decimal accepted."},
            "len":  {"description": "Watch range length in bytes. Hex/decimal accepted."}
        }
    });
    let watch_log_schema = json!({
        "type": "object",
        "properties": {
            "limit":  {"type": "integer", "minimum": 1, "maximum": 8192, "default": 64},
            "unique": {"type": "boolean", "default": false,
                       "description": "De-dupe by (PC, addr, value). Drops repeated identical writes."}
        }
    });
    let poke_word_schema = json!({
        "type": "object",
        "required": ["addr", "val"],
        "properties": {
            "addr": {"description": "Address to write (any bus-routed address — chip RAM, custom register, etc.). Hex/decimal accepted."},
            "val":  {"description": "16-bit value to write. Hex/decimal accepted."}
        }
    });
    add(registry, "poke_word",
        "Backdoor word write via the machine's `poke_word` path. Useful for testing: e.g. force-write to a chipset COLOR register and see if the display reflects it.",
        poke_word_schema, tool_poke_word);
    let chipset_read_schema = json!({
        "type": "object",
        "properties": {
            "limit":   {"type": "integer", "minimum": 1, "maximum": 8192, "default": 64},
            "unique":  {"type": "boolean", "default": false,
                        "description": "Shorthand for dedupe:pc_off_val."},
            "dedupe":  {"type": "string", "enum": ["none", "pc_off", "pc_off_val", "off"],
                        "default": "none",
                        "description": "Dedupe granularity. `pc_off` collapses identical read sites (noisy beam-position polls)."},
            "offset":  {"description": "Optional chipset offset to filter by (hex/decimal)."},
            "cck_min": {"type": "integer", "description": "Only include reads at or after this cck."},
            "cck_max": {"type": "integer", "description": "Only include reads at or before this cck."}
        }
    });
    add(registry, "chipset_read_log",
        "Every CPU read from a chipset register ($DFFxxx) with the returned value and PC. Filter by `offset:` to see one register's read history, e.g. what value KS observed for DENISEID across the boot.",
        chipset_read_schema, tool_chipset_read_log);
    let chipset_write_schema = json!({
        "type": "object",
        "properties": {
            "limit":      {"type": "integer", "minimum": 1, "maximum": 8192, "default": 64},
            "dedupe":     {"type": "string", "enum": ["none", "pc_off", "pc_off_val", "off"],
                           "default": "none",
                           "description": "Dedupe granularity. `pc_off` collapses identical write sites (e.g. copper-driven per-line writes)."},
            "offset":     {"description": "Exact chipset offset to filter by (hex/decimal)."},
            "offset_min": {"type": "integer", "description": "Inclusive lower offset bound (e.g. 0x80 to capture only copper-list-pointer writes)."},
            "offset_max": {"type": "integer", "description": "Inclusive upper offset bound."},
            "cck_min":    {"type": "integer", "description": "Only include writes at or after this cck."},
            "cck_max":    {"type": "integer", "description": "Only include writes at or before this cck."}
        }
    });
    add(registry, "chipset_write_log",
        "Every CPU write to a chipset register ($DFFxxx). Filter by `offset:` for one register's history, or `offset_min`/`offset_max` for a range (e.g. 0x080..0x086 to track all COP1LC/COP2LC writes). Useful for answering 'when did cop2lc change?' or 'what writes hit $DFF000 during boot?' without polling.",
        chipset_write_schema, tool_chipset_write_log);
    add(registry, "watch_memory",
        "Set a write-watchpoint on a chip-RAM byte range. Captures every CPU bus write that lands in the range as (cck, pc, addr, val, size). Clears any prior log.",
        watch_set_schema, tool_watch_memory);
    add(registry, "watch_memory_clear",
        "Clear the active write-watchpoint (stops further capture). Returns how many writes were captured.",
        empty(), tool_watch_memory_clear);
    add(registry, "watch_memory_log",
        "Dump the writes captured by the watchpoint. `unique:true` de-dupes by (PC, addr, value).",
        watch_log_schema, tool_watch_memory_log);
    let bplcon0_log_schema = json!({
        "type": "object",
        "properties": {
            "unique": {"type": "boolean", "default": false,
                       "description": "Return only the first occurrence of each distinct BPLCON0 value."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 1024, "default": 64}
        }
    });
    add(registry, "bplcon0_log", "Every BPLCON0 write captured during the run (CPU + copper). Includes BPU histogram so 'does KS ever try BPU>0?' is one query.", bplcon0_log_schema, tool_bplcon0_log);
    let dump_fb_schema = json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Optional filesystem path for a PNG snapshot. Omit to skip the write."}
        }
    });
    add(registry, "dump_framebuffer", "Snapshot the Denise ARGB framebuffer: top colours, FNV-1a hash, optional PNG write.", dump_fb_schema, tool_dump_framebuffer);
    let start_rec_schema = json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string", "description": "Output MP4 path. Parent directories are created."},
            "fps": {"type": "integer", "minimum": 1, "default": 50,
                    "description": "Frame rate written to the MP4. Default is PAL (50)."}
        }
    });
    add(registry, "start_video_recording",
        "Begin recording the live framebuffer to one MP4 file (uses ffmpeg from PATH).",
        start_rec_schema, tool_start_video_recording);
    add(registry, "stop_video_recording",
        "Finalise the in-flight recording and return the MP4 summary.",
        empty(), tool_stop_video_recording);
    add(registry, "query_copper_list", "Decode the copper list at `addr` (or COP1LC) into MOVE/WAIT/SKIP entries.", copper_list_schema, tool_query_copper_list);
    add(registry, "query_stack", "Read `count` longwords off SSP (or USP via `usp:true`).", stack_schema, tool_query_stack);
    add(registry, "memory_read", "Read raw bytes from any address (chip RAM / ROM / chipset).", memory_schema, tool_memory_read);
    add(registry, "memory_read_long", "Read a 32-bit longword from an address.", addr_only, tool_memory_read_long);
    add(registry, "disasm",      "Disassemble `count` m68k instructions starting at `addr`.", disasm_schema, tool_disasm);
    add(registry, "insert_media", "Insert disk media into DF0 (only `adf` kind today; use `change_pending:true` to fire a disk-change event).", insert_media_schema, tool_insert_media);
    add(registry, "eject_media",  "Eject any disk currently in DF0.", empty(), tool_eject_media);
    add(registry, "query_disk",   "DF0 drive status (cylinder, head, motor, status bits, has_disk).", empty(), tool_query_disk);
}
