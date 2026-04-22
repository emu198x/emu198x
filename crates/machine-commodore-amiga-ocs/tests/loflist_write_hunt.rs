//! Task #96 — who populates `GfxBase->LOFlist` past the ExecBase
//! placeholder?
//!
//! In slow-RAM, `GfxBase->LOFlist` starts at the slow-RAM ExecBase
//! (~$C00276) then advances to a real per-frame copper-list buffer
//! (~$B888) between frame 115 and 230. In chip-only it starts at the
//! chip-RAM ExecBase ($676) and never advances — which leaves COP2LC
//! pointing at ExecBase, causing the copper to execute library-struct
//! bytes and corrupt chipset registers (INTENA especially).
//!
//! This test:
//!  1. Boots slow-RAM, locates GfxBase via `ExecBase->LibList` walk.
//!  2. Polls `GfxBase->LOFlist` every CCK.
//!  3. On every value change, logs tick, frame, PC, and new value.
//!  4. Does the same for chip-only.
//!
//! The divergence: slow-RAM's value-change log should contain a
//! transition to the real buffer; chip-only's should not.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const EXEC_LIB_LIST: u32 = 378;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;
const GFX_LOFLIST_OFFSET: u32 = 0x32;

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

fn find_library(amiga: &AmigaOcs, exec_base: u32, target: &str) -> Option<u32> {
    let list_addr = exec_base.wrapping_add(EXEC_LIB_LIST);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut node = head;
    for _ in 0..32 {
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

fn hunt_loflist_writer(label: &str, use_slow_ram: bool) {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = if use_slow_ram {
        AmigaOcs::with_slow_ram(rom, 512 * 1024)
    } else {
        AmigaOcs::new(rom)
    };

    // GfxBase is deterministic across runs with the same config —
    // we pre-learned the addresses from the `gfxbase_state.rs` test:
    let gfx_base = if use_slow_ram {
        0x00C0_1E1E
    } else {
        0x0000_221E
    };
    let loflist_addr = gfx_base + GFX_LOFLIST_OFFSET;

    eprintln!("\n########## {label} ##########");
    eprintln!("GfxBase       = ${gfx_base:08X} (known-good)");
    eprintln!("LOFlist field = ${loflist_addr:08X}");

    // Poll LOFlist every tick from frame 0; log every change.
    let mut last_val = read_long(&amiga, loflist_addr);
    eprintln!("Initial LOFlist value = ${last_val:08X}");
    let mut tick = 0u64;
    let end = 700u64 * PAL_FRAME_TICKS as u64;
    let mut changes = Vec::new();
    while tick < end {
        amiga.tick();
        tick += 1;
        let v = read_long(&amiga, loflist_addr);
        if v != last_val {
            let pc = amiga.cpu().regs.pc;
            let frame = tick / PAL_FRAME_TICKS as u64;
            changes.push((frame, tick, pc, last_val, v));
            last_val = v;
        }
    }

    eprintln!("LOFlist changes over frames 120..700 ({}):", changes.len());
    for (frame, tick, pc, from, to) in changes.iter().take(30) {
        eprintln!("  frame~{frame:<3}  tick={tick:<10}  pc=${pc:08X}  ${from:08X} → ${to:08X}");
    }
    if changes.len() > 30 {
        eprintln!("  ... and {} more", changes.len() - 30);
    }
    eprintln!("Final LOFlist value = ${last_val:08X}");
}

#[test]
#[ignore]
fn slow_ram_loflist_writer() {
    hunt_loflist_writer("slow-RAM", true);
}

#[test]
#[ignore]
fn chip_only_loflist_writer() {
    hunt_loflist_writer("chip-only", false);
}
