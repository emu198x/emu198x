//! Snapshot boot state after ~300 frames to characterise where the
//! boot has settled. Tells us:
//!   - What the CPU is doing (PC, SR, tick_count)
//!   - Chipset registers (DMACON, INTENA, INTREQ, BPLCON0, colors)
//!   - Framebuffer content (background-only vs bitplane graphics)
//!   - TaskReady head vs lh_Tail (is the ready list empty?)
//!   - Key ExecBase fields (IdleCount, TDNestCnt, ThisTask)

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

const EXEC_THIS_TASK: u32 = 276;
const EXEC_IDLE_COUNT: u32 = 280;
const EXEC_DISP_COUNT: u32 = 284;
const EXEC_QUANTUM: u32 = 288;
const EXEC_SYS_FLAGS: u32 = 292;
const EXEC_TD_NEST_CNT: u32 = 295;
const EXEC_TASK_READY: u32 = 406;
const EXEC_TASK_WAIT: u32 = 420;
/// Task struct field offsets (exec/tasks.h).
const TASK_LN_SUCC: u32 = 0;
const TASK_LN_NAME: u32 = 10;
/// tc_State (BYTE, offset 0x0F inside task = 15 = after the Node).
/// Values: 0 = INVALID, 1 = ADDED, 2 = RUN, 3 = READY, 4 = WAIT,
///         5 = EXCEPT, 6 = REMOVED.
const TASK_STATE: u32 = 15;
/// tc_SigWait (ULONG) — which signals this task is waiting on.
const TASK_SIG_WAIT: u32 = 22;
/// tc_SigRecvd (ULONG) — signals received but not yet consumed.
const TASK_SIG_RECVD: u32 = 26;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn chip_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    let b0 = u32::from(amiga.read_chip_ram_byte(addr));
    let b1 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(1)));
    let b2 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(2)));
    let b3 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(3)));
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

fn read_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    amiga.read_long(addr)
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

fn dump_task_list(amiga: &AmigaOcs, label: &str, list_addr: u32) {
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    eprintln!(
        "\n=== {label} List @ ${list_addr:08X} ===\n\
         head = ${head:08X}  (tail-sentinel = ${tail_sentinel:08X})"
    );
    if head == tail_sentinel || head == 0 {
        eprintln!("→ LIST IS EMPTY");
        return;
    }
    let mut node = head;
    for i in 0..8 {
        if node == 0 || node == tail_sentinel {
            break;
        }
        let succ = read_long(amiga, node.wrapping_add(TASK_LN_SUCC));
        let name_ptr = read_long(amiga, node.wrapping_add(TASK_LN_NAME));
        let name = read_cstring(amiga, name_ptr, 32);
        let state = read_byte(amiga, node.wrapping_add(TASK_STATE));
        let sig_wait = read_long(amiga, node.wrapping_add(TASK_SIG_WAIT));
        let sig_recvd = read_long(amiga, node.wrapping_add(TASK_SIG_RECVD));
        let state_name = match state {
            0 => "INVALID",
            1 => "ADDED",
            2 => "RUN",
            3 => "READY",
            4 => "WAIT",
            5 => "EXCEPT",
            6 => "REMOVED",
            _ => "???",
        };
        eprintln!(
            "  [{i}] ${node:08X} \"{name}\" state={state_name}({state}) \
             sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X}",
        );
        node = succ;
    }
}

fn snapshot(amiga: &AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");
    let pc = amiga.cpu().regs.pc;
    let sr = amiga.cpu().regs.sr;

    eprintln!("\n=== CPU ===");
    eprintln!("PC     = ${pc:08X}");
    eprintln!("SR     = ${sr:04X}");
    eprintln!("ticks  = {}", amiga.tick_count());
    eprintln!("ccks   = {}", amiga.cck_count());

    eprintln!("\n=== Chipset ===");
    eprintln!("DMACON  = ${:04X}", amiga.dmacon());
    eprintln!("INTENA  = ${:04X}", amiga.intena());
    eprintln!("INTREQ  = ${:04X}", amiga.intreq());
    eprintln!("BPLCON0 = ${:04X}", amiga.bplcon0());
    for i in 0..8 {
        eprintln!("COLOR{i:02} = ${:04X}", amiga.color(i));
    }

    let copper = amiga.copper();
    eprintln!("\n=== Copper ===");
    eprintln!("COP1LC  = ${:08X}", copper.cop1lc);
    eprintln!("COP2LC  = ${:08X}", copper.cop2lc);
    eprintln!("PC      = ${:08X}", copper.pc);
    eprintln!("waiting = {}", copper.waiting);
    if copper.cop1lc != 0 {
        eprintln!("\n-- first 8 copper instructions at COP1LC --");
        let mut addr = copper.cop1lc;
        for _ in 0..8 {
            let word1 = (u16::from(amiga.read_chip_ram_byte(addr)) << 8)
                | u16::from(amiga.read_chip_ram_byte(addr + 1));
            let word2 = (u16::from(amiga.read_chip_ram_byte(addr + 2)) << 8)
                | u16::from(amiga.read_chip_ram_byte(addr + 3));
            let kind = if word1 & 1 == 0 {
                format!("MOVE reg=${:03X} val=${word2:04X}", word1 & 0x1FE)
            } else if word2 & 1 == 0 {
                format!("WAIT v=${:02X} h=${:02X} mask=${word2:04X}",
                    word1 >> 8, word1 & 0xFE)
            } else {
                format!("SKIP v=${:02X} h=${:02X} mask=${word2:04X}",
                    word1 >> 8, word1 & 0xFE)
            };
            eprintln!("  ${addr:08X}: ${word1:04X} ${word2:04X}  {kind}");
            if word1 == 0xFFFF && word2 == 0xFFFE {
                eprintln!("  (end-of-list sentinel)");
                break;
            }
            addr = addr.wrapping_add(4);
        }
    }

    eprintln!("\n=== Bitplane pointers ===");
    for i in 0..6 {
        let bpl = amiga.read_long(0x00DF_F0E0 + (i as u32) * 4);
        // Note: these are read-side of chipset registers which aren't
        // mapped for BPLxPT. Let me expose them directly.
        let _ = bpl;
    }

    eprintln!("\n=== ExecBase ===");
    let exec_base = chip_long(&amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");
    if exec_base == 0 {
        eprintln!("(ExecBase uninitialised — can't walk)");
        return;
    }
    let this_task = read_long(&amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
    let idle = read_long(&amiga, exec_base.wrapping_add(EXEC_IDLE_COUNT));
    let disp = read_long(&amiga, exec_base.wrapping_add(EXEC_DISP_COUNT));
    let quantum = amiga.read_word(exec_base.wrapping_add(EXEC_QUANTUM));
    let sys_flags = amiga.read_word(exec_base.wrapping_add(EXEC_SYS_FLAGS));
    let td_nest = u32::from(amiga.read_word(exec_base.wrapping_add(EXEC_TD_NEST_CNT)));
    eprintln!("ThisTask = ${this_task:08X}");
    eprintln!("IdleCount = {idle}  (scheduler idle ticks)");
    eprintln!("DispCount = {disp}  (task dispatch count)");
    eprintln!("Quantum   = {quantum}");
    eprintln!("SysFlags  = ${sys_flags:04X}");
    eprintln!("TDNestCnt = ${td_nest:02X}  (task-disable nest)");

    // Also dump ThisTask details to see who was just running.
    if this_task != 0 {
        let name_ptr = read_long(&amiga, this_task.wrapping_add(TASK_LN_NAME));
        let name = read_cstring(&amiga, name_ptr, 32);
        let state = read_byte(&amiga, this_task.wrapping_add(TASK_STATE));
        let sig_wait = read_long(&amiga, this_task.wrapping_add(TASK_SIG_WAIT));
        let sig_recvd = read_long(&amiga, this_task.wrapping_add(TASK_SIG_RECVD));
        eprintln!(
            "\n=== ThisTask @ ${this_task:08X} ===\n\
             name='{name}' state={state} sigWait=${sig_wait:08X} sigRecvd=${sig_recvd:08X}"
        );
    }

    dump_task_list(&amiga, "TaskReady", exec_base.wrapping_add(EXEC_TASK_READY));
    dump_task_list(&amiga, "TaskWait", exec_base.wrapping_add(EXEC_TASK_WAIT));

    eprintln!("\n=== Framebuffer ===");
    let fb = amiga.denise().framebuffer();
    let (w, h) = amiga.denise().framebuffer_size();
    let mut distinct: std::collections::BTreeMap<u32, u32> =
        std::collections::BTreeMap::new();
    for &px in fb.iter() {
        *distinct.entry(px).or_insert(0) += 1;
    }
    let total = w * h;
    eprintln!("framebuffer = {w}×{h} = {total} pixels");
    eprintln!("distinct colours: {}", distinct.len());
    for (c, n) in distinct.iter().take(8) {
        let pct = (*n as f64 / total as f64) * 100.0;
        eprintln!("  ${c:08X}: {n:7} px ({pct:5.1}%)");
    }
}

#[test]
#[ignore]
fn snapshot_boot_state_at_frame_300() {
    let Some(rom) = load_kickstart() else { return };

    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let mut chip_only = AmigaOcs::new(rom);

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        slow.tick();
        chip_only.tick();
    }

    snapshot(&slow, "slow-RAM (512K chip + 512K slow)");
    snapshot(&chip_only, "chip-only (512K chip)");
}
