//! Diagnostic: trace what happens when WB 1.3 is inserted and left
//! to boot. Counts motor / step / disk-DMA / bootblock read events
//! at frame checkpoints so we can see *where* the boot stalls.
//!
//! Not asserting correctness — this is a "show me the picture"
//! probe run with `cargo test -- --nocapture` when the golden-
//! matrix wb13 row is mysteriously blank.

use std::path::PathBuf;

use format_commodore_amiga_adf::Adf;
use peripheral_commodore_amiga_floppy::mfm::{
    MFM_TRACK_BYTES, decode_mfm_track, encode_mfm_track,
};
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

    // Keep a copy of the first 1024 bytes of the ADF so we can
    // byte-compare against the decoded bootblock at the end — if
    // the MFM encode/decode round trip lost a bit, we'll see
    // exactly where.
    let adf_bootblock: Vec<u8> = adf_bytes[..1024].to_vec();

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

    // Phase A probe 1 — strap checkpoints from strap_path_trap.rs.
    // If the CPU's PC ever equals one of these, we know strap got
    // that far. $FE85F2 ("STRAP_EXEC_BOOT") is the one we care
    // about most: it's where strap JSRs into the decoded bootblock.
    const STRAP_POST_CMD_READ: u32 = 0x00FE_85A0;
    const STRAP_DOS_MAGIC_OK: u32 = 0x00FE_85AC;
    const STRAP_EXEC_BOOT: u32 = 0x00FE_85F2;
    const STRAP_ERR_EXIT: u32 = 0x00FE_867C;
    let strap_points = [
        (STRAP_POST_CMD_READ, "post-CMD_READ"),
        (STRAP_DOS_MAGIC_OK, "DOS-magic-OK"),
        (STRAP_EXEC_BOOT, "EXEC_BOOT"),
        (STRAP_ERR_EXIT, "err-exit"),
    ];
    let mut strap_hits = [0u64; 4];

    // Phase A probe 2 — any PC in the bootblock / chip-RAM code
    // range. The decoded bootblock lands in the low 32 KB of chip
    // RAM (at $604C in our runs); any JSR target the bootblock
    // calls would also typically land below $8000.
    let bootblock_range = 0x0000_0400u32..0x0000_8000u32;
    let mut max_chip_pc = 0u32;
    let mut min_chip_pc = u32::MAX;
    let mut chip_pc_hits = 0u64;

    // Phase A probe 3 — CIA-A /IRQ edge count. The scheduler idle
    // loop can only wake on an interrupt; if CIA-A (Timer A or
    // Timer B or TOD alarm) never pulses /IRQ, scheduled tasks
    // never run. Track the trailing 200 frames so early boot
    // traffic doesn't dominate.
    let mut prev_cia_a_irq = false;
    let mut cia_a_irq_edges_total = 0u64;
    let mut cia_a_irq_edges_tail = 0u64;

    let total_frames = *checkpoints.last().unwrap();
    for frame in 0..total_frames {
        for _ in 0..A500_PAL_FRAME_TICKS {
            rt.machine_mut().tick();

            // Per-tick sampling: read PC once, do O(1) checks.
            let m = rt.machine();
            let pc = m.cpu().regs.pc;
            for (i, (addr, _)) in strap_points.iter().enumerate() {
                if pc == *addr {
                    strap_hits[i] = strap_hits[i].saturating_add(1);
                }
            }
            if bootblock_range.contains(&pc) {
                chip_pc_hits += 1;
                if pc > max_chip_pc { max_chip_pc = pc; }
                if pc < min_chip_pc { min_chip_pc = pc; }
            }
            let cia_irq_now = m.cia_a().irq_active();
            if cia_irq_now && !prev_cia_a_irq {
                cia_a_irq_edges_total += 1;
                if frame + 200 >= total_frames {
                    cia_a_irq_edges_tail += 1;
                }
            }
            prev_cia_a_irq = cia_irq_now;
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

    // Record the byte offsets of every $4489 sync pair so we can
    // see the sector boundaries trackdisk scanned.
    let mut sync_offsets = Vec::new();
    for i in 0..(16 * 1024 - 1) {
        let a = m.memory().read_chip_ram_byte(target + i);
        let b = m.memory().read_chip_ram_byte(target + i + 1);
        if a == 0x44 && b == 0x89 {
            sync_offsets.push(i);
        }
    }
    println!("Sync offsets (rel to ${target:06X}):");
    for chunk in sync_offsets.chunks(4) {
        let mut line = String::from("  ");
        for o in chunk {
            line.push_str(&format!("${o:04X} "));
        }
        println!("{line}");
    }

    // Feed the same chip-RAM buffer trackdisk sees through our own
    // `decode_mfm_track`. If our decoder recovers sector 1 correctly
    // but the KS trackdisk output in the bootblock buffer doesn't
    // match, the encoder + our decoder agree but KS's decoder
    // diverges — MFM format bug, not a DMA bug. If our decoder
    // *also* fails on sector 1, the encoder itself is wrong.
    let mut mfm_words: Vec<u16> = Vec::with_capacity(16 * 512);
    for i in (0..(16 * 1024)).step_by(2) {
        let hi = m.memory().read_chip_ram_byte(target + i) as u16;
        let lo = m.memory().read_chip_ram_byte(target + i + 1) as u16;
        mfm_words.push((hi << 8) | lo);
    }
    let decoded = decode_mfm_track(&mfm_words);
    println!("Our decoder: recovered {} sectors from chip RAM", decoded.len());
    for ds in &decoded {
        if ds.sector <= 1 {
            let sec_off = ds.sector as usize * 512;
            let adf_sec = &adf_bootblock[sec_off..sec_off + 512];
            let matches = ds.data.as_slice() == adf_sec;
            println!(
                "  track={} sector={} data matches ADF? {}",
                ds.track, ds.sector,
                if matches { "YES" } else { "NO" }
            );
        }
    }

    // Encode the track fresh from the ADF using our encoder, then
    // compare byte-for-byte with what DMA actually landed in chip
    // RAM. If these diverge, the bug is in the DMA/memory path, not
    // in the encoder itself.
    let adf_full = std::fs::read(
        home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"),
    ).expect("re-read ADF");
    let track0 = &adf_full[0..11 * 512];
    let expected_mfm = encode_mfm_track(track0, 0, 11);
    assert_eq!(expected_mfm.len(), MFM_TRACK_BYTES);
    let mut encode_vs_dma_mismatches = 0u32;
    let mut first_mismatch: Option<(u32, u8, u8)> = None;
    for i in 0..MFM_TRACK_BYTES {
        let got = m.memory().read_chip_ram_byte(target + i as u32);
        let want = expected_mfm[i];
        if got != want {
            encode_vs_dma_mismatches += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some((i as u32, want, got));
            }
        }
    }
    println!(
        "encoder → chip RAM diff: {} / {} bytes, first {}",
        encode_vs_dma_mismatches, MFM_TRACK_BYTES,
        match first_mismatch {
            Some((o, w, g)) => format!("off=${o:04X} want=${w:02X} got=${g:02X}"),
            None => "<none — identical>".into(),
        }
    );

    // What MFM bytes back the corrupted sector-1 decoded byte 9?
    // Sector 1 starts at offset $440 in the MFM buffer. Data
    // starts at sector-internal offset $40 (= $480 absolute).
    // For byte 9 of the decoded sector, the odd half lives at
    // sector_data_start + 9 and the even half at +9 +512.
    let sec1_data_start = 0x440 + 0x40;
    let odd_off = sec1_data_start + 9;
    let even_off = sec1_data_start + 9 + 512;
    println!(
        "sector-1 data byte 9 MFM: odd @ ${:04X} = ${:02X}  even @ ${:04X} = ${:02X}",
        odd_off,
        m.memory().read_chip_ram_byte(target + odd_off as u32),
        even_off,
        m.memory().read_chip_ram_byte(target + even_off as u32),
    );

    // DSKSYNC value at end of run — trackdisk typically sets this
    // to $4489. If it's something else we have a different story.
    println!("paula.dsksync = ${:04X}", m.paula().dsksync());

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
    // garbled. Also verify the Amiga bootblock checksum: sum all
    // 256 longwords with end-around carry, the result should be
    // $FFFFFFFF for a valid block. strap.resource in KS 1.3 will
    // refuse to JSR into a block with a bad checksum — which
    // matches our Phase A observation of EXEC_BOOT reached but
    // bootblock code never executed.
    for base in &dos_hits {
        let mut line = format!("  bootblock @ ${base:06X}: ");
        for i in 0..16 {
            line.push_str(&format!(
                "{:02X} ",
                m.memory().read_chip_ram_byte(base + i)
            ));
        }
        println!("{line}");
        // Bootblock length is 2 sectors = 1024 bytes for OFS/FFS.
        let mut sum: u32 = 0;
        for off in (0..1024u32).step_by(4) {
            let b0 = m.memory().read_chip_ram_byte(base + off) as u32;
            let b1 = m.memory().read_chip_ram_byte(base + off + 1) as u32;
            let b2 = m.memory().read_chip_ram_byte(base + off + 2) as u32;
            let b3 = m.memory().read_chip_ram_byte(base + off + 3) as u32;
            let lw = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
            let (ns, carry) = sum.overflowing_add(lw);
            sum = ns.wrapping_add(if carry { 1 } else { 0 });
        }
        println!(
            "     checksum sum=${sum:08X}  (valid if $FFFFFFFF)  \
             {}",
            if sum == 0xFFFF_FFFF { "PASS" } else { "FAIL" }
        );
        // Byte-wise diff against the raw ADF. The decoded bootblock
        // in chip RAM *should* equal adf[0..1024] byte for byte, with
        // the checksum field reconstructed by the MFM layer. Any
        // mismatch is a round-trip bug somewhere in encode/decode.
        let mut mismatches = 0u32;
        let mut first_mismatch: Option<(u32, u8, u8)> = None;
        for off in 0..1024u32 {
            let got = m.memory().read_chip_ram_byte(base + off);
            let want = adf_bootblock[off as usize];
            if got != want {
                mismatches += 1;
                if first_mismatch.is_none() {
                    first_mismatch = Some((off, want, got));
                }
            }
        }
        println!(
            "     raw-ADF diff: {mismatches} bytes differ, first \
             at {}",
            match first_mismatch {
                Some((o, w, g)) => format!("off=${o:03X} want=${w:02X} got=${g:02X}"),
                None => "<none — identical>".into(),
            }
        );
        // Diff by sector (512 bytes each) so we see which sector
        // decoded cleanly and which didn't. The bootblock is
        // sector 0 + sector 1.
        for sector in 0..2u32 {
            let mut bad = 0u32;
            let sec_base = sector * 512;
            for off in 0..512u32 {
                let got = m.memory().read_chip_ram_byte(base + sec_base + off);
                let want = adf_bootblock[(sec_base + off) as usize];
                if got != want { bad += 1; }
            }
            println!("     sector {sector}: {bad}/512 bytes differ");
        }
        // Dump bootblock[$200..$230] (start of sector 1) side by
        // side with adf[$200..$230] to see the pattern of divergence.
        println!("     sector 1 head (decoded | raw ADF):");
        for row in 0..3u32 {
            let mut got_line = format!("       got ${:03X}: ", 0x200 + row * 16);
            let mut want_line = format!("       adf ${:03X}: ", 0x200 + row * 16);
            for i in 0..16u32 {
                let off = 0x200 + row * 16 + i;
                got_line.push_str(&format!(
                    "{:02X} ",
                    m.memory().read_chip_ram_byte(base + off)
                ));
                want_line.push_str(&format!(
                    "{:02X} ",
                    adf_bootblock[off as usize]
                ));
            }
            println!("{got_line}");
            println!("{want_line}");
        }
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

    // ── Phase A yes/no answers ────────────────────────────────
    println!();
    println!("=== Phase A probes ===");
    println!("1) Strap checkpoint hits (per-tick PC samples):");
    for ((addr, label), hits) in strap_points.iter().zip(strap_hits.iter()) {
        println!("   ${addr:08X}  {label:<16}  hits={hits}");
    }
    println!("2) Bootblock / chip-RAM code range (PC in $0400..$8000):");
    if chip_pc_hits == 0 {
        println!("   NO HIT — CPU never executed bootblock code");
    } else {
        println!(
            "   {chip_pc_hits} PC samples, range ${min_chip_pc:06X}..${max_chip_pc:06X}"
        );
    }
    let cia = rt.machine().cia_a();
    let ta = cia.timer_a();
    let tb = cia.timer_b();
    let cra = cia.cra();
    let crb = cia.crb();
    let icr_status = cia.icr_status();
    let icr_mask = cia.icr_mask();
    println!(
        "3) CIA-A IRQ edges: total={cia_a_irq_edges_total} \
         last-200-frames={cia_a_irq_edges_tail}"
    );
    println!(
        "   Timer A: counter=${ta:04X} CRA=${cra:02X} running={} \
         (START bit0 = {})",
        cia.timer_a_running(),
        if cra & 0x01 != 0 { "SET" } else { "CLEAR" }
    );
    println!(
        "   Timer B: counter=${tb:04X} CRB=${crb:02X} running={}",
        cia.timer_b_running(),
    );
    println!(
        "   CIA-A ICR: status=${icr_status:02X} mask=${icr_mask:02X}"
    );
}
