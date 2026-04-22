//! Trap intuition.library's screen / window / alert LVOs during
//! the fresh OCS boot to see whether *anything* is trying to
//! bring up a display.
//!
//! msg_port_trap showed DoIO / SendIO / PutMsg are all 0 in both
//! configs, so the disk path isn't driving the stall. The next
//! candidate is Intuition itself: if OpenScreen is never called
//! (by the ROM's "no boot" routine, by intuition.library's own
//! init, or by anyone else), we know the Intuition setup chain
//! is dormant.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::collections::BTreeMap;
use std::path::PathBuf;

const EXEC_THIS_TASK: u32 = 276;
const EXEC_LIB_LIST: u32 = 378;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;

// intuition.library V34 LVOs (negative from intuition base).
const LVO_DISPLAY_ALERT: i32 = -90;
const LVO_DISPLAY_BEEP: i32 = -96;
const LVO_OPEN_SCREEN: i32 = -198;
const LVO_OPEN_WINDOW: i32 = -204;
const LVO_PRINT_ITEXT: i32 = -216;
const LVO_VIEW_ADDRESS: i32 = -294;
const LVO_VIEW_PORT_ADDRESS: i32 = -300;
const LVO_AUTO_REQUEST: i32 = -348;
const LVO_INIT_REQUESTER: i32 = -138;

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

fn find_library(amiga: &AmigaOcs, exec_base: u32, target: &str) -> Option<u32> {
    let list_addr = exec_base.wrapping_add(EXEC_LIB_LIST);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut node = head;
    for _ in 0..16 {
        if node == 0 || node == tail_sentinel {
            return None;
        }
        let name_ptr = read_long(amiga, node.wrapping_add(LN_NAME));
        if read_cstring(amiga, name_ptr, 32) == target {
            return Some(node);
        }
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
    }
    None
}

fn resolve_lvo(amiga: &AmigaOcs, base: u32, lvo: i32) -> Option<u32> {
    let slot = (base as i64 + lvo as i64) as u32;
    let opcode = amiga.read_word(slot);
    if opcode != 0x4EF9 {
        return None;
    }
    Some(read_long(amiga, slot.wrapping_add(2)))
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    for _ in 0..(200 * PAL_FRAME_TICKS) {
        amiga.tick();
    }
    let exec_base = read_long(amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");

    let Some(int_base) = find_library(amiga, exec_base, "intuition.library") else {
        eprintln!("intuition.library not in LibList — abort");
        return;
    };
    eprintln!("intuition.library base = ${int_base:08X}");

    let targets = [
        (
            "DisplayAlert  ",
            resolve_lvo(amiga, int_base, LVO_DISPLAY_ALERT),
        ),
        (
            "DisplayBeep   ",
            resolve_lvo(amiga, int_base, LVO_DISPLAY_BEEP),
        ),
        (
            "OpenScreen    ",
            resolve_lvo(amiga, int_base, LVO_OPEN_SCREEN),
        ),
        (
            "OpenWindow    ",
            resolve_lvo(amiga, int_base, LVO_OPEN_WINDOW),
        ),
        (
            "PrintIText    ",
            resolve_lvo(amiga, int_base, LVO_PRINT_ITEXT),
        ),
        (
            "ViewAddress   ",
            resolve_lvo(amiga, int_base, LVO_VIEW_ADDRESS),
        ),
        (
            "ViewPortAddress",
            resolve_lvo(amiga, int_base, LVO_VIEW_PORT_ADDRESS),
        ),
        (
            "AutoRequest   ",
            resolve_lvo(amiga, int_base, LVO_AUTO_REQUEST),
        ),
        (
            "InitRequester ",
            resolve_lvo(amiga, int_base, LVO_INIT_REQUESTER),
        ),
    ];

    eprintln!("\n=== LVO entry points ===");
    for (name, ep) in &targets {
        match ep {
            Some(ep) => eprintln!("  {name} = ${ep:08X}"),
            None => eprintln!("  {name} = (not resolved)"),
        }
    }

    let mut counts: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut events: Vec<String> = Vec::new();
    let mut prev_pc = amiga.cpu().regs.pc;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        for (name, ep) in &targets {
            if let Some(ep) = ep
                && pc == *ep
            {
                *counts.entry(*name).or_insert(0) += 1;
                if events.len() < 40 {
                    let this_task = read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
                    let src = task_name(amiga, this_task);
                    events.push(format!("{name} src={src}"));
                }
            }
        }
        prev_pc = pc;
    }

    eprintln!("\n=== Call counts (400 frames phase 2) ===");
    for (name, _) in &targets {
        let c = counts.get(name).copied().unwrap_or(0);
        eprintln!("  {name} = {c}");
    }

    if !events.is_empty() {
        eprintln!("\n=== First {} events ===", events.len());
        for e in &events {
            eprintln!("  {e}");
        }
    }

    eprintln!("\n=== Interpretation ===");
    let any = counts.values().sum::<u64>();
    if any == 0 {
        eprintln!(
            "• Zero Intuition activity across 400 frames. The entire\n  \
            Intuition screen/window/alert chain is dormant — no caller\n  \
            is trying to bring up a display. The stall is UPSTREAM of\n  \
            Intuition. Either:\n  \
              (a) Intuition's own init never ran its screen-creation\n  \
                  path (so opening the insert-disk screen as part of\n  \
                  InitCode is the missing step), or\n  \
              (b) the ROM has a separate 'no-boot' routine that would\n  \
                  call Intuition but is itself blocked."
        );
    }
}

#[test]
#[ignore]
fn trap_intuition_lvos() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
