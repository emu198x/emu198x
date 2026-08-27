//! Focused Workbench 1.3 bootblock-read trace for the inserted-disk case.
//!
//! The broad `diag_wb13_boot_state` runtime test proved that:
//! - the raw DMA MFM buffer is sane
//! - our own decoder recovers sectors 0 and 1 from that buffer
//! - the second successful CMD_READ only produces one READ-side decode blit
//!
//! This narrower machine-level trace answers the next question:
//! what does STRAP actually ask trackdisk to read, and what does the
//! `trackdisk.device` per-sector loop think its live limits are when it
//! exits after that single sector?

use std::path::PathBuf;

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use motorola_68000::flags::{C, N, Status, V, X, Z};
use peripheral_commodore_amiga_floppy::Adf;

const STRAP_CMD_READ_CALL: u32 = 0x00FE_859C;
const STRAP_POST_CMD_READ: u32 = 0x00FE_85A0;
const TD_READ_LOOP_HEAD: u32 = 0x00FE_A552;
const TD_READ_LOOP_EXIT: u32 = 0x00FE_A57E;
const TD_READ_LOOP_BCC_IR: u16 = 0x6420;
const TD_READ_LOOP_CONTINUE: u32 = 0x00FE_A580;
const TD_READ_LOOP_DONE: u32 = 0x00FE_A5A0;
const TD_READ_BLT0_WRITE: u32 = 0x00FE_A996;
const TD_CKSUM_MISMATCH: u32 = 0x00FE_ACFA;
const TD_FMT_MISMATCH: u32 = 0x00FE_AD10;
const TD_TRK_MISMATCH: u32 = 0x00FE_AD1C;

const IO_DEVICE: u32 = 20;
const IO_UNIT: u32 = 24;
const IO_COMMAND: u32 = 28;
const IO_FLAGS: u32 = 30;
const IO_ERROR: u32 = 31;
const IO_ACTUAL: u32 = 32;
const IO_LENGTH: u32 = 36;
const IO_DATA: u32 = 40;
const IO_OFFSET: u32 = 44;
const IO_HIGH_OFFSET: u32 = 48;

const EXEC_THIS_TASK: u32 = 276;
const LN_NAME: u32 = 10;

const TD_UNIT44_STORE: u32 = 0x00FE_A3BE;
const TD_UNIT44_CLEAR_ACTUAL: u32 = 0x00FE_A3C2;
const TD_UNIT56_SEED_DST: u32 = 0x00FE_A3CA;
const EXEC_BLOCK_FILL_CMP: u32 = 0x00FF_441E;
const EXEC_BLOCK_SET_CMD: u32 = 0x00FF_450A;
const EXEC_BLOCK_SET_DATA: u32 = 0x00FF_4510;
const EXEC_BLOCK_SET_LENGTH: u32 = 0x00FF_4514;
const EXEC_BLOCK_SET_OFFSET: u32 = 0x00FF_4518;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct IoStdReqSnapshot {
    ptr: u32,
    device: u32,
    unit: u32,
    command: u16,
    flags: u8,
    error: i8,
    actual: u32,
    length: u32,
    data: u32,
    offset: u32,
    high_offset: u32,
}

#[derive(Clone, Copy)]
struct LoopStateSnapshot {
    d0: u32,
    a0: u32,
    a1: u32,
    a2: u32,
    a3: u32,
    sr: u16,
    ir: u16,
    instr_start_pc: u32,
    next_fetch_addr: u32,
    a2_20: u32,
    a2_24: u32,
    unit_slot: u8,
    unit_expected_track: u8,
    unit_buf: u32,
    unit_dst: u32,
    bcc_from_sr: bool,
    bcc_from_operands: bool,
    req: IoStdReqSnapshot,
}

#[derive(Clone, Copy)]
struct LoopExitSnapshot {
    attempt: u32,
    frame: u64,
    cck: u64,
    first_pc: u32,
    resolved_instr_start_pc: u32,
    resolved_pc: u32,
    resolved_ir: u16,
    state: LoopStateSnapshot,
}

#[derive(Clone, Copy)]
struct PendingLoopExit {
    snapshot: LoopExitSnapshot,
    branch_instr_start_pc: u32,
}

struct Unit44PassResult {
    unit_base: u32,
    strap_req_ptr: u32,
    later_block_ptr: u32,
    pointer_timeline: Vec<String>,
    pointer_writes: Vec<String>,
}

fn load_artifact(path: &PathBuf) -> Option<Vec<u8>> {
    if !path.exists() {
        emu198x_test_skip::record(&format!("skipping: missing {}", path.display()));
        return None;
    }
    std::fs::read(path).ok()
}

fn read_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    amiga.read_long(addr)
}

fn read_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    amiga.read_word(addr)
}

fn read_byte(amiga: &AmigaOcs, addr: u32) -> u8 {
    (amiga.read_word(addr & !1) >> (if addr & 1 == 0 { 8 } else { 0 })) as u8
}

fn read_iostdreq(amiga: &AmigaOcs, req: u32) -> IoStdReqSnapshot {
    IoStdReqSnapshot {
        ptr: req,
        device: read_long(amiga, req.wrapping_add(IO_DEVICE)),
        unit: read_long(amiga, req.wrapping_add(IO_UNIT)),
        command: read_word(amiga, req.wrapping_add(IO_COMMAND)),
        flags: read_byte(amiga, req.wrapping_add(IO_FLAGS)),
        error: read_byte(amiga, req.wrapping_add(IO_ERROR)) as i8,
        actual: read_long(amiga, req.wrapping_add(IO_ACTUAL)),
        length: read_long(amiga, req.wrapping_add(IO_LENGTH)),
        data: read_long(amiga, req.wrapping_add(IO_DATA)),
        offset: read_long(amiga, req.wrapping_add(IO_OFFSET)),
        high_offset: read_long(amiga, req.wrapping_add(IO_HIGH_OFFSET)),
    }
}

fn read_iostdreq_if(amiga: &AmigaOcs, req: u32) -> IoStdReqSnapshot {
    if req == 0 {
        IoStdReqSnapshot::default()
    } else {
        read_iostdreq(amiga, req)
    }
}

fn read_cstring(amiga: &AmigaOcs, addr: u32, max: u32) -> String {
    if addr == 0 {
        return "<null>".into();
    }
    let mut s = String::new();
    for i in 0..max {
        let b = read_byte(amiga, addr.wrapping_add(i));
        if b == 0 {
            break;
        }
        if b.is_ascii() && !b.is_ascii_control() {
            s.push(b as char);
        } else {
            s.push('?');
        }
    }
    s
}

fn current_task_name(amiga: &AmigaOcs) -> String {
    let exec_base = read_long(amiga, 0x0000_0004);
    if exec_base == 0 {
        return "<no-exec>".into();
    }
    let task = read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
    if task == 0 {
        return "<null-task>".into();
    }
    let name_ptr = read_long(amiga, task.wrapping_add(LN_NAME));
    let name = read_cstring(amiga, name_ptr, 32);
    if name.is_empty() {
        format!("<addr=${task:08X}>")
    } else {
        name
    }
}

fn fmt_req(req: &IoStdReqSnapshot) -> String {
    format!(
        "req=${:08X} cmd=${:04X} flags=${:02X} err={} \
         dev=${:08X} unit=${:08X} actual=${:08X} len=${:08X} \
         data=${:08X} off=${:08X} high=${:08X}",
        req.ptr,
        req.command,
        req.flags,
        req.error,
        req.device,
        req.unit,
        req.actual,
        req.length,
        req.data,
        req.offset,
        req.high_offset,
    )
}

fn fmt_req_delta(old: &IoStdReqSnapshot, new: &IoStdReqSnapshot) -> String {
    let mut parts = Vec::new();
    if old.command != new.command {
        parts.push(format!("cmd ${:04X}->${:04X}", old.command, new.command));
    }
    if old.flags != new.flags {
        parts.push(format!("flags ${:02X}->${:02X}", old.flags, new.flags));
    }
    if old.error != new.error {
        parts.push(format!("err {}->{}", old.error, new.error));
    }
    if old.actual != new.actual {
        parts.push(format!("actual ${:08X}->${:08X}", old.actual, new.actual));
    }
    if old.length != new.length {
        parts.push(format!("len ${:08X}->${:08X}", old.length, new.length));
    }
    if old.data != new.data {
        parts.push(format!("data ${:08X}->${:08X}", old.data, new.data));
    }
    if old.offset != new.offset {
        parts.push(format!("off ${:08X}->${:08X}", old.offset, new.offset));
    }
    if old.device != new.device {
        parts.push(format!("dev ${:08X}->${:08X}", old.device, new.device));
    }
    if old.unit != new.unit {
        parts.push(format!("unit ${:08X}->${:08X}", old.unit, new.unit));
    }
    if old.high_offset != new.high_offset {
        parts.push(format!(
            "high ${:08X}->${:08X}",
            old.high_offset, new.high_offset
        ));
    }
    if parts.is_empty() {
        "no io-field change".into()
    } else {
        parts.join(" ")
    }
}

fn capture_loop_state(amiga: &AmigaOcs, req: IoStdReqSnapshot) -> LoopStateSnapshot {
    let cpu = amiga.cpu();
    let regs = &cpu.regs;
    let a2 = regs.a[2];
    let a3 = regs.a[3];
    LoopStateSnapshot {
        d0: regs.d[0],
        a0: regs.a[0],
        a1: regs.a[1],
        a2,
        a3,
        sr: regs.sr,
        ir: cpu.ir,
        instr_start_pc: cpu.instr_start_pc,
        next_fetch_addr: cpu.next_fetch_addr,
        a2_20: read_long(amiga, a2.wrapping_add(0x20)),
        a2_24: read_long(amiga, a2.wrapping_add(0x24)),
        unit_slot: read_byte(amiga, a3.wrapping_add(0x49)),
        unit_expected_track: read_byte(amiga, a3.wrapping_add(0x4B)),
        unit_buf: read_long(amiga, a3.wrapping_add(0x4E)),
        unit_dst: read_long(amiga, a3.wrapping_add(0x56)),
        bcc_from_sr: Status::condition(regs.sr, 0x4),
        bcc_from_operands: regs.d[0] >= read_long(amiga, a2.wrapping_add(0x24)),
        req,
    }
}

fn fmt_sr(sr: u16) -> String {
    format!(
        "SR=${sr:04X} [X={} N={} Z={} V={} C={}]",
        u8::from(sr & X != 0),
        u8::from(sr & N != 0),
        u8::from(sr & Z != 0),
        u8::from(sr & V != 0),
        u8::from(sr & C != 0),
    )
}

fn fmt_loop_state(state: &LoopStateSnapshot) -> String {
    format!(
        "D0=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} \
         {} IR=${:04X} instr_start=${:08X} next_fetch=${:08X} \
         A2[$20]=${:08X} A2[$24]=${:08X} \
         unit[$49]=${:02X} unit[$4B]=${:02X} unit[$4E]=${:08X} unit[$56]=${:08X} \
         bcc(sr)={} bcc(D0>=limit)={} \
         {}",
        state.d0,
        state.a0,
        state.a1,
        state.a2,
        state.a3,
        fmt_sr(state.sr),
        state.ir,
        state.instr_start_pc,
        state.next_fetch_addr,
        state.a2_20,
        state.a2_24,
        state.unit_slot,
        state.unit_expected_track,
        state.unit_buf,
        state.unit_dst,
        state.bcc_from_sr,
        state.bcc_from_operands,
        fmt_req(&state.req),
    )
}

fn loop_exit_target_label(instr_start_pc: u32) -> &'static str {
    match instr_start_pc {
        TD_READ_LOOP_CONTINUE => "continue / next sector",
        TD_READ_LOOP_DONE => "done / limit reached",
        _ => "other branch",
    }
}

fn origin_context_label(instr_start_pc: u32) -> &'static str {
    match instr_start_pc {
        EXEC_BLOCK_FILL_CMP => "exec block fill loop",
        EXEC_BLOCK_SET_CMD => "exec request set cmd",
        EXEC_BLOCK_SET_DATA => "exec request set data",
        EXEC_BLOCK_SET_LENGTH => "exec request set len",
        EXEC_BLOCK_SET_OFFSET => "exec request set offset",
        TD_UNIT44_STORE => "trackdisk store unit[$44]",
        TD_UNIT44_CLEAR_ACTUAL => "trackdisk clear actual",
        TD_UNIT56_SEED_DST => "trackdisk seed dst",
        TD_READ_LOOP_HEAD => "trackdisk later read loop head",
        _ => "other writer",
    }
}

fn format_watch_write(cck: u64, pc: u32, addr: u32, val: u16, is_word: bool) -> String {
    format!("cck={cck} pc=${pc:08X} addr=${addr:08X} val=${val:04X} word={is_word}",)
}

fn active_sp(amiga: &AmigaOcs) -> u32 {
    let regs = &amiga.cpu().regs;
    if regs.sr & 0x2000 != 0 {
        regs.ssp
    } else {
        regs.usp
    }
}

fn fmt_stack_top(amiga: &AmigaOcs, sp: u32) -> String {
    format!(
        "[sp]=${:08X} [sp+4]=${:08X} [sp+8]=${:08X}",
        read_long(amiga, sp),
        read_long(amiga, sp.wrapping_add(4)),
        read_long(amiga, sp.wrapping_add(8)),
    )
}

fn run_unit44_watch_pass(rom: &[u8], adf_bytes: &[u8]) -> Unit44PassResult {
    let mut amiga = AmigaOcs::with_slow_ram(rom.to_vec(), 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes.to_vec()).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr_start_pc = amiga.cpu().instr_start_pc;
    let mut tick = 0u64;

    let mut strap_req_ptr = 0u32;
    let mut unit_base = 0u32;
    let mut last_unit44 = None;
    let mut pointer_timeline = Vec::new();
    let mut later_block_ptr = 0u32;

    for _ in 0..(350u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;

        let pc = amiga.cpu().regs.pc;
        let instr_start_pc = amiga.cpu().instr_start_pc;
        if pc == prev_pc && instr_start_pc == prev_instr_start_pc {
            continue;
        }

        let cck = tick / 2;

        if pc == STRAP_CMD_READ_CALL {
            let req = read_iostdreq(&amiga, amiga.cpu().regs.a[1]);
            strap_req_ptr = req.ptr;
            unit_base = req.unit;
            amiga.debug_watch_addr = Some((unit_base.wrapping_add(0x44), 4));
            let unit44 = read_long(&amiga, unit_base.wrapping_add(0x44));
            last_unit44 = Some(unit44);
            pointer_timeline.push(format!(
                "cck={cck} STRAP req=${:08X} unit=${:08X} initial unit[$44]=${:08X}",
                strap_req_ptr, unit_base, unit44
            ));
        }

        if unit_base != 0 {
            let unit44 = read_long(&amiga, unit_base.wrapping_add(0x44));
            if last_unit44 != Some(unit44) {
                pointer_timeline.push(format!(
                    "cck={cck} pc=${pc:08X} instr=${instr_start_pc:08X} ir=${:04X} \
                     unit[$44] ${:08X} -> ${unit44:08X}",
                    amiga.cpu().ir,
                    last_unit44.unwrap_or(0),
                ));
                last_unit44 = Some(unit44);
                if strap_req_ptr != 0 && unit44 != 0 && unit44 != strap_req_ptr {
                    later_block_ptr = unit44;
                }
            }
        }

        if pc == TD_READ_LOOP_HEAD
            && later_block_ptr != 0
            && amiga.cpu().regs.a[2] == later_block_ptr
        {
            pointer_timeline.push(format!(
                "cck={cck} later READ loop entered with A2=${:08X}",
                amiga.cpu().regs.a[2]
            ));
            break;
        }

        prev_pc = pc;
        prev_instr_start_pc = instr_start_pc;
    }

    let pointer_writes = amiga
        .debug_watch_writes
        .iter()
        .map(|(cck, pc, addr, val, is_word)| format_watch_write(*cck, *pc, *addr, *val, *is_word))
        .collect();

    Unit44PassResult {
        unit_base,
        strap_req_ptr,
        later_block_ptr,
        pointer_timeline,
        pointer_writes,
    }
}

fn run_later_request_origin_pass(
    rom: &[u8],
    adf_bytes: &[u8],
    unit_base: u32,
    later_block_ptr: u32,
) -> Vec<String> {
    let mut amiga = AmigaOcs::with_slow_ram(rom.to_vec(), 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes.to_vec()).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr_start_pc = amiga.cpu().instr_start_pc;
    let mut tick = 0u64;

    let mut last_unit44 = read_long(&amiga, unit_base.wrapping_add(0x44));
    let mut last_req = read_iostdreq_if(&amiga, later_block_ptr);
    let mut events = Vec::new();

    for _ in 0..(350u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;

        let pc = amiga.cpu().regs.pc;
        let instr_start_pc = amiga.cpu().instr_start_pc;
        if pc == prev_pc && instr_start_pc == prev_instr_start_pc {
            continue;
        }

        let cck = tick / 2;
        let cur_unit44 = read_long(&amiga, unit_base.wrapping_add(0x44));
        let cur_req = read_iostdreq_if(&amiga, later_block_ptr);
        let unit44_changed = cur_unit44 != last_unit44;
        let req_changed = cur_req != last_req;

        if unit44_changed || req_changed {
            let regs = &amiga.cpu().regs;
            let sp = active_sp(&amiga);
            events.push(format!(
                "cck={cck} task={} pc=${pc:08X} instr=${instr_start_pc:08X} ir=${:04X} \
                 {} sp=${sp:08X} {} \
                 D1=${:08X} D2=${:08X} D3=${:08X} D4=${:08X} \
                 A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} A4=${:08X} A6=${:08X} \
                 unit[$44] ${:08X}->${:08X} {} {}",
                current_task_name(&amiga),
                amiga.cpu().ir,
                origin_context_label(instr_start_pc),
                fmt_stack_top(&amiga, sp),
                regs.d[1],
                regs.d[2],
                regs.d[3],
                regs.d[4],
                regs.a[0],
                regs.a[1],
                regs.a[2],
                regs.a[3],
                regs.a[4],
                regs.a[6],
                last_unit44,
                cur_unit44,
                fmt_req_delta(&last_req, &cur_req),
                fmt_req(&cur_req),
            ));
            last_unit44 = cur_unit44;
            last_req = cur_req;
        }

        if instr_start_pc == TD_READ_LOOP_HEAD && amiga.cpu().regs.a[2] == later_block_ptr {
            events.push(format!(
                "cck={cck} task={} pc=${pc:08X} instr=${instr_start_pc:08X} ir=${:04X} \
                 {} final {}",
                current_task_name(&amiga),
                amiga.cpu().ir,
                origin_context_label(instr_start_pc),
                fmt_req(&cur_req),
            ));
            break;
        }

        prev_pc = pc;
        prev_instr_start_pc = instr_start_pc;
    }

    events
}

fn run_later_block_field_watch_pass(
    rom: &[u8],
    adf_bytes: &[u8],
    later_block_ptr: u32,
) -> (Vec<String>, Vec<String>) {
    let mut amiga = AmigaOcs::with_slow_ram(rom.to_vec(), 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes.to_vec()).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);
    amiga.debug_watch_addr = Some((later_block_ptr.wrapping_add(0x20), 8));

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr_start_pc = amiga.cpu().instr_start_pc;
    let mut tick = 0u64;
    let mut field_timeline = Vec::new();

    let mut last_20 = read_long(&amiga, later_block_ptr.wrapping_add(0x20));
    let mut last_24 = read_long(&amiga, later_block_ptr.wrapping_add(0x24));
    field_timeline.push(format!(
        "cck=0 initial block=${later_block_ptr:08X} +$20=${last_20:08X} +$24=${last_24:08X}"
    ));

    for _ in 0..(350u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;

        let pc = amiga.cpu().regs.pc;
        let instr_start_pc = amiga.cpu().instr_start_pc;
        if pc == prev_pc && instr_start_pc == prev_instr_start_pc {
            continue;
        }

        let cck = tick / 2;
        let cur_20 = read_long(&amiga, later_block_ptr.wrapping_add(0x20));
        let cur_24 = read_long(&amiga, later_block_ptr.wrapping_add(0x24));
        if cur_20 != last_20 || cur_24 != last_24 {
            field_timeline.push(format!(
                "cck={cck} pc=${pc:08X} instr=${instr_start_pc:08X} ir=${:04X} \
                 block=${later_block_ptr:08X} +$20 ${last_20:08X}->{cur_20:08X} \
                 +$24 ${last_24:08X}->{cur_24:08X}",
                amiga.cpu().ir,
            ));
            last_20 = cur_20;
            last_24 = cur_24;
        }

        if pc == TD_READ_LOOP_HEAD && amiga.cpu().regs.a[2] == later_block_ptr {
            field_timeline.push(format!(
                "cck={cck} later READ loop head sees block=${later_block_ptr:08X} \
                 +$20=${cur_20:08X} +$24=${cur_24:08X}"
            ));
            break;
        }

        prev_pc = pc;
        prev_instr_start_pc = instr_start_pc;
    }

    let field_writes = amiga
        .debug_watch_writes
        .iter()
        .map(|(cck, pc, addr, val, is_word)| format_watch_write(*cck, *pc, *addr, *val, *is_word))
        .collect();

    (field_timeline, field_writes)
}

#[test]
#[ignore = "FIXTURE: needs KS 1.3 ROM + Workbench 1.3 ADF locally"]
fn trace_wb13_cmd_read_request_and_loop_state() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr_start_pc = amiga.cpu().instr_start_pc;
    let mut tick = 0u64;
    let mut current_attempt = 0u32;
    let mut last_req_ptr = 0u32;
    let mut last_req = IoStdReqSnapshot::default();
    let mut pending_exit: Option<PendingLoopExit> = None;

    let mut cmd_read_calls = Vec::new();
    let mut cmd_read_returns = Vec::new();
    let mut loop_heads = Vec::new();
    let mut loop_exits = Vec::new();
    let mut decode_blits = Vec::new();
    let mut validation_fails = Vec::new();

    for _ in 0..(350u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;

        let pc = amiga.cpu().regs.pc;
        let instr_start_pc = amiga.cpu().instr_start_pc;
        if pc == prev_pc && instr_start_pc == prev_instr_start_pc {
            continue;
        }

        let frame = tick / PAL_FRAME_TICKS;
        let cck = tick / 2;

        if let Some(mut pending) = pending_exit {
            if pending.snapshot.first_pc == 0 {
                pending.snapshot.first_pc = pc;
            }
            if instr_start_pc != pending.branch_instr_start_pc {
                pending.snapshot.resolved_instr_start_pc = instr_start_pc;
                pending.snapshot.resolved_pc = pc;
                pending.snapshot.resolved_ir = amiga.cpu().ir;
                loop_exits.push(format!(
                    "attempt {} frame~{} cck={} first_pc=${:08X} \
                     resolved_instr=${:08X} ({}) resolved_pc=${:08X} resolved_ir=${:04X} {}",
                    pending.snapshot.attempt,
                    pending.snapshot.frame,
                    pending.snapshot.cck,
                    pending.snapshot.first_pc,
                    pending.snapshot.resolved_instr_start_pc,
                    loop_exit_target_label(pending.snapshot.resolved_instr_start_pc),
                    pending.snapshot.resolved_pc,
                    pending.snapshot.resolved_ir,
                    fmt_loop_state(&pending.snapshot.state),
                ));
                pending_exit = None;
            } else {
                pending_exit = Some(pending);
            }
        }

        match pc {
            STRAP_CMD_READ_CALL => {
                current_attempt = current_attempt.saturating_add(1);
                last_req_ptr = amiga.cpu().regs.a[1];
                last_req = read_iostdreq(&amiga, last_req_ptr);
                cmd_read_calls.push(format!(
                    "attempt {} frame~{} cck={} {}",
                    current_attempt,
                    frame,
                    cck,
                    fmt_req(&last_req),
                ));
            }
            STRAP_POST_CMD_READ => {
                let req_ptr = if last_req_ptr != 0 {
                    last_req_ptr
                } else {
                    amiga.cpu().regs.a[1]
                };
                last_req = read_iostdreq_if(&amiga, req_ptr);
                cmd_read_returns.push(format!(
                    "attempt {} frame~{} cck={} D0=${:08X} {}",
                    current_attempt,
                    frame,
                    cck,
                    amiga.cpu().regs.d[0],
                    fmt_req(&last_req),
                ));
            }
            TD_READ_LOOP_HEAD => {
                let state = capture_loop_state(&amiga, last_req);
                loop_heads.push(format!(
                    "attempt {} frame~{} cck={} pc=${:08X} {}",
                    current_attempt,
                    frame,
                    cck,
                    pc,
                    fmt_loop_state(&state),
                ));
            }
            _ if instr_start_pc == TD_READ_LOOP_EXIT && amiga.cpu().ir == TD_READ_LOOP_BCC_IR => {
                let state = capture_loop_state(&amiga, last_req);
                pending_exit = Some(PendingLoopExit {
                    snapshot: LoopExitSnapshot {
                        attempt: current_attempt,
                        frame,
                        cck,
                        first_pc: 0,
                        resolved_instr_start_pc: 0,
                        resolved_pc: 0,
                        resolved_ir: 0,
                        state,
                    },
                    branch_instr_start_pc: state.instr_start_pc,
                });
            }
            TD_READ_BLT0_WRITE => {
                decode_blits.push(format!(
                    "attempt {} frame~{} cck={} D0=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                    current_attempt,
                    frame,
                    cck,
                    amiga.cpu().regs.d[0],
                    amiga.cpu().regs.a[0],
                    amiga.cpu().regs.a[1],
                    amiga.cpu().regs.a[2],
                    amiga.cpu().regs.a[3],
                ));
            }
            TD_CKSUM_MISMATCH | TD_FMT_MISMATCH | TD_TRK_MISMATCH => {
                let label = match pc {
                    TD_CKSUM_MISMATCH => "hdr-cksum mismatch",
                    TD_FMT_MISMATCH => "fmt != $FF",
                    TD_TRK_MISMATCH => "track mismatch",
                    _ => unreachable!(),
                };
                validation_fails.push(format!(
                    "attempt {} frame~{} cck={} ${:08X} {} \
                     D0=${:08X} D2=${:08X} D3=${:08X} A2=${:08X} A3=${:08X} \
                     unit[$49]=${:02X} unit[$4B]=${:02X} unit[$4E]=${:08X} unit[$56]=${:08X} \
                     {}",
                    current_attempt,
                    frame,
                    cck,
                    pc,
                    label,
                    amiga.cpu().regs.d[0],
                    amiga.cpu().regs.d[2],
                    amiga.cpu().regs.d[3],
                    amiga.cpu().regs.a[2],
                    amiga.cpu().regs.a[3],
                    read_byte(&amiga, amiga.cpu().regs.a[3].wrapping_add(0x49)),
                    read_byte(&amiga, amiga.cpu().regs.a[3].wrapping_add(0x4B)),
                    read_long(&amiga, amiga.cpu().regs.a[3].wrapping_add(0x4E)),
                    read_long(&amiga, amiga.cpu().regs.a[3].wrapping_add(0x56)),
                    fmt_req(&last_req),
                ));
            }
            _ => {}
        }

        prev_pc = pc;
        prev_instr_start_pc = instr_start_pc;
    }

    if let Some(pending) = pending_exit {
        loop_exits.push(format!(
            "attempt {} frame~{} cck={} unresolved branch observation {}",
            pending.snapshot.attempt,
            pending.snapshot.frame,
            pending.snapshot.cck,
            fmt_loop_state(&pending.snapshot.state),
        ));
    }

    println!("=== STRAP CMD_READ calls ===");
    for line in &cmd_read_calls {
        println!("  {line}");
    }

    println!("\n=== STRAP post-CMD_READ returns ===");
    for line in &cmd_read_returns {
        println!("  {line}");
    }

    println!("\n=== trackdisk READ loop head hits ($FEA552) ===");
    for line in &loop_heads {
        println!("  {line}");
    }

    println!("\n=== trackdisk READ loop BCC outcomes (instr_start=$FEA57E) ===");
    for line in &loop_exits {
        println!("  {line}");
    }

    println!("\n=== READ decode blit setup hits ($FEA996) ===");
    for line in &decode_blits {
        println!("  {line}");
    }

    println!("\n=== validation-failure branch hits ===");
    for line in &validation_fails {
        println!("  {line}");
    }
}

#[test]
#[ignore = "FIXTURE: needs KS 1.3 ROM + Workbench 1.3 ADF locally"]
fn trace_wb13_later_read_block_writers() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let unit44 = run_unit44_watch_pass(&rom, &adf_bytes);
    println!("=== pass 1: unit[$44] pointer timeline ===");
    println!(
        "  unit=${:08X} strap_req=${:08X} later_block=${:08X}",
        unit44.unit_base, unit44.strap_req_ptr, unit44.later_block_ptr
    );
    for line in &unit44.pointer_timeline {
        println!("  {line}");
    }

    println!("\n=== pass 1: CPU writes hitting unit[$44] ===");
    for line in &unit44.pointer_writes {
        println!("  {line}");
    }

    if unit44.later_block_ptr == 0 {
        println!("\nno later block pointer discovered before later READ loop");
        return;
    }

    let (field_timeline, field_writes) =
        run_later_block_field_watch_pass(&rom, &adf_bytes, unit44.later_block_ptr);

    println!("\n=== pass 2: later block +$20/+$24 timeline ===");
    for line in &field_timeline {
        println!("  {line}");
    }

    println!("\n=== pass 2: CPU writes hitting later block +$20..+$27 ===");
    for line in &field_writes {
        println!("  {line}");
    }
}

#[test]
#[ignore = "FIXTURE: needs KS 1.3 ROM + Workbench 1.3 ADF locally"]
fn trace_wb13_later_request_origin_context() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let unit44 = run_unit44_watch_pass(&rom, &adf_bytes);
    if unit44.unit_base == 0 || unit44.later_block_ptr == 0 {
        println!(
            "could not discover unit/later block: unit=${:08X} later=${:08X}",
            unit44.unit_base, unit44.later_block_ptr
        );
        return;
    }

    let events =
        run_later_request_origin_pass(&rom, &adf_bytes, unit44.unit_base, unit44.later_block_ptr);

    println!("=== later request origin context ===");
    println!(
        "  unit=${:08X} later_block=${:08X}",
        unit44.unit_base, unit44.later_block_ptr
    );
    for line in &events {
        println!("  {line}");
    }
}
