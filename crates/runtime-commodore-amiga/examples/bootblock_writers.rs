//! Find which CPU code paths write the wrong bytes to the bootblock
//! destination at $1558. The bootblock_check example shows specific
//! cooked-longs are corrupted (sec0+$0, +$4, +$18; sec1+$20, +$24,
//! +$38) — this diagnostic identifies the writer PCs and what they
//! actually write at each position.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::collections::BTreeMap;
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

    // Watch the entire bootblock destination range.
    amiga.debug_cpu_write_watch = Some((0x1558, 0x1957));

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    for _ in 0..(600 * ccks_per_frame) {
        amiga.tick_cck();
    }

    println!("Captured {} CPU writes to $1558..$1957.", amiga.debug_cpu_write_log.len());

    // Group writes by destination address — show what was last written
    // at each address and from which PC.
    let mut by_addr: BTreeMap<u32, Vec<(u32, char, u32)>> = BTreeMap::new();
    for (pc, addr, size, val) in &amiga.debug_cpu_write_log {
        by_addr.entry(*addr).or_default().push((*pc, *size, *val));
    }

    // Focus on the wrong-cooked-long positions plus a few neighbours.
    let wrong_offsets = [0x000u32, 0x004, 0x018, 0x220, 0x224, 0x238];
    println!("\n== Writes to wrong cooked-long positions ==");
    for off in wrong_offsets {
        let target = 0x1558 + off;
        println!("\n@ ${target:08X} (bb +${off:03X}):");
        // Show writes that touched this 4-byte long (target..target+3).
        for addr in target..target + 4 {
            if let Some(writes) = by_addr.get(&addr) {
                for (pc, size, val) in writes {
                    println!("  ${addr:08X}  {size}  ${val:08X}  PC=${pc:08X}");
                }
            } else {
                println!("  ${addr:08X}  (no CPU write captured)");
            }
        }
    }

    // Also show writes to a known-correct position (root block at +$08)
    // for comparison.
    println!("\n== Writes to known-CORRECT position (+$008 root block) ==");
    for addr in 0x1560u32..0x1564 {
        if let Some(writes) = by_addr.get(&addr) {
            for (pc, size, val) in writes {
                println!("  ${addr:08X}  {size}  ${val:08X}  PC=${pc:08X}");
            }
        } else {
            println!("  ${addr:08X}  (no CPU write captured)");
        }
    }

    // Histogram of writer PCs for wrong-position writes only.
    println!("\n== Writer PC histogram for wrong positions ==");
    let mut pc_hist: BTreeMap<u32, u32> = BTreeMap::new();
    for off in wrong_offsets {
        let target = 0x1558 + off;
        for addr in target..target + 4 {
            if let Some(writes) = by_addr.get(&addr) {
                for (pc, _, _) in writes {
                    *pc_hist.entry(*pc).or_insert(0) += 1;
                }
            }
        }
    }
    for (pc, count) in &pc_hist {
        println!("  PC=${pc:08X}  count={count}");
    }

    println!("\n== Bytes around each writer PC ==");
    let saved = amiga.memory.overlay;
    amiga.memory.overlay = false;
    for (pc, _) in &pc_hist {
        print!("  PC=${pc:08X}  [PC-8..PC+24):");
        for offset in 0..32u32 {
            let b = amiga.memory.read_byte(pc.wrapping_sub(8).wrapping_add(offset));
            if offset == 8 {
                print!(" |");
            }
            print!(" {b:02X}");
        }
        println!();
    }
    amiga.memory.overlay = saved;
}
