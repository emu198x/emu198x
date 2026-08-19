//! Diagnostic: late Workbench 1.3 boot state with the real WB 1.3 ADF inserted.
//!
//! Earlier probes established that KS 1.3 STRAP does complete its initial
//! 1024-byte bootblock read and does execute bootblock code in chip RAM.
//! The unresolved question is later: why does the machine settle into the
//! Exec idle region instead of continuing on to a visible Workbench boot?
//!
//! This test instruments the *late* boot window rather than the bootblock
//! read itself:
//! - boot the actual WB 1.3 disk for 800 frames
//! - trace the final 100 frames of Exec Wait/Signal/message-port traffic
//! - sample the hot PC + running task each frame
//! - dump TaskReady / TaskWait / ThisTask with stack-derived resume-PC
//!   candidates for blocked tasks

use std::collections::BTreeMap;
use std::path::PathBuf;

use commodore_agnus_ocs::SlotOwner;
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use motorola_68000::disasm::disassemble;

const EXEC_THIS_TASK: u32 = 276;
const EXEC_IDLE_COUNT: u32 = 280;
const EXEC_DISP_COUNT: u32 = 284;
const EXEC_LIB_LIST: u32 = 378;
const EXEC_DEVICE_LIST: u32 = 350;
const EXEC_PORT_LIST: u32 = 392;
const EXEC_TASK_READY: u32 = 406;
const EXEC_TASK_WAIT: u32 = 420;

const LN_SUCC: u32 = 0;
const LN_PRED: u32 = 4;
const LN_NAME: u32 = 10;

const TASK_STATE: u32 = 15;
const TASK_SIG_ALLOC: u32 = 18;
const TASK_SIG_WAIT: u32 = 22;
const TASK_SIG_RECVD: u32 = 26;
const TASK_SP_REG: u32 = 54;

const LVO_WAIT: i32 = -318;
const LVO_ALLOC_SIGNAL: i32 = -330;
const LVO_SIGNAL: i32 = -324;
const LVO_CAUSE: i32 = -180;
const LVO_SET_SIGNAL: i32 = -306;
const LVO_PUT_MSG: i32 = -366;
const LVO_GET_MSG: i32 = -372;
const LVO_REPLY_MSG: i32 = -378;
const LVO_WAIT_PORT: i32 = -384;
const LVO_DO_IO: i32 = -456;
const LVO_SEND_IO: i32 = -462;
const LVO_BEGIN_IO: i32 = -30;

const INTUITION_LVO_OPEN_SCREEN: i32 = -198;
const INTUITION_LVO_OPEN_WINDOW: i32 = -204;
const INTUITION_LVO_INIT_REQUESTER: i32 = -138;
const INTUITION_LVO_AUTO_REQUEST: i32 = -348;

const INTERRUPT_IS_DATA: u32 = 0x0E;
const INTERRUPT_IS_CODE: u32 = 0x12;
const TIMER_VBL_INT_OFFSET: u32 = 0xA2;
const TIMER_CIA_INT_OFFSET: u32 = 0x54;

const IO_DEVICE: u32 = 20;
const IO_UNIT: u32 = 24;
const IO_COMMAND: u32 = 28;
const IO_FLAGS: u32 = 30;
const IO_ERROR: u32 = 31;
const IO_ACTUAL: u32 = 32;
const IO_LENGTH: u32 = 36;
const IO_DATA: u32 = 40;
const IO_OFFSET: u32 = 44;

const PORT_FLAGS: u32 = 0x0E;
const PORT_SIGBIT: u32 = 0x0F;
const PORT_SIGTASK: u32 = 0x10;
const PORT_MSG_LIST: u32 = 0x14;

const MN_REPLYPORT: u32 = 14;
const MN_LENGTH: u32 = 18;

const VALIDATOR_IDCMP_SETUP_ENTRY: u32 = 0x00FD_56F0;
const VALIDATOR_IDCMP_SETUP_AFTER_HELPER: u32 = 0x00FD_56FA;
const VALIDATOR_IDCMP_DECIDER_ENTRY: u32 = 0x00FD_5B8A;
const VALIDATOR_WAIT_WRAPPER_ROM: u32 = 0x00FE_024A;
const VALIDATOR_WAIT_WRAPPER_RAM: u32 = 0x00C0_024A;

const VALIDATOR_OWNER_CTRL: u32 = 0x52;
const VALIDATOR_OWNER_SIGNAL_PORT: u32 = 0x56;
const VALIDATOR_OWNER_IGNORE_PORT: u32 = 0x5A;
const VALIDATOR_OWNER_BYTE62: u32 = 0x62;
const VALIDATOR_OWNER_BYTE63: u32 = 0x63;
const VALIDATOR_OWNER_LONG64: u32 = 0x64;

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

fn state_name(state: u8) -> &'static str {
    match state {
        0 => "INVALID",
        1 => "ADDED",
        2 => "RUN",
        3 => "READY",
        4 => "WAIT",
        5 => "EXCEPT",
        6 => "REMOVED",
        _ => "???",
    }
}

fn resolve_lvo(amiga: &AmigaOcs, exec_base: u32, lvo: i32) -> Option<u32> {
    let slot = (exec_base as i64 + lvo as i64) as u32;
    if read_word(amiga, slot) != 0x4EF9 {
        return None;
    }
    Some(read_long(amiga, slot.wrapping_add(2)))
}

fn find_library(amiga: &AmigaOcs, exec_base: u32, target: &str) -> Option<u32> {
    let list_addr = exec_base.wrapping_add(EXEC_LIB_LIST);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut node = head;
    for _ in 0..24 {
        if node == 0 || node == tail_sentinel {
            return None;
        }
        if task_name(amiga, node) == target {
            return Some(node);
        }
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
    }
    None
}

fn find_device(amiga: &AmigaOcs, exec_base: u32, target: &str) -> Option<u32> {
    let list_addr = exec_base.wrapping_add(EXEC_DEVICE_LIST);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut node = head;
    for _ in 0..24 {
        if node == 0 || node == tail_sentinel {
            return None;
        }
        if task_name(amiga, node) == target {
            return Some(node);
        }
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
    }
    None
}

fn snapshot_device_list(amiga: &AmigaOcs, exec_base: u32, limit: usize) -> Vec<(u32, String)> {
    let list_addr = exec_base.wrapping_add(EXEC_DEVICE_LIST);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut out = Vec::new();
    let mut node = head;
    let mut guard = 0usize;
    while node != 0 && node != tail_sentinel && guard < limit {
        out.push((node, task_name(amiga, node)));
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
        guard += 1;
    }
    out
}

fn current_task_addr(amiga: &AmigaOcs) -> u32 {
    let exec_base = read_long(amiga, 0x0000_0004);
    if exec_base == 0 {
        return 0;
    }
    read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK))
}

fn task_name(amiga: &AmigaOcs, task_addr: u32) -> String {
    if task_addr == 0 {
        return "<null>".into();
    }
    let name_ptr = read_long(amiga, task_addr.wrapping_add(LN_NAME));
    let name = read_cstring(amiga, name_ptr, 32);
    if name.is_empty() {
        format!("<addr=${task_addr:08X}>")
    } else {
        name
    }
}

fn active_sp(amiga: &AmigaOcs) -> u32 {
    let regs = &amiga.cpu().regs;
    if regs.sr & 0x2000 != 0 {
        regs.ssp
    } else {
        regs.usp
    }
}

fn is_codeish_addr(addr: u32) -> bool {
    (0x00FC_0000..0x0100_0000).contains(&addr)
        || (0x0000_0400..0x0000_8000).contains(&addr)
        || (0x00C0_0000..0x00C8_0000).contains(&addr)
}

fn scan_stack_candidates(amiga: &AmigaOcs, sp: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if sp == 0 {
        return out;
    }
    for off in (0..=96u32).step_by(2) {
        let value = read_long(amiga, sp.wrapping_add(off));
        if is_codeish_addr(value) {
            out.push((off, value));
            if out.len() >= 12 {
                break;
            }
        }
    }
    out
}

fn fmt_stack_candidates(candidates: &[(u32, u32)]) -> String {
    if candidates.is_empty() {
        return "<none>".into();
    }
    candidates
        .iter()
        .map(|(off, value)| format!("sp+{off}=${value:08X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn describe_iorequest(amiga: &AmigaOcs, io: u32) -> String {
    if io == 0 {
        return "<null>".into();
    }
    let device = read_long(amiga, io.wrapping_add(IO_DEVICE));
    let unit = read_long(amiga, io.wrapping_add(IO_UNIT));
    let command = read_word(amiga, io.wrapping_add(IO_COMMAND));
    let flags = read_byte(amiga, io.wrapping_add(IO_FLAGS));
    let error = read_byte(amiga, io.wrapping_add(IO_ERROR)) as i8;
    let actual = read_long(amiga, io.wrapping_add(IO_ACTUAL));
    let length = read_long(amiga, io.wrapping_add(IO_LENGTH));
    let data = read_long(amiga, io.wrapping_add(IO_DATA));
    let offset = read_long(amiga, io.wrapping_add(IO_OFFSET));
    let dev_name = if device == 0 {
        "<null>".into()
    } else {
        let name_ptr = read_long(amiga, device.wrapping_add(LN_NAME));
        read_cstring(amiga, name_ptr, 32)
    };
    format!(
        "io=${io:08X} dev={dev_name} unit=${unit:08X} cmd=${command:04X} \
         flags=${flags:02X} err={error} actual=${actual:08X} len=${length:08X} \
         data=${data:08X} off=${offset:08X}"
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct CopperDisplayState {
    bplcon0: Option<u16>,
    bpl1mod: Option<i16>,
    bpl2mod: Option<i16>,
    ddfstrt: Option<u16>,
    ddfstop: Option<u16>,
    diwstrt: Option<u16>,
    diwstop: Option<u16>,
    bpl_pt: [Option<u32>; 6],
}

fn decode_copper_display_state(amiga: &AmigaOcs, base: u32, len_instrs: u32) -> CopperDisplayState {
    let base = base & 0x001F_FFFE;
    let mut state = CopperDisplayState::default();
    let mut hi = [None; 6];
    let mut lo = [None; 6];
    for i in 0..len_instrs {
        let addr = base.wrapping_add(i.wrapping_mul(4));
        let w0 = read_word(amiga, addr);
        let w1 = read_word(amiga, addr.wrapping_add(2));
        if w0 & 1 != 0 {
            continue;
        }
        let reg = w0 & 0x01FE;
        match reg {
            0x092 => state.ddfstrt = Some(w1),
            0x094 => state.ddfstop = Some(w1),
            0x08E => state.diwstrt = Some(w1),
            0x090 => state.diwstop = Some(w1),
            0x100 => state.bplcon0 = Some(w1),
            0x108 => state.bpl1mod = Some(w1 as i16),
            0x10A => state.bpl2mod = Some(w1 as i16),
            0x0E0..=0x0F6 => {
                let plane = ((reg - 0x0E0) / 4) as usize;
                let is_lo = (reg & 0x0002) != 0;
                if plane < 6 {
                    if is_lo {
                        lo[plane] = Some(u32::from(w1 & 0xFFFE));
                    } else {
                        hi[plane] = Some(u32::from(w1) << 16);
                    }
                    if let (Some(hi), Some(lo)) = (hi[plane], lo[plane]) {
                        state.bpl_pt[plane] = Some((hi | lo) & 0x001F_FFFE);
                    }
                }
            }
            _ => {}
        }
    }
    state
}

fn last_dskpt_write(amiga: &AmigaOcs) -> Option<(u64, u32, u32)> {
    let mut hi = None;
    let mut lo = None;
    let mut last = None;
    for (cck, pc, offset, value) in &amiga.debug_dsk_log {
        match *offset {
            0x020 => {
                hi = Some(u32::from(*value) << 16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    last = Some((*cck, *pc, (hi | lo) & 0x001F_FFFE));
                }
            }
            0x022 => {
                lo = Some(u32::from(*value & 0xFFFE));
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    last = Some((*cck, *pc, (hi | lo) & 0x001F_FFFE));
                }
            }
            _ => {}
        }
    }
    last
}

fn count_sync_words(amiga: &AmigaOcs, base: u32, bytes: u32) -> usize {
    let mut count = 0usize;
    for off in 0..bytes.saturating_sub(1) {
        if read_byte(amiga, base.wrapping_add(off)) == 0x44
            && read_byte(amiga, base.wrapping_add(off.wrapping_add(1))) == 0x89
        {
            count += 1;
        }
    }
    count
}

fn hex_bytes(amiga: &AmigaOcs, base: u32, len: u32) -> Vec<u8> {
    (0..len)
        .map(|off| read_byte(amiga, base.wrapping_add(off)))
        .collect()
}

fn ranges_overlap(a_base: u32, a_len: u32, b_base: u32, b_len: u32) -> bool {
    let a_end = a_base.wrapping_add(a_len);
    let b_end = b_base.wrapping_add(b_len);
    a_base < b_end && b_base < a_end
}

fn blit_dest_range(c1: u16, dpt: u32, size: u16) -> (u32, u32) {
    let height = u32::from((size >> 6) & 0x03FF).max(1);
    let width_words = u32::from(size & 0x003F).max(1);
    let len_bytes = width_words * 2 * height;
    if (c1 & 0x0002) != 0 {
        (dpt.wrapping_sub(len_bytes), dpt)
    } else {
        (dpt, dpt.wrapping_add(len_bytes))
    }
}

fn find_copper_move_value_addr(amiga: &AmigaOcs, base: u32, reg: u16, limit: u32) -> Option<u32> {
    let base = base & 0x001F_FFFE;
    for i in 0..limit {
        let addr = base.wrapping_add(i.wrapping_mul(4));
        if read_word(amiga, addr) == reg {
            return Some(addr.wrapping_add(2));
        }
    }
    None
}

fn validator_watch_label(validator_addr: u32, addr: u32, is_word: bool) -> &'static str {
    let access_lo = addr;
    let access_hi = addr.wrapping_add(if is_word { 2 } else { 1 });
    let fields = [
        (validator_addr.wrapping_add(TASK_STATE), 1u32, "tc_State"),
        (
            validator_addr.wrapping_add(TASK_SIG_ALLOC),
            4u32,
            "tc_SigAlloc",
        ),
        (
            validator_addr.wrapping_add(TASK_SIG_WAIT),
            4u32,
            "tc_SigWait",
        ),
        (
            validator_addr.wrapping_add(TASK_SIG_RECVD),
            4u32,
            "tc_SigRecvd",
        ),
    ];
    for (field_lo, len, label) in fields {
        let field_hi = field_lo.wrapping_add(len);
        if access_lo < field_hi && access_hi > field_lo {
            return label;
        }
    }
    "other"
}

fn port_field_name(port_addr: u32, addr: u32, is_word: bool) -> &'static str {
    let access_lo = addr;
    let access_hi = addr.wrapping_add(if is_word { 2 } else { 1 });
    let fields = [
        (port_addr.wrapping_add(LN_SUCC), 4u32, "ln_Succ"),
        (port_addr.wrapping_add(LN_PRED), 4u32, "ln_Pred"),
        (port_addr.wrapping_add(LN_NAME), 4u32, "ln_Name"),
        (port_addr.wrapping_add(PORT_FLAGS), 1u32, "mp_Flags"),
        (port_addr.wrapping_add(PORT_SIGBIT), 1u32, "mp_SigBit"),
        (port_addr.wrapping_add(PORT_SIGTASK), 4u32, "mp_SigTask"),
        (port_addr.wrapping_add(PORT_MSG_LIST), 12u32, "mp_MsgList"),
    ];
    for (field_lo, len, label) in fields {
        let field_hi = field_lo.wrapping_add(len);
        if access_lo < field_hi && access_hi > field_lo {
            return label;
        }
    }
    "other"
}

fn watched_port_label(signal_port: u32, ignore_port: u32, addr: u32, is_word: bool) -> String {
    if (signal_port..signal_port.wrapping_add(0x24)).contains(&addr) {
        return format!("signal.{}", port_field_name(signal_port, addr, is_word));
    }
    if (ignore_port..ignore_port.wrapping_add(0x24)).contains(&addr) {
        return format!("ignore.{}", port_field_name(ignore_port, addr, is_word));
    }
    format!("other@${addr:08X}")
}

fn scan_longword_refs(amiga: &AmigaOcs, value: u32, limit: usize) -> Vec<u32> {
    let ranges = [
        (0x0000_0000u32, 0x0008_0000u32),
        (0x00C0_0000u32, 0x0008_0000u32),
    ];
    let mut out = Vec::new();
    for (base, len) in ranges {
        let mut addr = base;
        let end = base.wrapping_add(len);
        while addr < end {
            if read_long(amiga, addr) == value {
                out.push(addr);
                if out.len() >= limit {
                    return out;
                }
            }
            addr = addr.wrapping_add(2);
        }
    }
    out
}

fn fmt_long_window(amiga: &AmigaOcs, center: u32) -> String {
    let mut out = Vec::new();
    for delta in [-8i32, -4, 0, 4, 8, 12, 16] {
        let addr = if delta < 0 {
            center.wrapping_sub((-delta) as u32)
        } else {
            center.wrapping_add(delta as u32)
        };
        out.push(format!("[${addr:08X}]=${:08X}", read_long(amiga, addr)));
    }
    out.join(" ")
}

fn owner_field_name(owner_root: u32, addr: u32, is_word: bool) -> &'static str {
    let access_lo = addr;
    let access_hi = addr.wrapping_add(if is_word { 2 } else { 1 });
    let fields = [
        (
            owner_root.wrapping_add(VALIDATOR_OWNER_CTRL),
            4u32,
            "owner+$52",
        ),
        (
            owner_root.wrapping_add(VALIDATOR_OWNER_SIGNAL_PORT),
            4u32,
            "owner+$56",
        ),
        (
            owner_root.wrapping_add(VALIDATOR_OWNER_IGNORE_PORT),
            4u32,
            "owner+$5A",
        ),
        (
            owner_root.wrapping_add(VALIDATOR_OWNER_BYTE62),
            1u32,
            "owner+$62",
        ),
        (
            owner_root.wrapping_add(VALIDATOR_OWNER_BYTE63),
            1u32,
            "owner+$63",
        ),
        (
            owner_root.wrapping_add(VALIDATOR_OWNER_LONG64),
            4u32,
            "owner+$64",
        ),
    ];
    for (field_lo, len, label) in fields {
        let field_hi = field_lo.wrapping_add(len);
        if access_lo < field_hi && access_hi > field_lo {
            return label;
        }
    }
    "owner+other"
}

fn infer_validator_idcmp_owner_root(
    amiga: &AmigaOcs,
    signal_port: u32,
    ignore_port: u32,
) -> Option<u32> {
    for signal_ref in scan_longword_refs(amiga, signal_port, 64) {
        if signal_ref < VALIDATOR_OWNER_SIGNAL_PORT {
            continue;
        }
        let candidate = signal_ref.wrapping_sub(VALIDATOR_OWNER_SIGNAL_PORT);
        if read_long(amiga, candidate.wrapping_add(VALIDATOR_OWNER_SIGNAL_PORT)) == signal_port
            && read_long(amiga, candidate.wrapping_add(VALIDATOR_OWNER_IGNORE_PORT)) == ignore_port
        {
            return Some(candidate);
        }
    }
    for ignore_ref in scan_longword_refs(amiga, ignore_port, 64) {
        if ignore_ref < VALIDATOR_OWNER_IGNORE_PORT {
            continue;
        }
        let candidate = ignore_ref.wrapping_sub(VALIDATOR_OWNER_IGNORE_PORT);
        if read_long(amiga, candidate.wrapping_add(VALIDATOR_OWNER_SIGNAL_PORT)) == signal_port
            && read_long(amiga, candidate.wrapping_add(VALIDATOR_OWNER_IGNORE_PORT)) == ignore_port
        {
            return Some(candidate);
        }
    }
    None
}

fn fmt_owner_snapshot(amiga: &AmigaOcs, owner_root: u32) -> String {
    let ctrl = read_long(amiga, owner_root.wrapping_add(VALIDATOR_OWNER_CTRL));
    let signal_port = read_long(amiga, owner_root.wrapping_add(VALIDATOR_OWNER_SIGNAL_PORT));
    let ignore_port = read_long(amiga, owner_root.wrapping_add(VALIDATOR_OWNER_IGNORE_PORT));
    let byte62 = read_byte(amiga, owner_root.wrapping_add(VALIDATOR_OWNER_BYTE62));
    let byte63 = read_byte(amiga, owner_root.wrapping_add(VALIDATOR_OWNER_BYTE63));
    let long64 = read_long(amiga, owner_root.wrapping_add(VALIDATOR_OWNER_LONG64));
    format!(
        "owner=${owner_root:08X} +52=${ctrl:08X} +56=${signal_port:08X} +5A=${ignore_port:08X} +62=${byte62:02X} +63=${byte63:02X} +64=${long64:08X}"
    )
}

fn push_recent(recent: &mut Vec<String>, event: String, cap: usize) {
    if recent.len() >= cap {
        recent.remove(0);
    }
    recent.push(event);
}

fn raw_words_live(amiga: &AmigaOcs, addr: u32, len: u8) -> String {
    let mut out = String::new();
    let words = usize::from(len.max(2)) / 2;
    for i in 0..words {
        if i != 0 {
            out.push(' ');
        }
        let base = addr.wrapping_add((i as u32) * 2);
        let hi = read_byte(amiga, base);
        let lo = read_byte(amiga, base.wrapping_add(1));
        out.push_str(&format!("{hi:02X}{lo:02X}"));
    }
    out
}

fn disassemble_live_region(amiga: &AmigaOcs, start: u32, end: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut pc = start & !1;
    while pc < end {
        let (mnemonic, len) = disassemble(pc, |addr| read_byte(amiga, addr));
        let len = len.max(2);
        out.push(format!(
            "${pc:08X}: {:<19} {mnemonic}",
            raw_words_live(amiga, pc, len),
        ));
        pc = pc.wrapping_add(u32::from(len));
    }
    out
}

fn disassemble_live_line(amiga: &AmigaOcs, addr: u32) -> String {
    let (mnemonic, len) = disassemble(addr, |at| read_byte(amiga, at));
    let len = len.max(2);
    format!(
        "${addr:08X}: {:<19} {mnemonic}",
        raw_words_live(amiga, addr, len),
    )
}

fn likely_rom_string(amiga: &AmigaOcs, addr: u32) -> Option<String> {
    if !(0x00FC_0000..0x0100_0000).contains(&addr) {
        return None;
    }
    let s = read_cstring(amiga, addr, 96);
    if s.len() < 4 {
        return None;
    }
    let printable = s.bytes().all(|b| b.is_ascii_graphic() || b == b' ');
    if !printable {
        return None;
    }
    Some(s)
}

fn scan_string_pointers(amiga: &AmigaOcs, base: u32, len: u32) -> Vec<String> {
    let mut out = Vec::new();
    let end = base.wrapping_add(len);
    let mut addr = base;
    while addr.wrapping_add(4) <= end {
        let ptr = read_long(amiga, addr);
        if let Some(s) = likely_rom_string(amiga, ptr) {
            out.push(format!("[${addr:08X}] -> ${ptr:08X} \"{s}\""));
        }
        addr = addr.wrapping_add(2);
    }
    out
}

fn dump_long_block(amiga: &AmigaOcs, base: u32, longs: usize) -> Vec<String> {
    let mut out = Vec::new();
    for row in 0..longs.div_ceil(4) {
        let addr = base.wrapping_add((row as u32) * 16);
        let mut line = format!("${addr:08X}:");
        for col in 0..4 {
            let long_index = row * 4 + col;
            if long_index >= longs {
                break;
            }
            let long_addr = addr.wrapping_add((col as u32) * 4);
            line.push_str(&format!(" ${:08X}", read_long(amiga, long_addr)));
        }
        out.push(line);
    }
    out
}

fn dump_word_block(amiga: &AmigaOcs, base: u32, words: usize) -> Vec<String> {
    let mut out = Vec::new();
    for row in 0..words.div_ceil(8) {
        let addr = base.wrapping_add((row as u32) * 16);
        let mut line = format!("${addr:08X}:");
        for col in 0..8 {
            let word_index = row * 8 + col;
            if word_index >= words {
                break;
            }
            let word_addr = addr.wrapping_add((col as u32) * 2);
            line.push_str(&format!(" ${:04X}", read_word(amiga, word_addr)));
        }
        out.push(line);
    }
    out
}

fn find_word_matches(amiga: &AmigaOcs, base: u32, len_bytes: u32, needle: u16) -> Vec<u32> {
    let mut out = Vec::new();
    let mut off = 0u32;
    while off + 1 < len_bytes {
        let addr = base.wrapping_add(off);
        if read_word(amiga, addr) == needle {
            out.push(addr);
        }
        off = off.wrapping_add(2);
    }
    out
}

fn watch_ref_holder_window(
    rom: &[u8],
    adf_bytes: &[u8],
    watch_lo: u32,
    watch_len: u32,
) -> Vec<String> {
    let mut amiga = AmigaOcs::with_slow_ram(rom.to_vec(), 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes.to_vec()).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    const WINDOW_START: u64 = 330;
    const WINDOW_END: u64 = 390;

    let mut events = Vec::<String>::new();
    let mut last_watch_len = 0usize;
    let mut watch_armed = false;

    for frame in 0..410u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        if !watch_armed && frame_num == WINDOW_START {
            amiga.debug_watch_addr = Some((watch_lo, watch_len));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
            watch_armed = true;
            events.push(format!(
                "frame={frame_num} cck={} armed ref-holder watch addr=${watch_lo:08X} len=${watch_len:04X}",
                amiga.cck_count(),
            ));
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            if !in_window {
                continue;
            }

            while last_watch_len < amiga.debug_watch_writes.len() {
                let (cck, writer_pc, addr, val, is_word) = amiga.debug_watch_writes[last_watch_len];
                events.push(format!(
                    "frame={frame_num} cck={cck} addr=${addr:08X} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={} {}",
                    task_name(&amiga, current_task_addr(&amiga)),
                    fmt_long_window(&amiga, addr & !1),
                ));
                last_watch_len += 1;
            }
        }

        if watch_armed && frame_num == WINDOW_END {
            amiga.debug_watch_addr = None;
        }
    }

    events
}

fn list_len(amiga: &AmigaOcs, list_addr: u32, limit: usize) -> usize {
    let mut node = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut count = 0usize;
    while node != 0 && node != tail_sentinel && count < limit {
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
        count += 1;
    }
    count
}

fn validator_port_snapshot(amiga: &AmigaOcs, validator_addr: u32) -> Vec<String> {
    let exec_base = read_long(amiga, 0x0000_0004);
    if exec_base == 0 || validator_addr == 0 {
        return Vec::new();
    }

    let list_addr = exec_base.wrapping_add(EXEC_PORT_LIST);
    let mut node = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut out = Vec::new();
    let mut guard = 0usize;
    while node != 0 && node != tail_sentinel && guard < 64 {
        let sigtask = read_long(amiga, node.wrapping_add(PORT_SIGTASK));
        if sigtask == validator_addr {
            let flags = read_byte(amiga, node.wrapping_add(PORT_FLAGS));
            let sigbit = read_byte(amiga, node.wrapping_add(PORT_SIGBIT));
            let name = read_cstring(amiga, read_long(amiga, node.wrapping_add(LN_NAME)), 32);
            let msg_count = list_len(amiga, node.wrapping_add(PORT_MSG_LIST), 16);
            out.push(format!(
                "port=${node:08X} name={name} flags=${flags:02X} sigBit={sigbit} mask=${:08X} msgCount={msg_count}",
                1u32 << sigbit
            ));
        }
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
        guard += 1;
    }
    out
}

fn validator_port_addrs(amiga: &AmigaOcs, validator_addr: u32) -> Vec<u32> {
    let exec_base = read_long(amiga, 0x0000_0004);
    if exec_base == 0 || validator_addr == 0 {
        return Vec::new();
    }

    let list_addr = exec_base.wrapping_add(EXEC_PORT_LIST);
    let mut node = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut out = Vec::new();
    let mut guard = 0usize;
    while node != 0 && node != tail_sentinel && guard < 64 {
        if read_long(amiga, node.wrapping_add(PORT_SIGTASK)) == validator_addr {
            out.push(node);
        }
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
        guard += 1;
    }
    out
}

fn write_long(amiga: &mut AmigaOcs, addr: u32, val: u32) {
    amiga.poke_word(addr, (val >> 16) as u16);
    amiga.poke_word(addr.wrapping_add(2), val as u16);
}

fn write_byte(amiga: &mut AmigaOcs, addr: u32, val: u8) {
    amiga.poke_byte(addr, val);
}

fn zero_region(amiga: &mut AmigaOcs, addr: u32, len: u32) {
    for off in (0..len).step_by(2) {
        amiga.poke_word(addr.wrapping_add(off), 0);
    }
}

fn list_remove_node(amiga: &mut AmigaOcs, node: u32) {
    let succ = read_long(amiga, node.wrapping_add(LN_SUCC));
    let pred = read_long(amiga, node.wrapping_add(LN_PRED));
    if succ == 0 || pred == 0 {
        return;
    }
    write_long(amiga, pred.wrapping_add(LN_SUCC), succ);
    write_long(amiga, succ.wrapping_add(LN_PRED), pred);
    write_long(amiga, node.wrapping_add(LN_SUCC), 0);
    write_long(amiga, node.wrapping_add(LN_PRED), 0);
}

fn list_add_head(amiga: &mut AmigaOcs, list_addr: u32, node: u32) {
    let first = read_long(amiga, list_addr);
    write_long(amiga, node.wrapping_add(LN_PRED), list_addr);
    write_long(amiga, node.wrapping_add(LN_SUCC), first);
    write_long(amiga, list_addr, node);
    write_long(amiga, first.wrapping_add(LN_PRED), node);
}

fn list_add_tail(amiga: &mut AmigaOcs, list_addr: u32, node: u32) {
    let tail_sentinel = list_addr.wrapping_add(4);
    let prev = read_long(amiga, list_addr.wrapping_add(8));
    write_long(amiga, node.wrapping_add(LN_SUCC), tail_sentinel);
    write_long(amiga, node.wrapping_add(LN_PRED), prev);
    write_long(amiga, prev.wrapping_add(LN_SUCC), node);
    write_long(amiga, tail_sentinel.wrapping_add(LN_PRED), node);
}

fn force_signal_delivery(amiga: &mut AmigaOcs, exec_base: u32, task: u32, mask: u32) {
    let sig_recvd = read_long(amiga, task.wrapping_add(TASK_SIG_RECVD));
    write_long(amiga, task.wrapping_add(TASK_SIG_RECVD), sig_recvd | mask);

    let state = read_byte(amiga, task.wrapping_add(TASK_STATE));
    let sig_wait = read_long(amiga, task.wrapping_add(TASK_SIG_WAIT));
    if state == 4 && (sig_wait & mask) != 0 {
        list_remove_node(amiga, task);
        write_byte(amiga, task.wrapping_add(TASK_STATE), 3);
        list_add_head(amiga, exec_base.wrapping_add(EXEC_TASK_READY), task);
    }
}

fn queue_message(amiga: &mut AmigaOcs, port: u32, msg_addr: u32, reply_port: u32, length: u16) {
    zero_region(amiga, msg_addr, 0x40);
    write_long(amiga, msg_addr.wrapping_add(MN_REPLYPORT), reply_port);
    amiga.poke_word(msg_addr.wrapping_add(MN_LENGTH), length);
    list_add_tail(amiga, port.wrapping_add(PORT_MSG_LIST), msg_addr);
}

#[derive(Clone, Copy)]
struct ValidatorWaitContext {
    frame: u64,
    exec_base: u32,
    validator_addr: u32,
    signal_port: u32,
    ignore_port: u32,
}

fn find_validator_wait_context(amiga: &mut AmigaOcs) -> Option<ValidatorWaitContext> {
    let mut exec_base = 0u32;
    let mut validator_addr = 0u32;
    for frame in 0..500u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
        }
        if exec_base == 0 {
            exec_base = read_long(amiga, 0x0000_0004);
        }
        if validator_addr == 0
            && let Some(found) = find_named_task(amiga, "Validator")
        {
            validator_addr = found;
        }
        if exec_base == 0 || validator_addr == 0 {
            continue;
        }

        let sig_wait = read_long(amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
        if sig_wait != 0x8000_0000 {
            continue;
        }

        let mut signal_port = 0u32;
        let mut ignore_port = 0u32;
        for port in validator_port_addrs(amiga, validator_addr) {
            match read_byte(amiga, port.wrapping_add(PORT_FLAGS)) {
                0 => signal_port = port,
                2 => ignore_port = port,
                _ => {}
            }
        }
        if signal_port != 0 && ignore_port != 0 {
            return Some(ValidatorWaitContext {
                frame: frame_num,
                exec_base,
                validator_addr,
                signal_port,
                ignore_port,
            });
        }
    }
    None
}

fn dump_task_list(amiga: &AmigaOcs, label: &str, list_addr: u32) {
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    eprintln!(
        "\n=== {label} @ ${list_addr:08X} ===\nhead=${head:08X} tail_sentinel=${tail_sentinel:08X}"
    );

    let mut node = head;
    let mut index = 0usize;
    while node != 0 && node != tail_sentinel && index < 24 {
        let name = task_name(amiga, node);
        let state = read_byte(amiga, node.wrapping_add(TASK_STATE));
        let sig_alloc = read_long(amiga, node.wrapping_add(TASK_SIG_ALLOC));
        let sig_wait = read_long(amiga, node.wrapping_add(TASK_SIG_WAIT));
        let sig_recvd = read_long(amiga, node.wrapping_add(TASK_SIG_RECVD));
        let sp = read_long(amiga, node.wrapping_add(TASK_SP_REG));
        let candidates = scan_stack_candidates(amiga, sp);
        eprintln!(
            "[{index:2}] task=${node:08X} name={name} state={}({state}) \
             sigAlloc=${sig_alloc:08X} sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X} \
             tc_SPReg=${sp:08X}",
            state_name(state),
        );
        eprintln!(
            "     resume candidates: {}",
            fmt_stack_candidates(&candidates)
        );
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
        index += 1;
    }

    if index == 0 {
        eprintln!("(empty)");
    }
    if index == 24 && node != 0 && node != tail_sentinel {
        eprintln!("(truncated after 24 entries)");
    }
}

fn find_named_task(amiga: &AmigaOcs, name: &str) -> Option<u32> {
    let exec_base = read_long(amiga, 0x0000_0004);
    if exec_base == 0 {
        return None;
    }

    let this_task = current_task_addr(amiga);
    if this_task != 0 && task_name(amiga, this_task) == name {
        return Some(this_task);
    }

    for list_addr in [
        exec_base.wrapping_add(EXEC_TASK_READY),
        exec_base.wrapping_add(EXEC_TASK_WAIT),
    ] {
        let head = read_long(amiga, list_addr);
        let tail_sentinel = list_addr.wrapping_add(4);
        let mut node = head;
        let mut guard = 0usize;
        while node != 0 && node != tail_sentinel && guard < 32 {
            if task_name(amiga, node) == name {
                return Some(node);
            }
            node = read_long(amiga, node.wrapping_add(LN_SUCC));
            guard += 1;
        }
    }

    None
}

#[test]
#[ignore]
fn trace_wb13_late_boot_tasks_and_signals() {
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

    for _ in 0..(800u64 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let exec_base = read_long(&amiga, 0x0000_0004);
    if exec_base == 0 {
        eprintln!("ExecBase is still zero after 800 frames");
        return;
    }

    let lvo_targets = [
        ("Wait", resolve_lvo(&amiga, exec_base, LVO_WAIT)),
        ("Signal", resolve_lvo(&amiga, exec_base, LVO_SIGNAL)),
        ("Cause", resolve_lvo(&amiga, exec_base, LVO_CAUSE)),
        ("SetSignal", resolve_lvo(&amiga, exec_base, LVO_SET_SIGNAL)),
        ("PutMsg", resolve_lvo(&amiga, exec_base, LVO_PUT_MSG)),
        ("GetMsg", resolve_lvo(&amiga, exec_base, LVO_GET_MSG)),
        ("ReplyMsg", resolve_lvo(&amiga, exec_base, LVO_REPLY_MSG)),
        ("WaitPort", resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT)),
        ("DoIO", resolve_lvo(&amiga, exec_base, LVO_DO_IO)),
        ("SendIO", resolve_lvo(&amiga, exec_base, LVO_SEND_IO)),
    ];

    eprintln!("=== Late-window LVO entry points ===");
    for (name, ep) in &lvo_targets {
        match ep {
            Some(ep) => eprintln!("  {name:<9} ${ep:08X}"),
            None => eprintln!("  {name:<9} (not resolved)"),
        }
    }

    let disp_before = read_long(&amiga, exec_base.wrapping_add(EXEC_DISP_COUNT));
    let idle_before = read_long(&amiga, exec_base.wrapping_add(EXEC_IDLE_COUNT));

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut lvo_counts = BTreeMap::<&'static str, u64>::new();
    let mut wait_counts = BTreeMap::<(String, u32), u64>::new();
    let mut signal_counts = BTreeMap::<(String, String, u32), u64>::new();
    let mut io_counts = BTreeMap::<(&'static str, String, String, u16), u64>::new();
    let mut port_counts = BTreeMap::<(&'static str, String, u32, String), u64>::new();
    let mut pc_hist = BTreeMap::<u32, u64>::new();
    let mut task_hist = BTreeMap::<String, u64>::new();
    let mut events = Vec::<String>::new();

    for _frame in 0..100u64 {
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            for (name, ep) in &lvo_targets {
                let Some(ep) = ep else { continue };
                if pc != *ep {
                    continue;
                }

                *lvo_counts.entry(name).or_insert(0) += 1;

                let this_task = current_task_addr(&amiga);
                let source = task_name(&amiga, this_task);
                let detail = match *name {
                    "Wait" => {
                        let mask = amiga.cpu().regs.d[0];
                        *wait_counts.entry((source.clone(), mask)).or_insert(0) += 1;
                        format!("src={source} mask=${mask:08X}")
                    }
                    "Signal" => {
                        let target_addr = amiga.cpu().regs.a[1];
                        let target = task_name(&amiga, target_addr);
                        let mask = amiga.cpu().regs.d[0];
                        *signal_counts
                            .entry((source.clone(), target.clone(), mask))
                            .or_insert(0) += 1;
                        format!("src={source} target={target} mask=${mask:08X}")
                    }
                    "Cause" => {
                        let interrupt = amiga.cpu().regs.a[1];
                        format!("src={source} interrupt=${interrupt:08X}")
                    }
                    "SetSignal" => {
                        format!(
                            "src={source} new=${:08X} mask=${:08X}",
                            amiga.cpu().regs.d[0],
                            amiga.cpu().regs.d[1]
                        )
                    }
                    "PutMsg" => {
                        let port = amiga.cpu().regs.a[0];
                        let message = amiga.cpu().regs.a[1];
                        let port_name_ptr = read_long(&amiga, port.wrapping_add(LN_NAME));
                        let port_name = read_cstring(&amiga, port_name_ptr, 32);
                        *port_counts
                            .entry(("PutMsg", source.clone(), port, port_name.clone()))
                            .or_insert(0) += 1;
                        format!("src={source} port=${port:08X}({port_name}) msg=${message:08X}")
                    }
                    "GetMsg" | "WaitPort" => {
                        let port = amiga.cpu().regs.a[0];
                        let port_name_ptr = read_long(&amiga, port.wrapping_add(LN_NAME));
                        let port_name = read_cstring(&amiga, port_name_ptr, 32);
                        *port_counts
                            .entry((name, source.clone(), port, port_name.clone()))
                            .or_insert(0) += 1;
                        format!("src={source} port=${port:08X}({port_name})")
                    }
                    "ReplyMsg" => {
                        let message = amiga.cpu().regs.a[1];
                        format!("src={source} msg=${message:08X}")
                    }
                    "DoIO" | "SendIO" => {
                        let io = amiga.cpu().regs.a[1];
                        let device = read_long(&amiga, io.wrapping_add(IO_DEVICE));
                        let dev_name = if device == 0 {
                            "<null>".into()
                        } else {
                            let name_ptr = read_long(&amiga, device.wrapping_add(LN_NAME));
                            read_cstring(&amiga, name_ptr, 32)
                        };
                        let command = read_word(&amiga, io.wrapping_add(IO_COMMAND));
                        *io_counts
                            .entry((name, source.clone(), dev_name.clone(), command))
                            .or_insert(0) += 1;
                        format!("src={source} {}", describe_iorequest(&amiga, io))
                    }
                    _ => format!("src={source}"),
                };

                if events.len() < 120 {
                    events.push(format!(
                        "cck={} pc=${pc:08X} {name:<9} {detail}",
                        amiga.cck_count()
                    ));
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        let pc = amiga.cpu().regs.pc;
        *pc_hist.entry(pc).or_insert(0) += 1;
        *task_hist
            .entry(task_name(&amiga, current_task_addr(&amiga)))
            .or_insert(0) += 1;
    }

    let disp_after = read_long(&amiga, exec_base.wrapping_add(EXEC_DISP_COUNT));
    let idle_after = read_long(&amiga, exec_base.wrapping_add(EXEC_IDLE_COUNT));
    let this_task = current_task_addr(&amiga);
    let active_sp = active_sp(&amiga);
    let this_task_saved_sp = read_long(&amiga, this_task.wrapping_add(TASK_SP_REG));

    eprintln!("\n=== Late-window Exec deltas (frames 801-900) ===");
    eprintln!(
        "DispCount ${disp_before:08X} -> ${disp_after:08X}  delta={}",
        disp_after.wrapping_sub(disp_before)
    );
    eprintln!(
        "IdleCount ${idle_before:08X} -> ${idle_after:08X}  delta={}",
        idle_after.wrapping_sub(idle_before)
    );

    let mut pc_hist_vec: Vec<_> = pc_hist.into_iter().collect();
    pc_hist_vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    eprintln!("\n=== PC histogram (last 100 frames) ===");
    for (pc, count) in pc_hist_vec.into_iter().take(12) {
        eprintln!("  {count:>3} × ${pc:08X}");
    }

    let mut task_hist_vec: Vec<_> = task_hist.into_iter().collect();
    task_hist_vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    eprintln!("\n=== Running-task histogram (last 100 frames) ===");
    for (name, count) in task_hist_vec {
        eprintln!("  {count:>3} × {name}");
    }

    eprintln!("\n=== LVO counts (last 100 frames) ===");
    for (name, _) in &lvo_targets {
        eprintln!("  {name:<9} {}", lvo_counts.get(name).copied().unwrap_or(0));
    }

    if !wait_counts.is_empty() {
        eprintln!("\n=== Wait traffic ===");
        for ((source, mask), count) in wait_counts {
            eprintln!("  {count:>4} × {source:<20} mask=${mask:08X}");
        }
    }

    if !signal_counts.is_empty() {
        eprintln!("\n=== Signal traffic ===");
        for ((source, target, mask), count) in signal_counts {
            eprintln!("  {count:>4} × {source:<20} -> {target:<20} mask=${mask:08X}");
        }
    }

    if !io_counts.is_empty() {
        eprintln!("\n=== I/O traffic ===");
        for ((op, source, dev_name, command), count) in io_counts {
            eprintln!("  {count:>4} × {op:<6} {source:<20} -> {dev_name:<16} cmd=${command:04X}");
        }
    }

    if !port_counts.is_empty() {
        eprintln!("\n=== Port traffic ===");
        for ((op, source, port, port_name), count) in port_counts {
            eprintln!("  {count:>4} × {op:<8} {source:<20} port=${port:08X}({port_name})");
        }
    }

    if !events.is_empty() {
        eprintln!("\n=== First {} late-window events ===", events.len());
        for event in &events {
            eprintln!("  {event}");
        }
    }

    let this_name = task_name(&amiga, this_task);
    let this_state = read_byte(&amiga, this_task.wrapping_add(TASK_STATE));
    let this_sig_alloc = read_long(&amiga, this_task.wrapping_add(TASK_SIG_ALLOC));
    let this_sig_wait = read_long(&amiga, this_task.wrapping_add(TASK_SIG_WAIT));
    let this_sig_recvd = read_long(&amiga, this_task.wrapping_add(TASK_SIG_RECVD));
    eprintln!("\n=== ThisTask ===");
    eprintln!(
        "task=${this_task:08X} name={this_name} state={}({this_state}) \
         sigAlloc=${this_sig_alloc:08X} sigWait=${this_sig_wait:08X} \
         sigRecvd=${this_sig_recvd:08X}",
        state_name(this_state),
    );
    eprintln!(
        "PC=${:08X} SR=${:04X} USP=${:08X} SSP=${:08X} activeSP=${active_sp:08X} tc_SPReg=${this_task_saved_sp:08X}",
        amiga.cpu().regs.pc,
        amiga.cpu().regs.sr,
        amiga.cpu().regs.usp,
        amiga.cpu().regs.ssp,
    );
    eprintln!(
        "active stack candidates: {}",
        fmt_stack_candidates(&scan_stack_candidates(&amiga, active_sp))
    );

    dump_task_list(&amiga, "TaskReady", exec_base.wrapping_add(EXEC_TASK_READY));
    dump_task_list(&amiga, "TaskWait", exec_base.wrapping_add(EXEC_TASK_WAIT));
}

#[test]
#[ignore]
fn trace_wb13_validator_lifecycle() {
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

    let mut exec_base = 0u32;
    let mut lvo_wait = None;
    let mut lvo_signal = None;
    let mut lvo_doio = None;
    let mut lvo_sendio = None;
    let mut validator_addr = 0u32;
    let mut validator_first_seen = None;
    let mut validator_last_running = None;
    let mut validator_running_frames = 0u64;
    let mut validator_pc_hist = BTreeMap::<u32, u64>::new();
    let mut validator_events = Vec::<String>::new();

    let mut prev_state = None;
    let mut prev_sig_wait = None;
    let mut prev_sig_recvd = None;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;

    for frame in 0..700u64 {
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();

            if exec_base == 0 {
                exec_base = read_long(&amiga, 0x0000_0004);
                if exec_base != 0 {
                    lvo_wait = resolve_lvo(&amiga, exec_base, LVO_WAIT);
                    lvo_signal = resolve_lvo(&amiga, exec_base, LVO_SIGNAL);
                    lvo_doio = resolve_lvo(&amiga, exec_base, LVO_DO_IO);
                    lvo_sendio = resolve_lvo(&amiga, exec_base, LVO_SEND_IO);
                }
            }

            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let this_task = current_task_addr(&amiga);
            let source = task_name(&amiga, this_task);

            if validator_addr != 0 {
                if let Some(ep) = lvo_wait
                    && pc == ep
                    && this_task == validator_addr
                    && validator_events.len() < 160
                {
                    validator_events.push(format!(
                        "frame={} cck={} Wait src=Validator mask=${:08X}",
                        frame + 1,
                        amiga.cck_count(),
                        amiga.cpu().regs.d[0],
                    ));
                }

                if let Some(ep) = lvo_signal
                    && pc == ep
                    && validator_events.len() < 160
                {
                    let target_addr = amiga.cpu().regs.a[1];
                    if this_task == validator_addr || target_addr == validator_addr {
                        validator_events.push(format!(
                            "frame={} cck={} Signal src={} target={} mask=${:08X}",
                            frame + 1,
                            amiga.cck_count(),
                            source,
                            task_name(&amiga, target_addr),
                            amiga.cpu().regs.d[0],
                        ));
                    }
                }

                if let Some(ep) = lvo_doio
                    && pc == ep
                    && this_task == validator_addr
                    && validator_events.len() < 160
                {
                    validator_events.push(format!(
                        "frame={} cck={} DoIO {}",
                        frame + 1,
                        amiga.cck_count(),
                        describe_iorequest(&amiga, amiga.cpu().regs.a[1]),
                    ));
                }

                if let Some(ep) = lvo_sendio
                    && pc == ep
                    && this_task == validator_addr
                    && validator_events.len() < 160
                {
                    validator_events.push(format!(
                        "frame={} cck={} SendIO {}",
                        frame + 1,
                        amiga.cck_count(),
                        describe_iorequest(&amiga, amiga.cpu().regs.a[1]),
                    ));
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
            validator_first_seen = Some(frame + 1);
            validator_events.push(format!(
                "frame={} cck={} Validator first seen at ${validator_addr:08X}",
                frame + 1,
                amiga.cck_count(),
            ));
        }

        if validator_addr == 0 {
            continue;
        }

        let this_task = current_task_addr(&amiga);
        if this_task == validator_addr {
            validator_running_frames += 1;
            validator_last_running = Some(frame + 1);
            *validator_pc_hist.entry(amiga.cpu().regs.pc).or_insert(0) += 1;
        }

        let state = read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE));
        let sig_wait = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
        let sig_recvd = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD));

        if prev_state != Some(state)
            || prev_sig_wait != Some(sig_wait)
            || prev_sig_recvd != Some(sig_recvd)
        {
            validator_events.push(format!(
                "frame={} cck={} Validator state={}({state}) sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X} pc=${:08X}",
                frame + 1,
                amiga.cck_count(),
                state_name(state),
                amiga.cpu().regs.pc,
            ));
            prev_state = Some(state);
            prev_sig_wait = Some(sig_wait);
            prev_sig_recvd = Some(sig_recvd);
        }
    }

    eprintln!("=== Validator lifecycle ===");
    eprintln!(
        "first_seen_frame={:?} running_frames={} last_running_frame={:?}",
        validator_first_seen, validator_running_frames, validator_last_running,
    );

    if validator_addr != 0 {
        eprintln!("validator task @ ${validator_addr:08X}");
        eprintln!(
            "final state={}({}) sigWait=${:08X} sigRecvd=${:08X} tc_SPReg=${:08X}",
            state_name(read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE))),
            read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE)),
            read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT)),
            read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD)),
            read_long(&amiga, validator_addr.wrapping_add(TASK_SP_REG)),
        );
        eprintln!(
            "resume candidates: {}",
            fmt_stack_candidates(&scan_stack_candidates(
                &amiga,
                read_long(&amiga, validator_addr.wrapping_add(TASK_SP_REG)),
            ))
        );
    } else {
        eprintln!("Validator task was never found");
    }

    let mut hist_vec: Vec<_> = validator_pc_hist.into_iter().collect();
    hist_vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if !hist_vec.is_empty() {
        eprintln!("\n=== Validator running PC histogram ===");
        for (pc, count) in hist_vec.into_iter().take(16) {
            eprintln!("  {count:>3} × ${pc:08X}");
        }
    }

    if !validator_events.is_empty() {
        eprintln!("\n=== Validator events ===");
        for event in &validator_events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_transition_window() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discover = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discover_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discover.insert_adf(discover_adf);
    for _ in 0..(220u64 * PAL_FRAME_TICKS) {
        discover.tick();
    }

    let discover_exec_base = read_long(&discover, 0x0000_0004);
    let discover_timer_base = if discover_exec_base != 0 {
        find_device(&discover, discover_exec_base, "timer.device").unwrap_or(0)
    } else {
        0
    };
    let discover_timer_beginio = if discover_timer_base != 0 {
        resolve_lvo(&discover, discover_timer_base, LVO_BEGIN_IO)
    } else {
        None
    };
    let (discover_timer_vbl_handler, discover_timer_vbl_data) = if discover_timer_base != 0 {
        let vbl_int = discover_timer_base.wrapping_add(TIMER_VBL_INT_OFFSET);
        (
            read_long(&discover, vbl_int.wrapping_add(INTERRUPT_IS_CODE)),
            read_long(&discover, vbl_int.wrapping_add(INTERRUPT_IS_DATA)),
        )
    } else {
        (0, 0)
    };
    let (discover_timer_cia_handler, discover_timer_cia_data) = if discover_timer_base != 0 {
        let cia_int = discover_timer_base.wrapping_add(TIMER_CIA_INT_OFFSET);
        (
            read_long(&discover, cia_int.wrapping_add(INTERRUPT_IS_CODE)),
            read_long(&discover, cia_int.wrapping_add(INTERRUPT_IS_DATA)),
        )
    } else {
        (0, 0)
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    const WINDOW_START: u64 = 340;
    const WINDOW_END: u64 = 390;

    let mut exec_base = 0u32;
    let mut lvo_wait = None;
    let mut lvo_signal = None;
    let mut lvo_doio = None;
    let mut lvo_sendio = None;

    let timer_base = discover_timer_base;
    let timer_beginio = discover_timer_beginio;
    let timer_vbl_handler = discover_timer_vbl_handler;
    let timer_vbl_data = discover_timer_vbl_data;
    let timer_cia_handler = discover_timer_cia_handler;
    let timer_cia_data = discover_timer_cia_data;

    let mut validator_addr = 0u32;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;

    let mut events = Vec::<String>::new();
    let mut signal_to_validator = BTreeMap::<(String, u32), u64>::new();
    let mut timer_beginio_counts = BTreeMap::<(String, u16, String), u64>::new();
    let mut validator_wait_masks = BTreeMap::<u32, u64>::new();
    let mut vbl_hits = 0u64;
    let mut cia_hits = 0u64;
    let mut validator_pc_events = Vec::<String>::new();
    let mut prev_validator_state = None;
    let mut prev_validator_sig_wait = None;
    let mut prev_validator_sig_recvd = None;

    for frame in 0..420u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            if !in_window {
                continue;
            }

            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let this_task = current_task_addr(&amiga);
            let source = task_name(&amiga, this_task);

            if validator_addr != 0 && this_task == validator_addr && validator_pc_events.len() < 160
            {
                validator_pc_events.push(format!(
                    "frame={frame_num} cck={} pc=${pc:08X} instr=${instr:08X} ir=${:04X}",
                    amiga.cck_count(),
                    amiga.cpu().ir,
                ));
            }

            if let Some(ep) = lvo_wait
                && pc == ep
                && this_task == validator_addr
            {
                let mask = amiga.cpu().regs.d[0];
                *validator_wait_masks.entry(mask).or_insert(0) += 1;
                if events.len() < 200 {
                    events.push(format!(
                        "frame={frame_num} cck={} Validator Wait mask=${mask:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_signal
                && pc == ep
            {
                let target_addr = amiga.cpu().regs.a[1];
                let target = task_name(&amiga, target_addr);
                let mask = amiga.cpu().regs.d[0];
                if target_addr == validator_addr {
                    *signal_to_validator
                        .entry((source.clone(), mask))
                        .or_insert(0) += 1;
                }
                if (this_task == validator_addr
                    || target_addr == validator_addr
                    || mask == 0x8000_0000)
                    && events.len() < 200
                {
                    events.push(format!(
                        "frame={frame_num} cck={} Signal src={source} target={target} mask=${mask:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_doio
                && pc == ep
                && this_task != 0
            {
                let io = amiga.cpu().regs.a[1];
                let desc = describe_iorequest(&amiga, io);
                if (this_task == validator_addr || desc.contains("dev=timer.device"))
                    && events.len() < 200
                {
                    events.push(format!(
                        "frame={frame_num} cck={} DoIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_sendio
                && pc == ep
            {
                let io = amiga.cpu().regs.a[1];
                let desc = describe_iorequest(&amiga, io);
                if (this_task == validator_addr || desc.contains("dev=timer.device"))
                    && events.len() < 200
                {
                    events.push(format!(
                        "frame={frame_num} cck={} SendIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(beginio) = timer_beginio
                && pc == beginio
            {
                let io = amiga.cpu().regs.a[1];
                let command = read_word(&amiga, io.wrapping_add(IO_COMMAND));
                let desc = describe_iorequest(&amiga, io);
                *timer_beginio_counts
                    .entry((source.clone(), command, desc.clone()))
                    .or_insert(0) += 1;
                if events.len() < 200 {
                    events.push(format!(
                        "frame={frame_num} cck={} timer.BeginIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }

            if timer_vbl_handler != 0 && pc == timer_vbl_handler {
                vbl_hits += 1;
                if events.len() < 200 {
                    events.push(format!(
                        "frame={frame_num} cck={} timer VBL handler is_Data=${timer_vbl_data:08X}",
                        amiga.cck_count(),
                    ));
                }
            }
            if timer_cia_handler != 0 && pc == timer_cia_handler {
                cia_hits += 1;
                if events.len() < 200 {
                    events.push(format!(
                        "frame={frame_num} cck={} timer CIA handler is_Data=${timer_cia_data:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_wait = resolve_lvo(&amiga, exec_base, LVO_WAIT);
                lvo_signal = resolve_lvo(&amiga, exec_base, LVO_SIGNAL);
                lvo_doio = resolve_lvo(&amiga, exec_base, LVO_DO_IO);
                lvo_sendio = resolve_lvo(&amiga, exec_base, LVO_SEND_IO);
            }
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
        }

        if validator_addr != 0 && (WINDOW_START..=WINDOW_END).contains(&frame_num) {
            let state = read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE));
            let sig_wait = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
            let sig_recvd = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD));
            if prev_validator_state != Some(state)
                || prev_validator_sig_wait != Some(sig_wait)
                || prev_validator_sig_recvd != Some(sig_recvd)
            {
                if events.len() < 200 {
                    events.push(format!(
                        "frame={frame_num} cck={} Validator state={}({state}) sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X} thisTask={}",
                        amiga.cck_count(),
                        state_name(state),
                        task_name(&amiga, current_task_addr(&amiga)),
                    ));
                }
                prev_validator_state = Some(state);
                prev_validator_sig_wait = Some(sig_wait);
                prev_validator_sig_recvd = Some(sig_recvd);
            }
        }
    }

    eprintln!("=== Validator transition window ===");
    if validator_addr != 0 {
        eprintln!("validator @ ${validator_addr:08X}");
    } else {
        eprintln!("validator was never found");
    }
    if timer_base != 0 {
        eprintln!(
            "timer.device @ ${timer_base:08X} BeginIO={} VBL=${timer_vbl_handler:08X} CIA=${timer_cia_handler:08X}",
            timer_beginio
                .map(|v| format!("${v:08X}"))
                .unwrap_or_else(|| "<none>".into()),
        );
    } else {
        eprintln!("timer.device was never found");
        if exec_base != 0 {
            eprintln!("device list snapshot:");
            for (node, name) in snapshot_device_list(&amiga, exec_base, 16) {
                eprintln!("  ${node:08X} {name}");
            }
        }
    }
    eprintln!("window frames = {WINDOW_START}..={WINDOW_END}");
    eprintln!("timer VBL hits in window = {vbl_hits}");
    eprintln!("timer CIA hits in window = {cia_hits}");

    if !validator_wait_masks.is_empty() {
        eprintln!("\n=== Validator Wait masks in window ===");
        for (mask, count) in validator_wait_masks {
            eprintln!("  {count:>3} × ${mask:08X}");
        }
    }

    if !signal_to_validator.is_empty() {
        eprintln!("\n=== Signal traffic to Validator ===");
        for ((source, mask), count) in signal_to_validator {
            eprintln!("  {count:>3} × {source:<20} mask=${mask:08X}");
        }
    }

    if !timer_beginio_counts.is_empty() {
        eprintln!("\n=== timer.device BeginIO in window ===");
        for ((source, command, desc), count) in timer_beginio_counts {
            eprintln!("  {count:>3} × {source:<20} cmd=${command:04X} {desc}");
        }
    }

    if !validator_pc_events.is_empty() {
        eprintln!("\n=== Validator PC transitions in window ===");
        for event in &validator_pc_events {
            eprintln!("  {event}");
        }
    }

    if !events.is_empty() {
        eprintln!("\n=== Transition events ===");
        for event in &events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_signal_window() {
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

    const WINDOW_START: u64 = 360;
    const WINDOW_END: u64 = 380;

    let mut exec_base = 0u32;
    let mut lvo_wait = None;
    let mut lvo_signal = None;
    let mut lvo_doio = None;
    let mut lvo_sendio = None;
    let mut lvo_putmsg = None;
    let mut lvo_getmsg = None;
    let mut lvo_replymsg = None;
    let mut lvo_waitport = None;

    let mut validator_addr = 0u32;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut prev_validator_state = None;
    let mut prev_validator_sig_wait = None;
    let mut prev_validator_sig_recvd = None;

    let mut events = Vec::<String>::new();

    for frame in 0..390u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            if !in_window {
                continue;
            }

            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let this_task = current_task_addr(&amiga);
            let source = task_name(&amiga, this_task);

            if let Some(ep) = lvo_wait
                && pc == ep
            {
                let mask = amiga.cpu().regs.d[0];
                if this_task == validator_addr
                    || matches!(
                        source.as_str(),
                        "Validator" | "File System" | "trackdisk.device" | "input.device"
                    )
                {
                    events.push(format!(
                        "frame={frame_num} cck={} Wait src={source} mask=${mask:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_signal
                && pc == ep
            {
                let target_addr = amiga.cpu().regs.a[1];
                let target = task_name(&amiga, target_addr);
                let mask = amiga.cpu().regs.d[0];
                if target_addr == validator_addr
                    || mask == 0x8000_0000
                    || matches!(
                        source.as_str(),
                        "Validator" | "File System" | "trackdisk.device" | "input.device"
                    )
                    || matches!(
                        target.as_str(),
                        "Validator" | "File System" | "trackdisk.device" | "input.device"
                    )
                {
                    events.push(format!(
                        "frame={frame_num} cck={} Signal src={source} target={target} mask=${mask:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_doio
                && pc == ep
            {
                let io = amiga.cpu().regs.a[1];
                let desc = describe_iorequest(&amiga, io);
                if matches!(
                    source.as_str(),
                    "Validator" | "File System" | "trackdisk.device" | "input.device"
                ) || desc.contains("dev=timer.device")
                {
                    events.push(format!(
                        "frame={frame_num} cck={} DoIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_sendio
                && pc == ep
            {
                let io = amiga.cpu().regs.a[1];
                let desc = describe_iorequest(&amiga, io);
                if matches!(
                    source.as_str(),
                    "Validator" | "File System" | "trackdisk.device" | "input.device"
                ) || desc.contains("dev=timer.device")
                {
                    events.push(format!(
                        "frame={frame_num} cck={} SendIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_putmsg
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                let msg = amiga.cpu().regs.a[1];
                if matches!(
                    source.as_str(),
                    "Validator" | "File System" | "trackdisk.device" | "input.device"
                ) {
                    events.push(format!(
                        "frame={frame_num} cck={} PutMsg src={source} port=${port:08X} msg=${msg:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_getmsg
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if matches!(
                    source.as_str(),
                    "Validator" | "File System" | "trackdisk.device" | "input.device"
                ) {
                    events.push(format!(
                        "frame={frame_num} cck={} GetMsg src={source} port=${port:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_replymsg
                && pc == ep
            {
                let msg = amiga.cpu().regs.a[1];
                if matches!(
                    source.as_str(),
                    "Validator" | "File System" | "trackdisk.device" | "input.device"
                ) {
                    events.push(format!(
                        "frame={frame_num} cck={} ReplyMsg src={source} msg=${msg:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            if let Some(ep) = lvo_waitport
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if matches!(
                    source.as_str(),
                    "Validator" | "File System" | "trackdisk.device" | "input.device"
                ) {
                    events.push(format!(
                        "frame={frame_num} cck={} WaitPort src={source} port=${port:08X}",
                        amiga.cck_count(),
                    ));
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_wait = resolve_lvo(&amiga, exec_base, LVO_WAIT);
                lvo_signal = resolve_lvo(&amiga, exec_base, LVO_SIGNAL);
                lvo_doio = resolve_lvo(&amiga, exec_base, LVO_DO_IO);
                lvo_sendio = resolve_lvo(&amiga, exec_base, LVO_SEND_IO);
                lvo_putmsg = resolve_lvo(&amiga, exec_base, LVO_PUT_MSG);
                lvo_getmsg = resolve_lvo(&amiga, exec_base, LVO_GET_MSG);
                lvo_replymsg = resolve_lvo(&amiga, exec_base, LVO_REPLY_MSG);
                lvo_waitport = resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT);
            }
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
        }

        if validator_addr != 0 && in_window {
            let state = read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE));
            let sig_wait = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
            let sig_recvd = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD));
            if prev_validator_state != Some(state)
                || prev_validator_sig_wait != Some(sig_wait)
                || prev_validator_sig_recvd != Some(sig_recvd)
            {
                events.push(format!(
                    "frame={frame_num} cck={} Validator state={}({state}) sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X} thisTask={}",
                    amiga.cck_count(),
                    state_name(state),
                    task_name(&amiga, current_task_addr(&amiga)),
                ));
                prev_validator_state = Some(state);
                prev_validator_sig_wait = Some(sig_wait);
                prev_validator_sig_recvd = Some(sig_recvd);
            }
        }
    }

    eprintln!("=== Validator signal window ({WINDOW_START}..={WINDOW_END}) ===");
    if validator_addr != 0 {
        eprintln!("validator @ ${validator_addr:08X}");
    }
    for event in &events {
        eprintln!("  {event}");
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_task_field_writers() {
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

    const WINDOW_START: u64 = 360;
    const WINDOW_END: u64 = 380;

    let mut validator_addr = 0u32;
    let mut watch_armed = false;
    let mut last_watch_len = 0usize;
    let mut prev_state = None;
    let mut prev_sig_wait = None;
    let mut prev_sig_recvd = None;
    let mut events = Vec::<String>::new();

    for frame in 0..390u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
        }

        if !watch_armed && validator_addr != 0 && frame_num == WINDOW_START {
            amiga.debug_watch_addr = Some((validator_addr.wrapping_add(TASK_STATE), 16));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
            watch_armed = true;
            events.push(format!(
                "frame={frame_num} cck={} armed task watch @ ${:08X}",
                amiga.cck_count(),
                validator_addr.wrapping_add(TASK_STATE),
            ));
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            if !in_window || !watch_armed {
                continue;
            }

            while last_watch_len < amiga.debug_watch_writes.len() {
                let (cck, writer_pc, addr, val, is_word) = amiga.debug_watch_writes[last_watch_len];
                let writer_task = task_name(&amiga, current_task_addr(&amiga));
                let state = read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE));
                let sig_wait = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
                let sig_recvd = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD));
                events.push(format!(
                    "frame={frame_num} cck={cck} WATCH {} addr=${addr:08X} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={writer_task} -> state={}({state}) sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X}",
                    validator_watch_label(validator_addr, addr, is_word),
                    state_name(state),
                ));
                last_watch_len += 1;
            }
        }

        if validator_addr != 0 && in_window {
            let state = read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE));
            let sig_wait = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
            let sig_recvd = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD));
            if prev_state != Some(state)
                || prev_sig_wait != Some(sig_wait)
                || prev_sig_recvd != Some(sig_recvd)
            {
                events.push(format!(
                    "frame={frame_num} cck={} SAMPLE state={}({state}) sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X} thisTask={}",
                    amiga.cck_count(),
                    state_name(state),
                    task_name(&amiga, current_task_addr(&amiga)),
                ));
                prev_state = Some(state);
                prev_sig_wait = Some(sig_wait);
                prev_sig_recvd = Some(sig_recvd);
            }
        }

        if watch_armed && frame_num == WINDOW_END {
            amiga.debug_watch_addr = None;
        }
    }

    eprintln!("=== Validator task-field writers ({WINDOW_START}..={WINDOW_END}) ===");
    if validator_addr != 0 {
        eprintln!("validator @ ${validator_addr:08X}");
    } else {
        eprintln!("validator was never found");
    }
    if events.is_empty() {
        eprintln!("(no watched writes or state samples)");
    } else {
        for event in &events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_ports_and_sigalloc() {
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

    const WINDOW_START: u64 = 330;
    const WINDOW_END: u64 = 390;

    let mut exec_base = 0u32;
    let mut lvo_alloc_signal = None;
    let mut lvo_wait = None;
    let mut lvo_signal = None;
    let mut lvo_putmsg = None;
    let mut lvo_getmsg = None;
    let mut lvo_replymsg = None;
    let mut lvo_waitport = None;

    let mut validator_addr = 0u32;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut prev_sig_alloc = None;
    let mut prev_port_snapshot = Vec::<String>::new();

    let mut sig_alloc_events = Vec::<String>::new();
    let mut port_snapshot_events = Vec::<String>::new();
    let mut traffic_events = Vec::<String>::new();

    for frame in 0..410u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_alloc_signal = resolve_lvo(&amiga, exec_base, LVO_ALLOC_SIGNAL);
                lvo_wait = resolve_lvo(&amiga, exec_base, LVO_WAIT);
                lvo_signal = resolve_lvo(&amiga, exec_base, LVO_SIGNAL);
                lvo_putmsg = resolve_lvo(&amiga, exec_base, LVO_PUT_MSG);
                lvo_getmsg = resolve_lvo(&amiga, exec_base, LVO_GET_MSG);
                lvo_replymsg = resolve_lvo(&amiga, exec_base, LVO_REPLY_MSG);
                lvo_waitport = resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT);
            }
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
            let sig_alloc = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_ALLOC));
            sig_alloc_events.push(format!(
                "frame={frame_num} cck={} first seen validator @ ${validator_addr:08X} sigAlloc=${sig_alloc:08X}",
                amiga.cck_count(),
            ));
            prev_sig_alloc = Some(sig_alloc);
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            if validator_addr != 0 {
                let this_task = current_task_addr(&amiga);
                let source = task_name(&amiga, this_task);
                let validator_ports = validator_port_addrs(&amiga, validator_addr);

                if let Some(ep) = lvo_alloc_signal
                    && pc == ep
                    && source == "Validator"
                {
                    let req = amiga.cpu().regs.d[0] as i32;
                    sig_alloc_events.push(format!(
                        "frame={frame_num} cck={} AllocSignal src=Validator req={} ret=${:08X}",
                        amiga.cck_count(),
                        req,
                        read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_ALLOC)),
                    ));
                }

                if in_window {
                    if let Some(ep) = lvo_wait
                        && pc == ep
                        && source == "Validator"
                    {
                        traffic_events.push(format!(
                            "frame={frame_num} cck={} Wait src=Validator mask=${:08X}",
                            amiga.cck_count(),
                            amiga.cpu().regs.d[0],
                        ));
                    }

                    if let Some(ep) = lvo_signal
                        && pc == ep
                    {
                        let target_addr = amiga.cpu().regs.a[1];
                        let mask = amiga.cpu().regs.d[0];
                        if target_addr == validator_addr || mask == 0x8000_0000 {
                            let target = task_name(&amiga, target_addr);
                            traffic_events.push(format!(
                                "frame={frame_num} cck={} Signal src={source} target={target} mask=${mask:08X}",
                                amiga.cck_count(),
                            ));
                        }
                    }

                    if let Some(ep) = lvo_putmsg
                        && pc == ep
                    {
                        let port = amiga.cpu().regs.a[0];
                        let msg = amiga.cpu().regs.a[1];
                        if validator_ports.contains(&port) {
                            traffic_events.push(format!(
                                "frame={frame_num} cck={} PutMsg src={source} port=${port:08X} msg=${msg:08X}",
                                amiga.cck_count(),
                            ));
                        }
                    }

                    if let Some(ep) = lvo_getmsg
                        && pc == ep
                    {
                        let port = amiga.cpu().regs.a[0];
                        if validator_ports.contains(&port) {
                            traffic_events.push(format!(
                                "frame={frame_num} cck={} GetMsg src={source} port=${port:08X}",
                                amiga.cck_count(),
                            ));
                        }
                    }

                    if let Some(ep) = lvo_waitport
                        && pc == ep
                    {
                        let port = amiga.cpu().regs.a[0];
                        if validator_ports.contains(&port) {
                            traffic_events.push(format!(
                                "frame={frame_num} cck={} WaitPort src={source} port=${port:08X}",
                                amiga.cck_count(),
                            ));
                        }
                    }

                    if let Some(ep) = lvo_replymsg
                        && pc == ep
                    {
                        let msg = amiga.cpu().regs.a[1];
                        let reply_port = read_long(&amiga, msg.wrapping_add(MN_REPLYPORT));
                        if validator_ports.contains(&reply_port) {
                            traffic_events.push(format!(
                                "frame={frame_num} cck={} ReplyMsg src={source} msg=${msg:08X} replyPort=${reply_port:08X}",
                                amiga.cck_count(),
                            ));
                        }
                    }
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if validator_addr != 0 {
            let sig_alloc = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_ALLOC));
            if prev_sig_alloc != Some(sig_alloc) {
                sig_alloc_events.push(format!(
                    "frame={frame_num} cck={} sigAlloc=${sig_alloc:08X}",
                    amiga.cck_count(),
                ));
                prev_sig_alloc = Some(sig_alloc);
            }

            if in_window {
                let snapshot = validator_port_snapshot(&amiga, validator_addr);
                if snapshot != prev_port_snapshot {
                    port_snapshot_events.push(format!(
                        "frame={frame_num} cck={} {}",
                        amiga.cck_count(),
                        if snapshot.is_empty() {
                            "validator-owned ports: <none>".into()
                        } else {
                            format!("validator-owned ports: {}", snapshot.join(" | "))
                        }
                    ));
                    prev_port_snapshot = snapshot;
                }
            }
        }
    }

    eprintln!("=== Validator ports / sigalloc trace ({WINDOW_START}..={WINDOW_END}) ===");
    if validator_addr != 0 {
        eprintln!("validator @ ${validator_addr:08X}");
    } else {
        eprintln!("validator was never found");
    }
    if let Some(ep) = lvo_alloc_signal {
        eprintln!("AllocSignal = ${ep:08X}");
    }

    if !sig_alloc_events.is_empty() {
        eprintln!("\n=== SigAlloc events ===");
        for event in &sig_alloc_events {
            eprintln!("  {event}");
        }
    }

    if !port_snapshot_events.is_empty() {
        eprintln!("\n=== Validator-owned port snapshots ===");
        for event in &port_snapshot_events {
            eprintln!("  {event}");
        }
    }

    if !traffic_events.is_empty() {
        eprintln!("\n=== Traffic touching Validator bit/ports ===");
        for event in &traffic_events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_idcmp_port_traffic() {
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

    let mut exec_base = 0u32;
    let mut lvo_putmsg = None;
    let mut lvo_getmsg = None;
    let mut lvo_replymsg = None;
    let mut lvo_waitport = None;
    let mut validator_addr = 0u32;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut first_port_snapshot = Vec::<String>::new();
    let mut traffic_counts = BTreeMap::<(String, u32, &'static str), u64>::new();
    let mut raw_events = Vec::<String>::new();
    let mut frame_snapshots = Vec::<String>::new();

    for frame in 0..900u64 {
        let frame_num = frame + 1;

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_putmsg = resolve_lvo(&amiga, exec_base, LVO_PUT_MSG);
                lvo_getmsg = resolve_lvo(&amiga, exec_base, LVO_GET_MSG);
                lvo_replymsg = resolve_lvo(&amiga, exec_base, LVO_REPLY_MSG);
                lvo_waitport = resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT);
            }
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            if validator_addr != 0 {
                let validator_ports = validator_port_addrs(&amiga, validator_addr);
                if !validator_ports.is_empty() {
                    let source = task_name(&amiga, current_task_addr(&amiga));

                    if let Some(ep) = lvo_putmsg
                        && pc == ep
                    {
                        let port = amiga.cpu().regs.a[0];
                        if validator_ports.contains(&port) {
                            *traffic_counts
                                .entry((source.clone(), port, "PutMsg"))
                                .or_insert(0) += 1;
                            if raw_events.len() < 80 {
                                raw_events.push(format!(
                                    "frame={frame_num} cck={} PutMsg src={source} port=${port:08X} msg=${:08X}",
                                    amiga.cck_count(),
                                    amiga.cpu().regs.a[1],
                                ));
                            }
                        }
                    }

                    if let Some(ep) = lvo_getmsg
                        && pc == ep
                    {
                        let port = amiga.cpu().regs.a[0];
                        if validator_ports.contains(&port) {
                            *traffic_counts
                                .entry((source.clone(), port, "GetMsg"))
                                .or_insert(0) += 1;
                            if raw_events.len() < 80 {
                                raw_events.push(format!(
                                    "frame={frame_num} cck={} GetMsg src={source} port=${port:08X}",
                                    amiga.cck_count(),
                                ));
                            }
                        }
                    }

                    if let Some(ep) = lvo_waitport
                        && pc == ep
                    {
                        let port = amiga.cpu().regs.a[0];
                        if validator_ports.contains(&port) {
                            *traffic_counts
                                .entry((source.clone(), port, "WaitPort"))
                                .or_insert(0) += 1;
                            if raw_events.len() < 80 {
                                raw_events.push(format!(
                                    "frame={frame_num} cck={} WaitPort src={source} port=${port:08X}",
                                    amiga.cck_count(),
                                ));
                            }
                        }
                    }

                    if let Some(ep) = lvo_replymsg
                        && pc == ep
                    {
                        let msg = amiga.cpu().regs.a[1];
                        let reply_port = read_long(&amiga, msg.wrapping_add(MN_REPLYPORT));
                        if validator_ports.contains(&reply_port) {
                            *traffic_counts
                                .entry((source.clone(), reply_port, "ReplyMsg"))
                                .or_insert(0) += 1;
                            if raw_events.len() < 80 {
                                raw_events.push(format!(
                                    "frame={frame_num} cck={} ReplyMsg src={source} msg=${msg:08X} replyPort=${reply_port:08X}",
                                    amiga.cck_count(),
                                ));
                            }
                        }
                    }
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if validator_addr != 0 {
            let snapshot = validator_port_snapshot(&amiga, validator_addr);
            if first_port_snapshot.is_empty() && !snapshot.is_empty() {
                first_port_snapshot = snapshot.clone();
                frame_snapshots.push(format!(
                    "frame={frame_num} cck={} first ports: {}",
                    amiga.cck_count(),
                    snapshot.join(" | "),
                ));
            }
            if matches!(frame_num, 368 | 376 | 500 | 700 | 900) && !snapshot.is_empty() {
                frame_snapshots.push(format!(
                    "frame={frame_num} cck={} ports: {}",
                    amiga.cck_count(),
                    snapshot.join(" | "),
                ));
            }
        }
    }

    eprintln!("=== Validator IDCMP port traffic ===");
    if validator_addr != 0 {
        eprintln!("validator @ ${validator_addr:08X}");
    } else {
        eprintln!("validator was never found");
    }

    if !frame_snapshots.is_empty() {
        eprintln!("\n=== Port snapshots ===");
        for snapshot in &frame_snapshots {
            eprintln!("  {snapshot}");
        }
    }

    if !traffic_counts.is_empty() {
        eprintln!("\n=== Traffic counts ===");
        for ((source, port, kind), count) in &traffic_counts {
            eprintln!("  {count:>3} × {kind:<8} src={source:<20} port=${port:08X}");
        }
    }

    if !raw_events.is_empty() {
        eprintln!("\n=== First {} raw events ===", raw_events.len());
        for event in &raw_events {
            eprintln!("  {event}");
        }
    }
}

#[derive(Clone, Copy)]
enum ForceWakeMode {
    SignalOnly,
    SignalPortPutMsg,
    IgnorePortPutMsg,
    BothPortsPutMsg,
}

impl ForceWakeMode {
    fn label(self) -> &'static str {
        match self {
            Self::SignalOnly => "signal-only",
            Self::SignalPortPutMsg => "putmsg-signal-port",
            Self::IgnorePortPutMsg => "putmsg-ignore-port",
            Self::BothPortsPutMsg => "putmsg-both-ports",
        }
    }
}

fn run_force_wake_experiment(mode: ForceWakeMode) {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        return;
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        return;
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let Some(ctx) = find_validator_wait_context(&mut amiga) else {
        eprintln!("{}: validator wait context was never reached", mode.label());
        return;
    };

    let signal_mask = 1u32 << read_byte(&amiga, ctx.signal_port.wrapping_add(PORT_SIGBIT));
    let signal_port_flags = read_byte(&amiga, ctx.signal_port.wrapping_add(PORT_FLAGS));
    let ignore_port_flags = read_byte(&amiga, ctx.ignore_port.wrapping_add(PORT_FLAGS));
    let signal_port_name = read_cstring(
        &amiga,
        read_long(&amiga, ctx.signal_port.wrapping_add(LN_NAME)),
        32,
    );
    let ignore_port_name = read_cstring(
        &amiga,
        read_long(&amiga, ctx.ignore_port.wrapping_add(LN_NAME)),
        32,
    );

    eprintln!("\n=== Force-Wake Experiment: {} ===", mode.label());
    eprintln!(
        "injection point: frame={} cck={} validator=${:08X} sigWait=${:08X} sigRecvd=${:08X}",
        ctx.frame,
        amiga.cck_count(),
        ctx.validator_addr,
        read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_WAIT)),
        read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_RECVD)),
    );
    eprintln!(
        "signal port=${:08X} name={} flags=${signal_port_flags:02X} ignore port=${:08X} name={} flags=${ignore_port_flags:02X}",
        ctx.signal_port, signal_port_name, ctx.ignore_port, ignore_port_name,
    );

    match mode {
        ForceWakeMode::SignalOnly => {
            force_signal_delivery(&mut amiga, ctx.exec_base, ctx.validator_addr, signal_mask);
        }
        ForceWakeMode::SignalPortPutMsg => {
            queue_message(&mut amiga, ctx.signal_port, 0x00C7_E000, 0, 0x20);
            force_signal_delivery(&mut amiga, ctx.exec_base, ctx.validator_addr, signal_mask);
        }
        ForceWakeMode::IgnorePortPutMsg => {
            queue_message(&mut amiga, ctx.ignore_port, 0x00C7_E100, 0, 0x20);
        }
        ForceWakeMode::BothPortsPutMsg => {
            queue_message(&mut amiga, ctx.ignore_port, 0x00C7_E100, 0, 0x20);
            queue_message(&mut amiga, ctx.signal_port, 0x00C7_E000, 0, 0x20);
            force_signal_delivery(&mut amiga, ctx.exec_base, ctx.validator_addr, signal_mask);
        }
    }

    let lvo_wait = resolve_lvo(&amiga, ctx.exec_base, LVO_WAIT);
    let lvo_getmsg = resolve_lvo(&amiga, ctx.exec_base, LVO_GET_MSG);
    let lvo_waitport = resolve_lvo(&amiga, ctx.exec_base, LVO_WAIT_PORT);
    let lvo_doio = resolve_lvo(&amiga, ctx.exec_base, LVO_DO_IO);
    let lvo_sendio = resolve_lvo(&amiga, ctx.exec_base, LVO_SEND_IO);

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut prev_state = None;
    let mut prev_sig_wait = None;
    let mut prev_sig_recvd = None;
    let mut validator_run_samples = 0u64;
    let mut validator_run_pcs = BTreeMap::<u32, u64>::new();
    let mut events = Vec::<String>::new();

    const POST_FRAMES: u64 = 180;
    for post_frame in 0..POST_FRAMES {
        let frame_num = ctx.frame + post_frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();

            let current_task = current_task_addr(&amiga);
            if current_task == ctx.validator_addr {
                validator_run_samples += 1;
                *validator_run_pcs.entry(amiga.cpu().regs.pc).or_insert(0) += 1;
            }

            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let source = task_name(&amiga, current_task);
            if current_task == ctx.validator_addr && events.len() < 120 {
                events.push(format!(
                    "frame={frame_num} cck={} Validator pc=${pc:08X} instr=${instr:08X}",
                    amiga.cck_count(),
                ));
            }

            if let Some(ep) = lvo_wait
                && pc == ep
                && source == "Validator"
                && events.len() < 120
            {
                events.push(format!(
                    "frame={frame_num} cck={} Wait src=Validator mask=${:08X}",
                    amiga.cck_count(),
                    amiga.cpu().regs.d[0],
                ));
            }
            if let Some(ep) = lvo_getmsg
                && pc == ep
                && events.len() < 120
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.signal_port || port == ctx.ignore_port || source == "Validator" {
                    events.push(format!(
                        "frame={frame_num} cck={} GetMsg src={source} port=${port:08X}",
                        amiga.cck_count(),
                    ));
                }
            }
            if let Some(ep) = lvo_waitport
                && pc == ep
                && events.len() < 120
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.signal_port || port == ctx.ignore_port || source == "Validator" {
                    events.push(format!(
                        "frame={frame_num} cck={} WaitPort src={source} port=${port:08X}",
                        amiga.cck_count(),
                    ));
                }
            }
            if let Some(ep) = lvo_doio
                && pc == ep
                && events.len() < 120
            {
                let desc = describe_iorequest(&amiga, amiga.cpu().regs.a[1]);
                if source == "Validator" || desc.contains("trackdisk.device") {
                    events.push(format!(
                        "frame={frame_num} cck={} DoIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }
            if let Some(ep) = lvo_sendio
                && pc == ep
                && events.len() < 120
            {
                let desc = describe_iorequest(&amiga, amiga.cpu().regs.a[1]);
                if source == "Validator" || desc.contains("trackdisk.device") {
                    events.push(format!(
                        "frame={frame_num} cck={} SendIO src={source} {desc}",
                        amiga.cck_count(),
                    ));
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        let state = read_byte(&amiga, ctx.validator_addr.wrapping_add(TASK_STATE));
        let sig_wait = read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_WAIT));
        let sig_recvd = read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_RECVD));
        if prev_state != Some(state)
            || prev_sig_wait != Some(sig_wait)
            || prev_sig_recvd != Some(sig_recvd)
        {
            if events.len() < 120 {
                events.push(format!(
                    "frame={frame_num} cck={} SAMPLE state={}({state}) sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X} signalPortMsgs={} ignorePortMsgs={}",
                    amiga.cck_count(),
                    state_name(state),
                    list_len(&amiga, ctx.signal_port.wrapping_add(PORT_MSG_LIST), 16),
                    list_len(&amiga, ctx.ignore_port.wrapping_add(PORT_MSG_LIST), 16),
                ));
            }
            prev_state = Some(state);
            prev_sig_wait = Some(sig_wait);
            prev_sig_recvd = Some(sig_recvd);
        }
    }

    let final_state = read_byte(&amiga, ctx.validator_addr.wrapping_add(TASK_STATE));
    let final_sig_wait = read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_WAIT));
    let final_sig_recvd = read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_RECVD));
    let final_signal_port_msgs = list_len(&amiga, ctx.signal_port.wrapping_add(PORT_MSG_LIST), 16);
    let final_ignore_port_msgs = list_len(&amiga, ctx.ignore_port.wrapping_add(PORT_MSG_LIST), 16);

    eprintln!(
        "validator run samples after injection = {validator_run_samples}; final state={}({final_state}) sigWait=${final_sig_wait:08X} sigRecvd=${final_sig_recvd:08X}",
        state_name(final_state),
    );
    eprintln!(
        "final port msg counts: signal={} ignore={}",
        final_signal_port_msgs, final_ignore_port_msgs,
    );
    if !validator_run_pcs.is_empty() {
        eprintln!("top validator PCs after injection:");
        let mut pcs: Vec<_> = validator_run_pcs.into_iter().collect();
        pcs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (pc, count) in pcs.into_iter().take(8) {
            eprintln!("  {count:>4} × ${pc:08X}");
        }
    }
    if !events.is_empty() {
        eprintln!("events:");
        for event in &events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn force_wake_validator_experiments() {
    for mode in [
        ForceWakeMode::SignalOnly,
        ForceWakeMode::SignalPortPutMsg,
        ForceWakeMode::IgnorePortPutMsg,
        ForceWakeMode::BothPortsPutMsg,
    ] {
        run_force_wake_experiment(mode);
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_idcmp_creator_path() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discovery = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discovery_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discovery.insert_adf(discovery_adf);
    let Some(ctx) = find_validator_wait_context(&mut discovery) else {
        eprintln!("validator IDCMP wait context was never reached");
        return;
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    const WINDOW_START: u64 = 330;
    const WINDOW_END: u64 = 390;

    let watch_lo = ctx.signal_port.min(ctx.ignore_port);
    let watch_hi = ctx.signal_port.max(ctx.ignore_port).wrapping_add(0x24);
    let watch_len = watch_hi.wrapping_sub(watch_lo);

    let mut exec_base = 0u32;
    let mut intuition_base = 0u32;
    let mut lvo_open_screen = None;
    let mut lvo_open_window = None;
    let mut lvo_init_requester = None;
    let mut lvo_auto_request = None;
    let mut lvo_putmsg = None;
    let mut lvo_getmsg = None;
    let mut lvo_waitport = None;

    let mut validator_addr = 0u32;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut prev_port_snapshot = Vec::<String>::new();
    let mut watch_armed = false;
    let mut last_watch_len = 0usize;

    let mut summary = Vec::<String>::new();
    let mut intuition_events = Vec::<String>::new();
    let mut port_birth_events = Vec::<String>::new();
    let mut watch_events = Vec::<String>::new();
    let mut exec_events = Vec::<String>::new();

    summary.push(format!(
        "discovery: frame={} validator=${:08X} signalPort=${:08X} ignorePort=${:08X}",
        ctx.frame, ctx.validator_addr, ctx.signal_port, ctx.ignore_port,
    ));

    for frame in 0..410u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
        }
        if exec_base != 0
            && intuition_base == 0
            && let Some(found) = find_library(&amiga, exec_base, "intuition.library")
        {
            intuition_base = found;
            lvo_open_screen = resolve_lvo(&amiga, intuition_base, INTUITION_LVO_OPEN_SCREEN);
            lvo_open_window = resolve_lvo(&amiga, intuition_base, INTUITION_LVO_OPEN_WINDOW);
            lvo_init_requester = resolve_lvo(&amiga, intuition_base, INTUITION_LVO_INIT_REQUESTER);
            lvo_auto_request = resolve_lvo(&amiga, intuition_base, INTUITION_LVO_AUTO_REQUEST);
            lvo_putmsg = resolve_lvo(&amiga, exec_base, LVO_PUT_MSG);
            lvo_getmsg = resolve_lvo(&amiga, exec_base, LVO_GET_MSG);
            lvo_waitport = resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT);
            summary.push(format!(
                "intuition.library=${intuition_base:08X} OpenScreen={:?} OpenWindow={:?} InitRequester={:?} AutoRequest={:?}",
                lvo_open_screen, lvo_open_window, lvo_init_requester, lvo_auto_request,
            ));
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
            summary.push(format!(
                "validator first seen in creator trace at frame={frame_num} addr=${validator_addr:08X}"
            ));
        }

        if !watch_armed && frame_num == WINDOW_START {
            amiga.debug_watch_addr = Some((watch_lo, watch_len));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
            watch_armed = true;
            summary.push(format!(
                "armed port watch frame={frame_num} addr=${watch_lo:08X} len=${watch_len:04X}"
            ));
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let source = task_name(&amiga, current_task_addr(&amiga));
            if in_window {
                for (name, ep) in [
                    ("OpenScreen", lvo_open_screen),
                    ("OpenWindow", lvo_open_window),
                    ("InitRequester", lvo_init_requester),
                    ("AutoRequest", lvo_auto_request),
                ] {
                    if let Some(ep) = ep
                        && pc == ep
                    {
                        intuition_events.push(format!(
                            "frame={frame_num} cck={} {name} src={source} A0=${:08X} A1=${:08X} D0=${:08X} D1=${:08X}",
                            amiga.cck_count(),
                            amiga.cpu().regs.a[0],
                            amiga.cpu().regs.a[1],
                            amiga.cpu().regs.d[0],
                            amiga.cpu().regs.d[1],
                        ));
                    }
                }

                if let Some(ep) = lvo_putmsg
                    && pc == ep
                {
                    let port = amiga.cpu().regs.a[0];
                    if port == ctx.signal_port || port == ctx.ignore_port || source == "Validator" {
                        exec_events.push(format!(
                            "frame={frame_num} cck={} PutMsg src={source} port=${port:08X} msg=${:08X}",
                            amiga.cck_count(),
                            amiga.cpu().regs.a[1],
                        ));
                    }
                }

                if let Some(ep) = lvo_getmsg
                    && pc == ep
                {
                    let port = amiga.cpu().regs.a[0];
                    if port == ctx.signal_port || port == ctx.ignore_port || source == "Validator" {
                        exec_events.push(format!(
                            "frame={frame_num} cck={} GetMsg src={source} port=${port:08X}",
                            amiga.cck_count(),
                        ));
                    }
                }

                if let Some(ep) = lvo_waitport
                    && pc == ep
                {
                    let port = amiga.cpu().regs.a[0];
                    if port == ctx.signal_port || port == ctx.ignore_port || source == "Validator" {
                        exec_events.push(format!(
                            "frame={frame_num} cck={} WaitPort src={source} port=${port:08X}",
                            amiga.cck_count(),
                        ));
                    }
                }

                while last_watch_len < amiga.debug_watch_writes.len() {
                    let (cck, writer_pc, addr, val, is_word) =
                        amiga.debug_watch_writes[last_watch_len];
                    watch_events.push(format!(
                        "frame={frame_num} cck={cck} WATCH {} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={} portMsgs(signal={},ignore={})",
                        watched_port_label(ctx.signal_port, ctx.ignore_port, addr, is_word),
                        task_name(&amiga, current_task_addr(&amiga)),
                        list_len(&amiga, ctx.signal_port.wrapping_add(PORT_MSG_LIST), 16),
                        list_len(&amiga, ctx.ignore_port.wrapping_add(PORT_MSG_LIST), 16),
                    ));
                    last_watch_len += 1;
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if validator_addr != 0 && in_window {
            let snapshot = validator_port_snapshot(&amiga, validator_addr);
            if snapshot != prev_port_snapshot {
                let label = if prev_port_snapshot.is_empty() {
                    "first ports"
                } else {
                    "ports changed"
                };
                port_birth_events.push(format!(
                    "frame={frame_num} cck={} {label}: {}",
                    amiga.cck_count(),
                    if snapshot.is_empty() {
                        "<none>".into()
                    } else {
                        snapshot.join(" | ")
                    }
                ));
                prev_port_snapshot = snapshot;
            }
        }

        if watch_armed && frame_num == WINDOW_END {
            amiga.debug_watch_addr = None;
        }
    }

    let signal_refs = scan_longword_refs(&amiga, ctx.signal_port, 24);
    let ignore_refs = scan_longword_refs(&amiga, ctx.ignore_port, 24);
    let mut paired_ref_events = Vec::<String>::new();
    for &signal_ref in &signal_refs {
        for &ignore_ref in &ignore_refs {
            if signal_ref.abs_diff(ignore_ref) <= 0x40 {
                paired_ref_events.push(format!(
                    "signal_ref=${signal_ref:08X} ignore_ref=${ignore_ref:08X} delta=${:02X}",
                    signal_ref.abs_diff(ignore_ref),
                ));
            }
        }
    }

    eprintln!("=== Validator IDCMP creator path ({WINDOW_START}..={WINDOW_END}) ===");
    for line in &summary {
        eprintln!("  {line}");
    }

    if !intuition_events.is_empty() {
        eprintln!("\n=== Intuition calls in window ===");
        for event in &intuition_events {
            eprintln!("  {event}");
        }
    }

    if !port_birth_events.is_empty() {
        eprintln!("\n=== Port appearance ===");
        for event in &port_birth_events {
            eprintln!("  {event}");
        }
    }

    if !watch_events.is_empty() {
        eprintln!("\n=== Port-struct writes ===");
        for event in &watch_events {
            eprintln!("  {event}");
        }
    }

    if !exec_events.is_empty() {
        eprintln!("\n=== Exec traffic touching Validator/IDCMP ===");
        for event in &exec_events {
            eprintln!("  {event}");
        }
    }

    eprintln!("\n=== Signal-port refs (${:#010X}) ===", ctx.signal_port);
    if signal_refs.is_empty() {
        eprintln!("  (none found)");
    } else {
        for addr in &signal_refs {
            eprintln!("  ref=${addr:08X} {}", fmt_long_window(&amiga, *addr));
        }
    }

    eprintln!("\n=== Ignore-port refs (${:#010X}) ===", ctx.ignore_port);
    if ignore_refs.is_empty() {
        eprintln!("  (none found)");
    } else {
        for addr in &ignore_refs {
            eprintln!("  ref=${addr:08X} {}", fmt_long_window(&amiga, *addr));
        }
    }

    if !paired_ref_events.is_empty() {
        eprintln!("\n=== Nearby ref pairs ===");
        for event in &paired_ref_events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_idcmp_ref_holder_writers() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discovery = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discovery_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discovery.insert_adf(discovery_adf);
    let Some(ctx) = find_validator_wait_context(&mut discovery) else {
        eprintln!("validator IDCMP wait context was never reached");
        return;
    };

    let signal_refs: Vec<u32> = scan_longword_refs(&discovery, ctx.signal_port, 24)
        .into_iter()
        .filter(|addr| *addr < ctx.ignore_port || *addr >= ctx.signal_port.wrapping_add(0x24))
        .collect();
    let ignore_refs: Vec<u32> = scan_longword_refs(&discovery, ctx.ignore_port, 24)
        .into_iter()
        .filter(|addr| *addr < ctx.ignore_port || *addr >= ctx.signal_port.wrapping_add(0x24))
        .collect();

    let mut windows = Vec::<(u32, u32)>::new();
    for &signal_ref in &signal_refs {
        for &ignore_ref in &ignore_refs {
            if signal_ref.abs_diff(ignore_ref) <= 0x10 {
                let lo = signal_ref.min(ignore_ref).wrapping_sub(8);
                let hi = signal_ref.max(ignore_ref).wrapping_add(12);
                if !windows
                    .iter()
                    .any(|(existing_lo, existing_hi)| *existing_lo == lo && *existing_hi == hi)
                {
                    windows.push((lo, hi.wrapping_sub(lo)));
                }
            }
        }
    }

    eprintln!("=== Validator IDCMP ref-holder writers ===");
    eprintln!(
        "discovery: validator=${:08X} signalPort=${:08X} ignorePort=${:08X}",
        ctx.validator_addr, ctx.signal_port, ctx.ignore_port,
    );
    eprintln!("external signal refs: {:?}", signal_refs);
    eprintln!("external ignore refs: {:?}", ignore_refs);

    if windows.is_empty() {
        eprintln!("no nearby non-port ref pairs found");
        return;
    }

    for (index, (watch_lo, watch_len)) in windows.iter().copied().enumerate() {
        eprintln!(
            "\n=== Ref-holder window {} addr=${watch_lo:08X} len=${watch_len:04X} ===",
            index + 1
        );
        let events = watch_ref_holder_window(&rom, &adf_bytes, watch_lo, watch_len);
        if events.is_empty() {
            eprintln!("  (no writes)");
        } else {
            for event in &events {
                eprintln!("  {event}");
            }
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_requester_entry_path() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discovery = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discovery_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discovery.insert_adf(discovery_adf);
    let Some(ctx) = find_validator_wait_context(&mut discovery) else {
        eprintln!("validator IDCMP wait context was never reached");
        return;
    };
    let Some(owner_root) =
        infer_validator_idcmp_owner_root(&discovery, ctx.signal_port, ctx.ignore_port)
    else {
        eprintln!(
            "could not infer owner root for signal=${:08X} ignore=${:08X}",
            ctx.signal_port, ctx.ignore_port
        );
        return;
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    const WINDOW_START: u64 = 340;
    const WINDOW_END: u64 = 390;
    const RECENT_CAP: usize = 16;

    let mut exec_base = 0u32;
    let mut lvo_doio = None;
    let mut lvo_sendio = None;
    let mut lvo_putmsg = None;
    let mut lvo_getmsg = None;
    let mut lvo_replymsg = None;
    let mut lvo_waitport = None;
    let mut lvo_signal = None;

    let mut validator_addr = 0u32;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut watch_armed = false;
    let mut last_watch_len = 0usize;

    let mut recent_events = Vec::<String>::new();
    let mut entry_events = Vec::<String>::new();
    let mut owner_watch_events = Vec::<String>::new();
    let mut frame_snapshots = Vec::<String>::new();
    let mut caller_returns = BTreeMap::<u32, u64>::new();

    for frame in 0..410u64 {
        let frame_num = frame + 1;
        let in_window = (WINDOW_START..=WINDOW_END).contains(&frame_num);

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_doio = resolve_lvo(&amiga, exec_base, LVO_DO_IO);
                lvo_sendio = resolve_lvo(&amiga, exec_base, LVO_SEND_IO);
                lvo_putmsg = resolve_lvo(&amiga, exec_base, LVO_PUT_MSG);
                lvo_getmsg = resolve_lvo(&amiga, exec_base, LVO_GET_MSG);
                lvo_replymsg = resolve_lvo(&amiga, exec_base, LVO_REPLY_MSG);
                lvo_waitport = resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT);
                lvo_signal = resolve_lvo(&amiga, exec_base, LVO_SIGNAL);
            }
        }

        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
        }

        if !watch_armed && frame_num == WINDOW_START {
            amiga.debug_watch_addr = Some((owner_root.wrapping_add(VALIDATOR_OWNER_CTRL), 0x16));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
            watch_armed = true;
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let current_task = current_task_addr(&amiga);
            let source = task_name(&amiga, current_task);
            let cck = amiga.cck_count();

            if let Some(ep) = lvo_doio
                && pc == ep
                && source == "Validator"
            {
                push_recent(
                    &mut recent_events,
                    format!(
                        "frame={frame_num} cck={cck} DoIO {}",
                        describe_iorequest(&amiga, amiga.cpu().regs.a[1]),
                    ),
                    RECENT_CAP,
                );
            }
            if let Some(ep) = lvo_sendio
                && pc == ep
                && source == "Validator"
            {
                push_recent(
                    &mut recent_events,
                    format!(
                        "frame={frame_num} cck={cck} SendIO {}",
                        describe_iorequest(&amiga, amiga.cpu().regs.a[1]),
                    ),
                    RECENT_CAP,
                );
            }
            if let Some(ep) = lvo_getmsg
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.ignore_port || port == ctx.signal_port || source == "Validator" {
                    push_recent(
                        &mut recent_events,
                        format!("frame={frame_num} cck={cck} GetMsg src={source} port=${port:08X}"),
                        RECENT_CAP,
                    );
                }
            }
            if let Some(ep) = lvo_putmsg
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.ignore_port || port == ctx.signal_port || source == "Validator" {
                    push_recent(
                        &mut recent_events,
                        format!(
                            "frame={frame_num} cck={cck} PutMsg src={source} port=${port:08X} msg=${:08X}",
                            amiga.cpu().regs.a[1],
                        ),
                        RECENT_CAP,
                    );
                }
            }
            if let Some(ep) = lvo_replymsg
                && pc == ep
            {
                let msg = amiga.cpu().regs.a[1];
                let reply_port = read_long(&amiga, msg.wrapping_add(MN_REPLYPORT));
                if reply_port == ctx.ignore_port
                    || reply_port == ctx.signal_port
                    || source == "Validator"
                {
                    push_recent(
                        &mut recent_events,
                        format!(
                            "frame={frame_num} cck={cck} ReplyMsg src={source} msg=${msg:08X} replyPort=${reply_port:08X}"
                        ),
                        RECENT_CAP,
                    );
                }
            }
            if let Some(ep) = lvo_waitport
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.ignore_port || port == ctx.signal_port || source == "Validator" {
                    push_recent(
                        &mut recent_events,
                        format!(
                            "frame={frame_num} cck={cck} WaitPort src={source} port=${port:08X}"
                        ),
                        RECENT_CAP,
                    );
                }
            }
            if let Some(ep) = lvo_signal
                && pc == ep
            {
                let target = amiga.cpu().regs.a[1];
                let mask = amiga.cpu().regs.d[0];
                if target == validator_addr || mask == 0x8000_0000 {
                    push_recent(
                        &mut recent_events,
                        format!(
                            "frame={frame_num} cck={cck} Signal src={source} target={} mask=${mask:08X}",
                            task_name(&amiga, target),
                        ),
                        RECENT_CAP,
                    );
                }
            }

            if source == "Validator"
                && (pc == VALIDATOR_IDCMP_DECIDER_ENTRY
                    || pc == VALIDATOR_IDCMP_SETUP_ENTRY
                    || pc == VALIDATOR_IDCMP_SETUP_AFTER_HELPER)
            {
                let sp = active_sp(&amiga);
                let ret_pc = read_long(&amiga, sp);
                let ret_pc_2 = read_long(&amiga, sp.wrapping_add(4));
                if pc == VALIDATOR_IDCMP_SETUP_ENTRY {
                    *caller_returns.entry(ret_pc).or_insert(0) += 1;
                }
                let recent = if recent_events.is_empty() {
                    "<none>".into()
                } else {
                    recent_events.join(" || ")
                };
                let label = match pc {
                    VALIDATOR_IDCMP_DECIDER_ENTRY => "FD5B8A entry",
                    VALIDATOR_IDCMP_SETUP_ENTRY => "FD56F0 entry",
                    VALIDATOR_IDCMP_SETUP_AFTER_HELPER => "FD56FA after-helper",
                    _ => unreachable!(),
                };
                entry_events.push(format!(
                    "frame={frame_num} cck={cck} {label} ret=${ret_pc:08X} ret+4=${ret_pc_2:08X} D0=${:08X} D1=${:08X} D2=${:08X} D3=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} {} recent=[{}]",
                    amiga.cpu().regs.d[0],
                    amiga.cpu().regs.d[1],
                    amiga.cpu().regs.d[2],
                    amiga.cpu().regs.d[3],
                    amiga.cpu().regs.a[0],
                    amiga.cpu().regs.a[1],
                    amiga.cpu().regs.a[2],
                    amiga.cpu().regs.a[3],
                    fmt_owner_snapshot(&amiga, owner_root),
                    recent,
                ));
            }

            if in_window {
                while last_watch_len < amiga.debug_watch_writes.len() {
                    let (watch_cck, writer_pc, addr, val, is_word) =
                        amiga.debug_watch_writes[last_watch_len];
                    owner_watch_events.push(format!(
                        "frame={frame_num} cck={watch_cck} WATCH {} addr=${addr:08X} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={} {}",
                        owner_field_name(owner_root, addr, is_word),
                        task_name(&amiga, current_task_addr(&amiga)),
                        fmt_owner_snapshot(&amiga, owner_root),
                    ));
                    last_watch_len += 1;
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if matches!(frame_num, 363 | 368 | 376 | 390) {
            frame_snapshots.push(format!(
                "frame={frame_num} cck={} {} signalMsgs={} ignoreMsgs={} validatorState={}({}) sigWait=${:08X} sigRecvd=${:08X}",
                amiga.cck_count(),
                fmt_owner_snapshot(&amiga, owner_root),
                list_len(&amiga, ctx.signal_port.wrapping_add(PORT_MSG_LIST), 16),
                list_len(&amiga, ctx.ignore_port.wrapping_add(PORT_MSG_LIST), 16),
                state_name(read_byte(&amiga, ctx.validator_addr.wrapping_add(TASK_STATE))),
                read_byte(&amiga, ctx.validator_addr.wrapping_add(TASK_STATE)),
                read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_WAIT)),
                read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_RECVD)),
            ));
        }

        if watch_armed && frame_num == WINDOW_END {
            amiga.debug_watch_addr = None;
        }
    }

    eprintln!("=== Validator requester-entry path ===");
    eprintln!(
        "discovery: validator=${:08X} owner=${owner_root:08X} signalPort=${:08X} ignorePort=${:08X}",
        ctx.validator_addr, ctx.signal_port, ctx.ignore_port,
    );

    if !entry_events.is_empty() {
        eprintln!("\n=== Requester-path entry events ===");
        for event in &entry_events {
            eprintln!("  {event}");
        }
    }

    if !owner_watch_events.is_empty() {
        eprintln!("\n=== Owner-field writes ===");
        for event in &owner_watch_events {
            eprintln!("  {event}");
        }
    }

    if !frame_snapshots.is_empty() {
        eprintln!("\n=== Frame snapshots ===");
        for snapshot in &frame_snapshots {
            eprintln!("  {snapshot}");
        }
    }

    if !caller_returns.is_empty() {
        eprintln!("\n=== Caller return PCs ===");
        for (ret_pc, count) in &caller_returns {
            eprintln!("  {count:>2} × ${ret_pc:08X}");
            let start = ret_pc.wrapping_sub(0x20);
            let end = ret_pc.wrapping_add(0x20);
            for line in disassemble_live_region(&amiga, start, end) {
                eprintln!("    {line}");
            }
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_idcmp_bridge_gap() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discovery = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discovery_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discovery.insert_adf(discovery_adf);
    let Some(ctx) = find_validator_wait_context(&mut discovery) else {
        eprintln!("validator IDCMP wait context was never reached");
        return;
    };
    let Some(owner_root) =
        infer_validator_idcmp_owner_root(&discovery, ctx.signal_port, ctx.ignore_port)
    else {
        eprintln!(
            "could not infer owner root for signal=${:08X} ignore=${:08X}",
            ctx.signal_port, ctx.ignore_port
        );
        return;
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut exec_base = 0u32;
    let mut lvo_putmsg = None;
    let mut lvo_getmsg = None;
    let mut lvo_replymsg = None;
    let mut lvo_waitport = None;
    let mut lvo_signal = None;

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;

    let mut ignore_getmsg_input = 0u64;
    let mut signal_getmsg_any = 0u64;
    let mut ignore_putmsg_any = 0u64;
    let mut signal_putmsg_any = 0u64;
    let mut reply_to_ignore = 0u64;
    let mut reply_to_signal = 0u64;
    let mut waitport_signal_validator = 0u64;
    let mut waitport_ignore_any = 0u64;
    let mut signal_to_validator_bit31 = 0u64;

    let mut raw_events = Vec::<String>::new();
    let mut frame_snapshots = Vec::<String>::new();

    for frame in 0..900u64 {
        let frame_num = frame + 1;

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_putmsg = resolve_lvo(&amiga, exec_base, LVO_PUT_MSG);
                lvo_getmsg = resolve_lvo(&amiga, exec_base, LVO_GET_MSG);
                lvo_replymsg = resolve_lvo(&amiga, exec_base, LVO_REPLY_MSG);
                lvo_waitport = resolve_lvo(&amiga, exec_base, LVO_WAIT_PORT);
                lvo_signal = resolve_lvo(&amiga, exec_base, LVO_SIGNAL);
            }
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let source = task_name(&amiga, current_task_addr(&amiga));
            let cck = amiga.cck_count();

            if let Some(ep) = lvo_getmsg
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.ignore_port && source == "input.device" {
                    ignore_getmsg_input += 1;
                    if raw_events.len() < 120 {
                        raw_events.push(format!(
                            "frame={frame_num} cck={cck} GetMsg src=input.device port=${port:08X} {}",
                            fmt_owner_snapshot(&amiga, owner_root),
                        ));
                    }
                } else if port == ctx.signal_port {
                    signal_getmsg_any += 1;
                    if raw_events.len() < 120 {
                        raw_events.push(format!(
                            "frame={frame_num} cck={cck} GetMsg src={source} port=${port:08X}"
                        ));
                    }
                }
            }

            if let Some(ep) = lvo_putmsg
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.ignore_port {
                    ignore_putmsg_any += 1;
                }
                if port == ctx.signal_port {
                    signal_putmsg_any += 1;
                }
                if raw_events.len() < 120 && (port == ctx.ignore_port || port == ctx.signal_port) {
                    raw_events.push(format!(
                        "frame={frame_num} cck={cck} PutMsg src={source} port=${port:08X} msg=${:08X}",
                        amiga.cpu().regs.a[1],
                    ));
                }
            }

            if let Some(ep) = lvo_replymsg
                && pc == ep
            {
                let msg = amiga.cpu().regs.a[1];
                let reply_port = read_long(&amiga, msg.wrapping_add(MN_REPLYPORT));
                if reply_port == ctx.ignore_port {
                    reply_to_ignore += 1;
                }
                if reply_port == ctx.signal_port {
                    reply_to_signal += 1;
                }
                if raw_events.len() < 120
                    && (reply_port == ctx.ignore_port || reply_port == ctx.signal_port)
                {
                    raw_events.push(format!(
                        "frame={frame_num} cck={cck} ReplyMsg src={source} msg=${msg:08X} replyPort=${reply_port:08X}"
                    ));
                }
            }

            if let Some(ep) = lvo_waitport
                && pc == ep
            {
                let port = amiga.cpu().regs.a[0];
                if port == ctx.signal_port && source == "Validator" {
                    waitport_signal_validator += 1;
                }
                if port == ctx.ignore_port {
                    waitport_ignore_any += 1;
                }
                if raw_events.len() < 120 && (port == ctx.ignore_port || port == ctx.signal_port) {
                    raw_events.push(format!(
                        "frame={frame_num} cck={cck} WaitPort src={source} port=${port:08X}"
                    ));
                }
            }

            if let Some(ep) = lvo_signal
                && pc == ep
            {
                let target = amiga.cpu().regs.a[1];
                let mask = amiga.cpu().regs.d[0];
                if target == ctx.validator_addr && mask == 0x8000_0000 {
                    signal_to_validator_bit31 += 1;
                }
                if raw_events.len() < 120 && (target == ctx.validator_addr || mask == 0x8000_0000) {
                    raw_events.push(format!(
                        "frame={frame_num} cck={cck} Signal src={source} target={} mask=${mask:08X}",
                        task_name(&amiga, target),
                    ));
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if matches!(frame_num, 368 | 376 | 500 | 700 | 900) {
            frame_snapshots.push(format!(
                "frame={frame_num} cck={} {} signalMsgs={} ignoreMsgs={} validatorState={}({}) sigWait=${:08X} sigRecvd=${:08X}",
                amiga.cck_count(),
                fmt_owner_snapshot(&amiga, owner_root),
                list_len(&amiga, ctx.signal_port.wrapping_add(PORT_MSG_LIST), 16),
                list_len(&amiga, ctx.ignore_port.wrapping_add(PORT_MSG_LIST), 16),
                state_name(read_byte(&amiga, ctx.validator_addr.wrapping_add(TASK_STATE))),
                read_byte(&amiga, ctx.validator_addr.wrapping_add(TASK_STATE)),
                read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_WAIT)),
                read_long(&amiga, ctx.validator_addr.wrapping_add(TASK_SIG_RECVD)),
            ));
        }
    }

    eprintln!("=== Validator IDCMP bridge gap ===");
    eprintln!(
        "discovery: validator=${:08X} owner=${owner_root:08X} signalPort=${:08X} ignorePort=${:08X}",
        ctx.validator_addr, ctx.signal_port, ctx.ignore_port,
    );
    eprintln!("ignore GetMsg by input.device = {ignore_getmsg_input}");
    eprintln!("signal GetMsg by any task = {signal_getmsg_any}");
    eprintln!("ignore PutMsg by any task = {ignore_putmsg_any}");
    eprintln!("signal PutMsg by any task = {signal_putmsg_any}");
    eprintln!("ReplyMsg to ignore port = {reply_to_ignore}");
    eprintln!("ReplyMsg to signal port = {reply_to_signal}");
    eprintln!("Validator WaitPort on signal port = {waitport_signal_validator}");
    eprintln!("WaitPort on ignore port = {waitport_ignore_any}");
    eprintln!("Signal(Validator, $80000000) calls = {signal_to_validator_bit31}");

    if !frame_snapshots.is_empty() {
        eprintln!("\n=== Frame snapshots ===");
        for snapshot in &frame_snapshots {
            eprintln!("  {snapshot}");
        }
    }

    if !raw_events.is_empty() {
        eprintln!("\n=== First {} IDCMP bridge events ===", raw_events.len());
        for event in &raw_events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_requester_ram_chain() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discovery = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discovery_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discovery.insert_adf(discovery_adf);
    let Some(ctx) = find_validator_wait_context(&mut discovery) else {
        eprintln!("validator IDCMP wait context was never reached");
        return;
    };
    let Some(owner_root) =
        infer_validator_idcmp_owner_root(&discovery, ctx.signal_port, ctx.ignore_port)
    else {
        eprintln!(
            "could not infer owner root for signal=${:08X} ignore=${:08X}",
            ctx.signal_port, ctx.ignore_port
        );
        return;
    };

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    const RECENT_CAP: usize = 48;

    let mut exec_base = 0u32;
    let mut lvo_wait = None;

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut last_validator_instr = u32::MAX;

    let mut validator_recent = Vec::<String>::new();
    let mut setup_recent = Vec::<String>::new();
    let mut wait_recent = Vec::<String>::new();
    let mut setup_stack = "<none>".to_string();
    let mut wait_stack = "<none>".to_string();
    let mut setup_summary = String::new();
    let mut wait_summary = String::new();
    let mut setup_regions = Vec::<String>::new();
    let mut wait_regions = Vec::<String>::new();
    let mut ram_instr_hist = BTreeMap::<u32, u64>::new();

    let mut saw_setup = false;
    let mut saw_wait = false;

    for frame in 0..390u64 {
        let frame_num = frame + 1;

        if exec_base == 0 {
            exec_base = read_long(&amiga, 0x0000_0004);
            if exec_base != 0 {
                lvo_wait = resolve_lvo(&amiga, exec_base, LVO_WAIT);
            }
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let source = task_name(&amiga, current_task_addr(&amiga));
            let cck = amiga.cck_count();

            if source == "Validator" && instr != last_validator_instr {
                last_validator_instr = instr;
                if (0x00C0_0000..0x00C8_0000).contains(&instr) {
                    *ram_instr_hist.entry(instr).or_insert(0) += 1;
                }
                if (0x00C0_0000..0x00C8_0000).contains(&instr)
                    || (0x00FD_5600..0x00FE_0300).contains(&instr)
                {
                    push_recent(
                        &mut validator_recent,
                        format!(
                            "frame={frame_num} cck={cck} {} D0=${:08X} D2=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} SP=${:08X}",
                            disassemble_live_line(&amiga, instr),
                            amiga.cpu().regs.d[0],
                            amiga.cpu().regs.d[2],
                            amiga.cpu().regs.a[0],
                            amiga.cpu().regs.a[1],
                            amiga.cpu().regs.a[2],
                            amiga.cpu().regs.a[3],
                            active_sp(&amiga),
                        ),
                        RECENT_CAP,
                    );
                }
            }

            if !saw_setup && source == "Validator" && instr == VALIDATOR_IDCMP_SETUP_ENTRY {
                saw_setup = true;
                let sp = active_sp(&amiga);
                let candidates = scan_stack_candidates(&amiga, sp);
                setup_stack = fmt_stack_candidates(&candidates);
                setup_recent = validator_recent.clone();
                setup_summary = format!(
                    "frame={frame_num} cck={cck} {} {} SR=${:04X} activeSP=${sp:08X}",
                    disassemble_live_line(&amiga, instr),
                    fmt_owner_snapshot(&amiga, owner_root),
                    amiga.cpu().regs.sr,
                );
                for (_, ret_pc) in candidates.iter().take(3) {
                    let start = ret_pc.wrapping_sub(0x10) & !1;
                    let end = ret_pc.wrapping_add(0x10);
                    setup_regions.push(format!("around ${ret_pc:08X}:"));
                    setup_regions.extend(
                        disassemble_live_region(&amiga, start, end)
                            .into_iter()
                            .map(|line| format!("  {line}")),
                    );
                }
            }

            let wait_wrapper_call = instr == VALIDATOR_WAIT_WRAPPER_ROM.wrapping_add(4)
                || instr == VALIDATOR_WAIT_WRAPPER_RAM.wrapping_add(4);
            if !saw_wait
                && source == "Validator"
                && (wait_wrapper_call || matches!(lvo_wait, Some(wait_ep) if pc == wait_ep))
                && amiga.cpu().regs.d[0] == 0x8000_0000
            {
                saw_wait = true;
                let sp = active_sp(&amiga);
                let candidates = scan_stack_candidates(&amiga, sp);
                wait_stack = fmt_stack_candidates(&candidates);
                wait_recent = validator_recent.clone();
                wait_summary = format!(
                    "frame={frame_num} cck={cck} {} mask=${:08X} {} SR=${:04X} activeSP=${sp:08X}",
                    disassemble_live_line(&amiga, instr),
                    amiga.cpu().regs.d[0],
                    fmt_owner_snapshot(&amiga, owner_root),
                    amiga.cpu().regs.sr,
                );
                for (_, ret_pc) in candidates.iter().take(3) {
                    let start = ret_pc.wrapping_sub(0x10) & !1;
                    let end = ret_pc.wrapping_add(0x10);
                    wait_regions.push(format!("around ${ret_pc:08X}:"));
                    wait_regions.extend(
                        disassemble_live_region(&amiga, start, end)
                            .into_iter()
                            .map(|line| format!("  {line}")),
                    );
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }

        if saw_setup && saw_wait && frame_num >= 376 {
            break;
        }
    }

    eprintln!("=== Validator requester RAM chain ===");
    eprintln!(
        "discovery: validator=${:08X} owner=${owner_root:08X} signalPort=${:08X} ignorePort=${:08X}",
        ctx.validator_addr, ctx.signal_port, ctx.ignore_port,
    );

    if !setup_summary.is_empty() {
        eprintln!("\n=== Requester setup trigger ===");
        eprintln!("  {setup_summary}");
        eprintln!("  stack candidates: {setup_stack}");
        for line in &setup_recent {
            eprintln!("  {line}");
        }
        if !setup_regions.is_empty() {
            eprintln!("\n=== Setup caller regions ===");
            for line in &setup_regions {
                eprintln!("  {line}");
            }
        }
    }

    if !wait_summary.is_empty() {
        eprintln!("\n=== Final bit-31 Wait trigger ===");
        eprintln!("  {wait_summary}");
        eprintln!("  stack candidates: {wait_stack}");
        for line in &wait_recent {
            eprintln!("  {line}");
        }
        if !wait_regions.is_empty() {
            eprintln!("\n=== Wait caller regions ===");
            for line in &wait_regions {
                eprintln!("  {line}");
            }
        }
    }

    if !ram_instr_hist.is_empty() {
        let mut hist = ram_instr_hist.into_iter().collect::<Vec<_>>();
        hist.sort_by(|(addr_a, count_a), (addr_b, count_b)| {
            count_b.cmp(count_a).then_with(|| addr_a.cmp(addr_b))
        });
        eprintln!("\n=== Validator RAM instruction histogram ===");
        for (addr, count) in hist.into_iter().take(24) {
            eprintln!("  {count:>3} × {}", disassemble_live_line(&amiga, addr));
        }
    }
}

#[test]
#[ignore]
fn compare_wb13_acknowledged_vs_pending_disk_change() {
    struct ScenarioSummary {
        wait_frame: Option<u64>,
        validator_addr: u32,
        validator_state: u8,
        validator_sig_wait: u32,
        validator_sig_recvd: u32,
        signal_port: u32,
        ignore_port: u32,
        signal_msgs: usize,
        ignore_msgs: usize,
        current_task: String,
        current_pc: u32,
        current_instr: u32,
        disk_change: bool,
        step_events: u32,
    }

    fn run_case(rom: &[u8], adf_bytes: &[u8], pending_change: bool) -> ScenarioSummary {
        let mut amiga = AmigaOcs::with_slow_ram(rom.to_vec(), 512 * 1024);
        let adf = Adf::from_bytes(adf_bytes.to_vec()).expect("decode WB 1.3 ADF");
        if pending_change {
            amiga.insert_adf_with_change_pending(adf);
        } else {
            amiga.insert_adf(adf);
        }

        let mut validator_addr = 0u32;
        let mut wait_frame = None;
        let mut signal_port = 0u32;
        let mut ignore_port = 0u32;

        for frame in 0..700u64 {
            let frame_num = frame + 1;
            for _ in 0..PAL_FRAME_TICKS {
                amiga.tick();
            }
            if validator_addr == 0
                && let Some(found) = find_named_task(&amiga, "Validator")
            {
                validator_addr = found;
            }
            if validator_addr == 0 || wait_frame.is_some() {
                continue;
            }
            let sig_wait = read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT));
            if sig_wait != 0x8000_0000 {
                continue;
            }
            for port in validator_port_addrs(&amiga, validator_addr) {
                match read_byte(&amiga, port.wrapping_add(PORT_FLAGS)) {
                    0 => signal_port = port,
                    2 => ignore_port = port,
                    _ => {}
                }
            }
            wait_frame = Some(frame_num);
        }

        let (validator_state, validator_sig_wait, validator_sig_recvd) = if validator_addr != 0 {
            (
                read_byte(&amiga, validator_addr.wrapping_add(TASK_STATE)),
                read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT)),
                read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_RECVD)),
            )
        } else {
            (0, 0, 0)
        };

        let signal_msgs = if signal_port != 0 {
            list_len(&amiga, signal_port.wrapping_add(PORT_MSG_LIST), 16)
        } else {
            0
        };
        let ignore_msgs = if ignore_port != 0 {
            list_len(&amiga, ignore_port.wrapping_add(PORT_MSG_LIST), 16)
        } else {
            0
        };

        ScenarioSummary {
            wait_frame,
            validator_addr,
            validator_state,
            validator_sig_wait,
            validator_sig_recvd,
            signal_port,
            ignore_port,
            signal_msgs,
            ignore_msgs,
            current_task: task_name(&amiga, current_task_addr(&amiga)),
            current_pc: amiga.cpu().regs.pc,
            current_instr: amiga.cpu().instr_start_pc,
            disk_change: amiga.drive().status().disk_change,
            step_events: amiga.drive().step_event_counter(),
        }
    }

    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let acknowledged = run_case(&rom, &adf_bytes, false);
    let pending = run_case(&rom, &adf_bytes, true);

    eprintln!("=== WB13 disk-change comparison ===");
    for (label, summary) in [("acknowledged", acknowledged), ("pending-change", pending)] {
        eprintln!("\n--- {label} ---");
        eprintln!(
            "  wait_frame={:?} validator=${:08X} state={}({}) sigWait=${:08X} sigRecvd=${:08X}",
            summary.wait_frame,
            summary.validator_addr,
            state_name(summary.validator_state),
            summary.validator_state,
            summary.validator_sig_wait,
            summary.validator_sig_recvd,
        );
        eprintln!(
            "  signalPort=${:08X} ignorePort=${:08X} signalMsgs={} ignoreMsgs={}",
            summary.signal_port, summary.ignore_port, summary.signal_msgs, summary.ignore_msgs,
        );
        eprintln!(
            "  currentTask={} pc=${:08X} instr=${:08X} diskChange={} stepEvents={}",
            summary.current_task,
            summary.current_pc,
            summary.current_instr,
            summary.disk_change,
            summary.step_events,
        );
    }
}

#[test]
#[ignore]
fn trace_wb13_validator_requester_payload_strings() {
    const REQUESTER_PATH_START: u32 = 0x00FD_EDD2;
    const REQUESTER_PATH_END: u32 = 0x00FD_EE92;

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
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut capture = None;

    for frame in 0..390u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            let source = task_name(&amiga, current_task_addr(&amiga));
            let regs = &amiga.cpu().regs;
            if source == "Validator"
                && (REQUESTER_PATH_START..=REQUESTER_PATH_END).contains(&instr)
                && (0x00C0_0000..0x00C8_0000).contains(&regs.d[2])
                && (0x00C0_0000..0x00C8_0000).contains(&regs.a[2])
                && (0x00C0_0000..0x00C8_0000).contains(&regs.a[3])
            {
                capture = Some((
                    frame_num,
                    amiga.cck_count(),
                    instr,
                    regs.d[2],
                    regs.a[2],
                    regs.a[3],
                    regs.a[5],
                    regs.d[4],
                    regs.d[5],
                    regs.d[6],
                ));
                break;
            }

            prev_pc = pc;
            prev_instr = instr;
        }
        if capture.is_some() {
            break;
        }
    }

    let Some((frame_num, cck, instr, d2, a2, a3, a5, d4, d5, d6)) = capture else {
        eprintln!("validator requester payload capture was never reached");
        return;
    };

    let d2_dump_base = d2.wrapping_sub(0x20);
    let a2_dump_base = a2.wrapping_sub(0x20);
    let a5_dump_base = a5.wrapping_sub(0x50);

    let mut d2_strings = scan_string_pointers(&amiga, d2_dump_base, 0x80);
    d2_strings.dedup();
    let mut a2_strings = scan_string_pointers(&amiga, a2_dump_base, 0x80);
    a2_strings.dedup();
    let mut a5_strings = scan_string_pointers(&amiga, a5_dump_base, 0x80);
    a5_strings.dedup();

    eprintln!("=== Validator requester payload strings ===");
    eprintln!(
        "frame={frame_num} cck={cck} {} D4=${d4:08X} D5=${d5:08X} D6=${d6:08X} D2=${d2:08X} A2=${a2:08X} A3=${a3:08X} A5=${a5:08X}",
        disassemble_live_line(&amiga, instr),
    );

    eprintln!("\n=== D2 state block (${d2:08X}) ===");
    for line in dump_long_block(&amiga, d2_dump_base, 16) {
        eprintln!("  {line}");
    }
    if d2_strings.is_empty() {
        eprintln!("  string pointers: <none>");
    } else {
        eprintln!("  string pointers:");
        for line in &d2_strings {
            eprintln!("    {line}");
        }
    }

    eprintln!("\n=== A2 requester block (${a2:08X}) ===");
    for line in dump_long_block(&amiga, a2_dump_base, 16) {
        eprintln!("  {line}");
    }
    if a2_strings.is_empty() {
        eprintln!("  string pointers: <none>");
    } else {
        eprintln!("  string pointers:");
        for line in &a2_strings {
            eprintln!("    {line}");
        }
    }

    eprintln!("\n=== A5 local frame window (${a5:08X}) ===");
    for line in dump_long_block(&amiga, a5_dump_base, 16) {
        eprintln!("  {line}");
    }
    if a5_strings.is_empty() {
        eprintln!("  string pointers: <none>");
    } else {
        eprintln!("  string pointers:");
        for line in &a5_strings {
            eprintln!("    {line}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_disk_write_path_gap() {
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

    let mut validator_addr = 0u32;
    let mut wait_frame = None;
    for frame in 0..390u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
        }
        if validator_addr == 0
            && let Some(found) = find_named_task(&amiga, "Validator")
        {
            validator_addr = found;
        }
        if validator_addr != 0
            && wait_frame.is_none()
            && read_long(&amiga, validator_addr.wrapping_add(TASK_SIG_WAIT)) == 0x8000_0000
        {
            wait_frame = Some(frame_num);
        }
    }

    let mut dmaen_streak = 0u8;
    let mut read_arm_events = Vec::<(u64, u32, u16)>::new();
    let mut write_arm_events = Vec::<(u64, u32, u16)>::new();
    let mut dskdat_writes = Vec::<(u64, u32, u16)>::new();
    for &(cck, pc, reg, val) in &amiga.debug_dsk_log {
        if reg == 0x024 {
            if val & 0x8000 != 0 {
                dmaen_streak += 1;
                if dmaen_streak == 2 {
                    if val & 0x4000 != 0 {
                        write_arm_events.push((cck, pc, val));
                    } else {
                        read_arm_events.push((cck, pc, val));
                    }
                }
            } else {
                dmaen_streak = 0;
            }
        } else if reg == 0x026 {
            dskdat_writes.push((cck, pc, val));
        }
    }

    eprintln!("=== WB13 disk write-path gap ===");
    eprintln!(
        "wait_frame={wait_frame:?} validator=${validator_addr:08X} writeArms={} readArms={} dskdatWrites={}",
        write_arm_events.len(),
        read_arm_events.len(),
        dskdat_writes.len(),
    );
    eprintln!(
        "paula writeDMAWords={} writePIOWords={} driveCapturedWords={}",
        amiga.paula().debug_disk_write_dma_log().len(),
        amiga.paula().debug_disk_write_pio_log().len(),
        amiga.drive().write_mfm_capture().len(),
    );

    if !write_arm_events.is_empty() {
        eprintln!("\n=== Write-mode DSKLEN arm events ===");
        for (cck, pc, val) in &write_arm_events {
            eprintln!("  cck={cck} pc=${pc:08X} DSKLEN=${val:04X}");
        }
    }
    if !dskdat_writes.is_empty() {
        eprintln!("\n=== First DSKDAT writes ===");
        for (cck, pc, val) in dskdat_writes.iter().take(12) {
            eprintln!("  cck={cck} pc=${pc:08X} DSKDAT=${val:04X}");
        }
    }
    if !amiga.paula().debug_disk_write_dma_log().is_empty() {
        eprintln!(
            "\nwrite DMA words: {:?}",
            amiga.paula().debug_disk_write_dma_log()
        );
    }
    if !amiga.paula().debug_disk_write_pio_log().is_empty() {
        eprintln!(
            "\nwrite PIO words: {:?}",
            amiga.paula().debug_disk_write_pio_log()
        );
    }
    if !amiga.drive().write_mfm_capture().is_empty() {
        eprintln!(
            "\ndrive captured write words: {:?}",
            amiga.drive().write_mfm_capture()
        );
    }
}

#[test]
#[ignore]
fn trace_wb13_trackdisk_beginio_requests() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discover = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discover_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discover.insert_adf(discover_adf);
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        discover.tick();
    }
    let exec_base = read_long(&discover, 0x0000_0004);
    let Some(td_base) = find_device(&discover, exec_base, "trackdisk.device") else {
        emu198x_test_skip::skip!("trackdisk.device not found during discovery");
    };
    let beginio_slot = td_base.wrapping_add(LVO_BEGIN_IO as u32);
    if read_word(&discover, beginio_slot) != 0x4EF9 {
        eprintln!("trackdisk BeginIO slot at ${beginio_slot:08X} is not JMP");
        return;
    }
    let beginio = read_long(&discover, beginio_slot.wrapping_add(2));

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut events = Vec::<String>::new();

    for frame in 0..390u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            if pc == beginio {
                let source = task_name(&amiga, current_task_addr(&amiga));
                let io = amiga.cpu().regs.a[1];
                events.push(format!(
                    "frame={frame_num} cck={} src={source} {}",
                    amiga.cck_count(),
                    describe_iorequest(&amiga, io),
                ));
            }

            prev_pc = pc;
            prev_instr = instr;
        }
    }

    eprintln!("=== WB13 trackdisk BeginIO requests ===");
    eprintln!("trackdisk.device=${td_base:08X} BeginIO=${beginio:08X}");
    if events.is_empty() {
        eprintln!("(no BeginIO hits through frame 390)");
    } else {
        for event in &events {
            eprintln!("  {event}");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_root_block_read_compare() {
    const ROOT_BLOCK_OFFSET: usize = 0x0006_E000;

    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discover = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discover_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discover.insert_adf(discover_adf);
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        discover.tick();
    }
    let exec_base = read_long(&discover, 0x0000_0004);
    let Some(td_base) = find_device(&discover, exec_base, "trackdisk.device") else {
        emu198x_test_skip::skip!("trackdisk.device not found during discovery");
    };
    let beginio_slot = td_base.wrapping_add(LVO_BEGIN_IO as u32);
    if read_word(&discover, beginio_slot) != 0x4EF9 {
        eprintln!("trackdisk BeginIO slot at ${beginio_slot:08X} is not JMP");
        return;
    }
    let beginio = read_long(&discover, beginio_slot.wrapping_add(2));

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut prev_instr = amiga.cpu().instr_start_pc;
    let mut root_read = None;

    'search: for frame in 0..320u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().regs.pc;
            let instr = amiga.cpu().instr_start_pc;
            if pc == prev_pc && instr == prev_instr {
                continue;
            }

            if pc == beginio {
                let io = amiga.cpu().regs.a[1];
                let cmd = read_word(&amiga, io.wrapping_add(IO_COMMAND));
                let len = read_long(&amiga, io.wrapping_add(IO_LENGTH));
                let off = read_long(&amiga, io.wrapping_add(IO_OFFSET));
                if cmd == 0x0002 && len == 0x0000_0200 && off == ROOT_BLOCK_OFFSET as u32 {
                    root_read = Some((
                        frame_num,
                        amiga.cck_count(),
                        io,
                        read_long(&amiga, io.wrapping_add(IO_DATA)),
                    ));
                    break 'search;
                }
            }

            prev_pc = pc;
            prev_instr = instr;
        }
    }

    let Some((frame_num, cck, io, data_ptr)) = root_read else {
        eprintln!("root-block CMD_READ was never observed");
        return;
    };

    let mut io_snapshots = Vec::<String>::new();
    let mut prev_snapshot = None;
    for frame in frame_num..=370u64 {
        if frame != frame_num {
            for _ in 0..PAL_FRAME_TICKS {
                amiga.tick();
            }
        }
        let actual = read_long(&amiga, io.wrapping_add(IO_ACTUAL));
        let error = read_byte(&amiga, io.wrapping_add(IO_ERROR)) as i8;
        let flags = read_byte(&amiga, io.wrapping_add(IO_FLAGS));
        let offset = read_long(&amiga, io.wrapping_add(IO_OFFSET));
        let first_long = read_long(&amiga, data_ptr);
        let cyl = amiga.drive().cylinder();
        let head = amiga.drive().head();
        let selected = amiga.drive().selected();
        let spinning = amiga.drive().motor_spinning();
        let prb = amiga.cia_b().port_b_output();
        let snapshot = (
            actual, error, flags, offset, first_long, cyl, head, selected, spinning, prb,
        );
        if prev_snapshot != Some(snapshot) {
            io_snapshots.push(format!(
                "frame={frame} actual=${actual:08X} err={error} flags=${flags:02X} off=${offset:08X} buf[0..4]=${first_long:08X} cyl={cyl} head={head} selected={selected} spinning={spinning} prb=${prb:02X}"
            ));
            prev_snapshot = Some(snapshot);
        }
    }

    let mut mismatch_at = None;
    for i in 0..0x200usize {
        let mem = read_byte(&amiga, data_ptr.wrapping_add(i as u32));
        let adf = adf_bytes[ROOT_BLOCK_OFFSET + i];
        if mem != adf {
            mismatch_at = Some((i, mem, adf));
            break;
        }
    }

    eprintln!("=== WB13 root-block read compare ===");
    eprintln!(
        "frame={frame_num} cck={cck} io=${io:08X} data=${data_ptr:08X} actual=${:08X} err={} flags=${:02X}",
        read_long(&amiga, io.wrapping_add(IO_ACTUAL)),
        read_byte(&amiga, io.wrapping_add(IO_ERROR)) as i8,
        read_byte(&amiga, io.wrapping_add(IO_FLAGS)),
    );
    eprintln!(
        "buffer[0..16]  = {:02X?}",
        (0..16usize)
            .map(|i| read_byte(&amiga, data_ptr.wrapping_add(i as u32)))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "adf[0..16]     = {:02X?}",
        &adf_bytes[ROOT_BLOCK_OFFSET..ROOT_BLOCK_OFFSET + 16]
    );
    if !io_snapshots.is_empty() {
        eprintln!("\n=== IO timeline ===");
        for line in &io_snapshots {
            eprintln!("  {line}");
        }
    }

    match mismatch_at {
        Some((index, mem, adf)) => {
            eprintln!("first mismatch at +${index:03X}: mem=${mem:02X} adf=${adf:02X}");
        }
        None => {
            eprintln!("root-block buffer matches ADF exactly");
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_display_memory() {
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

    let sample_frames = [850u64, 875, 900];
    let mut per_frame = Vec::<String>::new();

    for frame in 1..=900u64 {
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
        }
        if sample_frames.contains(&frame) {
            let a = amiga.agnus();
            let current_bpl = a.bpl_pt;
            per_frame.push(format!(
                "frame={frame} cck={} dmacon=${:04X} bplcon0=${:04X} \
                 cop.pc=${:08X} stopped={} waiting={} dsk_pt=${:08X} \
                 bpl_cur=[${:08X}, ${:08X}, ${:08X}, ${:08X}] cyl={} head={}",
                amiga.cck_count(),
                a.dmacon,
                a.bplcon0,
                amiga.copper().pc,
                amiga.copper().stopped,
                amiga.copper().waiting,
                a.dsk_pt,
                current_bpl[0],
                current_bpl[1],
                current_bpl[2],
                current_bpl[3],
                amiga.drive().cylinder(),
                amiga.drive().head(),
            ));
        }
    }

    let a = amiga.agnus();
    let copper = amiga.copper();
    let cop1 = decode_copper_display_state(&amiga, copper.cop1lc, 64);
    let cop2 = decode_copper_display_state(&amiga, copper.cop2lc, 128);
    let dskpt_written = last_dskpt_write(&amiga);

    eprintln!("=== WB13 Workbench display memory probe ===");
    for line in &per_frame {
        eprintln!("  {line}");
    }
    eprintln!(
        "\ncurrent: dmacon=${:04X} bplcon0=${:04X} bpl1mod={} bpl2mod={} dsk_pt=${:08X}",
        a.dmacon, a.bplcon0, a.bpl1mod, a.bpl2mod, a.dsk_pt
    );
    eprintln!(
        "copper: cop1lc=${:08X} cop2lc=${:08X} pc=${:08X} stopped={} waiting={}",
        copper.cop1lc, copper.cop2lc, copper.pc, copper.stopped, copper.waiting
    );
    eprintln!(
        "COP1 display regs: bplcon0={:?} bpl1mod={:?} bpl2mod={:?} ddf={:?}..{:?} diw={:?}..{:?}",
        cop1.bplcon0,
        cop1.bpl1mod,
        cop1.bpl2mod,
        cop1.ddfstrt,
        cop1.ddfstop,
        cop1.diwstrt,
        cop1.diwstop
    );
    eprintln!(
        "COP2 display regs: bplcon0={:?} bpl1mod={:?} bpl2mod={:?} ddf={:?}..{:?} diw={:?}..{:?}",
        cop2.bplcon0,
        cop2.bpl1mod,
        cop2.bpl2mod,
        cop2.ddfstrt,
        cop2.ddfstop,
        cop2.diwstrt,
        cop2.diwstop
    );

    for plane in 0..4usize {
        eprintln!(
            "  COP2 BPL{}PT = {}",
            plane + 1,
            cop2.bpl_pt[plane]
                .map(|ptr| format!("${ptr:08X}"))
                .unwrap_or_else(|| "<unset>".into())
        );
    }

    match dskpt_written {
        Some((cck, pc, ptr)) => {
            eprintln!("\nlast DSKPT write: cck={cck} pc=${pc:08X} ptr=${ptr:08X}");
            for plane in 0..2usize {
                if let Some(bpl) = cop2.bpl_pt[plane] {
                    let overlaps = ranges_overlap(bpl, 0x2000, ptr, 0x4000);
                    eprintln!(
                        "  overlap(BPL{} ${bpl:08X} len=$2000, DSK ${ptr:08X} len=$4000) = {}",
                        plane + 1,
                        overlaps
                    );
                }
            }
            eprintln!("  DSK bytes[0..32]  = {:02X?}", hex_bytes(&amiga, ptr, 32));
            eprintln!(
                "  DSK sync count in first 2KB = {}",
                count_sync_words(&amiga, ptr, 0x800)
            );
        }
        None => {
            eprintln!("\nlast DSKPT write: <none>");
        }
    }

    for plane in 0..2usize {
        if let Some(ptr) = cop2.bpl_pt[plane] {
            eprintln!(
                "\nBPL{} bytes[0..32] = {:02X?}",
                plane + 1,
                hex_bytes(&amiga, ptr, 32)
            );
            eprintln!(
                "BPL{} sync count in first 2KB = {}",
                plane + 1,
                count_sync_words(&amiga, ptr, 0x800)
            );
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_screen_blits() {
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

    for _ in 0..(900 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let cop2 = decode_copper_display_state(&amiga, amiga.copper().cop2lc, 128);
    let Some(bpl1) = cop2.bpl_pt[0] else {
        eprintln!("COP2 never programmed BPL1PT");
        return;
    };
    let Some(bpl2) = cop2.bpl_pt[1] else {
        eprintln!("COP2 never programmed BPL2PT");
        return;
    };
    let screen_len = 0x3000u32;

    let mut counts = BTreeMap::<(u16, u16), u32>::new();
    let mut examples = Vec::<String>::new();
    let mut overlap_total = 0u32;

    for (cck, pc, c0, c1, apt, bpt, cpt, dpt, size) in &amiga.debug_blit_log {
        let (dlo, dhi) = blit_dest_range(*c1, *dpt, *size);
        let hits_bpl1 = ranges_overlap(dlo, dhi.wrapping_sub(dlo), bpl1, screen_len);
        let hits_bpl2 = ranges_overlap(dlo, dhi.wrapping_sub(dlo), bpl2, screen_len);
        if !hits_bpl1 && !hits_bpl2 {
            continue;
        }
        overlap_total += 1;
        *counts.entry((*c0, *c1)).or_insert(0) += 1;
        if examples.len() < 20 {
            examples.push(format!(
                "cck={cck:>9} pc=${pc:08X} c0=${c0:04X} c1=${c1:04X} size=${size:04X} \
                 apt=${apt:08X} bpt=${bpt:08X} cpt=${cpt:08X} dpt=${dpt:08X} \
                 dest=${dlo:08X}..${dhi:08X} bpl1={} bpl2={} line={} desc={} fill={}",
                hits_bpl1,
                hits_bpl2,
                (c1 & 0x0001) != 0,
                (c1 & 0x0002) != 0,
                (c1 & 0x0018) != 0,
            ));
        }
    }

    eprintln!("=== WB13 Workbench screen blits ===");
    eprintln!(
        "screen ranges: BPL1=${bpl1:08X}..${:08X} BPL2=${bpl2:08X}..${:08X}",
        bpl1.wrapping_add(screen_len),
        bpl2.wrapping_add(screen_len),
    );
    eprintln!("blits overlapping screen ranges: {overlap_total}");
    for example in &examples {
        eprintln!("  {example}");
    }

    eprintln!("\nunique BLTCON0/1 pairs touching screen:");
    let mut sorted = counts.into_iter().collect::<Vec<_>>();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
    for ((c0, c1), count) in sorted {
        eprintln!(
            "  ${c0:04X}/${c1:04X} × {count}  line={} desc={} fill={} useA={} useB={} useC={} useD={}",
            (c1 & 0x0001) != 0,
            (c1 & 0x0002) != 0,
            (c1 & 0x0018) != 0,
            (c0 & 0x0800) != 0,
            (c0 & 0x0400) != 0,
            (c0 & 0x0200) != 0,
            (c0 & 0x0100) != 0,
        );
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_copper_display_build() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discover = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let discover_adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discover.insert_adf(discover_adf);
    for _ in 0..(900 * PAL_FRAME_TICKS) {
        discover.tick();
    }
    let cop2 = discover.copper().cop2lc;
    let bpl1h = find_copper_move_value_addr(&discover, cop2, 0x00E0, 128);
    let bpl1l = find_copper_move_value_addr(&discover, cop2, 0x00E2, 128);
    let bpl2h = find_copper_move_value_addr(&discover, cop2, 0x00E4, 128);
    let bpl2l = find_copper_move_value_addr(&discover, cop2, 0x00E6, 128);
    let bplcon0 = find_copper_move_value_addr(&discover, cop2, 0x0100, 128);

    let watch_points = [bpl1h, bpl1l, bpl2h, bpl2l, bplcon0]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if watch_points.is_empty() {
        eprintln!("failed to locate COP2 display moves");
        return;
    }
    let watch_lo = *watch_points.iter().min().expect("present") & !1;
    let watch_hi = watch_points
        .iter()
        .copied()
        .map(|addr| addr.wrapping_add(2))
        .max()
        .expect("present");

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    amiga.debug_watch_addr = Some((watch_lo, watch_hi.wrapping_sub(watch_lo)));
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);
    for _ in 0..(900 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    eprintln!("=== WB13 Workbench copper display build ===");
    eprintln!("COP2LC=${cop2:08X} watch=${watch_lo:08X}..${watch_hi:08X}");
    eprintln!(
        "slots: BPL1H={:?} BPL1L={:?} BPL2H={:?} BPL2L={:?} BPLCON0={:?}",
        bpl1h.map(|v| format!("${v:08X}")),
        bpl1l.map(|v| format!("${v:08X}")),
        bpl2h.map(|v| format!("${v:08X}")),
        bpl2l.map(|v| format!("${v:08X}")),
        bplcon0.map(|v| format!("${v:08X}")),
    );

    for (cck, pc, addr, val, is_word) in &amiga.debug_watch_writes {
        let label = if Some(*addr) == bpl1h || Some(addr.wrapping_sub(1)) == bpl1h {
            "BPL1PTH"
        } else if Some(*addr) == bpl1l || Some(addr.wrapping_sub(1)) == bpl1l {
            "BPL1PTL"
        } else if Some(*addr) == bpl2h || Some(addr.wrapping_sub(1)) == bpl2h {
            "BPL2PTH"
        } else if Some(*addr) == bpl2l || Some(addr.wrapping_sub(1)) == bpl2l {
            "BPL2PTL"
        } else if Some(*addr) == bplcon0 || Some(addr.wrapping_sub(1)) == bplcon0 {
            "BPLCON0"
        } else {
            "other"
        };
        eprintln!(
            "  cck={cck:>9} pc=${pc:08X} addr=${addr:08X} val=${val:04X} word={} {label}",
            is_word
        );
    }

    if let Some(addr) = bpl1h {
        eprintln!(
            "final BPL1PTH word @ ${addr:08X} = ${:04X}",
            read_word(&amiga, addr)
        );
    }
    if let Some(addr) = bpl1l {
        eprintln!(
            "final BPL1PTL word @ ${addr:08X} = ${:04X}",
            read_word(&amiga, addr)
        );
    }
    if let Some(addr) = bpl2h {
        eprintln!(
            "final BPL2PTH word @ ${addr:08X} = ${:04X}",
            read_word(&amiga, addr)
        );
    }
    if let Some(addr) = bpl2l {
        eprintln!(
            "final BPL2PTL word @ ${addr:08X} = ${:04X}",
            read_word(&amiga, addr)
        );
    }
    if let Some(addr) = bplcon0 {
        eprintln!(
            "final BPLCON0 word @ ${addr:08X} = ${:04X}",
            read_word(&amiga, addr)
        );
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_copper_builder_source_context() {
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

    let mut hits = Vec::<String>::new();

    'outer: for frame in 0..=900u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            let pc = amiga.cpu().instr_start_pc;
            if (0x00FD_171C..=0x00FD_1748).contains(&pc) {
                let regs = amiga.cpu().regs;
                hits.push(format!(
                    "frame={frame_num} cck={} pc=${pc:08X} \
                     A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} \
                     D0=${:08X} D1=${:08X} D2=${:08X} D3=${:08X} D4=${:08X}",
                    amiga.cck_count(),
                    regs.a[0],
                    regs.a[1],
                    regs.a[2],
                    regs.a[3],
                    regs.d[0],
                    regs.d[1],
                    regs.d[2],
                    regs.d[3],
                    regs.d[4],
                ));
                if hits.len() >= 24 {
                    break 'outer;
                }
            }
        }
    }

    if hits.is_empty() {
        eprintln!("never observed FD171C..FD1748");
        return;
    }

    eprintln!("=== WB13 Workbench copper builder source context ===");
    for hit in &hits {
        eprintln!("  {hit}");
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_copper_source_block_writers() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    const WATCH_LO: u32 = 0x00C0_5C60;
    const WATCH_LEN: u32 = 0x0000_0080;
    const WINDOW_START: u64 = 360;
    const WINDOW_END: u64 = 430;

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut last_watch_len = 0usize;
    let mut builder_hits = Vec::<String>::new();
    let mut write_events = Vec::<String>::new();

    for frame in 0..=430u64 {
        let frame_num = frame + 1;
        if frame_num == WINDOW_START {
            amiga.debug_watch_addr = Some((WATCH_LO, WATCH_LEN));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();

            let pc = amiga.cpu().instr_start_pc;
            if (0x00FD_171C..=0x00FD_1748).contains(&pc) && builder_hits.len() < 16 {
                let regs = amiga.cpu().regs;
                builder_hits.push(format!(
                    "frame={frame_num} cck={} pc=${pc:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} D0=${:08X} D1=${:08X}",
                    amiga.cck_count(),
                    regs.a[0],
                    regs.a[1],
                    regs.a[2],
                    regs.a[3],
                    regs.d[0],
                    regs.d[1],
                ));
            }

            while last_watch_len < amiga.debug_watch_writes.len() {
                let (cck, writer_pc, addr, val, is_word) = amiga.debug_watch_writes[last_watch_len];
                write_events.push(format!(
                    "frame={frame_num} cck={cck} addr=${addr:08X} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={} bytes={:02X?}",
                    task_name(&amiga, current_task_addr(&amiga)),
                    hex_bytes(&amiga, addr & !1, 8),
                ));
                last_watch_len += 1;
            }
        }

        if frame_num == WINDOW_END {
            amiga.debug_watch_addr = None;
        }
    }

    eprintln!("=== WB13 Workbench copper source block writers ===");
    eprintln!(
        "watch=${WATCH_LO:08X}..${:08X}",
        WATCH_LO.wrapping_add(WATCH_LEN)
    );

    eprintln!("\nbuilder hits:");
    for hit in &builder_hits {
        eprintln!("  {hit}");
    }

    eprintln!("\nwatch writes:");
    for line in &write_events {
        eprintln!("  {line}");
    }

    eprintln!("\nfinal source block words:");
    for line in dump_word_block(&amiga, WATCH_LO, WATCH_LEN as usize / 2) {
        eprintln!("  {line}");
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_copper_source_slots() {
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

    for _ in 0..(430 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    const SEARCH_BASE: u32 = 0x00C0_5C60;
    const SEARCH_LEN: u32 = 0x0000_0600;
    let needles = [
        (0xF100u16, "BPLCON0"),
        (0xF0E0u16, "BPL1PTH"),
        (0xF0E2u16, "BPL1PTL"),
        (0xF0E4u16, "BPL2PTH"),
        (0xF0E6u16, "BPL2PTL"),
        (0xF108u16, "BPL1MOD"),
        (0xF10Au16, "BPL2MOD"),
        (0xF092u16, "DDFSTRT"),
        (0xF094u16, "DDFSTOP"),
        (0xF08Eu16, "DIWSTRT"),
        (0xF090u16, "DIWSTOP"),
    ];

    eprintln!("=== WB13 Workbench copper source slots ===");
    eprintln!(
        "search=${SEARCH_BASE:08X}..${:08X}",
        SEARCH_BASE.wrapping_add(SEARCH_LEN)
    );
    for (needle, label) in needles {
        let matches = find_word_matches(&amiga, SEARCH_BASE, SEARCH_LEN, needle);
        eprintln!("\n{label} (${needle:04X}) matches:");
        if matches.is_empty() {
            eprintln!("  <none>");
            continue;
        }
        for addr in matches {
            let ctx_base = addr.saturating_sub(6);
            for line in dump_word_block(&amiga, ctx_base, 12) {
                eprintln!("  {line}");
            }
        }
    }
}

#[test]
#[ignore]
fn trace_wb13_workbench_display_slot_writers() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    const WATCH_LO: u32 = 0x00C0_5CE8;
    const WATCH_LEN: u32 = 0x0000_0070;
    const WINDOW_START: u64 = 414;
    const WINDOW_END: u64 = 418;

    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut last_watch_len = 0usize;
    let mut events = Vec::<String>::new();

    for frame in 0..=420u64 {
        let frame_num = frame + 1;
        if frame_num == WINDOW_START {
            amiga.debug_watch_addr = Some((WATCH_LO, WATCH_LEN));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
        }

        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            while last_watch_len < amiga.debug_watch_writes.len() {
                let (cck, writer_pc, addr, val, is_word) = amiga.debug_watch_writes[last_watch_len];
                let label = match addr & !1 {
                    0x00C0_5CE8 => "DIWSTRT.reg",
                    0x00C0_5CEA => "DIWSTRT.val",
                    0x00C0_5CEC => "BPLCON0.reg",
                    0x00C0_5CEE => "BPLCON0.val",
                    0x00C0_5CF0 => "BPLCON0.flags",
                    0x00C0_5CF2 => "BPLCON1.reg",
                    0x00C0_5CF4 => "BPLCON1.val",
                    0x00C0_5CF6 => "BPLCON1.flags",
                    0x00C0_5CF8 => "DIWSTOP.reg",
                    0x00C0_5CFA => "DIWSTOP.val",
                    0x00C0_5CFC => "DIWSTOP.flags",
                    0x00C0_5CFE => "DDFSTRT.reg",
                    0x00C0_5D00 => "DDFSTRT.val",
                    0x00C0_5D02 => "DDFSTRT.flags",
                    0x00C0_5D04 => "DDFSTOP.reg",
                    0x00C0_5D06 => "DDFSTOP.val",
                    0x00C0_5D08 => "DDFSTOP.flags",
                    0x00C0_5D0A => "BPLCON2.reg",
                    0x00C0_5D0C => "BPLCON2.val",
                    0x00C0_5D0E => "BPLCON2.flags",
                    0x00C0_5D10 => "BPL1MOD.reg",
                    0x00C0_5D12 => "BPL1MOD.val",
                    0x00C0_5D14 => "BPL1MOD.flags",
                    0x00C0_5D16 => "BPL2MOD.reg",
                    0x00C0_5D18 => "BPL2MOD.val",
                    0x00C0_5D1A => "BPL2MOD.flags",
                    0x00C0_5D1C => "BPL1PTH.reg",
                    0x00C0_5D1E => "BPL1PTH.val",
                    0x00C0_5D20 => "BPL1PTH.flags",
                    0x00C0_5D22 => "BPL1PTL.reg",
                    0x00C0_5D24 => "BPL1PTL.val",
                    0x00C0_5D26 => "BPL1PTL.flags",
                    0x00C0_5D28 => "BPL2PTH.reg",
                    0x00C0_5D2A => "BPL2PTH.val",
                    0x00C0_5D2C => "BPL2PTH.flags",
                    0x00C0_5D2E => "BPL2PTL.reg",
                    0x00C0_5D30 => "BPL2PTL.val",
                    0x00C0_5D32 => "BPL2PTL.flags",
                    0x00C0_5D3A => "mode.alt.pre",
                    0x00C0_5D3C => "mode.alt.reg",
                    0x00C0_5D3E => "mode.alt.val",
                    0x00C0_5D40 => "mode.alt.flags",
                    0x00C0_5D44 => "mode.post.pre",
                    0x00C0_5D46 => "mode.post.reg",
                    0x00C0_5D48 => "mode.post.val",
                    0x00C0_5D4A => "mode.post.flags",
                    _ => "other",
                };
                events.push(format!(
                    "frame={frame_num} cck={cck} addr=${addr:08X} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={} {label}",
                    task_name(&amiga, current_task_addr(&amiga)),
                ));
                last_watch_len += 1;
            }
        }

        if frame_num == WINDOW_END {
            amiga.debug_watch_addr = None;
        }
    }

    eprintln!("=== WB13 Workbench display slot writers ===");
    eprintln!(
        "watch=${WATCH_LO:08X}..${:08X}",
        WATCH_LO.wrapping_add(WATCH_LEN)
    );
    for line in &events {
        eprintln!("  {line}");
    }

    eprintln!("\nfinal slot block:");
    for line in dump_word_block(&amiga, WATCH_LO, WATCH_LEN as usize / 2) {
        eprintln!("  {line}");
    }
}

#[test]
#[ignore]
fn trace_wb13_display_mode_source_field() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        emu198x_test_skip::skip!("Amiga Workbench 1.3 trace artifacts not staged");
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        emu198x_test_skip::skip!("not staged: ~/.emu198x/media/commodore-amiga/workbench-1.3.adf");
    };

    let mut discover = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes.clone()).expect("decode WB 1.3 ADF");
    discover.insert_adf(adf);

    let mut source_ptr = None;
    let mut source_info = Vec::<String>::new();
    'discover: for frame in 0..=430u64 {
        let frame_num = frame + 1;
        for _ in 0..PAL_FRAME_TICKS {
            discover.tick();
            let pc = discover.cpu().instr_start_pc;
            if pc == 0x00FC_C9A2 || pc == 0x00FC_CFA8 {
                let regs = discover.cpu().regs;
                let ptr = regs.a[4];
                source_ptr = Some(ptr);
                source_info.push(format!(
                    "frame={frame_num} cck={} pc=${pc:08X} task={} A2=${:08X} A3=${:08X} A4=${:08X} A5=${:08X} mode_word=${:04X} a3+10=${:04X} a2+20=${:04X}",
                    discover.cck_count(),
                    task_name(&discover, current_task_addr(&discover)),
                    regs.a[2],
                    regs.a[3],
                    regs.a[4],
                    regs.a[5],
                    read_word(&discover, ptr.wrapping_add(0xA4)),
                    read_word(&discover, regs.a[3].wrapping_add(0x10)),
                    read_word(&discover, regs.a[2].wrapping_add(0x20)),
                ));
                if source_info.len() >= 2 {
                    break 'discover;
                }
            }
        }
    }

    let Some(source_ptr) = source_ptr else {
        eprintln!("never observed FCC9A2/FCCFA8");
        return;
    };

    let watch_lo = source_ptr.wrapping_add(0xA0);
    let watch_len = 8u32;
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    amiga.insert_adf(adf);

    let mut events = Vec::<String>::new();
    let mut last_watch_len = 0usize;
    for frame in 0..=430u64 {
        let frame_num = frame + 1;
        if frame_num == 1 {
            amiga.debug_watch_addr = Some((watch_lo, watch_len));
            amiga.debug_watch_writes.clear();
            last_watch_len = 0;
        }
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
            while last_watch_len < amiga.debug_watch_writes.len() {
                let (cck, writer_pc, addr, val, is_word) = amiga.debug_watch_writes[last_watch_len];
                if is_word
                    && ((addr & !1) == watch_lo.wrapping_add(4)
                        || (addr & !1) == watch_lo.wrapping_add(6))
                {
                    events.push(format!(
                        "frame={frame_num} cck={cck} addr=${addr:08X} val=${val:04X} word={is_word} writer_pc=${writer_pc:08X} writer_task={}",
                        task_name(&amiga, current_task_addr(&amiga)),
                    ));
                }
                last_watch_len += 1;
            }
        }
    }

    eprintln!("=== WB13 display-mode source field ===");
    eprintln!(
        "source_ptr=${source_ptr:08X} watch=${watch_lo:08X}..${:08X}",
        watch_lo.wrapping_add(watch_len)
    );
    eprintln!("hits:");
    for line in &source_info {
        eprintln!("  {line}");
    }
    eprintln!("\nwatch writes:");
    for line in &events {
        eprintln!("  {line}");
    }
    eprintln!(
        "\nfinal words @ source+0xA0: {:04X} {:04X} {:04X} {:04X}",
        read_word(&amiga, watch_lo),
        read_word(&amiga, watch_lo.wrapping_add(2)),
        read_word(&amiga, watch_lo.wrapping_add(4)),
        read_word(&amiga, watch_lo.wrapping_add(6)),
    );
}

#[test]
#[ignore]
fn trace_wb13_display_custom_byte_writes() {
    fn reg_name(offset: u16) -> &'static str {
        match offset {
            0x08E => "DIWSTRT",
            0x090 => "DIWSTOP",
            0x092 => "DDFSTRT",
            0x094 => "DDFSTOP",
            0x100 => "BPLCON0",
            0x102 => "BPLCON1",
            0x104 => "BPLCON2",
            0x108 => "BPL1MOD",
            0x10A => "BPL2MOD",
            0x0E0 => "BPL1PTH",
            0x0E2 => "BPL1PTL",
            0x0E4 => "BPL2PTH",
            0x0E6 => "BPL2PTL",
            0x0E8 => "BPL3PTH",
            0x0EA => "BPL3PTL",
            0x0EC => "BPL4PTH",
            0x0EE => "BPL4PTL",
            0x0F0 => "BPL5PTH",
            0x0F2 => "BPL5PTL",
            0x0F4 => "BPL6PTH",
            0x0F6 => "BPL6PTL",
            _ => "other",
        }
    }

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

    for _ in 0..(430u64 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let cck_per_frame = PAL_FRAME_TICKS / 2;
    let mut events = Vec::<String>::new();
    for (cck, pc, addr24, offset, raw_val, is_word) in &amiga.debug_custom_write_log {
        if *is_word {
            continue;
        }
        if !matches!(
            *offset,
            0x08E | 0x090 | 0x092 | 0x094 | 0x100 | 0x102 | 0x104 | 0x108 | 0x10A | 0x0E0..=0x0F6
        ) {
            continue;
        }
        let frame = cck / cck_per_frame + 1;
        let lane = if addr24 & 1 == 0 {
            "even/UDS"
        } else {
            "odd/LDS"
        };
        events.push(format!(
            "frame={frame} cck={cck} pc=${pc:08X} addr=${addr24:06X} offset=${offset:03X} {} raw=${raw_val:04X} reg={}",
            lane,
            reg_name(*offset),
        ));
    }

    eprintln!("=== WB13 display custom byte writes ===");
    if events.is_empty() {
        eprintln!("  no byte writes to display custom registers observed through frame 430");
    } else {
        for line in &events {
            eprintln!("  {line}");
        }
    }
    eprintln!(
        "\nfinal registers: BPLCON0=${:04X} BPLCON1=${:04X} BPLCON2=${:04X} BPL1MOD=${:04X} BPL2MOD=${:04X}",
        amiga.bplcon0(),
        amiga.denise().ocs.bplcon1,
        amiga.denise().ocs.bplcon2,
        amiga.agnus().bpl1mod as u16,
        amiga.agnus().bpl2mod as u16,
    );
}

#[test]
#[ignore]
fn trace_wb13_copper_bplcon0_mode_writes() {
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

    for _ in 0..(430u64 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let cck_per_frame = PAL_FRAME_TICKS / 2;
    let mut value_counts = std::collections::BTreeMap::<u16, usize>::new();
    let mut unique_positions = std::collections::BTreeSet::<(u16, u16, u16)>::new();
    let mut tail = Vec::<String>::new();

    for (cck, vpos, hpos, reg, val) in &amiga.debug_copper_move_log {
        if *reg != 0x0100 {
            continue;
        }
        *value_counts.entry(*val).or_insert(0) += 1;
        unique_positions.insert((*vpos, *hpos, *val));
        let frame = cck / cck_per_frame + 1;
        if frame >= 360 {
            tail.push(format!(
                "frame={frame} cck={cck} vpos=${vpos:03X} hpos=${hpos:03X} BPLCON0=${val:04X}"
            ));
        }
    }

    eprintln!("=== WB13 copper BPLCON0 mode writes ===");
    eprintln!("value counts:");
    for (val, count) in &value_counts {
        eprintln!("  ${val:04X}: {count}");
    }

    eprintln!("\nunique beam positions:");
    for (vpos, hpos, val) in &unique_positions {
        eprintln!("  vpos=${vpos:03X} hpos=${hpos:03X} -> ${val:04X}");
    }

    eprintln!("\nlate-window writes:");
    for line in &tail {
        eprintln!("  {line}");
    }
}

#[test]
#[ignore]
fn trace_wb13_hires_line_fetch_cadence() {
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

    let cck_per_frame = PAL_FRAME_TICKS / 2;
    let mut collecting = false;
    let mut frame_hit = 0u64;
    let mut fetches = Vec::<String>::new();
    let mut slot_counts = BTreeMap::<&'static str, usize>::new();

    for _ in 0..(430u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        if amiga.tick_count() & 1 == 0 {
            continue;
        }

        let cck = amiga.cck_count();
        let frame = cck / cck_per_frame + 1;
        let vpos = amiga.agnus().vpos;
        let hpos = amiga.agnus().hpos;
        let bplcon0 = amiga.bplcon0();

        if !collecting {
            if frame >= 360 && vpos == 0x002C && hpos == 0x0003 && bplcon0 == 0xA302 {
                collecting = true;
                frame_hit = frame;
            } else {
                continue;
            }
        }

        if vpos != 0x002C {
            break;
        }

        let slot = amiga.agnus().current_slot();
        let slot_name = match slot {
            SlotOwner::Bitplane(0) => "BPL1",
            SlotOwner::Bitplane(1) => "BPL2",
            SlotOwner::Bitplane(2) => "BPL3",
            SlotOwner::Bitplane(3) => "BPL4",
            SlotOwner::Bitplane(4) => "BPL5",
            SlotOwner::Bitplane(5) => "BPL6",
            SlotOwner::Bitplane(_) => "BPLX",
            SlotOwner::Copper => "Copper",
            SlotOwner::Cpu => "CPU",
            SlotOwner::Disk => "Disk",
            SlotOwner::Refresh => "Refresh",
            SlotOwner::Audio(_) => "Audio",
            SlotOwner::Sprite(_) => "Sprite",
        };
        *slot_counts.entry(slot_name).or_insert(0) += 1;

        if let SlotOwner::Bitplane(plane) = slot {
            fetches.push(format!(
                "frame={frame} cck={cck} hpos=${hpos:03X} bplcon0=${bplcon0:04X} fetch=BPL{}",
                plane + 1,
            ));
        }
    }

    eprintln!("=== WB13 hires line fetch cadence ===");
    eprintln!("first hires line hit at frame {frame_hit}");
    eprintln!("slot counts on vpos $02C:");
    for (name, count) in &slot_counts {
        eprintln!("  {name}: {count}");
    }

    eprintln!("\nbitplane fetches on that line:");
    for line in &fetches {
        eprintln!("  {line}");
    }
}

#[test]
#[ignore]
fn trace_wb13_hires_line_actual_bitplane_fetches() {
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

    let cck_per_frame = PAL_FRAME_TICKS / 2;
    let mut started = false;
    let mut frame_hit = 0u64;
    let mut start_bpl1 = 0u32;
    let mut start_bpl2 = 0u32;
    let mut prev_bpl1 = 0u32;
    let mut prev_bpl2 = 0u32;
    let mut bpl1_fetches = Vec::<String>::new();
    let mut bpl2_fetches = Vec::<String>::new();

    for _ in 0..(430u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        if amiga.tick_count() & 1 == 0 {
            continue;
        }

        let cck = amiga.cck_count();
        let frame = cck / cck_per_frame + 1;
        let vpos = amiga.agnus().vpos;
        let hpos = amiga.agnus().hpos;
        let bplcon0 = amiga.bplcon0();
        let bpl1 = amiga.agnus().bpl_pt[0];
        let bpl2 = amiga.agnus().bpl_pt[1];

        if !started {
            if frame >= 360 && vpos == 0x002C && hpos == 0x0003 && bplcon0 == 0xA302 {
                started = true;
                frame_hit = frame;
                start_bpl1 = bpl1;
                start_bpl2 = bpl2;
                prev_bpl1 = bpl1;
                prev_bpl2 = bpl2;
            }
            continue;
        }

        if vpos != 0x002C {
            break;
        }

        if bpl1 != prev_bpl1 {
            bpl1_fetches.push(format!(
                "frame={frame} cck={cck} hpos=${hpos:03X} bplcon0=${bplcon0:04X} BPL1 ${prev_bpl1:08X}->${bpl1:08X}"
            ));
            prev_bpl1 = bpl1;
        }
        if bpl2 != prev_bpl2 {
            bpl2_fetches.push(format!(
                "frame={frame} cck={cck} hpos=${hpos:03X} bplcon0=${bplcon0:04X} BPL2 ${prev_bpl2:08X}->${bpl2:08X}"
            ));
            prev_bpl2 = bpl2;
        }
    }

    eprintln!("=== WB13 hires line actual bitplane fetches ===");
    eprintln!("first hires line hit at frame {frame_hit}");
    eprintln!(
        "BPL1 start=${start_bpl1:08X} end=${prev_bpl1:08X} delta_bytes={}",
        prev_bpl1.wrapping_sub(start_bpl1)
    );
    eprintln!(
        "BPL2 start=${start_bpl2:08X} end=${prev_bpl2:08X} delta_bytes={}",
        prev_bpl2.wrapping_sub(start_bpl2)
    );

    eprintln!("\nBPL1 pointer advances:");
    for line in &bpl1_fetches {
        eprintln!("  {line}");
    }

    eprintln!("\nBPL2 pointer advances:");
    for line in &bpl2_fetches {
        eprintln!("  {line}");
    }
}
