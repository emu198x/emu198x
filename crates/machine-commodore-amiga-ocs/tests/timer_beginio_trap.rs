//! Count calls to timer.device's BeginIO to see whether the delay
//! requests trackdisk sends are actually reaching the timer device.
//!
//! If BeginIO hits > 0, the request was queued and we need to find
//! out why queue advancement isn't happening (path B).
//!
//! If BeginIO hits == 0, trackdisk's JSR DoIO at $FEA190 never
//! reaches timer.device at all — either DoIO dispatch is bad, or
//! trackdisk's A6 switch (MOVEA.L $34(A6), A6 at $FEA18C) loads
//! a bogus TimerBase (path A variant).

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const EXEC_DEVICE_LIST: u32 = 350;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;
const LVO_BEGIN_IO: i32 = -30;
const IO_COMMAND: u32 = 28;
const EXEC_THIS_TASK: u32 = 276;

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

fn run_with_trap(amiga_ctor: impl Fn() -> AmigaOcs, label: &str) {
    // Discovery pass to resolve the timer.device BeginIO ROM address.
    let mut discover = amiga_ctor();
    for _ in 0..(250 * PAL_FRAME_TICKS) {
        discover.tick();
    }
    let exec_base = read_long(&discover, 0x0000_0004);
    let Some(timer_base) = find_device(&discover, exec_base, "timer.device") else {
        emu198x_test_skip::skip!("timer.device not found");
    };
    let beginio_slot = timer_base.wrapping_add(LVO_BEGIN_IO as u32);
    let beginio_op = discover.read_word(beginio_slot);
    if beginio_op != 0x4EF9 {
        eprintln!(
            "timer.device BeginIO slot at ${beginio_slot:08X} isn't JMP (op=${beginio_op:04X})"
        );
        return;
    }
    let beginio = read_long(&discover, beginio_slot.wrapping_add(2));
    eprintln!("\n########## {label} ##########");
    eprintln!("timer.device base    = ${timer_base:08X}");
    eprintln!("timer.device BeginIO = ${beginio:08X}");

    // Fresh run with trap.
    let mut amiga = amiga_ctor();
    let mut hits = 0u64;
    use std::collections::BTreeMap;
    let mut caller_hits: BTreeMap<(String, u16), u64> = BTreeMap::new();
    let mut first_calls: Vec<(String, u32, u16, u64)> = Vec::new();
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
            hits += 1;
            let a1 = amiga.cpu().regs.a[1];
            let cmd = amiga.read_word(a1.wrapping_add(IO_COMMAND));
            let this_task = read_long(&amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
            let name = if this_task == 0 {
                "<null>".to_string()
            } else {
                read_cstring(
                    &amiga,
                    read_long(&amiga, this_task.wrapping_add(LN_NAME)),
                    32,
                )
            };
            *caller_hits.entry((name.clone(), cmd)).or_insert(0) += 1;
            // Always record trackdisk.device calls; cap others at 12.
            if name == "trackdisk.device" || first_calls.len() < 12 {
                // For trackdisk calls, also dump the timerequest
                // fields so we can see how long the delay is.
                if name == "trackdisk.device" {
                    let tv_secs = read_long(&amiga, a1.wrapping_add(32));
                    let tv_micro = read_long(&amiga, a1.wrapping_add(36));
                    let io_device = read_long(&amiga, a1.wrapping_add(20));
                    let io_unit = read_long(&amiga, a1.wrapping_add(24));
                    let io_flags = amiga.read_word(a1.wrapping_add(30)) >> 8;
                    let mn_reply = read_long(&amiga, a1.wrapping_add(14));
                    eprintln!(
                        "  trackdisk TR_ADDREQUEST A1=${a1:08X}: \
                        io_Device=${io_device:08X} io_Unit=${io_unit:08X} \
                        mn_ReplyPort=${mn_reply:08X} io_Flags=${io_flags:02X} \
                        tv_secs={tv_secs} tv_micro={tv_micro}"
                    );
                }
                first_calls.push((name, a1, cmd, tick));
            }
        }
        prev_pc = pc;
    }

    eprintln!("\ntimer.device BeginIO hits (400 frames): {hits}");
    eprintln!("\n=== Per-caller counts ===");
    for ((name, cmd), count) in &caller_hits {
        let cmd_name = match *cmd {
            0x0001 => "CMD_RESET",
            0x0004 => "CMD_UPDATE",
            0x0005 => "CMD_CLEAR",
            0x0009 => "TR_ADDREQUEST",
            0x000A => "TR_GETSYSTIME",
            0x000B => "TR_SETSYSTIME",
            _ => "?",
        };
        eprintln!("  {count:>4} × {name:<20} cmd=${cmd:04X} {cmd_name}");
    }
    eprintln!("\n=== First 12 calls ===");
    for (i, (name, a1, cmd, tick)) in first_calls.iter().enumerate() {
        let cck = tick / 2;
        let frame = cck / 70824;
        eprintln!("  [{i}] frame~{frame:<3}  by={name:<20}  A1=${a1:08X}  cmd=${cmd:04X}");
    }
    // Show trackdisk-specific calls.
    eprintln!("\n=== Trackdisk calls only ===");
    for (i, (name, a1, cmd, tick)) in first_calls.iter().enumerate() {
        if name == "trackdisk.device" {
            let frame = tick / (2 * 70824);
            eprintln!("  [{i}] frame~{frame:<3}  A1=${a1:08X}  cmd=${cmd:04X}");
        }
    }
}

#[test]
#[ignore]
fn timer_device_request_trap() {
    let Some(rom) = load_kickstart() else { return };
    let rom_a = rom.clone();
    let rom_b = rom;
    run_with_trap(
        move || AmigaOcs::with_slow_ram(rom_a.clone(), 512 * 1024),
        "slow-RAM",
    );
    run_with_trap(move || AmigaOcs::new(rom_b.clone()), "chip-only");
}
