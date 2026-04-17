//! Phase 5 — memory sizing and MemList construction.
//!
//! After Phase 5, ExecBase should have:
//! - $4 = ExecBase pointer
//! - ExecBase.ChkBase = ~ExecBase
//! - Two MemHeaders in MemList: chip RAM + slow RAM
//!   - Chip RAM: priority -10, attributes = CHIP|PUBLIC = $03
//!   - Slow RAM: priority 0, attributes = FAST|PUBLIC = $05
//!
//! MemList is at ExecBase + $142 (per reference).

use machine_commodore_amiga::Amiga;
use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Run through full boot init.
    for _ in 0..(200 * ccks_per_frame) {
        amiga.tick_cck();
    }

    let rl = |a: &Amiga, addr: u32| -> u32 {
        (u32::from(a.memory.read_word(addr)) << 16) | u32::from(a.memory.read_word(addr + 2))
    };
    let rw = |a: &Amiga, addr: u32| a.memory.read_word(addr);
    let rb = |a: &Amiga, addr: u32| a.memory.read_byte(addr);

    let execbase = rl(&amiga, 0x4);
    println!("=== Phase 5/6 — ExecBase + MemList after 70 frames ===\n");
    println!("ExecBase (at $4) = ${execbase:08X}");
    if execbase == 0 {
        println!("  FAIL: ExecBase not initialized yet");
        return;
    }

    let chkbase = rl(&amiga, execbase.wrapping_add(0x26));
    println!("ExecBase + $26 (ChkBase) = ${chkbase:08X}");
    println!("  ExecBase + ChkBase = ${:08X}  (expected: $FFFFFFFF)", execbase.wrapping_add(chkbase));

    // SysStkUpper at +$36, SysStkLower at +$3A, MaxLocMem at +$3E, MaxExtMem at +$4E
    // (using 1.x offsets from ref table)
    let sysstk_upper = rl(&amiga, execbase.wrapping_add(0x36));
    let sysstk_lower = rl(&amiga, execbase.wrapping_add(0x3A));
    let max_loc_mem = rl(&amiga, execbase.wrapping_add(0x3E));
    let max_ext_mem = rl(&amiga, execbase.wrapping_add(0x4E));
    println!();
    println!("SysStkUpper (+$36) = ${sysstk_upper:08X}  SysStkLower (+$3A) = ${sysstk_lower:08X}");
    println!("MaxLocMem (+$3E, chip top) = ${max_loc_mem:08X}");
    println!("MaxExtMem (+$4E, fast top) = ${max_ext_mem:08X}");

    // SoftVer at +$22
    println!();
    println!("SoftVer (+$22) = {}  (V34 = Kickstart 1.3)", rw(&amiga, execbase.wrapping_add(0x22)));

    // AttnFlags at +$128
    let attn = rw(&amiga, execbase.wrapping_add(0x128));
    println!("AttnFlags (+$128) = ${attn:04X}");

    // ResModules at +$12C
    let res_modules = rl(&amiga, execbase.wrapping_add(0x12C));
    println!("ResModules (+$12C) = ${res_modules:08X}");

    // MemList at +$142
    let memlist_head = rl(&amiga, execbase.wrapping_add(0x142));
    let memlist_tail = rl(&amiga, execbase.wrapping_add(0x146));
    let memlist_tailpred = rl(&amiga, execbase.wrapping_add(0x14A));
    println!();
    println!("MemList (ExecBase + $142):");
    println!("  head     = ${memlist_head:08X}");
    println!("  tail     = ${memlist_tail:08X}  (should be 0 in a List struct)");
    println!("  tailpred = ${memlist_tailpred:08X}");

    // Walk MemList
    println!("\nMemHeaders in MemList:");
    let mut node = memlist_head;
    let mut count = 0;
    while node != 0 && count < 10 {
        let succ = rl(&amiga, node);
        let ln_type = rb(&amiga, node.wrapping_add(8));
        let ln_pri = rb(&amiga, node.wrapping_add(9)) as i8;
        let name_ptr = rl(&amiga, node.wrapping_add(0xA));
        let mh_attr = rw(&amiga, node.wrapping_add(0xE));
        let mh_first = rl(&amiga, node.wrapping_add(0x10));
        let mh_lower = rl(&amiga, node.wrapping_add(0x14));
        let mh_upper = rl(&amiga, node.wrapping_add(0x18));
        let mh_free = rl(&amiga, node.wrapping_add(0x1C));

        let name = {
            let mut s = String::new();
            if (0xFC0000..=0xFFFFFF).contains(&name_ptr) || (name_ptr >= 0x400 && name_ptr < 0x100000) {
                for i in 0..32 {
                    let b = rb(&amiga, name_ptr.wrapping_add(i));
                    if b == 0 {
                        break;
                    }
                    if (b as char).is_ascii() && !(b as char).is_ascii_control() {
                        s.push(b as char);
                    } else {
                        s.push('?');
                        break;
                    }
                }
            }
            s
        };

        println!("  [{count}] MemHeader @ ${node:08X}:");
        println!("      ln_Succ=${succ:08X}  ln_Type=${ln_type:02X}  ln_Pri={ln_pri}  ln_Name='{name}'");
        println!(
            "      mh_Attr=${mh_attr:04X}  First=${mh_first:08X}  Lower=${mh_lower:08X}  Upper=${mh_upper:08X}  Free=${mh_free:08X}"
        );

        // Interpret attributes
        let mut attrs = Vec::new();
        if mh_attr & 0x01 != 0 { attrs.push("PUBLIC"); }
        if mh_attr & 0x02 != 0 { attrs.push("CHIP"); }
        if mh_attr & 0x04 != 0 { attrs.push("FAST"); }
        if mh_attr & 0x08 != 0 { attrs.push("LOCAL"); }
        if mh_attr & 0x10 != 0 { attrs.push("24BIT"); }
        if mh_attr & 0x80 != 0 { attrs.push("KICK"); }
        println!("      attrs = {:?}", attrs);

        node = succ;
        count += 1;
    }
    if count == 0 {
        println!("  (empty or broken list)");
    }
}
