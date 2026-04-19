//! Identify which CPU code path writes to the disk DMA buffer area
//! after disk DMA completes.
//!
//! `paula_dma_buffer` showed that disk DMA correctly writes the encoded
//! MFM bytes to chip RAM at $2064 onwards, but the CPU then overwrites
//! parts of the buffer with word writes that look like re-encoded MFM.
//! This diagnostic captures (PC, addr, size, val) for every CPU write
//! in the buffer area so we can disassemble the routine in Kickstart.

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

    // Watch CPU writes to the disk buffer area.
    amiga.debug_cpu_write_watch = Some((0x2064, 0x208F));

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    for _ in 0..(600 * ccks_per_frame) {
        amiga.tick_cck();
    }

    println!("Captured {} CPU writes to $2064..$208F.", amiga.debug_cpu_write_log.len());
    println!("\nUnique writer PCs (first 30 entries):");

    use std::collections::BTreeMap;
    let mut pc_counts: BTreeMap<u32, u32> = BTreeMap::new();
    for (pc, _, _, _) in &amiga.debug_cpu_write_log {
        *pc_counts.entry(*pc).or_insert(0) += 1;
    }
    for (pc, count) in pc_counts.iter().take(30) {
        println!("  PC ${pc:08X}  count {count}");
    }

    println!("\nFirst 40 writes (in order):");
    for (i, (pc, addr, size, val)) in amiga.debug_cpu_write_log.iter().take(40).enumerate() {
        println!("  [{i:3}] PC=${pc:08X}  addr=${addr:08X}  {size}  val=${val:08X}");
    }

    println!("\nLast 40 writes (in order):");
    let n = amiga.debug_cpu_write_log.len();
    let start = n.saturating_sub(40);
    for (i, (pc, addr, size, val)) in amiga.debug_cpu_write_log.iter().enumerate().skip(start) {
        println!("  [{i:3}] PC=${pc:08X}  addr=${addr:08X}  {size}  val=${val:08X}");
    }

    // Disassemble the most-frequent writer PCs by reading bytes from
    // chip RAM (overlay disabled by now) or Kickstart ROM.
    println!("\nBytes around top writer PCs (PC-16 to PC+16):");
    for (pc, count) in pc_counts.iter().take(5) {
        let saved_overlay = amiga.memory.overlay;
        amiga.memory.overlay = false;
        print!("  PC=${pc:08X} count={count}\n      [PC-16..PC):");
        for offset in 0..16u32 {
            let b = amiga.memory.read_byte(pc.wrapping_sub(16).wrapping_add(offset));
            print!(" {b:02X}");
        }
        println!();
        print!("      [PC..PC+16):");
        for offset in 0..16u32 {
            let b = amiga.memory.read_byte(pc.wrapping_add(offset));
            print!(" {b:02X}");
        }
        amiga.memory.overlay = saved_overlay;
        println!();
    }
}
