//! Task #96 — who is the caller that invokes LoadView at slow-RAM
//! frame 188 with View=$FE888E?
//!
//! From `loadview_trap.rs`:
//!   slow-RAM frame 188: LoadView called from $00005A10, View=$00FE888E
//!
//! That "called from $5A10" is chip-RAM — a code fragment the ROM
//! copied into RAM. We want to know:
//!  1. What's in memory at $5A08..$5A10 at frame 188? (should be
//!     a JSR instruction to the LoadView LVO)
//!  2. What TASK is running at that moment (ThisTask.ln_Name)?
//!  3. Does the same code sequence exist somewhere in chip-only?

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const EXEC_THIS_TASK: u32 = 276;
const LN_NAME: u32 = 10;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
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

#[test]
#[ignore]
fn inspect_slow_ram_frame_188_caller() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    for _ in 0..(190u64 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // Code at $5A08-$5A12 — look at the 6 bytes ending at $5A10
    // (inclusive, i.e. the JSR instruction — 68K JSR (d16,An) is
    // 6 bytes).
    eprintln!("=== Memory around caller $5A10 @ frame 190 ===");
    for offset in 0..16 {
        let addr = 0x00005A00u32 + offset * 2;
        eprintln!("  ${addr:08X}: {:04X}", amiga.read_word(addr));
    }

    let exec_base = amiga.read_long(0x0000_0004);
    let this_task = amiga.read_long(exec_base.wrapping_add(EXEC_THIS_TASK));
    let name_ptr = amiga.read_long(this_task.wrapping_add(LN_NAME));
    let name = read_cstring(&amiga, name_ptr, 32);
    eprintln!("\nExecBase  = ${exec_base:08X}");
    eprintln!("ThisTask  = ${this_task:08X} name='{name}'");
}

#[test]
#[ignore]
fn inspect_chip_only_at_same_address() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    for _ in 0..(190u64 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // What's at $5A00-$5A20 in chip-only?
    eprintln!("=== Chip-only memory at $5A00-$5A20 @ frame 190 ===");
    for offset in 0..16 {
        let addr = 0x00005A00u32 + offset * 2;
        eprintln!("  ${addr:08X}: {:04X}", amiga.read_word(addr));
    }
    let exec_base = amiga.read_long(0x0000_0004);
    let this_task = amiga.read_long(exec_base.wrapping_add(EXEC_THIS_TASK));
    let name_ptr = amiga.read_long(this_task.wrapping_add(LN_NAME));
    let name = read_cstring(&amiga, name_ptr, 32);
    eprintln!("\nExecBase  = ${exec_base:08X}");
    eprintln!("ThisTask  = ${this_task:08X} name='{name}'");
}
