//! Find what writes the corruption value $C0153E to $C015DC
//! (the mc_Bytes field of the corrupted free MemChunk identified by
//! freetwice_trace).
//!
//! Watches the 8-byte range $C015D8..$C015DF (mc_Next + mc_Bytes of
//! the first allocation) using BOTH:
//!   - debug_cpu_write_watch: captures (PC, addr, size, val) for CPU
//!     writes via service_cpu_bus
//!   - memory.watch_range: captures (addr, size, val) for ANY write
//!     (CPU + DMA + blitter, since they all go through Memory::write_*)
//!
//! Cross-referencing the two tells us:
//!   - Writes in BOTH lists with matching (addr, size, val): CPU writes
//!     (we know the writer PC)
//!   - Writes in memory.watch_log but NOT in cpu_write_log: DMA / blitter
//!     writes (no PC available — but we know who it was by which path)

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

    // Watch the 8 bytes of the corrupted MemChunk.
    let watch_lo: u32 = 0x00C0_15D8;
    let watch_hi: u32 = 0x00C0_15DF;
    amiga.debug_cpu_write_watch = Some((watch_lo, watch_hi));
    amiga.memory.watch_range = Some((watch_lo, watch_hi + 1));

    // freetwice_trace told us the alert fires at tick ~7,702,270.
    // Run a bit longer to also capture any post-alert mess.
    let target_ticks: u64 = 8_500_000;
    for _ in 0..target_ticks {
        amiga.tick_cck();
    }

    println!("CPU writes captured to ${watch_lo:08X}..${watch_hi:08X}: {}",
        amiga.debug_cpu_write_log.len());
    println!("All memory writes captured to ${watch_lo:08X}..${watch_hi:08X}: {}",
        amiga.memory.watch_log.len());

    // Print every CPU write with PC.
    println!("\n== CPU writes (PC, addr, size, val) ==");
    for (pc, addr, size, val) in &amiga.debug_cpu_write_log {
        let kind = if *val == 0xC0153E {
            " <-- THE CORRUPTION VALUE"
        } else {
            ""
        };
        println!("  PC=${pc:08X}  ${addr:08X}  {size}  ${val:08X}{kind}");
    }

    // Print every memory write (CPU + DMA + blitter) — cross-reference
    // by index. CPU writes appear in both lists; non-CPU writes appear
    // only in memory.watch_log.
    println!("\n== ALL memory writes to range (addr, size, val) ==");
    for (i, (addr, size, val)) in amiga.memory.watch_log.iter().enumerate() {
        let in_cpu_log = amiga.debug_cpu_write_log.iter().any(|(_, ca, cs, cv)| {
            ca == addr && cs == size && cv == val
        });
        let source = if in_cpu_log { "CPU" } else { "non-CPU (DMA/blitter)" };
        let kind = if *val == 0xC0153E {
            " <-- THE CORRUPTION VALUE"
        } else {
            ""
        };
        println!("  [{i:3}] ${addr:08X}  {size}  ${val:08X}  {source}{kind}");
    }

    // Final state of the 8 bytes.
    let saved_overlay = amiga.memory.overlay;
    amiga.memory.overlay = false;
    println!("\nFinal bytes at ${watch_lo:08X}..${watch_hi:08X}:");
    print!("  ");
    for off in 0..8u32 {
        let b = amiga.memory.read_byte(watch_lo + off);
        print!("{b:02X} ");
    }
    println!();
    amiga.memory.overlay = saved_overlay;
}
