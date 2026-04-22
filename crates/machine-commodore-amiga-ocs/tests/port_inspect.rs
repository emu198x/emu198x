//! Dump the two MsgPorts trackdisk uses as mn_ReplyPort for its
//! timer.device requests. These are the ports that timer.device
//! would signal on ReplyMsg when the delay expires.
//!
//! Port struct layout (exec/ports.h):
//!   $00..$0D  struct Node (ln_Succ, ln_Pred, ln_Type, ln_Pri, ln_Name)
//!   $0E       mp_Flags   (UBYTE; 0=PA_SIGNAL, 1=PA_SOFTINT, 2=PA_IGNORE)
//!   $0F       mp_SigBit  (UBYTE; bit number, so signal mask = 1<<sigBit)
//!   $10..$13  mp_SigTask (APTR — task to signal)
//!   $14..$1F  mp_MsgList (struct List)

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const PORT_FLAGS: u32 = 0x0E;
const PORT_SIGBIT: u32 = 0x0F;
const PORT_SIGTASK: u32 = 0x10;
const LN_NAME: u32 = 10;

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

fn dump_port(amiga: &AmigaOcs, addr: u32, label: &str) {
    eprintln!("\n=== {label} @ ${addr:08X} ===");
    let flags = read_byte(amiga, addr.wrapping_add(PORT_FLAGS));
    let sigbit = read_byte(amiga, addr.wrapping_add(PORT_SIGBIT));
    let sigtask = read_long(amiga, addr.wrapping_add(PORT_SIGTASK));
    let name = read_cstring(amiga, read_long(amiga, addr.wrapping_add(LN_NAME)), 32);
    let mask = 1u32 << sigbit;
    eprintln!(
        "  mp_Flags   = ${flags:02X}  ({})",
        match flags {
            0 => "PA_SIGNAL",
            1 => "PA_SOFTINT",
            2 => "PA_IGNORE",
            _ => "???",
        }
    );
    eprintln!("  mp_SigBit  = ${sigbit:02X}  (mask = ${mask:08X})");
    eprintln!("  mp_SigTask = ${sigtask:08X}");
    if sigtask != 0 {
        let task_name = read_cstring(amiga, read_long(amiga, sigtask.wrapping_add(LN_NAME)), 32);
        eprintln!("    → task \"{task_name}\"");
    }
    eprintln!("  mp_Name    = \"{name}\"");
}

#[test]
#[ignore]
fn dump_trackdisk_timer_reply_ports() {
    let Some(rom) = load_kickstart() else { return };

    // slow-RAM
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        slow.tick();
    }
    eprintln!("\n########## slow-RAM ##########");
    dump_port(&slow, 0x00C0_4730, "reply port (10.5s TR_ADDREQUEST)");
    dump_port(&slow, 0x00C0_47DE, "reply port (0.5s IOF_QUICK)");

    // chip-only
    let mut chip_only = AmigaOcs::new(rom);
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        chip_only.tick();
    }
    eprintln!("\n########## chip-only ##########");
    dump_port(&chip_only, 0x0000_60F0, "reply port (10.5s TR_ADDREQUEST)");
    dump_port(&chip_only, 0x0000_619E, "reply port (0.5s IOF_QUICK)");
}
