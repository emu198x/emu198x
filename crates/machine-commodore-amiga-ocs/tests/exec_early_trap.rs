//! Count calls to key exec.library LVOs from frame 0 so we don't
//! miss calls made during Exec InitCode (which is what the earlier
//! msg_port_trap missed).
//!
//! Hardcoded exec entry points (resolved once from the live ROM in
//! msg_port_trap and stable across runs):
//!   PutMsg    = $00FC1B70
//!   GetMsg    = $00FC1BEA
//!   ReplyMsg  = $00FC1C18
//!   WaitPort  = $00FC1C32
//!   DoIO      = $00FC0718
//!   SendIO    = $00FC0706
//!   OpenDev   = $00FC06EA  (approx; resolved = $FC06EA)
//!   Signal    = $00FC1E84

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const PUT_MSG: u32 = 0x00FC_1B70;
const GET_MSG: u32 = 0x00FC_1BEA;
const REPLY_MSG: u32 = 0x00FC_1C18;
const DO_IO: u32 = 0x00FC_0718;
const SEND_IO: u32 = 0x00FC_0706;
const SIGNAL: u32 = 0x00FC_1E84;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    let mut put_msg = 0u64;
    let mut get_msg = 0u64;
    let mut reply_msg = 0u64;
    let mut do_io = 0u64;
    let mut send_io = 0u64;
    let mut signal = 0u64;
    let mut first_do_io: Option<u64> = None;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut tick = 0u64;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == PUT_MSG {
            put_msg += 1;
        } else if pc == GET_MSG {
            get_msg += 1;
        } else if pc == REPLY_MSG {
            reply_msg += 1;
        } else if pc == DO_IO {
            do_io += 1;
            if first_do_io.is_none() {
                first_do_io = Some(tick);
            }
        } else if pc == SEND_IO {
            send_io += 1;
        } else if pc == SIGNAL {
            signal += 1;
        }
        prev_pc = pc;
    }

    eprintln!("=== 400-frame counts from tick 0 ===");
    eprintln!("  PutMsg   = {put_msg}");
    eprintln!("  GetMsg   = {get_msg}");
    eprintln!("  ReplyMsg = {reply_msg}");
    eprintln!("  DoIO     = {do_io}");
    eprintln!("  SendIO   = {send_io}");
    eprintln!("  Signal   = {signal}");
    if let Some(t) = first_do_io {
        let cck = t / 2;
        let frame = cck / 70824;
        eprintln!("\n  first DoIO at tick={t} (frame ~{frame})");
    }
}

#[test]
#[ignore]
fn exec_traffic_from_zero() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
