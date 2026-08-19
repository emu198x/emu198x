//! Find the VBL handler timer.device installed via AddIntServer and
//! trap it to see what it does each frame. If it walks both the
//! VBLANK and MICROHZ queues, MICROHZ would be driven by VBL
//! (50Hz, 20ms granularity — plenty for 500ms).
//!
//! timer.device's init stores the Interrupt struct at $A2(A2)
//! where A2 is the timer.device base. Interrupt.is_Code (offset
//! $12) holds the handler entry point.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const EXEC_DEVICE_LIST: u32 = 350;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;

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

#[test]
#[ignore]
fn trap_timer_vbl_handler() {
    let Some(rom) = load_kickstart() else { return };

    // Discovery run.
    let mut discover = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        discover.tick();
    }
    let exec_base = read_long(&discover, 0x0000_0004);
    let Some(tb) = find_device(&discover, exec_base, "timer.device") else {
        eprintln!("timer.device not in DeviceList");
        return;
    };
    let int_struct = tb.wrapping_add(0xA2);
    let is_code = read_long(&discover, int_struct.wrapping_add(0x12));
    let is_data = read_long(&discover, int_struct.wrapping_add(0x0E));
    let cia_int_struct = tb.wrapping_add(0x54);
    let cia_is_code = read_long(&discover, cia_int_struct.wrapping_add(0x12));
    let cia_is_data = read_long(&discover, cia_int_struct.wrapping_add(0x0E));

    eprintln!("\n########## slow-RAM ##########");
    eprintln!("timer.device base    = ${tb:08X}");
    eprintln!("VERTB Interrupt @ ${int_struct:08X}:");
    eprintln!("  is_Code  (VBL handler)    = ${is_code:08X}");
    eprintln!("  is_Data  (VBLANK unit)    = ${is_data:08X}");
    eprintln!("CIA Interrupt @ ${cia_int_struct:08X}:");
    eprintln!("  is_Code  (CIA-TB handler) = ${cia_is_code:08X}");
    eprintln!("  is_Data  (MICROHZ unit)   = ${cia_is_data:08X}");

    // Run with trap: count hits on both handler entry points.
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    let mut vbl_hits = 0u64;
    let mut cia_hits = 0u64;
    let mut prev_pc = amiga.cpu().regs.pc;

    for _ in 0..(700 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == is_code {
            vbl_hits += 1;
        }
        if pc == cia_is_code {
            cia_hits += 1;
        }
        prev_pc = pc;
    }

    eprintln!("\n=== Handler hits over 700 frames ===");
    eprintln!("VBL handler (${is_code:08X})     : {vbl_hits} hits");
    eprintln!("CIA-TB handler (${cia_is_code:08X}): {cia_hits} hits");
    if vbl_hits > 0 {
        let per_frame = vbl_hits as f64 / 700.0;
        eprintln!("  → VBL handler fires ~{per_frame:.2}×/frame (expected ~1)");
    }
    if cia_hits == 0 {
        eprintln!("  → CIA-TB handler NEVER fires (Timer B never underflows)");
    }
}
