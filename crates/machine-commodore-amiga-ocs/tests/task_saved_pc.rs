//! Read the saved PC of tasks that are currently blocked in Wait,
//! so we can see exactly where in the ROM they're parked.
//!
//! Exec's dispatcher, when a task blocks, saves its register set on
//! the task's stack via something close to:
//!
//!   MOVEM.L D0-D7/A0-A6, -(SP)   ; 15 longs = 60 bytes
//!
//! Then stores SP in tc_SPReg (offset 54 in the Task struct). The
//! return address of whatever called into the dispatcher (typically
//! inside Wait / WaitIO / etc) sits on the stack ABOVE the MOVEM
//! block. Reading the first plausible long after the 60-byte MOVEM
//! slice usually gives the saved PC (= the instruction the task
//! will resume at).
//!
//! We don't know V34's exact stack layout up front, so we dump the
//! first 20 longs starting at tc_SPReg so we can inspect them and
//! spot ROM addresses ($00FCxxxx..$00FEFFFF).

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const EXEC_TASK_WAIT: u32 = 420;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;
const TASK_STATE: u32 = 15;
const TASK_SIG_WAIT: u32 = 22;
const TASK_SIG_RECVD: u32 = 26;
const TASK_SP_REG: u32 = 54;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
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

fn is_rom_addr(addr: u32) -> bool {
    (0x00FC_0000..0x0100_0000).contains(&addr)
}

fn dump_task(amiga: &AmigaOcs, label: &str, task_addr: u32) {
    if task_addr == 0 {
        return;
    }
    let name = read_cstring(amiga, read_long(amiga, task_addr.wrapping_add(LN_NAME)), 32);
    let state = read_byte(amiga, task_addr.wrapping_add(TASK_STATE));
    let sig_wait = read_long(amiga, task_addr.wrapping_add(TASK_SIG_WAIT));
    let sig_recvd = read_long(amiga, task_addr.wrapping_add(TASK_SIG_RECVD));
    let sp = read_long(amiga, task_addr.wrapping_add(TASK_SP_REG));

    eprintln!("\n=== {label} ===");
    eprintln!(
        "task @ ${task_addr:08X}  name={name}  state={state}  \
         sigWait=${sig_wait:08X}  sigRecvd=${sig_recvd:08X}"
    );
    eprintln!("tc_SPReg = ${sp:08X}");

    if sp == 0 {
        eprintln!("(SP is 0 — task hasn't been suspended)");
        return;
    }

    eprintln!("\nFirst 24 longs at tc_SPReg:");
    for i in 0..24 {
        let a = sp.wrapping_add((i as u32) * 4);
        let v = read_long(amiga, a);
        let tag = if is_rom_addr(v) {
            "  ← ROM addr (candidate saved PC)"
        } else {
            ""
        };
        eprintln!("  +{:3}  ${a:08X}: ${v:08X}{tag}", i * 4);
    }

    // Exec's 15-long MOVEM save: tc_SPReg + 60 should be the top
    // of the pre-MOVEM stack, i.e. the first long the CPU pushed
    // itself (typically a return address from Wait → task code).
    let candidate = read_long(amiga, sp.wrapping_add(60));
    eprintln!(
        "\nHeuristic saved PC = long at tc_SPReg+60 = ${candidate:08X}{}",
        if is_rom_addr(candidate) {
            "  (looks like ROM — likely where task is parked)"
        } else {
            "  (not in ROM — layout assumption may be off)"
        }
    );
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    let frames: u64 = std::env::var("FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(250);
    for _ in 0..(frames * PAL_FRAME_TICKS) {
        amiga.tick();
    }
    eprintln!("(ran {frames} frames)");

    let exec_base = read_long(amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");

    // Walk TaskWait list.
    let list_addr = exec_base.wrapping_add(EXEC_TASK_WAIT);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut node = head;
    for _ in 0..8 {
        if node == 0 || node == tail_sentinel {
            break;
        }
        let name = read_cstring(amiga, read_long(amiga, node.wrapping_add(LN_NAME)), 32);
        dump_task(amiga, &name, node);
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
    }
}

#[test]
#[ignore]
fn dump_waiting_task_saved_pcs() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
