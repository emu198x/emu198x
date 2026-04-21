//! Diagnostic: trace what happens when WB 1.3 is inserted and left
//! to boot. Counts motor / step / disk-DMA / bootblock read events
//! at frame checkpoints so we can see *where* the boot stalls.
//!
//! Not asserting correctness — this is a "show me the picture"
//! probe run with `cargo test -- --nocapture` when the golden-
//! matrix wb13 row is mysteriously blank.

use std::path::PathBuf;

use format_commodore_amiga_adf::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntime, Model};

fn load_artifact(path: &PathBuf) -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!("skipping: missing {}", path.display());
        return None;
    }
    Some(std::fs::read(path).ok()?)
}

#[test]
fn wb13_boot_state_checkpoints() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        &home.join(".emu198x/roms/commodore-amiga/kick13.rom"),
    ) else {
        return;
    };
    let Some(adf_bytes) = load_artifact(
        &home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"),
    ) else {
        return;
    };

    let mut rt = AmigaRuntime::new(Model::A500OcsPalA501, rom)
        .expect("build runtime");
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    rt.machine_mut().insert_adf(adf);

    // Checkpoints at 50-frame (1-second) intervals through the 900-
    // frame settle used by the golden-matrix wb13 row.
    let checkpoints = [1u64, 50, 100, 200, 300, 500, 700, 900];
    let mut next_idx = 0;

    let mut saw_motor_on = false;
    let mut saw_motor_spinning = false;
    let mut saw_disk_dma_pending = false;
    let mut max_step_events = 0u32;
    let mut max_cylinder = 0u32;
    // Histogram of CPU PC sampled once per frame during the last
    // 50 frames — narrow enough to show the loop we're spinning in.
    let mut pc_histogram = std::collections::HashMap::<u32, u32>::new();

    let total_frames = *checkpoints.last().unwrap();
    for frame in 0..total_frames {
        for _ in 0..A500_PAL_FRAME_TICKS {
            rt.machine_mut().tick();
        }
        // Sample the CPU PC every frame over the last 50 frames
        // (by which point the boot has either finished loading or
        // is wedged in a waiter).
        if frame + 50 >= total_frames {
            let pc = rt.machine().cpu().regs.pc;
            *pc_histogram.entry(pc).or_insert(0) += 1;
        }
        let m = rt.machine();
        if m.drive().motor_on() { saw_motor_on = true; }
        if m.drive().motor_spinning() { saw_motor_spinning = true; }
        if m.paula().disk_dma_pending() { saw_disk_dma_pending = true; }
        let steps = m.drive().step_event_counter();
        if steps > max_step_events { max_step_events = steps; }
        let cyl = m.drive().cylinder();
        if cyl > max_cylinder { max_cylinder = cyl; }

        if next_idx < checkpoints.len()
            && frame + 1 == checkpoints[next_idx]
        {
            let m = rt.machine();
            let dmacon = m.dmacon();
            let intena = m.intena();
            let intreq = m.intreq();
            let adkcon = m.adkcon();
            let status = m.drive().status();
            println!(
                "frame {:4}  dmacon=${dmacon:04X} intena=${intena:04X} \
                 intreq=${intreq:04X} adkcon=${adkcon:04X}",
                frame + 1,
            );
            println!(
                "            motor_on={} motor_spinning={} \
                 cyl={} head={} step_ev={} disk_change={} \
                 disk_dma_pending={}",
                m.drive().motor_on(),
                m.drive().motor_spinning(),
                m.drive().cylinder(),
                m.drive().head(),
                m.drive().step_event_counter(),
                status.disk_change,
                m.paula().disk_dma_pending(),
            );
            println!(
                "            drive_selected={} has_disk={} \
                 dsk_writes_total={}",
                m.drive().selected(),
                m.drive().has_disk(),
                m.debug_dsk_log.len(),
            );
            next_idx += 1;
        }
    }

    // Dump the first and last 10 disk-register writes so we can see
    // both the bootstrap sequence (usually DSKSYNC / DSKLEN / DSKPT)
    // and whatever is happening right at the end.
    println!();
    println!("=== first 10 disk register writes ===");
    let dsk = &rt.machine().debug_dsk_log;
    for (cck, pc, off, val) in dsk.iter().take(10) {
        println!("  cck={cck:>9} pc=${pc:06X} off=${off:03X} val=${val:04X}");
    }
    if dsk.len() > 10 {
        println!("=== last 10 disk register writes ===");
        for (cck, pc, off, val) in dsk.iter().skip(dsk.len() - 10) {
            println!("  cck={cck:>9} pc=${pc:06X} off=${off:03X} val=${val:04X}");
        }
    }

    // Sample 128 bytes of chip RAM starting where trackdisk has
    // been pointing DSKPT ($0000_2064 in our runs). If the MFM DMA
    // completed properly the buffer should be non-zero; a bootable
    // disk has DSKSYNC/$4489 patterns embedded in the raw MFM.
    println!();
    let m = rt.machine();
    println!("=== chip RAM at DSKPT target ($2064..$20E4) ===");
    let target = 0x0000_2064u32;
    for row in 0..8u32 {
        let base = target + row * 16;
        let mut line = format!("  ${base:06X}: ");
        for off in 0..16 {
            let b = m.memory().read_chip_ram_byte(base + off);
            line.push_str(&format!("{b:02X} "));
        }
        println!("{line}");
    }

    // Count $44/$89 byte pairs (the DSKSYNC MFM sync word 0x4489)
    // across 16 KB starting at the target. Lots of hits = MFM data
    // landed; zero hits = data never made it.
    let mut sync_hits = 0;
    for i in 0..(16 * 1024 - 1) {
        let a = m.memory().read_chip_ram_byte(target + i);
        let b = m.memory().read_chip_ram_byte(target + i + 1);
        if a == 0x44 && b == 0x89 { sync_hits += 1; }
    }
    println!("DSKSYNC (0x4489) occurrences in 16KB at ${target:06X}: {sync_hits}");

    // Display pipeline state — tells us whether Intuition has put
    // something up (non-zero BPU, non-white COLOR00) or whether we
    // are still on the insert-disk screen or stuck at all-white.
    let bplcon0 = m.bplcon0();
    let bpu = (bplcon0 >> 12) & 0x7;
    let color00 = m.color(0);
    let color01 = m.color(1);
    println!();
    println!(
        "display: bplcon0=${bplcon0:04X} bpu={bpu} \
         color00=${color00:03X} color01=${color01:03X}"
    );

    // Look for the Workbench bootblock's signature or decoded DOS
    // rootblock somewhere in chip RAM. If the bootblock was decoded
    // from MFM, we'd expect "DOS\0" at the head of the bootblock
    // (offset 0 of a 1024-byte block, but trackdisk copies it
    // elsewhere).
    let mut dos_hits = Vec::new();
    for base in (0..(512 * 1024)).step_by(4) {
        if m.memory().read_chip_ram_byte(base) == b'D'
            && m.memory().read_chip_ram_byte(base + 1) == b'O'
            && m.memory().read_chip_ram_byte(base + 2) == b'S'
        {
            dos_hits.push(base);
            if dos_hits.len() >= 4 { break; }
        }
    }
    println!("'DOS' string occurrences (first 4): {dos_hits:?}");

    // Dump whatever's at each DOS hit — shows whether the
    // bootblock was decoded into a normal-looking block
    // (DOS\0 ... CHECKSUM ... ROOTBLOCK ... code) or something
    // garbled.
    for base in &dos_hits {
        let mut line = format!("  bootblock @ ${base:06X}: ");
        for i in 0..16 {
            line.push_str(&format!(
                "{:02X} ",
                m.memory().read_chip_ram_byte(base + i)
            ));
        }
        println!("{line}");
    }

    // CPU program counter at end — helps identify whether we're
    // spinning in a known Kickstart routine (WaitIO, etc.) or a
    // forward-progress loop.
    println!(
        "cpu_pc=${:06X} intena=${:04X} intreq=${:04X}",
        m.cpu().regs.pc, m.intena(), m.intreq()
    );

    // Top 5 most-frequent PCs sampled over the last 50 frames —
    // a tight loop will show one PC dominating.
    let mut pc_sorted: Vec<_> = pc_histogram.iter().collect();
    pc_sorted.sort_by(|a, b| b.1.cmp(a.1));
    println!("=== top PCs over last 50 frames ===");
    for (pc, count) in pc_sorted.iter().take(5) {
        println!("  pc=${pc:06X}  samples={count}/50");
    }

    // Dump 64 bytes around the hottest PC so we can see the loop
    // body. Readable even without a disassembler — useful for
    // spotting STOP #$2000 / BRA.S idle waits.
    if let Some((hot_pc, _)) = pc_sorted.first() {
        let base = hot_pc.saturating_sub(40);
        println!("=== bytes around hottest PC (${hot_pc:06X}) ===");
        for row in 0..4 {
            let row_base = base + row * 16;
            let mut line = format!("  ${row_base:06X}: ");
            for i in 0..16 {
                let b = rt.machine().memory().read_byte(row_base + i);
                line.push_str(&format!("{b:02X} "));
            }
            println!("{line}");
        }
    }

    // Also dump the CPU register file — if we're waiting on an
    // address like a MsgPort's sigBit, or polling a memory loc,
    // the pointer is in one of A0..A6.
    let cpu = rt.machine().cpu();
    println!(
        "  CPU: d0=${:08X} d1=${:08X} d2=${:08X} a0=${:08X} a1=${:08X} \
         a6=${:08X} sp=${:08X}",
        cpu.regs.d[0], cpu.regs.d[1], cpu.regs.d[2],
        cpu.regs.a[0], cpu.regs.a[1], cpu.regs.a[6],
        cpu.regs.ssp,
    );

    println!();
    println!(
        "summary: motor_on={saw_motor_on} motor_spinning={saw_motor_spinning} \
         disk_dma_pending={saw_disk_dma_pending} \
         max_step_events={max_step_events} max_cylinder={max_cylinder} \
         dsk_writes={}",
        dsk.len(),
    );
}
