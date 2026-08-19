//! Check whether trackdisk's BeginIO is ever called, and if so
//! with what command. If strap's DoIO dispatches correctly to
//! BeginIO, we'll see it fire at strap-time. If it doesn't fire,
//! DoIO is getting lost somewhere.
//!
//! trackdisk.device BeginIO lives at (device_base - 30) in the
//! library jump table (just like any LVO). That slot is `JMP
//! <addr>` — we follow it to the ROM entry and trap that.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const EXEC_DEVICE_LIST: u32 = 350;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;
const LVO_BEGIN_IO: i32 = -30;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
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

fn find_device(amiga: &AmigaOcs, exec_base: u32, target: &str) -> Option<u32> {
    let list_addr = exec_base.wrapping_add(EXEC_DEVICE_LIST);
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

/// Because resolving the BeginIO address needs the boot to have
/// run, we do it on a throwaway instance, then re-create and trap
/// from frame 0.
fn run_with_trap(amiga_ctor: impl Fn() -> AmigaOcs, label: &str) {
    // Discovery pass.
    let mut discover = amiga_ctor();
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        discover.tick();
    }
    let exec_base = read_long(&discover, 0x0000_0004);
    let Some(td_base) = find_device(&discover, exec_base, "trackdisk.device") else {
        emu198x_test_skip::skip!("trackdisk not found");
    };
    let beginio_slot = td_base.wrapping_add(LVO_BEGIN_IO as u32);
    let beginio = read_long(&discover, beginio_slot.wrapping_add(2));
    eprintln!("\n########## {label} ##########");
    eprintln!("trackdisk.device base = ${td_base:08X}");
    eprintln!("trackdisk BeginIO     = ${beginio:08X}");

    // Now trap from tick 0 on a fresh instance.
    let mut amiga = amiga_ctor();
    let mut beginio_hits = 0u64;
    let mut d0_at_beginio: Vec<u32> = Vec::new();
    let mut d1_at_beginio: Vec<u32> = Vec::new();
    let mut a1_at_beginio: Vec<u32> = Vec::new();
    let mut io_cmd_at_beginio: Vec<u16> = Vec::new();
    let mut first_tick: Option<u64> = None;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut tick = 0u64;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == beginio {
            beginio_hits += 1;
            if first_tick.is_none() {
                first_tick = Some(tick);
            }
            if d0_at_beginio.len() < 20 {
                d0_at_beginio.push(amiga.cpu().regs.d[0]);
                d1_at_beginio.push(amiga.cpu().regs.d[1]);
                let a1 = amiga.cpu().regs.a[1];
                a1_at_beginio.push(a1);
                // IORequest.io_Command is at offset 28 (UWORD).
                let cmd = amiga.read_word(a1.wrapping_add(28));
                io_cmd_at_beginio.push(cmd);
            }
        }
        prev_pc = pc;
    }

    eprintln!("\nBeginIO hits (400 frames): {beginio_hits}");
    if let Some(t) = first_tick {
        eprintln!("  first hit at tick={t} (frame~{})", t / (70824 * 2));
    }
    for i in 0..d0_at_beginio.len() {
        eprintln!(
            "  [{i}] A1=${:08X}  io_Command=${:04X}  D0=${:08X} D1=${:08X}",
            a1_at_beginio[i], io_cmd_at_beginio[i], d0_at_beginio[i], d1_at_beginio[i]
        );
    }
}

#[test]
#[ignore]
fn check_beginio_dispatch() {
    let Some(rom) = load_kickstart() else { return };
    let rom_a = rom.clone();
    let rom_b = rom;
    run_with_trap(
        move || AmigaOcs::with_slow_ram(rom_a.clone(), 512 * 1024),
        "slow-RAM",
    );
    run_with_trap(move || AmigaOcs::new(rom_b.clone()), "chip-only");
}
