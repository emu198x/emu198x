//! Phase 9-10 — ROMTag scan + InitCode(COLDSTART).
//!
//! Walk ResModules, verify expected libraries/devices, and check that
//! graphics.library + intuition.library are present AND properly
//! initialised.

use machine_commodore_amiga::Amiga;
use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Run long enough for all ROMTag inits to complete.
    for _ in 0..(200 * ccks_per_frame) {
        amiga.tick_cck();
    }

    let rl = |a: &Amiga, addr: u32| -> u32 {
        (u32::from(a.memory.read_word(addr)) << 16) | u32::from(a.memory.read_word(addr + 2))
    };
    let rw = |a: &Amiga, addr: u32| a.memory.read_word(addr);
    let rb = |a: &Amiga, addr: u32| a.memory.read_byte(addr);
    let rstr = |a: &Amiga, addr: u32, max: usize| -> String {
        let mut s = String::new();
        for i in 0..max {
            let c = rb(a, addr.wrapping_add(i as u32));
            if c == 0 {
                break;
            }
            if (c as char).is_ascii() && !(c as char).is_ascii_control() {
                s.push(c as char);
            } else {
                s.push('?');
                break;
            }
        }
        s
    };

    let execbase = rl(&amiga, 0x4);
    let res_modules = rl(&amiga, execbase.wrapping_add(0x12C));
    println!("=== Phase 9/10 — ROMTag scan + init ===\n");
    println!("ExecBase = ${execbase:08X}");
    println!("ResModules = ${res_modules:08X}\n");

    // Walk ResModules (null-terminated array of Resident*)
    println!("Residents found:");
    let mut i = 0u32;
    let mut residents = Vec::new();
    loop {
        let entry = rl(&amiga, res_modules.wrapping_add(i * 4));
        if entry == 0 {
            break;
        }
        if i > 100 {
            break;
        }
        // If high bit set, it's a link to another array
        let ptr = if entry & 0x8000_0000 != 0 {
            continue;
        } else {
            entry
        };
        residents.push(ptr);
        i += 1;
    }

    // For each Resident, print its fields
    for (idx, ptr) in residents.iter().enumerate() {
        let match_word = rw(&amiga, *ptr);
        let match_tag = rl(&amiga, *ptr + 2);
        let end_skip = rl(&amiga, *ptr + 6);
        let flags = rb(&amiga, *ptr + 10);
        let version = rb(&amiga, *ptr + 11);
        let typ = rb(&amiga, *ptr + 12);
        let pri = rb(&amiga, *ptr + 13) as i8;
        let name_ptr = rl(&amiga, *ptr + 14);
        let name = rstr(&amiga, name_ptr, 32);
        let type_name = match typ {
            1 => "TASK",
            2 => "INTR",
            3 => "DEV",
            4 => "MSGPORT",
            5 => "MESSAGE",
            6 => "FREEMSG",
            7 => "REPLY",
            8 => "RESOURCE",
            9 => "LIB",
            10 => "MEM",
            _ => "UNKNOWN",
        };
        println!(
            "  [{idx:>2}] ${:08X}  pri={pri:>4}  v{version:>3}  flags=${flags:02X}  type={type_name}  name='{name}'",
            *ptr
        );
        let _ = (match_word, match_tag, end_skip);
    }

    println!("\nTotal: {} residents", residents.len());

    // Dump LibList struct directly
    println!("\n=== LibList struct (ExecBase + $17A) ===");
    let lh = execbase.wrapping_add(0x17A);
    let head = rl(&amiga, lh);
    let tailpred = rl(&amiga, lh + 8);
    println!(
        "  @ ${lh:08X}: head=${head:08X}  tail=${:08X}  tailpred=${tailpred:08X}  type=${:02X}",
        rl(&amiga, lh + 4),
        rb(&amiga, lh + 12)
    );

    // Dump first 32 bytes of graphics.library at $C01E1E
    let graphics_base = 0x00C01E1Eu32;
    println!("\nraw bytes at graphics.library base ${graphics_base:08X}:");
    for i in 0..8 {
        let a = graphics_base + i * 4;
        println!("  +{:02X}: ${:08X}", i * 4, rl(&amiga, a));
    }
    let intuition_base = 0x00C03D24u32;
    println!("\nraw bytes at intuition.library base ${intuition_base:08X}:");
    for i in 0..8 {
        let a = intuition_base + i * 4;
        println!("  +{:02X}: ${:08X}", i * 4, rl(&amiga, a));
    }

    // Walk BACKWARDS from tailpred via ln_Pred to see all libraries.
    println!("\nWalking LibList BACKWARDS from tailpred:");
    let mut node = tailpred;
    let mut count = 0;
    while node != 0 && node != lh && count < 30 {
        let pred = rl(&amiga, node + 4);
        let ln_type = rb(&amiga, node + 8);
        let ln_pri = rb(&amiga, node + 9) as i8;
        let name_ptr = rl(&amiga, node + 10);
        let name = rstr(&amiga, name_ptr, 32);
        println!(
            "  [back {count:>2}] ${:08X}  type=${ln_type:02X}  pri={ln_pri:>4}  name='{name}'  pred=${pred:08X}",
            node
        );
        node = pred;
        count += 1;
    }

    // Now check the library list.
    println!("\n=== Library list (ExecBase + $17A) FORWARD ===");
    let lib_head = rl(&amiga, execbase.wrapping_add(0x17A));
    let mut node = lib_head;
    let mut count = 0;
    while node != 0 && count < 20 {
        let succ = rl(&amiga, node);
        if succ == 0 {
            break;
        }
        let ln_type = rb(&amiga, node + 8);
        if ln_type != 9 {
            // Not a library — probably list terminator
            break;
        }
        let ln_pri = rb(&amiga, node + 9) as i8;
        let name_ptr = rl(&amiga, node + 10);
        let lib_version = rw(&amiga, node + 20);
        let lib_revision = rw(&amiga, node + 22);
        let name = rstr(&amiga, name_ptr, 32);
        println!(
            "  lib @ ${:08X}  pri={ln_pri:>4}  v{lib_version}.{lib_revision}  name='{name}'",
            node
        );
        node = succ;
        count += 1;
    }

    println!("\n=== Device list (ExecBase + $15E) ===");
    let dev_head = rl(&amiga, execbase.wrapping_add(0x15E));
    let mut node = dev_head;
    let mut count = 0;
    while node != 0 && count < 20 {
        let succ = rl(&amiga, node);
        if succ == 0 {
            break;
        }
        let ln_type = rb(&amiga, node + 8);
        if ln_type != 3 {
            break;
        }
        let ln_pri = rb(&amiga, node + 9) as i8;
        let name_ptr = rl(&amiga, node + 10);
        let name = rstr(&amiga, name_ptr, 32);
        println!("  dev @ ${:08X}  pri={ln_pri:>4}  name='{name}'", node);
        node = succ;
        count += 1;
    }

    println!("\n=== Resource list (ExecBase + $150) ===");
    let res_head = rl(&amiga, execbase.wrapping_add(0x150));
    let mut node = res_head;
    let mut count = 0;
    while node != 0 && count < 20 {
        let succ = rl(&amiga, node);
        if succ == 0 {
            break;
        }
        let ln_type = rb(&amiga, node + 8);
        if ln_type != 8 {
            break;
        }
        let ln_pri = rb(&amiga, node + 9) as i8;
        let name_ptr = rl(&amiga, node + 10);
        let name = rstr(&amiga, name_ptr, 32);
        println!("  res @ ${:08X}  pri={ln_pri:>4}  name='{name}'", node);
        node = succ;
        count += 1;
    }
}
