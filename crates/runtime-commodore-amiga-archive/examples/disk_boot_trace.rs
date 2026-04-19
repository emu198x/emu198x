//! Trace disk activity during the Kickstart 1.3 boot sequence with a
//! bootable Workbench disk inserted.
//!
//! Goal: see whether the ROM *ever* issues a floppy DMA read (DSKLEN
//! write) and whether the bootblock is transferred into chip RAM at all.
//! If the ROM never reads, the issue is upstream (CIA PRA status bits,
//! disk-change flop, drive ready gating, etc). If it reads but we later
//! go off the rails, the issue is in the bootblock path (cksum, jump, etc).

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

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Snapshot of interesting state every N frames, plus event hooks on
    // the floppy-control write path (DSKLEN, DSKPTH/L, DSKSYNC, ADKCON).
    let mut last_dsklen = 0u16;
    let mut last_dskpt = 0u32;
    let mut last_dsksync = 0u16;
    let mut reads_started = 0usize;

    let total_ticks = 400u64 * ccks_per_frame;
    for tick in 0..total_ticks {
        amiga.tick_cck();
        let dsklen = amiga.paula.dsklen;
        let dskpt = amiga.agnus.dsk_pt;
        let dsksync = amiga.paula.dsksync;
        if dsklen != last_dsklen || dskpt != last_dskpt || dsksync != last_dsksync {
            let dma_on = dsklen & 0x8000 != 0;
            let write = dsklen & 0x4000 != 0;
            let words = dsklen & 0x3FFF;
            if dma_on && words != 0 {
                reads_started += 1;
            }
            let frame = tick / ccks_per_frame;
            println!(
                "tick={tick:>10} frame={frame:>3}  DSKLEN=${dsklen:04X} (dma={} wr={} words=${words:04X})  DSKPT=${dskpt:08X}  DSKSYNC=${dsksync:04X}",
                dma_on as u8,
                write as u8
            );
            last_dsklen = dsklen;
            last_dskpt = dskpt;
            last_dsksync = dsksync;
        }
    }

    // Final state summary.
    let pc = amiga.cpu.instr_start_pc;
    let sp = amiga.cpu.regs.active_sp();
    println!("\nFinal: PC=${pc:08X}  SP=${sp:08X}  reads started={reads_started}");

    // Dump first 32 bytes of chip RAM at $C0 (where boot block is usually loaded).
    // Typical: DSKPT starts at $BC (chip RAM buffer used by DoIO(TD_READ)).
    // Real bootblock signature: "DOS\0" at offset 0.
    print!("Chip RAM $000000..$000040: ");
    for i in 0..0x40 {
        print!("{:02X} ", amiga.memory.read_byte(i));
        if i % 16 == 15 {
            print!("\n                           ");
        }
    }
    println!();

    // Dump all custom register writes related to disk/interrupt flow.
    println!("\n--- Last custom writes (disk/int-related) ---");
    for entry in amiga.debug_custom_write_log.iter() {
        if entry.contains("offset=$020") || entry.contains("offset=$022") || entry.contains("offset=$024") || entry.contains("offset=$07E") || entry.contains("offset=$09A") || entry.contains("offset=$09C") {
            println!("  {entry}");
        }
    }

    // Dump CIA-B PRB history (drive control).
    println!("\n--- CIA-B PRB history (floppy control) ---");
    for entry in amiga.debug_cia_b_prb_log.iter() {
        println!("  {entry}");
    }
    println!("Current floppy: motor_on={} motor_spinning={} cylinder={} head={} selected={} has_disk={}",
        amiga.floppy.motor_on(),
        amiga.floppy.motor_spinning(),
        amiga.floppy.cylinder(),
        amiga.floppy.head(),
        amiga.floppy.selected(),
        amiga.floppy.has_disk()
    );
    let st = amiga.floppy.status();
    println!("Current status (active-low): disk_change={} write_protect={} track0={} ready={}",
        st.disk_change, st.write_protect, st.track0, st.ready
    );

    // Dump a few likely bootblock target areas.
    for base in [0x00000400u32, 0x00001000, 0x00007E00, 0x00070000] {
        let sig = [
            amiga.memory.read_byte(base),
            amiga.memory.read_byte(base + 1),
            amiga.memory.read_byte(base + 2),
            amiga.memory.read_byte(base + 3),
        ];
        print!("${base:08X}: ");
        for b in sig {
            print!("{:02X} ", b);
        }
        let ascii: String = sig.iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '.' }).collect();
        println!(" '{ascii}'");
    }
}
