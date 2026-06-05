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
use emu198x_shell::{
    CapturedFrame, HeadlessSession, MachineTime, PixelFormat, SessionQueryProvider, VideoRecorder,
};
use machine_commodore_amiga_a1200::{Adf, FB_HEIGHT, FB_WIDTH, PAL_FRAME_TICKS};
use motorola_68000::disasm::disassemble;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind};
use serde_json::{Value, json};

use super::lvo;
use super::session::AmigaSession;

/// Tool execution context — the live Amiga chip surface, behind a trait so the
/// tool bodies serve both the legacy [`AmigaSession`] and the shared
/// `HeadlessSession<AmigaRuntimeKind, _>` during the Phase-4 replatform. Each
/// ported tool takes `&mut impl AmigaCtx` and reads through `live()` /
/// `live_mut()`, so the *same* body works for both sessions and the eventual
/// `mcp/mod.rs` cutover is a session-type swap, not a tool rewrite. See the
/// Phase-4 port spec in `docs/plans/2026-06-05-refactor-amiga-unified-driver-replatform.md`.
pub(crate) trait AmigaCtx {
    /// Shared read view of the active chipset variant.
    fn live(&self) -> &dyn AmigaLiveAccess;
    /// Shared mutable view (memory pokes, tracer arming).
    fn live_mut(&mut self) -> &mut dyn AmigaLiveAccess;
}

impl AmigaCtx for AmigaSession {
    fn live(&self) -> &dyn AmigaLiveAccess {
        // Fully-qualified inherent call: `self.live()` would recurse, and the
        // file-wide `.live()` → `.live()` rewrite must not touch this line.
        AmigaSession::access(self)
    }
    fn live_mut(&mut self) -> &mut dyn AmigaLiveAccess {
        AmigaSession::access_mut(self)
    }
}

impl<Q> AmigaCtx for HeadlessSession<AmigaRuntimeKind, Q>
where
    Q: SessionQueryProvider<AmigaRuntimeKind>,
{
    fn live(&self) -> &dyn AmigaLiveAccess {
        self.machine()
    }
    fn live_mut(&mut self) -> &mut dyn AmigaLiveAccess {
        self.machine_mut()
    }
}

/// Wrap a free function as a `Tool` impl over any session context `C`.
/// The function receives parsed arguments and a mutable session
/// reference and returns the JSON response body. Generic over `C` so the
/// same inline tools register on both the legacy [`AmigaSession`] and the
/// shared `HeadlessSession` (the run fns are generic over `AmigaCtx`).
struct InlineTool<C> {
    name: &'static str,
    description: &'static str,
    schema: Value,
    run: fn(Value, &mut C) -> Result<Value, ToolError>,
}

// `InlineTool<C>` holds only a fn pointer + owned data, so it is
// unconditionally `Send + Sync` regardless of `C` — no bound needed.
impl<C> Tool<C> for InlineTool<C> {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn input_schema(&self) -> Value {
        self.schema.clone()
    }
    fn call(&self, arguments: Value, session: &mut C) -> Result<ToolResponse, ToolError> {
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
        let (body, radix) = if let Some(rest) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
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
    u32::try_from(v)
        .map_err(|_| ToolError::InvalidArguments(format!("`{key}` value {v} doesn't fit in u32")))
}

/// Read a longword from chip RAM (or wherever the machine routes
/// the address) using the machine's existing memory backdoor. Falls
/// back to assembling from bytes for non-chip-RAM addresses so we
/// can dump ROM too. Routes through [`AmigaLiveAccess`] so it works
/// against any chipset variant the session may be hosting.
fn read_long(session: &impl AmigaCtx, addr: u32) -> u32 {
    session.live().read_long(addr)
}

fn read_byte(session: &impl AmigaCtx, addr: u32) -> u8 {
    let aligned = addr & !1;
    let long = session.live().read_long(aligned & !2);
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
        let access = s.live();
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
    // Ticks route through the runtime's `tick_traced` (via the live
    // surface), so an armed `cpu_trace` captures every instruction
    // boundary regardless of which run tool drove it. Overhead when
    // disarmed is one bool check per tick.
    if s.recorder.is_none() {
        for _ in 0..ticks {
            s.access_mut().tick();
        }
        return Ok(());
    }
    for _ in 0..ticks {
        s.access_mut().tick();
        let now = s.live().tick_count();
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
        "pc": format!("${:08X}", s.live().cpu_pc()),
        "recording_frames": s.recorder.as_ref().map(|r| r.frames_written()),
    }))
}

fn tool_run_ticks(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
    let n = arg_u64_or(&args, "ticks", 1)?;
    tick_for(s, n)?;
    Ok(json!({
        "ticks_run": n,
        "pc": format!("${:08X}", s.live().cpu_pc()),
        "recording_frames": s.recorder.as_ref().map(|r| r.frames_written()),
    }))
}

fn tool_run_until_pc(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let target = arg_u32(&args, "target")?;
    let max_ticks = arg_u64_or(&args, "max_ticks", 100_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit = false;
    while ticks_taken < max_ticks {
        s.live_mut().tick();
        ticks_taken += 1;
        if s.live().cpu_pc() == target {
            hit = true;
            break;
        }
    }
    Ok(json!({
        "hit": hit,
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.live().cpu_pc()),
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
        "pc": format!("${:08X}", s.live().cpu_pc()),
    }))
}

fn tool_query_cpu(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let cpu = s.live().cpu_snapshot();
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

fn tool_query_chipset(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let m = s.live();
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

fn tool_query_paula(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let access = s.live();
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
        "TBE", "DSKBLK", "SOFT", "PORTS", "COPER", "VERTB", "BLIT", "AUD0", "AUD1", "AUD2", "AUD3",
        "RBF", "DSKSYN", "EXTER", "INTEN",
    ];
    let mut out = serde_json::Map::new();
    for (bit, name) in NAMES.iter().enumerate() {
        if val & (1 << bit) != 0 {
            out.insert((*name).to_string(), Value::Bool(true));
        }
    }
    Value::Object(out)
}

fn tool_query_cia(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
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
    let access = s.live();
    Ok(json!({
        "cia_a": snapshot(access.cia_a()),
        "cia_b": snapshot(access.cia_b()),
    }))
}

fn tool_query_agnus(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let a = s.live().agnus();
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

/// Decode Exec ln_Type into the RKM mnemonic. Used to flag
/// NT_PROCESS (=13) tasks so the inspector can decode their
/// trailing Process struct.
fn node_type_label(ln_type: u8) -> &'static str {
    match ln_type {
        0 => "UNKNOWN",
        1 => "TASK",
        2 => "INTERRUPT",
        3 => "DEVICE",
        4 => "MSGPORT",
        5 => "MESSAGE",
        6 => "FREEMSG",
        7 => "REPLYMSG",
        8 => "RESOURCE",
        9 => "LIBRARY",
        10 => "MEMORY",
        11 => "SOFTINT",
        12 => "FONT",
        13 => "PROCESS",
        14 => "SEMAPHORE",
        15 => "SIGNALSEM",
        16 => "BOOTNODE",
        17 => "KICKMEM",
        18 => "GRAPHICS",
        19 => "DEATHMESSAGE",
        _ => "?",
    }
}

/// Decode the Process extension that sits after the Task struct
/// when `ln_Type == NT_PROCESS` (=13). Offsets are from the start
/// of the Process struct (= same address as the Task), per
/// `dos/dosextens.i` in the RKM. BPTRs are stored as
/// `address >> 2` — we report both the raw BPTR and the converted
/// byte address so the consumer doesn't have to do the maths.
fn read_exec_process(access: &dyn runtime_commodore_amiga::AmigaLiveAccess, addr: u32) -> Value {
    // Process struct: pr_Task occupies bytes 0..92 (we already
    // decoded it). The Process-specific fields begin at +92.
    let pr_msgport = addr.wrapping_add(92); // struct MsgPort — embedded
    let pr_seg_list = access.read_long(addr.wrapping_add(128));
    let pr_stack_size = access.read_long(addr.wrapping_add(132));
    let pr_glob_vec = access.read_long(addr.wrapping_add(136));
    let pr_task_num = access.read_long(addr.wrapping_add(140));
    let pr_stack_base = access.read_long(addr.wrapping_add(144));
    let pr_result2 = access.read_long(addr.wrapping_add(148));
    let pr_current_dir = access.read_long(addr.wrapping_add(152));
    let pr_cis = access.read_long(addr.wrapping_add(156));
    let pr_cos = access.read_long(addr.wrapping_add(160));
    let pr_console_task = access.read_long(addr.wrapping_add(164));
    let pr_fs_task = access.read_long(addr.wrapping_add(168));
    let pr_cli = access.read_long(addr.wrapping_add(172));
    let pr_window_ptr = access.read_long(addr.wrapping_add(184));
    let pr_home_dir = access.read_long(addr.wrapping_add(188)); // 3.0+
    let pr_flags = access.read_long(addr.wrapping_add(192)); // 3.0+
    let pr_exit_code = access.read_long(addr.wrapping_add(196)); // 3.0+
    let pr_arguments = access.read_long(addr.wrapping_add(204));
    let pr_ces = access.read_long(addr.wrapping_add(224)); // 3.0+

    let bptr_to_addr = |b: u32| b.wrapping_shl(2);
    // BPTR(0) is the BCPL null. Report a `<null>` marker so callers
    // don't waste time chasing a zero BPTR.
    let fmt_bptr = |b: u32| {
        if b == 0 {
            json!({"bptr": "$00000000", "addr": "<null>"})
        } else {
            json!({
                "bptr": format!("${:08X}", b),
                "addr": format!("${:08X}", bptr_to_addr(b)),
            })
        }
    };

    // pr_MsgPort is an *embedded* MsgPort (not a pointer to one);
    // decode it in place using the existing port helper so the
    // caller can see if any messages have queued up for this Process.
    let msg_port = read_exec_port(access, pr_msgport);

    json!({
        "pr_msgport_addr":  format!("${:08X}", pr_msgport),
        "pr_msgport":       msg_port,
        "pr_seg_list":      fmt_bptr(pr_seg_list),
        "pr_stack_size":    pr_stack_size,
        "pr_glob_vec":      format!("${:08X}", pr_glob_vec),
        "pr_task_num":      pr_task_num,
        "pr_stack_base":    fmt_bptr(pr_stack_base),
        "pr_result2":       pr_result2,
        "pr_current_dir":   fmt_bptr(pr_current_dir),
        "pr_cis":           fmt_bptr(pr_cis),
        "pr_cos":           fmt_bptr(pr_cos),
        "pr_console_task":  format!("${:08X}", pr_console_task),
        "pr_fs_task":       format!("${:08X}", pr_fs_task),
        "pr_cli":           fmt_bptr(pr_cli),
        "pr_window_ptr":    format!("${:08X}", pr_window_ptr),
        "pr_home_dir":      fmt_bptr(pr_home_dir),
        "pr_flags":         format!("${:08X}", pr_flags),
        "pr_exit_code":     format!("${:08X}", pr_exit_code),
        "pr_arguments":     format!("${:08X}", pr_arguments),
        "pr_ces":           fmt_bptr(pr_ces),
    })
}

/// Decode one Exec `Task` struct (with embedded `Node` at offset 0)
/// into JSON. Field offsets follow the AmigaOS RKM (Exec/Tasks).
/// When `ln_Type == NT_PROCESS` (=13), the trailing Process
/// extension is decoded too — IPrefs / Workbench / shell are all
/// Processes, not bare Tasks, so this is essential for tracing
/// WB-side wedges.
fn read_exec_task(access: &dyn runtime_commodore_amiga::AmigaLiveAccess, addr: u32) -> Value {
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
    let mut obj = json!({
        "addr": format!("${:08X}", addr),
        "ln_name": name,
        "ln_succ": format!("${:08X}", ln_succ),
        "ln_pred": format!("${:08X}", ln_pred),
        "ln_type": ln_type,
        "ln_type_label": node_type_label(ln_type),
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
    });
    if ln_type == 13 {
        // NT_PROCESS: decode the trailing Process struct so callers
        // can see pr_MsgPort, pr_CIS/COS/CES, pr_CurrentDir, etc.
        let process = read_exec_process(access, addr);
        if let Value::Object(map) = &mut obj {
            map.insert("process".to_string(), process);
        }
    }
    obj
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

fn tool_query_exec_tasks(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // ExecBase pointer lives at $00000004 — set by Exec during boot.
    // Until KS has booted past the very early stages, this read will
    // return $00000000 (chip RAM is zero) and the rest degrades
    // gracefully (empty lists, null this_task).
    let access = s.live();
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
fn read_exec_port(access: &dyn runtime_commodore_amiga::AmigaLiveAccess, addr: u32) -> Value {
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

/// Decode an Exec `Message` struct. Layout (Exec/Ports RKM):
///   Node (14B) + mn_ReplyPort (4B) + mn_Length (2B)
/// `mn_Length` is the size of the message *including* the Message
/// struct header itself, so callers can find any application
/// payload at `addr + 20` extending to `addr + mn_Length`.
fn read_exec_message(access: &dyn runtime_commodore_amiga::AmigaLiveAccess, addr: u32) -> Value {
    let ln_succ = access.read_long(addr);
    let type_pri = access.read_word(addr.wrapping_add(8));
    let ln_type = (type_pri >> 8) as u8;
    let ln_pri = (type_pri & 0xFF) as i8;
    let ln_name = access.read_long(addr.wrapping_add(10));
    let name = read_amiga_cstring(access, ln_name, 64);
    let mn_reply_port = access.read_long(addr.wrapping_add(14));
    let mn_length = access.read_word(addr.wrapping_add(18));
    json!({
        "addr":          format!("${:08X}", addr),
        "ln_succ":       format!("${:08X}", ln_succ),
        "ln_type":       ln_type,
        "ln_type_label": node_type_label(ln_type),
        "ln_pri":        ln_pri,
        "ln_name":       name,
        "mn_reply_port": format!("${:08X}", mn_reply_port),
        "mn_length":     mn_length,
    })
}

/// Walk one port's `mp_MsgList` and decode every queued message.
/// Mirrors the queue-count walk in `read_exec_port` but returns the
/// full decoded list instead of just a count. Use when a port has
/// `msg_count > 0` and you want to see what's queued (e.g. for a
/// device port to see pending IORequests, or for a process's DOS
/// port to see pending packets).
fn tool_dump_msgport_messages(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let port_addr = arg_u32(&args, "port")?;
    let max = arg_u64_or(&args, "max", 64)? as usize;
    let access = s.live();
    // Re-decode the port header so callers can sanity-check (e.g.
    // confirm ln_Type == NT_MSGPORT).
    let port_summary = read_exec_port(access, port_addr);
    let list_addr = port_addr.wrapping_add(20);
    let sentinel = list_addr.wrapping_add(4);
    let mut messages: Vec<Value> = Vec::new();
    let mut cur = access.read_long(list_addr);
    let mut seen = std::collections::HashSet::new();
    while messages.len() < max {
        if cur == 0 || cur == sentinel {
            break;
        }
        if !seen.insert(cur) {
            break;
        }
        messages.push(read_exec_message(access, cur));
        cur = access.read_long(cur);
    }
    let truncated = messages.len() >= max;
    Ok(json!({
        "port":      port_summary,
        "messages":  messages,
        "count":     messages.len(),
        "truncated": truncated,
    }))
}

/// Inject signal bits into a task's tc_SigRecvd. This is a
/// debugging-only mutator — it ORs bits into tc_SigRecvd. It does
/// NOT call exec's Signal() and does NOT touch tc_State or the
/// TaskReady/TaskWait lists.
///
/// Important: this is NOT a wake-up tool. In real exec, Signal()
/// performs the wake transition synchronously inside the API call —
/// move from TaskWait → TaskReady, set tc_State = READY, kick the
/// dispatcher. The scheduler itself does NOT poll tc_SigRecvd; it
/// acts only on the API path. So writing tc_SigRecvd from outside
/// makes the bits *visible* on the task struct (a subsequent
/// `query_exec_tasks` will see them) but the task stays parked
/// until something else triggers the list manipulation.
///
/// Use cases:
///   * Inspect the wake-condition. Set bits, run a frame, see if
///     the task got woken (it usually won't, for the reason above
///     — that itself is useful info: it tells you the OS path
///     responsible for delivering the signal hasn't run).
///   * Pre-stage bits so the NEXT scheduler-driven Wait() call
///     returns immediately (because sig_recvd will already cover
///     sig_wait when Wait() checks).
///
/// Safety notes:
///   * If the bits you set don't intersect tc_SigAlloc, the task
///     wasn't expecting them and may produce unexpected behaviour
///     when it next checks signals.
///   * `would_wake` in the response is `(sig_recvd_after &
///     sig_wait) != 0 && state == WAIT` — i.e. "the wake CONDITION
///     is satisfied". Whether the task actually wakes depends on
///     whether something invokes Signal() afterwards.
fn tool_signal_task(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let task_addr = arg_u32(&args, "task_addr")?;
    let signals = arg_u32(&args, "signals")?;
    // Read current state for the before snapshot + a sanity check
    // that this looks like a Task.
    let (old_sig_recvd, sig_alloc, sig_wait, tc_state) = {
        let access = s.live();
        let type_pri = access.read_word(task_addr.wrapping_add(8));
        let ln_type = (type_pri >> 8) as u8;
        // NT_TASK = 1, NT_PROCESS = 13. Anything else and the user
        // likely passed an address that isn't a Task.
        if ln_type != 1 && ln_type != 13 {
            return Err(ToolError::InvalidArguments(format!(
                "address ${:08X} has ln_type={ln_type} ({}) — not NT_TASK (1) or NT_PROCESS (13)",
                task_addr,
                node_type_label(ln_type),
            )));
        }
        let flags_state = access.read_word(task_addr.wrapping_add(14));
        let state = (flags_state & 0xFF) as u8;
        let alloc = access.read_long(task_addr.wrapping_add(18));
        let wait = access.read_long(task_addr.wrapping_add(22));
        let recvd = access.read_long(task_addr.wrapping_add(26));
        (recvd, alloc, wait, state)
    };
    let new_sig_recvd = old_sig_recvd | signals;
    // Write back via two word pokes (poke_long isn't on the trait).
    {
        let access = s.live_mut();
        access.poke_word(task_addr.wrapping_add(26), (new_sig_recvd >> 16) as u16);
        access.poke_word(task_addr.wrapping_add(28), (new_sig_recvd & 0xFFFF) as u16);
    }
    let would_wake = (new_sig_recvd & sig_wait) != 0 && tc_state == 4; // 4 = WAIT
    Ok(json!({
        "task_addr":         format!("${:08X}", task_addr),
        "signals_requested": format!("${:08X}", signals),
        "sig_recvd_before":  format!("${:08X}", old_sig_recvd),
        "sig_recvd_after":   format!("${:08X}", new_sig_recvd),
        "sig_alloc":         format!("${:08X}", sig_alloc),
        "sig_wait":          format!("${:08X}", sig_wait),
        "tc_state":          tc_state,
        "tc_state_label":    match tc_state {
            0 => "INVALID", 1 => "ADDED", 2 => "RUN", 3 => "READY",
            4 => "WAIT", 5 => "EXCEPT", 6 => "REMOVED", _ => "?",
        },
        "bits_outside_sig_alloc": format!("${:08X}", signals & !sig_alloc),
        "would_wake":             would_wake,
        "scheduler_kick_note":    "Bits are written to tc_SigRecvd. The task is NOT moved to TaskReady — real exec's Signal() does that synchronously, the scheduler does not poll. To force a wake, use a future `wake_task` tool or call into exec.Signal yourself.",
    }))
}

/// Helper: write a 32-bit longword via two 16-bit pokes. The
/// AmigaLiveAccess trait has poke_byte / poke_word but not poke_long.
fn poke_long(access: &mut dyn runtime_commodore_amiga::AmigaLiveAccess, addr: u32, value: u32) {
    access.poke_word(addr, (value >> 16) as u16);
    access.poke_word(addr.wrapping_add(2), (value & 0xFFFF) as u16);
}

/// MUTATOR: do the full TaskWait → TaskReady transition that exec's
/// Signal() performs internally. Unlinks the node from TaskWait,
/// appends to TaskReady (at the tail — exec normally enqueues by
/// priority, but the dispatcher will run any READY task on the next
/// switch). Sets tc_State = READY. Optionally ORs `signals` into
/// tc_SigRecvd first; if not provided, defaults to OR'ing in
/// tc_SigWait so that the task's pending Wait() call returns
/// immediately when the dispatcher resumes it.
///
/// This bypasses Forbid/Permit and the supervisor-mode invariants
/// that real exec maintains around list manipulation. It is a
/// debugging tool only — use for testing "would the wedge clear
/// if this task could just run?" hypotheses.
///
/// Safety:
///   * Only operates on tasks in WAIT state. Tasks in RUN / READY
///     / ADDED / EXCEPT are rejected.
///   * Validates list integrity (ln_Pred.ln_Succ == task &&
///     ln_Succ.ln_Pred == task) before unlinking. A corrupt list is
///     refused — bailing out is safer than scribbling.
fn tool_wake_task(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let task_addr = arg_u32(&args, "task_addr")?;
    let extra_signals = match args.get("signals") {
        Some(v) if !v.is_null() => Some(arg_u32(&args, "signals")?),
        _ => None,
    };
    // Pre-flight: confirm this is a Task/Process in WAIT state.
    let (state_before, sig_wait, sig_alloc, sig_recvd_before, ln_succ, ln_pred) = {
        let access = s.live();
        let type_pri = access.read_word(task_addr.wrapping_add(8));
        let ln_type = (type_pri >> 8) as u8;
        if ln_type != 1 && ln_type != 13 {
            return Err(ToolError::InvalidArguments(format!(
                "address ${:08X} has ln_type={ln_type} ({}) — not NT_TASK (1) or NT_PROCESS (13)",
                task_addr,
                node_type_label(ln_type),
            )));
        }
        let flags_state = access.read_word(task_addr.wrapping_add(14));
        let state = (flags_state & 0xFF) as u8;
        if state != 4 {
            return Err(ToolError::InvalidArguments(format!(
                "task ${:08X} is in state {} ({}) — wake_task only operates on WAIT (4)",
                task_addr,
                state,
                match state {
                    0 => "INVALID",
                    1 => "ADDED",
                    2 => "RUN",
                    3 => "READY",
                    4 => "WAIT",
                    5 => "EXCEPT",
                    6 => "REMOVED",
                    _ => "?",
                },
            )));
        }
        (
            state,
            access.read_long(task_addr.wrapping_add(22)),
            access.read_long(task_addr.wrapping_add(18)),
            access.read_long(task_addr.wrapping_add(26)),
            access.read_long(task_addr),                 // ln_Succ
            access.read_long(task_addr.wrapping_add(4)), // ln_Pred
        )
    };
    // List-integrity check. In a healthy list:
    //   task.ln_Pred.ln_Succ == task
    //   task.ln_Succ.ln_Pred == task
    {
        let access = s.live();
        let pred_succ = access.read_long(ln_pred);
        let succ_pred = access.read_long(ln_succ.wrapping_add(4));
        if pred_succ != task_addr || succ_pred != task_addr {
            return Err(ToolError::Execution(format!(
                "list integrity check failed: pred.succ=${:08X}, succ.pred=${:08X}, task=${:08X} — bailing rather than scribble a bad list",
                pred_succ, succ_pred, task_addr,
            )));
        }
    }
    // OR in the wake signals. Default = sig_wait (Wait() returns
    // immediately when the dispatcher resumes the task).
    let signals = extra_signals.unwrap_or(sig_wait);
    let sig_recvd_after = sig_recvd_before | signals;
    // Locate TaskReady list. SysBase + 406.
    let exec_base = {
        let access = s.live();
        access.read_long(0x0000_0004)
    };
    if exec_base == 0 {
        return Err(ToolError::Execution(
            "ExecBase is null — can't locate TaskReady list".into(),
        ));
    }
    let task_ready_head = exec_base.wrapping_add(406);
    // Snapshot the TaskReady list's lh_TailPred so we can append.
    let ready_tail_pred = {
        let access = s.live();
        access.read_long(task_ready_head.wrapping_add(8))
    };
    let access = s.live_mut();
    // 1. Write the wake signals.
    poke_long(access, task_addr.wrapping_add(26), sig_recvd_after);
    // 2. Unlink the task from its current list (TaskWait):
    //      task.ln_Pred.ln_Succ = task.ln_Succ
    //      task.ln_Succ.ln_Pred = task.ln_Pred
    poke_long(access, ln_pred, ln_succ);
    poke_long(access, ln_succ.wrapping_add(4), ln_pred);
    // 3. Append the task to TaskReady's tail.
    //    new node's ln_Succ = &lh_Tail (= task_ready_head + 4)
    //    new node's ln_Pred = old_tail_pred
    //    old_tail_pred.ln_Succ = task
    //    task_ready_head.lh_TailPred = task
    poke_long(access, task_addr, task_ready_head.wrapping_add(4));
    poke_long(access, task_addr.wrapping_add(4), ready_tail_pred);
    poke_long(access, ready_tail_pred, task_addr);
    poke_long(access, task_ready_head.wrapping_add(8), task_addr);
    // 4. Set tc_State = READY (3). Keep the existing tc_Flags byte.
    let flags_state = access.read_word(task_addr.wrapping_add(14));
    let new_flags_state = (flags_state & 0xFF00) | 3;
    access.poke_word(task_addr.wrapping_add(14), new_flags_state);
    Ok(json!({
        "task_addr":           format!("${:08X}", task_addr),
        "state_before":        state_before,
        "state_before_label":  "WAIT",
        "state_after":         3,
        "state_after_label":   "READY",
        "sig_recvd_before":    format!("${:08X}", sig_recvd_before),
        "sig_recvd_after":     format!("${:08X}", sig_recvd_after),
        "sig_wait":            format!("${:08X}", sig_wait),
        "sig_alloc":           format!("${:08X}", sig_alloc),
        "signals_applied":     format!("${:08X}", signals),
        "wake_condition_met":  (sig_recvd_after & sig_wait) != 0,
        "next_step_note":      "Task is now in TaskReady. Call `run_frames` to let the dispatcher pick it up — usually the next VBlank IRQ triggers a Switch into the task.",
    }))
}

fn tool_query_exec_ports(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // ExecBase->PortList lives at SysBase + 392.
    let access = s.live();
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

/// Default ROM range for AGA-era Amigas (KS 2.0+ on A500+/A1200/A3000/A4000).
/// KS 1.x used a 256 KiB ROM mapped at $00FC0000 only — for those models
/// callers can pass an explicit `rom_lo` / `rom_hi`, but the default here
/// covers any 512 KiB Kickstart (the common case for this investigation).
const ROM_DEFAULT_LO: u32 = 0x00F8_0000;
const ROM_DEFAULT_HI: u32 = 0x00FF_FFFF;

/// Walk one library's struct Library at `addr` and produce a
/// schema-stable JSON entry. Library layout (from `exec/libraries.i`):
/// Node (14B) + lib_Flags (B) + lib_pad (B) + lib_NegSize (W) +
/// lib_PosSize (W) + lib_Version (W) + lib_Revision (W) +
/// lib_IdString (L) + lib_Sum (L) + lib_OpenCnt (W).
///
/// Two range pairs are reported:
///   * `chip_lo..chip_hi`  — `[base - NegSize, base + PosSize)`. The
///     chip-RAM extent: jump table + struct Library + private data.
///   * `code_lo..code_hi`  — derived from the library's JMP targets.
///     This is the ROM (or chip-RAM, for disk-loaded libraries) range
///     the jump table dispatches into. For Kickstart-resident
///     libraries this is the actual ROM code body; for LoadSeg'd
///     libraries it's wherever LoadSeg put the seglist.
fn read_exec_library(access: &dyn runtime_commodore_amiga::AmigaLiveAccess, addr: u32) -> Value {
    let type_pri = access.read_word(addr.wrapping_add(8));
    let ln_type = (type_pri >> 8) as u8;
    let ln_pri = (type_pri & 0xFF) as i8;
    let ln_name = access.read_long(addr.wrapping_add(10));
    let name = read_amiga_cstring(access, ln_name, 64);
    let flags_pad = access.read_word(addr.wrapping_add(14));
    let lib_flags = (flags_pad >> 8) as u8;
    let neg_size = access.read_word(addr.wrapping_add(16));
    let pos_size = access.read_word(addr.wrapping_add(18));
    let version = access.read_word(addr.wrapping_add(20));
    let revision = access.read_word(addr.wrapping_add(22));
    let id_string_addr = access.read_long(addr.wrapping_add(24));
    let id_string = if id_string_addr != 0 {
        read_amiga_cstring(access, id_string_addr, 96)
    } else {
        String::new()
    };
    let lib_sum = access.read_long(addr.wrapping_add(28));
    let open_cnt = access.read_word(addr.wrapping_add(32));
    let chip_lo = addr.wrapping_sub(u32::from(neg_size));
    let chip_hi = addr.wrapping_add(u32::from(pos_size));
    let targets = library_jmp_targets(access, addr, u32::from(neg_size));
    let code_lo = targets.iter().copied().min().unwrap_or(0);
    let code_hi = targets
        .iter()
        .copied()
        .max()
        .map(|v| v.wrapping_add(6))
        .unwrap_or(0);
    let jmp_target_count = targets.len();
    json!({
        "addr":          format!("${:08X}", addr),
        "ln_name":       name,
        "ln_type":       ln_type,
        "ln_type_label": node_type_label(ln_type),
        "ln_pri":        ln_pri,
        "lib_flags":     format!("${:02X}", lib_flags),
        "neg_size":      neg_size,
        "pos_size":      pos_size,
        "version":       version,
        "revision":      revision,
        "id_string":     id_string,
        "lib_sum":       format!("${:08X}", lib_sum),
        "open_cnt":      open_cnt,
        "chip_lo":       format!("${:08X}", chip_lo),
        "chip_hi":       format!("${:08X}", chip_hi),
        "code_lo":       format!("${:08X}", code_lo),
        "code_hi":       format!("${:08X}", code_hi),
        "jmp_target_count": jmp_target_count,
        "code_note":     "code_lo/code_hi is the min/max of JMP-table targets; libraries' actual ROM code is non-contiguous within that span. Use `address_to_library` for accurate attribution — it does closest-preceding-JMP-target lookup.",
    })
}

fn tool_query_library(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let access = s.live();
    let exec_base = access.read_long(0x0000_0004);
    if exec_base == 0 {
        return Ok(json!({
            "exec_base": "$00000000",
            "note": "ExecBase not yet initialised — run a few hundred frames after boot before querying.",
        }));
    }
    // ExecBase->LibList lives at SysBase + 378 (struct List).
    let list_addr = exec_base.wrapping_add(378);
    let sentinel = list_addr.wrapping_add(4);
    let filter = args.get("name").and_then(Value::as_str).map(str::to_string);
    let mut libs: Vec<Value> = Vec::new();
    let mut cur = access.read_long(list_addr);
    let mut seen = std::collections::HashSet::new();
    while libs.len() < 64 {
        if cur == 0 || cur == sentinel {
            break;
        }
        if !seen.insert(cur) {
            break;
        }
        let lib = read_exec_library(access, cur);
        let name_ok = filter
            .as_deref()
            .is_none_or(|wanted| lib.get("ln_name").and_then(Value::as_str) == Some(wanted));
        if name_ok {
            libs.push(lib);
        }
        cur = access.read_long(cur);
    }
    Ok(json!({
        "exec_base":     format!("${:08X}", exec_base),
        "list_addr":     format!("${:08X}", list_addr),
        "library_count": libs.len(),
        "libraries":     libs,
    }))
}

/// One library's effective extent. Stores chip-RAM range (covers the
/// jump table + struct Library + private data) and the full sorted
/// list of JMP-table targets. Libraries are NOT contiguous in ROM —
/// internal helpers between LVO entry-points are still library code
/// even though they're not directly reachable via jmp -N(a6) — so a
/// simple [min, max] range produces false attributions when ranges
/// interleave. We resolve by "closest preceding JMP target wins".
#[derive(Debug, Clone)]
struct LibraryExtent {
    name: String,
    lib_base: u32,
    chip_lo: u32, // base - NegSize
    chip_hi: u32, // base + PosSize
    /// Every resolved JMP target from this library's jump table, in
    /// chip-RAM-table order (NOT sorted by address). Useful for
    /// reporting; lookup uses the global sorted index built by
    /// `collect_library_extents`.
    jmp_targets: Vec<u32>,
    /// Convenience min/max across `jmp_targets`. Reported for
    /// inspection only; lookup uses the global sorted index.
    code_lo: u32,
    code_hi: u32,
}

/// Walk one library's jump table to extract every JMP $abs.l target.
/// Format: each LIB_VECTSIZE (=6) bytes before `lib_base` is a JMP
/// instruction `4EF9 xxxx xxxx`. NegSize bytes = NegSize/6 entries.
fn library_jmp_targets(
    access: &dyn runtime_commodore_amiga::AmigaLiveAccess,
    lib_base: u32,
    neg_size: u32,
) -> Vec<u32> {
    let mut targets = Vec::new();
    if neg_size < 6 {
        return targets;
    }
    let entries = (neg_size / 6).min(2048); // sanity cap
    for i in 1..=entries {
        let entry = lib_base.wrapping_sub(i.wrapping_mul(6));
        // The opcode `4EF9` (JMP abs.l) should occupy the first word.
        // Other instructions (TRAP, RTS, RTE) are possible for special
        // slots; ignore them.
        if access.read_word(entry) != 0x4EF9 {
            continue;
        }
        let target = access.read_long(entry.wrapping_add(2));
        if target != 0 {
            targets.push(target);
        }
    }
    targets
}

/// Build the LibraryExtent table by walking ExecBase->LibList and,
/// for each library, scanning its jump-table targets.
fn collect_library_extents(
    access: &dyn runtime_commodore_amiga::AmigaLiveAccess,
) -> Vec<LibraryExtent> {
    let mut out = Vec::new();
    let exec_base = access.read_long(0x0000_0004);
    if exec_base == 0 {
        return out;
    }
    let list_addr = exec_base.wrapping_add(378);
    let sentinel = list_addr.wrapping_add(4);
    let mut cur = access.read_long(list_addr);
    let mut seen = std::collections::HashSet::new();
    while out.len() < 64 {
        if cur == 0 || cur == sentinel {
            break;
        }
        if !seen.insert(cur) {
            break;
        }
        let neg_size = u32::from(access.read_word(cur.wrapping_add(16)));
        let pos_size = u32::from(access.read_word(cur.wrapping_add(18)));
        let ln_name = access.read_long(cur.wrapping_add(10));
        let name = read_amiga_cstring(access, ln_name, 64);
        let jmp_targets = library_jmp_targets(access, cur, neg_size);
        let code_lo = jmp_targets.iter().copied().min().unwrap_or(0);
        let code_hi = jmp_targets
            .iter()
            .copied()
            .max()
            .map(|v| v.wrapping_add(6))
            .unwrap_or(0);
        out.push(LibraryExtent {
            name,
            lib_base: cur,
            chip_lo: cur.wrapping_sub(neg_size),
            chip_hi: cur.wrapping_add(pos_size),
            jmp_targets,
            code_lo,
            code_hi,
        });
        cur = access.read_long(cur);
    }
    out
}

/// Build a globally-sorted index of `(jmp_target, extent_idx)` pairs.
/// Each library is identified by its index into `extents`. This index
/// is what lookup uses to attribute an address by closest preceding
/// JMP target.
fn build_jmp_index(extents: &[LibraryExtent]) -> Vec<(u32, usize)> {
    let mut idx: Vec<(u32, usize)> = extents
        .iter()
        .enumerate()
        .flat_map(|(i, e)| e.jmp_targets.iter().copied().map(move |t| (t, i)))
        .collect();
    idx.sort_unstable_by_key(|(t, _)| *t);
    idx
}

/// Maximum distance from a JMP target that we'll attribute to a
/// library. A KS library's largest function body is comfortably
/// under 16 KiB; 64 KiB is conservative and avoids false hits for
/// addresses in gaps between libraries.
const LIBRARY_GAP_MAX: u32 = 0x10000;

/// Find the library that contains `target`. The lookup hierarchy:
///   1. JMP-table closest-preceding match across all libraries (the
///      common case for ROM PCs and internal helpers).
///   2. Chip-RAM struct + jump-table range (for in-table addresses).
///
/// Returns the LibraryExtent + a "match kind" tag.
fn library_containing<'a>(
    extents: &'a [LibraryExtent],
    jmp_index: &[(u32, usize)],
    target: u32,
) -> Option<(&'a LibraryExtent, &'static str)> {
    // Step 1: binary-search the global JMP index for the largest
    // target ≤ `target`.
    if !jmp_index.is_empty() {
        let pos = jmp_index.partition_point(|(t, _)| *t <= target);
        if pos > 0 {
            let (closest, lib_idx) = jmp_index[pos - 1];
            if target.saturating_sub(closest) < LIBRARY_GAP_MAX {
                return Some((&extents[lib_idx], "code_range"));
            }
        }
    }
    // Step 2: fall back to the chip-RAM range for in-table addresses.
    for ext in extents {
        if target >= ext.chip_lo && target < ext.lib_base {
            return Some((ext, "jump_table"));
        }
        if target >= ext.lib_base && target < ext.chip_hi {
            return Some((ext, "library_data"));
        }
    }
    None
}

/// Reverse-lookup: given any address, find which loaded library's
/// effective range contains it. Walks ExecBase->LibList, extracts
/// each library's JMP-target range (the ROM code it dispatches into)
/// AND its chip-RAM range, and returns the first hit. Returns
/// `null` if the address falls outside every library.
fn tool_address_to_library(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let target = arg_u32(&args, "addr")?;
    let access = s.live();
    let exec_base = access.read_long(0x0000_0004);
    if exec_base == 0 {
        return Ok(json!({
            "addr": format!("${:08X}", target),
            "match": "exec_base_uninitialised",
        }));
    }
    let extents = collect_library_extents(access);
    let jmp_index = build_jmp_index(&extents);
    if let Some((ext, kind)) = library_containing(&extents, &jmp_index, target) {
        // For code_range hits, report the distance from the nearest
        // preceding JMP target — that's the function entry-point this
        // address is part of (give or take internal helpers).
        let nearest_jmp = if kind == "code_range" {
            let pos = jmp_index.partition_point(|(t, _)| *t <= target);
            if pos > 0 {
                Some(jmp_index[pos - 1].0)
            } else {
                None
            }
        } else {
            None
        };
        let offset = (target as i64) - (ext.lib_base as i64);
        let mut body = json!({
            "addr":             format!("${:08X}", target),
            "match":            "hit",
            "match_kind":       kind,
            "library":          ext.name,
            "library_addr":     format!("${:08X}", ext.lib_base),
            "chip_lo":          format!("${:08X}", ext.chip_lo),
            "chip_hi":          format!("${:08X}", ext.chip_hi),
            "code_lo":          format!("${:08X}", ext.code_lo),
            "code_hi":          format!("${:08X}", ext.code_hi),
            "offset_from_base": offset,
        });
        if let Some(j) = nearest_jmp {
            body["nearest_jmp_target"] = Value::String(format!("${:08X}", j));
            body["distance_from_jmp"] = Value::from(target.wrapping_sub(j));
        }
        return Ok(body);
    }
    Ok(json!({
        "addr": format!("${:08X}", target),
        "match": "no_library_contains_addr",
        "note": "Address is outside every loaded library's code range and chip-RAM range. Could be unmapped ROM (Kickstart code outside any library — exec stub, dispatcher, reset vectors), free chip RAM, or a stale value.",
    }))
}

/// Walk a task's saved stack starting at `tc_SPReg` and surface
/// every 2-byte-aligned ROM-pointing 32-bit value as a return-PC
/// candidate. Optionally cross-reference each candidate against the
/// loaded library list so the response says `intuition.library +
/// $XYZ` directly. Wraps the two-pass byte-scan that found the
/// IPrefs blocker — folds it into one MCP call.
fn tool_read_task_stack(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // Caller may supply either a task addr (we read tc_SPReg ourselves)
    // or a stack pointer directly (for non-Task callers).
    let sp = if let Some(t) = args.get("task_addr") {
        let task_addr = if let Some(n) = t.as_u64() {
            u32::try_from(n)
                .map_err(|_| ToolError::InvalidArguments("task_addr out of u32 range".into()))?
        } else if let Some(_s) = t.as_str() {
            arg_u32(&args, "task_addr")?
        } else {
            return Err(ToolError::InvalidArguments(
                "task_addr must be integer or hex string".into(),
            ));
        };
        s.live().read_long(task_addr.wrapping_add(54))
    } else if args.get("sp").is_some() {
        arg_u32(&args, "sp")?
    } else {
        return Err(ToolError::InvalidArguments(
            "must supply either `task_addr` (we read tc_SPReg) or `sp` (raw stack pointer)".into(),
        ));
    };
    if sp == 0 {
        return Ok(json!({
            "sp": "$00000000",
            "note": "stack pointer is null — task has never been switched out, or task_addr wasn't a task.",
        }));
    }
    let bytes_len = arg_u64_or(&args, "bytes", 256)? as u32;
    if !(8..=4096).contains(&bytes_len) {
        return Err(ToolError::InvalidArguments("bytes must be 8..=4096".into()));
    }
    let rom_lo = match args.get("rom_lo") {
        Some(v) if !v.is_null() => arg_u32(&args, "rom_lo")?,
        _ => ROM_DEFAULT_LO,
    };
    let rom_hi = match args.get("rom_hi") {
        Some(v) if !v.is_null() => arg_u32(&args, "rom_hi")?,
        _ => ROM_DEFAULT_HI,
    };
    let resolve = args
        .get("resolve_libraries")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // Read raw bytes via word reads. tc_SPReg is always word-aligned.
    let access = s.live();
    let mut raw: Vec<u8> = Vec::with_capacity(bytes_len as usize);
    let words = bytes_len.div_ceil(2);
    for i in 0..words {
        let w = access.read_word(sp.wrapping_add(i.wrapping_mul(2)));
        raw.push((w >> 8) as u8);
        raw.push((w & 0xFF) as u8);
    }
    raw.truncate(bytes_len as usize);

    // Pre-fetch library extents + JMP-target index once so each
    // candidate is a binary-search instead of a linear scan.
    let extents = if resolve {
        collect_library_extents(access)
    } else {
        Vec::new()
    };
    let jmp_index = if resolve {
        build_jmp_index(&extents)
    } else {
        Vec::new()
    };

    // Scan every 2-byte boundary for ROM hits, annotating with the
    // owning library when one is found.
    let mut hits: Vec<Value> = Vec::new();
    let mut off = 0usize;
    while off + 4 <= raw.len() {
        let v = u32::from_be_bytes([raw[off], raw[off + 1], raw[off + 2], raw[off + 3]]);
        if v >= rom_lo && v <= rom_hi {
            let mut entry = json!({
                "offset_from_sp": off as u64,
                "addr":           format!("${:08X}", sp.wrapping_add(off as u32)),
                "value":          format!("${:08X}", v),
            });
            if let Some((ext, kind)) = library_containing(&extents, &jmp_index, v) {
                entry["library"] = Value::String(ext.name.clone());
                entry["match_kind"] = Value::String(kind.to_string());
            }
            hits.push(entry);
        }
        off += 2;
    }

    let hit_count = hits.len();
    Ok(json!({
        "sp":                 format!("${:08X}", sp),
        "bytes":              bytes_len,
        "rom_lo":             format!("${:08X}", rom_lo),
        "rom_hi":             format!("${:08X}", rom_hi),
        "rom_hits":           hits,
        "rom_hit_count":      hit_count,
        "libraries_searched": extents.len(),
        "layout_note":        "ROM-hit scan is layout-independent — every 2-byte boundary checked. The KS 3.x Switch frame format varies between AmigaOS versions (some save SR + MOVEM + PC, others save the full m68k exception frame), so we don't pretend to decode a canonical layout. Use the rom_hits list, ordered by `offset_from_sp`, to walk the call chain from most-recent (lowest offset) to oldest (highest offset).",
    }))
}

fn tool_start_video_recording(args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
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
    let started_at = MachineTime::new(s.live().tick_count());
    let recorder = VideoRecorder::start(PathBuf::from(path), FB_WIDTH, FB_HEIGHT, fps, started_at)
        .map_err(|err| ToolError::Execution(format!("start recording: {err}")))?;
    s.recorder = Some(recorder);
    s.last_recorded_tick = s.live().tick_count();
    // Push one frame immediately so the recording begins with the
    // current screen state, not a missing first frame.
    push_recorder_frame(s)?;
    Ok(json!({
        "started": true,
        "path": path,
        "width": FB_WIDTH,
        "height": FB_HEIGHT,
        "fps": fps,
        "started_at_tick": s.live().tick_count(),
    }))
}

fn tool_stop_video_recording(_args: Value, s: &mut AmigaSession) -> Result<Value, ToolError> {
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

fn tool_dump_framebuffer(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    use machine_commodore_amiga_a1200::{FB_HEIGHT, FB_WIDTH};
    let fb = s.live().framebuffer();
    let total_pixels = (FB_WIDTH * FB_HEIGHT) as usize;

    // Histogram top colours so the caller can see "what's on screen" without
    // saving anything to disk — useful when running headlessly.
    let mut hist: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for &p in fb {
        *hist.entry(p).or_insert(0) += 1;
    }
    let mut by_count: Vec<(u32, u32)> = hist.into_iter().collect();
    by_count.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
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
        if let Some(parent) = path_buf.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|err| ToolError::Execution(format!("mkdir: {err}")))?;
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

fn tool_bplcon0_log(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // BPLCON0 write tracing is mirrored across OCS / ECS / AGA — the
    // trait returns the live slice for whichever chipset variant the
    // session is hosting.
    let log = s.live().bplcon0_log();
    let unique_only = args.get("unique").and_then(Value::as_bool).unwrap_or(false);
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

fn tool_query_aga(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let ocs_palette: Vec<String> = (0..32)
        .map(|i| format!("${:03X}", s.live().color(i)))
        .collect();
    // AGA Lisa state via the trait — `None` on OCS / ECS sessions.
    let Some(aga) = s.live().aga_lisa() else {
        return Err(ToolError::Execution(
            "query_aga: active session is not AGA (no Lisa state)".into(),
        ));
    };
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
    if args
        .get("all_banks")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        full_palette = Some(
            aga.palette_24
                .iter()
                .map(|c| format!("${:06X}", c))
                .collect(),
        );
    }
    Ok(json!({
        "deniseid": format!("${:04X}", aga.deniseid),
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

fn tool_chipset_read_log(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // Chipset-register read tracing is mirrored across OCS / ECS /
    // AGA. The trait hands back a slice directly.
    let log = s.live().reg_read_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let unique = args.get("unique").and_then(Value::as_bool).unwrap_or(false);
    let offset_filter = args.get("offset").and_then(|v| {
        if v.is_null() {
            None
        } else {
            let one = json!({ "x": v });
            arg_u32(&one, "x").ok().map(|n| n as u16)
        }
    });

    let mut filtered: Vec<&(u64, u32, u16, u16)> = log
        .iter()
        .filter(|(_, _, off, _)| offset_filter.is_none_or(|want| *off == want))
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
                "off" => *off as u64,
                _ => ((*pc as u64) << 32) | ((*off as u64) << 16) | (*val as u64),
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

fn tool_chipset_write_log(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // Every CPU write that lands in `dispatch_custom_register`'s write
    // arm. Lets callers answer "when did COP2LC change?" or "what
    // were all the writes to $DFF0xx during boot?" without polling
    // `query_chipset` every N frames. Mirrors the shape and filters
    // of `chipset_read_log`. Cross-cutting across OCS / ECS / AGA.
    let log = s.live().custom_write_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let offset_filter = args.get("offset").and_then(|v| {
        if v.is_null() {
            None
        } else {
            let one = json!({ "x": v });
            arg_u32(&one, "x").ok().map(|n| n as u16)
        }
    });
    let offset_min = args
        .get("offset_min")
        .and_then(Value::as_u64)
        .map(|n| n as u16);
    let offset_max = args
        .get("offset_max")
        .and_then(Value::as_u64)
        .map(|n| n as u16);
    let cck_lo = args.get("cck_min").and_then(Value::as_u64);
    let cck_hi = args.get("cck_max").and_then(Value::as_u64);
    let dedupe_mode = args.get("dedupe").and_then(Value::as_str).unwrap_or("none");

    let mut filtered: Vec<&(u64, u32, u32, u16, u16, bool)> = log
        .iter()
        .filter(|(_, _, _, off, _, _)| offset_filter.is_none_or(|want| *off == want))
        .filter(|(_, _, _, off, _, _)| offset_min.is_none_or(|lo| *off >= lo))
        .filter(|(_, _, _, off, _, _)| offset_max.is_none_or(|hi| *off <= hi))
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
                "off" => *off as u64,
                _ => ((*pc as u64) << 32) | ((*off as u64) << 16) | (*val as u64),
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

fn tool_poke_word(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let val = arg_u32(&args, "val")?;
    let val_u16 = u16::try_from(val & 0xFFFF).unwrap_or(0);
    s.live_mut().poke_word(addr, val_u16);
    Ok(json!({
        "poked": true,
        "addr": format!("${:08X}", addr),
        "val":  format!("${:04X}", val_u16),
    }))
}

fn tool_watch_memory(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let lo = arg_u32(&args, "addr")?;
    let len = arg_u32(&args, "len")?;
    if len == 0 {
        return Err(ToolError::InvalidArguments("`len` must be ≥ 1".into()));
    }
    s.live_mut().set_watch(Some((lo, len)));
    Ok(json!({
        "watching": {
            "lo":  format!("${:08X}", lo),
            "len": len,
            "hi_exclusive": format!("${:08X}", lo.wrapping_add(len)),
        },
        "log_cleared": true,
    }))
}

fn tool_watch_memory_clear(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let prior = s.live().watch_range();
    let count = s.live().watch_log().len();
    s.live_mut().set_watch(None);
    Ok(json!({
        "had_watch": prior.is_some(),
        "writes_captured_before_clear": count,
    }))
}

fn tool_watch_memory_log(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let access = s.live();
    let log = access.watch_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let unique = args.get("unique").and_then(Value::as_bool).unwrap_or(false);

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

fn tool_restart(args: Value, _s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
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

fn tool_palette_log(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // Palette-write tracing is mirrored across OCS / ECS / AGA. The
    // fifth field of each entry is `Option<u16>` — `Some(bplcon3)` on
    // ECS / AGA where BPLCON3 is a real register, `None` on OCS where
    // $0106 isn't backed by any chip state.
    let log = s.live().palette_log();
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(64) as usize;
    let only_color = args
        .get("only_color")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let only_bplcon3 = args
        .get("only_bplcon3")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let unique = args.get("unique").and_then(Value::as_bool).unwrap_or(false);

    let idx_range: Option<(u16, u16)> = args
        .get("color_idx_range")
        .and_then(Value::as_array)
        .and_then(|a| {
            let lo = a.first()?.as_u64()? as u16;
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

fn tool_query_blitter(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // Cross-cutting: every field below is on the shared OCS Agnus base
    // type that the ECS / AGA wrappers Deref to, so the trait surface
    // suffices.
    let a = s.live().agnus();
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

fn tool_query_copper_list(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // The copper instruction encoding (MOVE / WAIT / SKIP) is identical
    // across OCS / ECS / AGA — only the host `Copper` struct type
    // differs between chipsets. So the disassembly uses the trait's
    // `copper_cop1lc()` for the start address and `read_word()` for
    // the per-word fetch, sidestepping the typed struct entirely.
    let access = s.live();
    let default_start = access.copper_cop1lc();
    let start = if let Some(v) = args.get("addr") {
        if v.is_null() {
            default_start
        } else {
            arg_u32(&args, "addr")?
        }
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

fn tool_query_stack(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let usp = args.get("usp").and_then(Value::as_bool).unwrap_or(false);
    let count = arg_u64_or(&args, "count", 16)?;
    if count == 0 || count > 256 {
        return Err(ToolError::InvalidArguments("count must be 1..=256".into()));
    }
    let regs = s.live().cpu_snapshot().regs;
    let base = if usp { regs.usp } else { regs.ssp };
    let entries: Vec<Value> = (0..count)
        .map(|i| {
            let addr = base.wrapping_add((i as u32) * 4);
            json!({
                "addr": format!("${:08X}", addr),
                "value": format!("${:08X}", s.live().read_long(addr)),
            })
        })
        .collect();
    Ok(json!({
        "stack": if usp { "USP" } else { "SSP" },
        "base": format!("${:08X}", base),
        "entries": entries,
    }))
}

fn tool_cpu_trace_arm(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    // Enable the instruction-boundary trace. Clears any previously
    // captured entries so the new run starts fresh; the PC filter
    // and max_entries from the previous arm are replaced if the
    // caller supplies new values.
    let pc_min = args.get("pc_min").and_then(|v| {
        if v.is_null() {
            None
        } else {
            let one = json!({ "x": v });
            arg_u32(&one, "x").ok()
        }
    });
    let pc_max = args.get("pc_max").and_then(|v| {
        if v.is_null() {
            None
        } else {
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
    s.live_mut().cpu_trace_arm(pc_filter, max_entries);
    Ok(json!({
        "armed": true,
        "pc_filter": pc_filter.map(|(lo, hi)| json!({
            "min": format!("${:08X}", lo),
            "max": format!("${:08X}", hi),
        })),
        "max_entries": max_entries,
    }))
}

fn tool_cpu_trace_disarm(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let captured = s.live_mut().cpu_trace_disarm();
    Ok(json!({
        "armed": false,
        "captured": captured,
    }))
}

fn tool_cpu_trace_clear(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let dropped = s.live_mut().cpu_trace_clear();
    Ok(json!({
        "dropped": dropped,
        "armed": s.live().cpu_trace_armed(),
    }))
}

fn tool_cpu_trace_log(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
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

    let armed = s.live().cpu_trace_armed();
    let captured = s.live().cpu_trace_entries().len();
    let max_entries = s.live().cpu_trace_max_entries();
    let mut filtered: Vec<&(u64, u32, u16, u16)> = s
        .live()
        .cpu_trace_entries()
        .iter()
        .filter(|(cck, _, _, _)| cck_lo.is_none_or(|lo| *cck >= lo))
        .filter(|(cck, _, _, _)| cck_hi.is_none_or(|hi| *cck <= hi))
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
        "armed": armed,
        "captured": captured,
        "filtered_total": total,
        "returned": entries.len(),
        "at_limit": captured >= max_entries,
        "max_entries": max_entries,
        "entries": entries,
    }))
}

fn tool_step(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let n = arg_u64_or(&args, "count", 1)?;
    let max_ticks = arg_u64_or(&args, "max_ticks", 1_000_000)?;
    let start = s.live().cpu_instruction_starts();
    let target = start.wrapping_add(n);
    let mut ticks_taken: u64 = 0;
    let mut trace: Vec<Value> = Vec::new();
    let mut last_seen = start;
    while s.live().cpu_instruction_starts() != target && ticks_taken < max_ticks {
        s.live_mut().tick();
        ticks_taken += 1;
        let now = s.live().cpu_instruction_starts();
        if now != last_seen && !s.live().cpu_in_followup() {
            last_seen = now;
            trace.push(json!({
                "step": now.wrapping_sub(start),
                "pc": format!("${:08X}", s.live().cpu_pc()),
            }));
            if trace.len() as u64 >= n {
                break;
            }
        }
    }
    Ok(json!({
        "requested": n,
        "completed": s.live().cpu_instruction_starts().wrapping_sub(start),
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.live().cpu_pc()),
        "trace": trace,
    }))
}

fn tool_run_until_any_pc(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
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
        return Err(ToolError::InvalidArguments(
            "`targets` must be non-empty".into(),
        ));
    }
    let max_ticks = arg_u64_or(&args, "max_ticks", 100_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit: Option<u32> = None;
    while ticks_taken < max_ticks {
        s.live_mut().tick();
        ticks_taken += 1;
        let pc = s.live().cpu_pc();
        if wanted.contains(&pc) {
            hit = Some(pc);
            break;
        }
    }
    Ok(json!({
        "hit": hit.map(|p| format!("${:08X}", p)),
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.live().cpu_pc()),
    }))
}

fn tool_insert_media(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("missing string `path`".into()))?;
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("adf");
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
            s.live_mut().insert_floppy0(adf, change_pending);
            Ok(json!({
                "inserted": true,
                "kind": "adf",
                "path": path_buf.display().to_string(),
                "source": source_label,
                "change_pending": change_pending,
                "has_disk": s.live().drive().has_disk(),
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
        if let Some(want) = entry_hint
            && name == want
        {
            chosen_index = Some(i);
            break;
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

fn tool_eject_media(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let had_disk = s.live().drive().has_disk();
    s.live_mut().eject_floppy0();
    Ok(json!({
        "ejected": had_disk,
        "has_disk": s.live().drive().has_disk(),
    }))
}

fn tool_query_disk(_args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let drive = s.live().drive();
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

fn tool_run_until_mem_change(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let addrs = args
        .get("addrs")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::InvalidArguments("missing array `addrs`".into()))?;
    let mut watch: Vec<(u32, u32)> = Vec::with_capacity(addrs.len());
    {
        let access = s.live();
        for a in addrs {
            let one = json!({ "x": a });
            let addr = arg_u32(&one, "x")?;
            watch.push((addr, access.read_long(addr)));
        }
    }
    if watch.is_empty() {
        return Err(ToolError::InvalidArguments(
            "`addrs` must be non-empty".into(),
        ));
    }
    let max_ticks = arg_u64_or(&args, "max_ticks", 50_000_000)?;
    let mut ticks_taken: u64 = 0;
    let mut hit: Option<(u32, u32, u32)> = None;
    while ticks_taken < max_ticks {
        s.live_mut().tick();
        ticks_taken += 1;
        for (addr, old) in &watch {
            let now = s.live().read_long(*addr);
            if now != *old {
                hit = Some((*addr, *old, now));
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let result = hit.map(|(a, o, n)| {
        json!({
            "addr": format!("${:08X}", a),
            "old": format!("${:08X}", o),
            "new": format!("${:08X}", n),
        })
    });
    Ok(json!({
        "hit": result,
        "ticks_taken": ticks_taken,
        "pc": format!("${:08X}", s.live().cpu_pc()),
    }))
}

fn tool_memory_read(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let len = arg_u64_or(&args, "len", 16)?;
    let len =
        u32::try_from(len).map_err(|_| ToolError::InvalidArguments("len exceeds u32".into()))?;
    if len == 0 || len > 4096 {
        return Err(ToolError::InvalidArguments("len must be 1..=4096".into()));
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

fn tool_memory_read_long(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    Ok(json!({
        "addr": format!("${:08X}", addr),
        "value": format!("${:08X}", read_long(s, addr)),
    }))
}

/// Scan a memory range for every longword-aligned address whose
/// 32-bit big-endian value matches `value`. Generalises the chip-RAM
/// scan that found Workbench's private MsgPorts (every MsgPort's
/// mp_SigTask field points at its owner task — scanning for the task
/// address surfaces the ports). `max_hits` caps the response so a
/// bad mask doesn't return megabytes of matches.
fn tool_memory_scan(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let start = arg_u32(&args, "start")?;
    let end = arg_u32(&args, "end")?;
    if end <= start {
        return Err(ToolError::InvalidArguments(
            "`end` must be greater than `start`".into(),
        ));
    }
    let span = end.saturating_sub(start);
    if span > 16 * 1024 * 1024 {
        return Err(ToolError::InvalidArguments(
            "scan span exceeds 16 MiB — narrow the range".into(),
        ));
    }
    let value = arg_u32(&args, "value")?;
    let mask = match args.get("mask") {
        Some(v) if !v.is_null() => arg_u32(&args, "mask")?,
        _ => 0xFFFF_FFFF,
    };
    let max_hits = arg_u64_or(&args, "max_hits", 256)? as usize;
    let stride = arg_u64_or(&args, "stride", 2)? as u32;
    if stride == 0 || (stride & 1) != 0 {
        return Err(ToolError::InvalidArguments(
            "`stride` must be a positive even number (longword reads need 2-byte alignment)".into(),
        ));
    }
    let target = value & mask;
    let access = s.live();
    let mut hits: Vec<Value> = Vec::new();
    let mut scanned: u64 = 0;
    let mut truncated = false;
    let aligned_start = start & !1;
    let mut addr = aligned_start;
    while addr.wrapping_add(3) < end && addr.wrapping_add(3) >= addr {
        let v = access.read_long(addr);
        scanned += 1;
        if (v & mask) == target {
            if hits.len() >= max_hits {
                truncated = true;
                break;
            }
            hits.push(json!({
                "addr": format!("${:08X}", addr),
                "value": format!("${:08X}", v),
            }));
        }
        let next = addr.wrapping_add(stride);
        if next <= addr {
            break;
        }
        addr = next;
    }
    let hit_count = hits.len();
    Ok(json!({
        "start":     format!("${:08X}", start),
        "end":       format!("${:08X}", end),
        "stride":    stride,
        "value":     format!("${:08X}", value),
        "mask":      format!("${:08X}", mask),
        "scanned":   scanned,
        "hits":      hits,
        "hit_count": hit_count,
        "truncated": truncated,
        "max_hits":  max_hits,
    }))
}

/// Resolve `jsr -N(a6)` style LVO calls to function names. Two
/// modes:
///
/// * `offset` (single integer, positive or negative): resolve one
///   offset; reply carries `name` (or null) and `match` describing
///   what happened.
/// * No `offset`: list every known LVO for the requested library
///   so the caller can browse the surface.
///
/// Always validates the library name against the static table and
/// returns the supported library list when the name is unknown —
/// makes it self-documenting for tool users.
fn tool_resolve_lvo(args: Value, _s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let library = args
        .get("library")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("missing `library` (string)".into()))?;
    if lvo::lvo_table(library).is_none() {
        return Ok(json!({
            "library":            library,
            "match":              "unknown_library",
            "supported_libraries": lvo::LIBRARY_NAMES,
        }));
    }
    if let Some(off_value) = args.get("offset") {
        let offset = if let Some(n) = off_value.as_i64() {
            i32::try_from(n)
                .map_err(|_| ToolError::InvalidArguments("offset out of i32 range".into()))?
        } else if let Some(s) = off_value.as_str() {
            // accept hex / signed-decimal strings (`-318`, `-0x13E`,
            // `$13E`). We normalise to negative below regardless.
            let trimmed = s.trim();
            let (sign, body) = if let Some(rest) = trimmed.strip_prefix('-') {
                (-1, rest)
            } else if let Some(rest) = trimmed.strip_prefix('+') {
                (1, rest)
            } else {
                (1, trimmed)
            };
            let (b, radix) =
                if let Some(rest) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
                    (rest, 16)
                } else if let Some(rest) = body.strip_prefix('$') {
                    (rest, 16)
                } else {
                    (body, 10)
                };
            let mag = i32::from_str_radix(b, radix)
                .map_err(|_| ToolError::InvalidArguments(format!("offset `{s}` not parseable")))?;
            sign * mag
        } else {
            return Err(ToolError::InvalidArguments(
                "offset must be integer or hex/decimal string".into(),
            ));
        };
        let name = lvo::resolve(library, offset);
        // Normalise the offset we report back so the caller sees the
        // canonical negative form even if they passed a magnitude.
        let canonical = if offset > 0 { -offset } else { offset };
        return Ok(json!({
            "library":      library,
            "offset":       canonical,
            "offset_input": offset,
            "name":         name,
            "match":        if name.is_some() { "hit" } else { "miss" },
        }));
    }
    // No offset → dump the full table for the library.
    let Some(table) = lvo::lvo_table(library) else {
        return Err(ToolError::InvalidArguments(format!(
            "unknown library `{library}`"
        )));
    };
    let entries: Vec<Value> = table
        .iter()
        .map(|(off, name)| json!({"offset": *off, "name": *name}))
        .collect();
    let entry_count = entries.len();
    Ok(json!({
        "library":     library,
        "match":       "library_dump",
        "entries":     entries,
        "entry_count": entry_count,
    }))
}

fn tool_disasm(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let addr = arg_u32(&args, "addr")?;
    let count = arg_u64_or(&args, "count", 8)? as u32;
    if count == 0 || count > 128 {
        return Err(ToolError::InvalidArguments("count must be 1..=128".into()));
    }
    let mut pc = addr;
    let mut lines: Vec<Value> = Vec::new();
    let access = s.live();
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

/// Disasm `before` instructions ending at `addr`, then `addr` itself,
/// then `after` instructions following. Backward disasm on m68k is
/// heuristic (instruction length varies 2-10 bytes), so we try each
/// possible start offset (target-2, target-4, ..., target-max) and
/// pick the alignment whose forward disasm lands EXACTLY at target.
/// That guarantees the `before` window is a valid instruction
/// boundary, not a misaligned slice mid-instruction.
///
/// If no alignment lands exactly at `target` (rare — happens when
/// target is itself mid-instruction, e.g. a stale pointer), we fall
/// back to the closest-overshoot alignment so the caller still sees
/// something coherent + an `aligned: false` flag.
fn tool_disasm_around(args: Value, s: &mut impl AmigaCtx) -> Result<Value, ToolError> {
    let target = arg_u32(&args, "addr")?;
    let before = arg_u64_or(&args, "before", 4)? as u32;
    let after = arg_u64_or(&args, "after", 4)? as u32;
    if before == 0 && after == 0 {
        return Err(ToolError::InvalidArguments(
            "at least one of `before` / `after` must be > 0".into(),
        ));
    }
    if before > 32 || after > 64 {
        return Err(ToolError::InvalidArguments(
            "before must be 0..=32, after must be 0..=64".into(),
        ));
    }
    let access = s.live();
    let read = |a: u32| -> u8 {
        let aligned = a & !3;
        let long = access.read_long(aligned);
        let shift = (3 - (a & 3)) * 8;
        ((long >> shift) & 0xFF) as u8
    };

    // Find the start offset where forward disasm lands exactly at
    // target AND produces ≥ `before` instructions. Try every even
    // offset from largest down to 2 — the first valid alignment that
    // covers the requested instruction count wins. Fall back to the
    // largest valid alignment we found if none covers `before`.
    // (address, size, mnemonic) for each decoded instruction in a run.
    type DisasmRun = Vec<(u32, u8, String)>;
    let max_window = (before as i32 * 12).max(64) as u32;
    let mut best_aligned: Option<(u32, DisasmRun)> = None;
    let mut closest_overshoot: Option<(u32, i64, DisasmRun)> = None;

    // Walk start_off from large to small. The first hit that
    // produces ≥ before instructions is our answer; otherwise track
    // the largest alignment we found.
    let start_step = 2u32;
    let mut start_off = max_window - (max_window % start_step);
    if start_off == 0 {
        start_off = start_step;
    }
    while start_off >= start_step {
        let start = target.wrapping_sub(start_off);
        let mut pc = start;
        let mut instrs: Vec<(u32, u8, String)> = Vec::new();
        for _ in 0..200u32 {
            if pc == target {
                break;
            }
            if pc > target {
                break; // overshot — this alignment isn't clean
            }
            let (mn, len) = disassemble(pc, read);
            instrs.push((pc, len, mn));
            pc = pc.wrapping_add(u32::from(len));
        }
        if pc == target && !instrs.is_empty() {
            // Clean alignment. If it covers the requested count, win.
            if instrs.len() >= before as usize {
                best_aligned = Some((start, instrs));
                break;
            }
            // Otherwise track as fallback — keep the largest valid
            // alignment we've found (this one is larger than any
            // previous since we iterate large→small).
            if best_aligned.is_none() {
                best_aligned = Some((start, instrs));
            }
        } else {
            // Overshoot — track for the no-alignment-at-all fallback.
            let gap = (pc as i64) - (target as i64);
            if closest_overshoot
                .as_ref()
                .is_none_or(|(_, prev_gap, _)| gap.abs() < prev_gap.abs())
            {
                closest_overshoot = Some((start, gap, instrs));
            }
        }
        start_off -= start_step;
    }

    let (aligned, used_start, before_instrs) = if let Some((start, instrs)) = best_aligned {
        (true, start, instrs)
    } else if let Some((start, _, instrs)) = closest_overshoot {
        (false, start, instrs)
    } else {
        (false, target, Vec::new())
    };

    // Trim before_instrs to the requested count (take the last N).
    let before_count = (before as usize).min(before_instrs.len());
    let before_drop = before_instrs.len().saturating_sub(before_count);
    let mut lines: Vec<Value> = before_instrs
        .into_iter()
        .skip(before_drop)
        .map(|(pc, len, mn)| {
            let bytes_hex = (0..len)
                .map(|i| format!("{:02X}", read(pc.wrapping_add(u32::from(i)))))
                .collect::<Vec<_>>()
                .join(" ");
            json!({
                "addr":   format!("${:08X}", pc),
                "bytes":  bytes_hex,
                "disasm": mn,
                "is_target": false,
            })
        })
        .collect();

    // Now disasm `after + 1` from target (the +1 includes the target
    // instruction itself).
    let mut pc = target;
    for i in 0..=after {
        let (mn, len) = disassemble(pc, read);
        let bytes_hex = (0..len)
            .map(|j| format!("{:02X}", read(pc.wrapping_add(u32::from(j)))))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(json!({
            "addr":   format!("${:08X}", pc),
            "bytes":  bytes_hex,
            "disasm": mn,
            "is_target": i == 0,
        }));
        pc = pc.wrapping_add(u32::from(len));
    }

    Ok(json!({
        "target":         format!("${:08X}", target),
        "before":         before,
        "after":          after,
        "aligned":        aligned,
        "alignment_start": format!("${:08X}", used_start),
        "alignment_note": if aligned {
            "Forward disasm from alignment_start lands exactly at target — `before` instructions are real boundaries."
        } else {
            "No clean backward alignment found within search window. `before` entries may be mid-instruction garbage (target itself may be mid-instruction)."
        },
        "instructions":   lines,
    }))
}

// ─── Registration ─────────────────────────────────────────────────────

/// Registers every chipset-agnostic Amiga MCP tool on the supplied
/// registry. Generic over the session context `C: AmigaCtx`, so the same
/// table serves both the legacy [`AmigaSession`] and the shared
/// `HeadlessSession`. Order is the order shown by `tools/list`.
///
/// Does NOT register the recorder run tools (`run_frames` / `run_ticks` /
/// `start_video_recording` / `stop_video_recording`) or `reset` — those
/// are session-local on `AmigaSession` (added by [`register_all`]) and
/// are provided by the shared `register_common_tools` on the
/// `HeadlessSession` path.
pub fn register_amiga_tools<C: AmigaCtx + 'static>(registry: &mut ToolRegistry<C>) {
    fn add<C: AmigaCtx + 'static>(
        registry: &mut ToolRegistry<C>,
        name: &'static str,
        description: &'static str,
        schema: Value,
        run: fn(Value, &mut C) -> Result<Value, ToolError>,
    ) {
        registry.register(Box::new(InlineTool {
            name,
            description,
            schema,
            run,
        }));
    }

    let empty = || json!({"type": "object", "additionalProperties": false});
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

    add(
        registry,
        "run_until_pc",
        "Run until PC == target or max_ticks reached.",
        until_pc_schema,
        tool_run_until_pc,
    );
    add(
        registry,
        "run_until_any_pc",
        "Run until PC matches any address in `targets` or max_ticks reached.",
        any_pc_schema,
        tool_run_until_any_pc,
    );
    add(
        registry,
        "run_until_mem_change",
        "Run until any longword in `addrs` changes value, or max_ticks reached.",
        mem_change_schema,
        tool_run_until_mem_change,
    );
    add(
        registry,
        "step",
        "Step one or more CPU instructions, returning a PC trace.",
        step_schema,
        tool_step,
    );
    let cpu_trace_arm_schema = json!({
        "type": "object",
        "properties": {
            "pc_min":      {"description": "Optional inclusive PC lower bound; entries outside the range are dropped before capture (hex/decimal)."},
            "pc_max":      {"description": "Optional inclusive PC upper bound."},
            "max_entries": {"type": "integer", "minimum": 1, "maximum": 10_000_000, "default": 100_000,
                            "description": "Hard cap on captured entries; further pushes are silently dropped past this point."}
        }
    });
    add(
        registry,
        "cpu_trace_arm",
        "Start recording an instruction-boundary CPU trace. Captures (cck, instr_start_pc, sr, opcode_word) at every instruction completion that subsequent `run_*` / `step` calls cross. Clears any prior trace; replaces filter + max_entries. Use `pc_min`/`pc_max` to capture only inside a region of interest (e.g. KS palette init).",
        cpu_trace_arm_schema,
        tool_cpu_trace_arm,
    );
    add(
        registry,
        "cpu_trace_disarm",
        "Stop recording. The captured trace is kept; `cpu_trace_log` still reads it. Re-arming clears.",
        empty(),
        tool_cpu_trace_disarm,
    );
    add(
        registry,
        "cpu_trace_clear",
        "Discard captured entries without disarming. Lets you focus on a fresh window without re-arming.",
        empty(),
        tool_cpu_trace_clear,
    );
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
    add(
        registry,
        "cpu_trace_log",
        "Dump captured CPU trace entries. Tail-window by default (most recent `limit`); pass `from_start:true` for the leading window. Filter by cck range for a specific time slice.",
        cpu_trace_log_schema,
        tool_cpu_trace_log,
    );
    add(
        registry,
        "query_cpu",
        "Full CPU register snapshot (D0-D7, A0-A7, PC, SR, SSP, USP, VBR, IPL pin, exception state).",
        empty(),
        tool_query_cpu,
    );
    add(
        registry,
        "query_chipset",
        "BPLCON0 / DMACON / ADKCON / COLOR00 / COP1LC / copper PC / overlay state.",
        empty(),
        tool_query_chipset,
    );
    add(
        registry,
        "query_paula",
        "Paula INTENA / INTREQ with bit names decoded.",
        empty(),
        tool_query_paula,
    );
    add(
        registry,
        "query_cia",
        "CIA-A + CIA-B timer / ICR / port / TOD snapshot.",
        empty(),
        tool_query_cia,
    );
    add(
        registry,
        "query_agnus",
        "Agnus snapshot (vpos / hpos / bitplane pointers / blitter pointers).",
        empty(),
        tool_query_agnus,
    );
    add(
        registry,
        "query_blitter",
        "Blitter snapshot (busy, exec_pending, ccks_remaining, APT/BPT/CPT/DPT).",
        empty(),
        tool_query_blitter,
    );
    add(
        registry,
        "query_exec_tasks",
        "Walk ExecBase (at $00000004) and dump ThisTask, TaskReady, TaskWait. Each entry decodes the Exec Node (name, type, priority — `ln_type_label` resolves to TASK / PROCESS / etc.) + Task (state, tc_SigWait, tc_SigRecvd, SP, user data). When `ln_type` is NT_PROCESS (=13) — true for IPrefs, Workbench, the shell, every loaded executable — the entry's `process` field decodes the trailing Process struct: embedded pr_MsgPort (with queued message count!), pr_CIS/COS/CES streams, pr_CurrentDir, pr_HomeDir, pr_CLI, pr_TaskNum. Use to find what WB.Workbench is blocked on (signal-side via tc_sig_wait + tc_state=WAIT) and what messages are queued at its private port (process side).",
        empty(),
        tool_query_exec_tasks,
    );
    add(
        registry,
        "query_exec_ports",
        "Walk ExecBase->PortList (SysBase+392) and dump every public MsgPort: name, mp_SigBit (which signal bit notifies the owner), mp_SigTask (owning task address), mp_Flags (PA_SIGNAL / PA_SOFTINT / PA_IGNORE), and queued-message count. Use to find which port WB.Workbench is blocked on — cross-reference `mp_sigtask` against `query_exec_tasks` Workbench addr, look for the port with the matching `mp_sigbit_mask`.",
        empty(),
        tool_query_exec_ports,
    );
    let query_aga_schema = json!({
        "type": "object",
        "properties": {
            "all_banks": {"type": "boolean", "default": false,
                          "description": "Include the full 256-entry palette_24 dump in the response."}
        }
    });
    add(
        registry,
        "query_aga",
        "AGA Lisa state. DENISEID, BPLCON3 bank+LOCT, BPLCON4, palette_24 bank 0 + non-zero counts per bank, OCS 12-bit palette side-by-side. Pass `all_banks:true` for the full 256-entry dump.",
        query_aga_schema,
        tool_query_aga,
    );
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
    add(
        registry,
        "palette_log",
        "Every COLOR / BPLCON3 write captured during the run, with BPLCON3 BANK + LOCT decoded for each write. Use to reconstruct the AGA palette-programming sequence KS uses.",
        palette_log_schema,
        tool_palette_log,
    );
    let restart_schema = json!({
        "type": "object",
        "properties": {
            "exit_code": {"type": "integer", "default": 0,
                          "description": "Process exit code. Non-zero useful for hosts that only respawn on crash."}
        }
    });
    add(
        registry,
        "restart",
        "Exit the MCP server process so the host re-spawns the freshly built binary on the next call. Response is flushed before exit.",
        restart_schema,
        tool_restart,
    );
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
    add(
        registry,
        "poke_word",
        "Backdoor word write via the machine's `poke_word` path. Useful for testing: e.g. force-write to a chipset COLOR register and see if the display reflects it.",
        poke_word_schema,
        tool_poke_word,
    );
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
    add(
        registry,
        "chipset_read_log",
        "Every CPU read from a chipset register ($DFFxxx) with the returned value and PC. Filter by `offset:` to see one register's read history, e.g. what value KS observed for DENISEID across the boot.",
        chipset_read_schema,
        tool_chipset_read_log,
    );
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
    add(
        registry,
        "chipset_write_log",
        "Every CPU write to a chipset register ($DFFxxx). Filter by `offset:` for one register's history, or `offset_min`/`offset_max` for a range (e.g. 0x080..0x086 to track all COP1LC/COP2LC writes). Useful for answering 'when did cop2lc change?' or 'what writes hit $DFF000 during boot?' without polling.",
        chipset_write_schema,
        tool_chipset_write_log,
    );
    add(
        registry,
        "watch_memory",
        "Set a write-watchpoint on a chip-RAM byte range. Captures every CPU bus write that lands in the range as (cck, pc, addr, val, size). Clears any prior log.",
        watch_set_schema,
        tool_watch_memory,
    );
    add(
        registry,
        "watch_memory_clear",
        "Clear the active write-watchpoint (stops further capture). Returns how many writes were captured.",
        empty(),
        tool_watch_memory_clear,
    );
    add(
        registry,
        "watch_memory_log",
        "Dump the writes captured by the watchpoint. `unique:true` de-dupes by (PC, addr, value).",
        watch_log_schema,
        tool_watch_memory_log,
    );
    let bplcon0_log_schema = json!({
        "type": "object",
        "properties": {
            "unique": {"type": "boolean", "default": false,
                       "description": "Return only the first occurrence of each distinct BPLCON0 value."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 1024, "default": 64}
        }
    });
    add(
        registry,
        "bplcon0_log",
        "Every BPLCON0 write captured during the run (CPU + copper). Includes BPU histogram so 'does KS ever try BPU>0?' is one query.",
        bplcon0_log_schema,
        tool_bplcon0_log,
    );
    let dump_fb_schema = json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Optional filesystem path for a PNG snapshot. Omit to skip the write."}
        }
    });
    add(
        registry,
        "dump_framebuffer",
        "Snapshot the Denise ARGB framebuffer: top colours, FNV-1a hash, optional PNG write.",
        dump_fb_schema,
        tool_dump_framebuffer,
    );
    add(
        registry,
        "query_copper_list",
        "Decode the copper list at `addr` (or COP1LC) into MOVE/WAIT/SKIP entries.",
        copper_list_schema,
        tool_query_copper_list,
    );
    add(
        registry,
        "query_stack",
        "Read `count` longwords off SSP (or USP via `usp:true`).",
        stack_schema,
        tool_query_stack,
    );
    add(
        registry,
        "memory_read",
        "Read raw bytes from any address (chip RAM / ROM / chipset).",
        memory_schema,
        tool_memory_read,
    );
    add(
        registry,
        "memory_read_long",
        "Read a 32-bit longword from an address.",
        addr_only,
        tool_memory_read_long,
    );
    let memory_scan_schema = json!({
        "type": "object",
        "required": ["start", "end", "value"],
        "properties": {
            "start":    {"description": "Inclusive start address (hex string `$XXX` / `0xXXX` or decimal). Forced to even alignment."},
            "end":      {"description": "Exclusive end address; the last longword read is the one whose final byte is `< end`."},
            "value":    {"description": "32-bit big-endian longword to match. Use a known pointer (e.g. a task address) to find every structure that references it."},
            "mask":     {"description": "Optional AND mask applied to the read value before comparison (default $FFFFFFFF — exact match)."},
            "stride":   {"type": "integer", "minimum": 2, "default": 2,
                         "description": "Byte step between reads. Default 2 (word-aligned). Use 4 for longword-aligned-only sweeps (e.g. MsgPort.mp_SigTask is always longword-aligned)."},
            "max_hits": {"type": "integer", "minimum": 1, "default": 256,
                         "description": "Hard cap on returned hits. The reply sets `truncated: true` if reached. Scan span is capped at 16 MiB regardless."}
        }
    });
    add(
        registry,
        "memory_scan",
        "Scan a memory range for every aligned 32-bit longword whose value matches `value` (optionally AND'd with `mask`). Returns matching addresses + their values. Use to find structures that reference a known pointer — e.g. scan chip RAM for the WB task address to surface every MsgPort whose `mp_SigTask` field names WB.",
        memory_scan_schema,
        tool_memory_scan,
    );
    let resolve_lvo_schema = json!({
        "type": "object",
        "required": ["library"],
        "properties": {
            "library": {
                "type": "string",
                "description": "Library name: `exec.library`, `dos.library`, `intuition.library`, `graphics.library`, or `cia.resource`."
            },
            "offset": {
                "description": "LVO offset — negative (`-318`) or absolute (`318`); decimal or hex (`$13E`, `0x13E`) string accepted. If omitted, the full LVO table for the library is returned."
            }
        }
    });
    add(
        registry,
        "resolve_lvo",
        "Resolve a `jsr -N(a6)` LVO offset to its function name using the NDK 3.2 library tables (exec / dos / intuition / graphics + cia.resource). Omit `offset` to dump the entire library's LVO table. Match modes returned: `hit`, `miss`, `unknown_library`, `library_dump`.",
        resolve_lvo_schema,
        tool_resolve_lvo,
    );
    let query_library_schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Optional library name filter (`exec.library`, `dos.library`, ...). If omitted, every library in ExecBase->LibList is returned."
            }
        }
    });
    add(
        registry,
        "query_library",
        "Walk ExecBase->LibList and decode each `struct Library`: name, version + revision, lib_OpenCnt, lib_Sum, lib_Flags, neg_size + pos_size, and the [base - NegSize, base + PosSize) code range. Pass `name` to filter to one library. Use with `address_to_library` to identify which library a ROM address lives in.",
        query_library_schema,
        tool_query_library,
    );
    let address_to_library_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr": {"description": "Address to classify — decimal int or hex string ($XXX / 0xXXX)."}
        }
    });
    add(
        registry,
        "address_to_library",
        "Reverse-lookup: given any address, walk ExecBase->LibList and find which loaded library's code range contains it. Returns the library name, base address, code range, and signed offset from base (negative = jump-table / LVO region, positive = code body). Match modes: `hit`, `no_library_contains_addr`, `exec_base_uninitialised`.",
        address_to_library_schema,
        tool_address_to_library,
    );
    let read_task_stack_schema = json!({
        "type": "object",
        "properties": {
            "task_addr": {"description": "Task address — we'll read tc_SPReg (offset +54) ourselves. Either this or `sp` is required."},
            "sp":        {"description": "Raw stack pointer to read from — bypasses the tc_SPReg dereference. Use when investigating a non-task stack."},
            "bytes":     {"type": "integer", "minimum": 8, "maximum": 4096, "default": 256,
                          "description": "Bytes of stack to scan, from `sp` upward."},
            "rom_lo":    {"description": "Inclusive low end of the ROM range used to classify candidate return-PCs. Default $00F80000 (KS 2.0+ 512 KiB ROM)."},
            "rom_hi":    {"description": "Inclusive high end of the ROM range. Default $00FFFFFF."},
            "resolve_libraries": {"type": "boolean", "default": true,
                          "description": "If true, cross-reference each ROM hit against ExecBase->LibList and tag with the owning library name."}
        }
    });
    add(
        registry,
        "read_task_stack",
        "Walk a parked task's saved stack starting at `tc_SPReg`. Scans every 2-byte boundary for ROM-pointing 32-bit values (the return-PC chain through libraries), optionally cross-referenced against the loaded library list. Returns the assumed KS 3.x Switch-frame decode (SR at sp+0, MOVEM D2-D7/A2-A6 at sp+2, return PC at sp+46) AND a layout-independent ROM-hit scan so misaligned frames still surface useful data. Folds the two-pass byte scan that found the IPrefs Wait-call chain into one tool.",
        read_task_stack_schema,
        tool_read_task_stack,
    );
    let disasm_around_schema = json!({
        "type": "object",
        "required": ["addr"],
        "properties": {
            "addr":   {"description": "Target address (typically a return PC). Decimal int or hex string ($XXX / 0xXXX)."},
            "before": {"type": "integer", "minimum": 0, "maximum": 32, "default": 4,
                       "description": "Instructions to disasm BEFORE target. Uses alignment search — tries every even start offset and picks the one whose forward disasm lands exactly at target."},
            "after":  {"type": "integer", "minimum": 0, "maximum": 64, "default": 4,
                       "description": "Instructions to disasm AFTER target (target itself is always included)."}
        }
    });
    add(
        registry,
        "disasm_around",
        "Disasm N instructions before and M instructions after a target address. Backward disasm uses alignment search: each even offset before target is tried; the one whose forward disasm lands EXACTLY at target wins. Response carries `aligned: true/false` so the caller can tell if the `before` window is real instructions or mid-instruction garbage. Use after `read_task_stack` to see what called what — point at each return PC, see the JSR/BSR that put it there.",
        disasm_around_schema,
        tool_disasm_around,
    );
    let dump_msgport_messages_schema = json!({
        "type": "object",
        "required": ["port"],
        "properties": {
            "port": {"description": "MsgPort address — decimal int or hex string."},
            "max":  {"type": "integer", "minimum": 1, "maximum": 1024, "default": 64,
                     "description": "Maximum messages to return. `truncated: true` if the walk hit this cap."}
        }
    });
    add(
        registry,
        "dump_msgport_messages",
        "Walk a MsgPort's `mp_MsgList` and decode every queued `struct Message` (mn_ReplyPort, mn_Length, ln_Type / ln_Name). Use after `query_exec_ports` surfaces a port with `msg_count > 0` to see what's waiting to be processed — e.g. pending IORequests at a device port, pending DOS packets at a file-system handler.",
        dump_msgport_messages_schema,
        tool_dump_msgport_messages,
    );
    let signal_task_schema = json!({
        "type": "object",
        "required": ["task_addr", "signals"],
        "properties": {
            "task_addr": {"description": "Task / Process address — decimal int or hex string."},
            "signals":   {"description": "32-bit signal mask to OR into tc_SigRecvd. e.g. $00001000 to set bit 12 (one of the DOS-private signals)."}
        }
    });
    add(
        registry,
        "signal_task",
        "MUTATOR: OR `signals` into a task's tc_SigRecvd. This is NOT a wake-up tool — exec's Signal() does the wake transition (move to TaskReady, set tc_State = READY) synchronously inside the API call, and the scheduler does NOT poll tc_SigRecvd. This tool ONLY updates the field; the task stays parked until something else triggers the list move. Useful for (1) inspecting whether the bits would satisfy the wake condition (response's `would_wake` flag), and (2) pre-staging bits so the next Wait() call returns immediately when reached.",
        signal_task_schema,
        tool_signal_task,
    );
    let wake_task_schema = json!({
        "type": "object",
        "required": ["task_addr"],
        "properties": {
            "task_addr": {"description": "Task / Process address — decimal int or hex string. Must currently be in WAIT state."},
            "signals":   {"description": "Optional signal bits to OR into tc_SigRecvd before waking. Default: the task's tc_SigWait (so Wait() returns immediately with the awaited bits set)."}
        }
    });
    add(
        registry,
        "wake_task",
        "MUTATOR: do the full TaskWait → TaskReady transition that exec.Signal() performs internally. ORs `signals` (default = tc_SigWait) into tc_SigRecvd, unlinks the task from TaskWait, appends to TaskReady, sets tc_State = READY. Use AFTER signal_task to actually KICK the task — the scheduler doesn't poll, so signal_task alone doesn't unblock. Validates state == WAIT and list integrity (pred.succ == task && succ.pred == task) before scribbling. Companion to signal_task: signal_task is observation (will it wake?), wake_task is action (force the wake).",
        wake_task_schema,
        tool_wake_task,
    );
    add(
        registry,
        "disasm",
        "Disassemble `count` m68k instructions starting at `addr`.",
        disasm_schema,
        tool_disasm,
    );
    add(
        registry,
        "insert_media",
        "Insert disk media into DF0 (only `adf` kind today; use `change_pending:true` to fire a disk-change event).",
        insert_media_schema,
        tool_insert_media,
    );
    add(
        registry,
        "eject_media",
        "Eject any disk currently in DF0.",
        empty(),
        tool_eject_media,
    );
    add(
        registry,
        "query_disk",
        "DF0 drive status (cylinder, head, motor, status bits, has_disk).",
        empty(),
        tool_query_disk,
    );
}

/// Registers the full legacy tool set on an [`AmigaSession`]: every
/// chipset-agnostic tool from [`register_amiga_tools`] plus the five
/// session-local ones (the recorder run tools + `reset`) that hold
/// `AmigaSession`-private state. The shared `HeadlessSession` path skips
/// these — `register_common_tools` provides equivalents.
///
/// Retired in Phase 5 once the MCP server cuts over to `HeadlessSession`.
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

    register_amiga_tools(registry);

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
    let start_rec_schema = json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string", "description": "Output MP4 path. Parent directories are created."},
            "fps": {"type": "integer", "minimum": 1, "default": 50,
                    "description": "Frame rate written to the MP4. Default is PAL (50)."}
        }
    });
    add(
        registry,
        "run_frames",
        "Advance the machine by N PAL frames.",
        frames_schema,
        tool_run_frames,
    );
    add(
        registry,
        "run_ticks",
        "Advance the machine by N master/4 ticks.",
        ticks_schema,
        tool_run_ticks,
    );
    add(
        registry,
        "reset",
        "Reload the ROM and re-create the A1200 (fresh boot). Accepts an optional `kind` (\"hard\" / \"soft\"; both currently behave as hard).",
        reset_schema,
        tool_reset,
    );
    add(
        registry,
        "start_video_recording",
        "Begin recording the live framebuffer to one MP4 file (uses ffmpeg from PATH).",
        start_rec_schema,
        tool_start_video_recording,
    );
    add(
        registry,
        "stop_video_recording",
        "Finalise the in-flight recording and return the MP4 summary.",
        empty(),
        tool_stop_video_recording,
    );
}
