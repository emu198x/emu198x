//! Trace the VBLANK interrupt server chain.
//!
//! Structure of ExecBase (from exec/execbase.h):
//!   ExecBase+$54   — IntVects[0] (INTB_TBE)
//!   ExecBase+$90   — IntVects[5] (INTB_VERTB)
//!
//! Each IntVector is:
//!   +$0 iv_Data  — for server chains, pointer to a List/MinList of
//!                  struct Interrupt; for direct interrupts, user data.
//!   +$4 iv_Code  — the handler function address
//!   +$8 iv_Node  — node pointer
//!
//! An IntServer node (struct Interrupt) is:
//!   +$0..$D  ln_* (Node)
//!   +$A      ln_Name (APTR)
//!   +$E      is_Data (APTR)
//!   +$12     is_Code (APTR)
//!
//! We want to walk IntVects[5]'s chain and find each server's name + code.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::fs;
use std::path::Path;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();

    // Run through boot long enough to be past autoconfig and strap.
    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);
    let total_ticks = 500u64 * ccks_per_frame;
    for _ in 0..total_ticks {
        amiga.tick_cck();
    }

    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };
    let rb = |amiga: &Amiga, a: u32| amiga.memory.read_byte(a);

    let read_str = |amiga: &Amiga, p: u32, max: u32| -> String {
        if !((0xC0_0000..0xC8_0000).contains(&p)
            || (0x400..0x8_0000).contains(&p)
            || (0xF0_0000..0x100_0000).contains(&p))
        {
            return String::new();
        }
        let mut s = String::new();
        for i in 0..max {
            let c = rb(amiga, p.wrapping_add(i));
            if c == 0 { break; }
            if c.is_ascii_graphic() || c == b' ' { s.push(c as char); }
            else { s.push('?'); break; }
        }
        s
    };

    let execbase = rl(&amiga, 0x4);
    println!("ExecBase = ${execbase:08X}");
    println!("\n── All 16 IntVectors ──");
    println!("idx name         iv_Data   iv_Code   iv_Node");
    let names = [
        "TBE", "DSKBLK", "SOFTINT", "PORTS",
        "COPER", "VERTB", "BLIT", "AUD0",
        "AUD1", "AUD2", "AUD3", "RBF",
        "DSKSYNC", "EXTER", "INTEN", "NMI",
    ];
    for i in 0u32..16 {
        let base = execbase.wrapping_add(0x54 + i * 12);
        let d = rl(&amiga, base);
        let c = rl(&amiga, base.wrapping_add(4));
        let n = rl(&amiga, base.wrapping_add(8));
        println!(" {i:2}  {:<10} ${d:08X}  ${c:08X}  ${n:08X}", names[i as usize]);
    }

    // Walk IntVects[5] (VERTB). iv_Data points to a List of struct
    // Interrupt. If it's a List (mlh_Head/mlh_Tail/mlh_TailPred), then
    // mlh_Head points to the first node OR to mlh_Tail if empty.
    let vertb_base = execbase.wrapping_add(0x54 + 5 * 12);
    let vertb_data = rl(&amiga, vertb_base);
    let vertb_code = rl(&amiga, vertb_base.wrapping_add(4));
    println!("\n── VERTB vector (IntVects[5] @ ${vertb_base:08X}) ──");
    println!("  iv_Data = ${vertb_data:08X}");
    println!("  iv_Code = ${vertb_code:08X}");

    // Interpret iv_Data as if it's a List/MinList.
    // List { mlh_Head, mlh_Tail, mlh_TailPred } — 12 bytes.
    // If iv_Data is a list, first ULONG at iv_Data is the head pointer.
    if vertb_data >= 0x400 && vertb_data < 0x0100_0000 {
        let head = rl(&amiga, vertb_data);
        let tail = rl(&amiga, vertb_data.wrapping_add(4));
        let tp   = rl(&amiga, vertb_data.wrapping_add(8));
        println!("  iv_Data as List: head=${head:08X} tail=${tail:08X} tailpred=${tp:08X}");

        // Walk.
        let mut node = head;
        let mut n = 0;
        while node != 0 && n < 20 {
            let succ = rl(&amiga, node);
            let name_ptr = rl(&amiga, node.wrapping_add(0x0A));
            let is_data = rl(&amiga, node.wrapping_add(0x0E));
            let is_code = rl(&amiga, node.wrapping_add(0x12));
            let nm = read_str(&amiga, name_ptr, 40);
            println!(
                "    server @ ${node:08X} succ=${succ:08X} name=${name_ptr:08X} '{nm}' is_Data=${is_data:08X} is_Code=${is_code:08X}"
            );
            if succ == 0 || succ == tail {
                break;
            }
            node = succ;
            n += 1;
        }
    } else {
        println!("  iv_Data not a memory pointer — likely direct-dispatch or uninitialized.");
    }

    // Also dump device list to confirm timer.device is there.
    println!("\n── DeviceList ──");
    let device_list = execbase.wrapping_add(0x15E);
    let head = rl(&amiga, device_list);
    let mut node = head;
    let mut n = 0;
    while node != 0 && n < 20 {
        let succ = rl(&amiga, node);
        let name_ptr = rl(&amiga, node.wrapping_add(0x0A));
        let nm = read_str(&amiga, name_ptr, 40);
        println!("  dev @ ${node:08X} '{nm}'");
        if succ == 0 { break; }
        node = succ;
        n += 1;
    }

    // Resource list too.
    println!("\n── ResourceList ──");
    let rl_head = execbase.wrapping_add(0x150);
    let head = rl(&amiga, rl_head);
    let mut node = head;
    let mut n = 0;
    while node != 0 && n < 20 {
        let succ = rl(&amiga, node);
        let name_ptr = rl(&amiga, node.wrapping_add(0x0A));
        let nm = read_str(&amiga, name_ptr, 40);
        println!("  res @ ${node:08X} '{nm}'");
        if succ == 0 { break; }
        node = succ;
        n += 1;
    }

    // Inspect timer.device VBLANK unit (is_Data from the server node).
    // The unit is a MsgPort at the head; internal state follows.
    let tvu = 0x00C02366u32; // VBLANK unit pointer from VERTB chain
    println!("\n── timer.device VBLANK unit @ ${tvu:08X} ──");
    let mp_flags = rb(&amiga, tvu.wrapping_add(0x0E));
    let mp_sigbit = rb(&amiga, tvu.wrapping_add(0x0F));
    let mp_sigtask = rl(&amiga, tvu.wrapping_add(0x10));
    let mp_mlhead = rl(&amiga, tvu.wrapping_add(0x14));
    let mp_mltail = rl(&amiga, tvu.wrapping_add(0x18));
    let mp_mltp   = rl(&amiga, tvu.wrapping_add(0x1C));
    println!("  mp_Flags   = ${mp_flags:02X}");
    println!("  mp_SigBit  = {mp_sigbit}");
    println!("  mp_SigTask = ${mp_sigtask:08X}");
    println!("  mp_MsgList: head=${mp_mlhead:08X} tail=${mp_mltail:08X} tailpred=${mp_mltp:08X}");

    // Dump 128 bytes of the unit so we can eyeball the internal queue.
    print!("  raw bytes +$00..+$80:\n   ");
    for i in 0..128 {
        if i > 0 && i % 16 == 0 { print!("\n   "); }
        print!(" {:02X}", rb(&amiga, tvu.wrapping_add(i)));
    }
    println!();

    // Walk the MsgList (port message queue) — pending requests not yet
    // processed by the VBLANK server.
    println!("\n  MsgList walk:");
    let mut node = mp_mlhead;
    let mut n = 0;
    while node != 0 && node != mp_mltail && n < 10 {
        let succ = rl(&amiga, node);
        println!("    msg @ ${node:08X} succ=${succ:08X}");
        if succ == 0 { break; }
        node = succ;
        n += 1;
    }
    if n == 0 {
        println!("    (empty)");
    }

    // Common pattern: timer.device unit has its own internal active list
    // after the MsgPort. Let me dump the obvious candidate list heads.
    println!("\n  Candidate internal lists at unit+$22, +$2E, +$3A, +$46:");
    for base_off in [0x22u32, 0x2E, 0x3A, 0x46, 0x52, 0x5E] {
        let lh = tvu.wrapping_add(base_off);
        let h = rl(&amiga, lh);
        let t = rl(&amiga, lh.wrapping_add(4));
        let tp = rl(&amiga, lh.wrapping_add(8));
        println!("    @ +${base_off:02X}: head=${h:08X} tail=${t:08X} tailpred=${tp:08X}");
    }

    println!("\nFinal PC=${:08X}", amiga.cpu.instr_start_pc);
}
