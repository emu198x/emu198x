//! Diagnostic: trace what happens when WB 1.3 is inserted and left
//! to boot. Counts motor / step / disk-DMA / bootblock read events
//! at frame checkpoints so we can see *where* the boot stalls.
//!
//! Not asserting correctness — this is a "show me the picture"
//! probe run with `cargo test -- --nocapture` when the golden-
//! matrix wb13 row is mysteriously blank.

use std::error::Error;
use std::path::PathBuf;

use format_commodore_amiga_adf::Adf;
use peripheral_commodore_amiga_floppy::mfm::{MFM_TRACK_BYTES, decode_mfm_track, encode_mfm_track};
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntime, Model};

fn load_artifact(path: &PathBuf) -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!("skipping: missing {}", path.display());
        return None;
    }
    std::fs::read(path).ok()
}

#[test]
fn wb13_boot_state_checkpoints() -> Result<(), Box<dyn Error>> {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        return Ok(());
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        return Ok(());
    };

    // Keep a copy of the first 1024 bytes of the ADF so we can
    // byte-compare against the decoded bootblock at the end — if
    // the MFM encode/decode round trip lost a bit, we'll see
    // exactly where.
    let adf_bootblock: Vec<u8> = adf_bytes[..1024].to_vec();

    let mut rt = AmigaRuntime::new(Model::A500OcsPalA501, rom)?;
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    rt.machine_mut().insert_adf(adf);

    // Watch trackdisk's per-unit buffer[3] (state[$4E] + 3 = $19E3),
    // which holds the result code of the most-recent CMD_READ
    // attempt. Captures every CPU write to that byte so we can see
    // the sequence of validation results over the whole run.
    rt.machine_mut().debug_watch_addr = Some((0x0000_19E3, 1));

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
    // trackdisk validation sub-failures that all return $1B:
    // $FEACFA = header cksum mismatch
    // $FEAD10 = format byte (info[0]) != $FF
    // $FEAD1C = track byte (info[1]) != expected
    const TD_CKSUM_MISMATCH: u32 = 0x00FE_ACFA;
    const TD_FMT_MISMATCH: u32 = 0x00FE_AD10;
    const TD_TRK_MISMATCH: u32 = 0x00FE_AD1C;
    // graphics.library Init entry, found by walking ROM resident
    // table — graphics.library resident @ $FC53E4, init at $FCABA2.
    // If this PC is never sampled, gfx library was never initialized
    // → no QBlit dispatcher, no BLITINT setup, etc.
    const GFX_LIB_INIT: u32 = 0x00FC_ABA2;
    // Six PCs in the gfx library code area that contain
    // `move.w #$8040, $DFF09A.l` (= SET INTEN + INT_BLIT). If any
    // of these is hit, gfx tried to enable BLITINT.
    const GFX_BLIT_ENABLE_1: u32 = 0x00FC_5916;
    const GFX_BLIT_ENABLE_2: u32 = 0x00FC_5984;
    const GFX_BLIT_ENABLE_3: u32 = 0x00FC_641E;
    const GFX_BLIT_ENABLE_4: u32 = 0x00FC_6508;
    const GFX_BLIT_ENABLE_5: u32 = 0x00FC_6DE8;
    const GFX_BLIT_ENABLE_6: u32 = 0x00FC_6F18;
    // trackdisk decode-and-copy for READ — called once per sector
    // after successful validation. (CMD_WRITE goes through $FEA7CA
    // / $FEA81E — those are the WRITE side. CMD_READ uses these.)
    const TD_READ_DECODE_CALL: u32 = 0x00FE_A552; // bsr.w $FEA932
    const TD_READ_DECODE_ENTRY: u32 = 0x00FE_A932; // entry point
    const TD_READ_DECODE_CB: u32 = 0x00FE_A970; // QBlit callback
    const TD_READ_BLT0_WRITE: u32 = 0x00FE_A996; // BLTCON0=$1DD8 write
    let strap_points = [
        (STRAP_POST_CMD_READ, "post-CMD_READ"),
        (STRAP_DOS_MAGIC_OK, "DOS-magic-OK"),
        (STRAP_EXEC_BOOT, "EXEC_BOOT"),
        (STRAP_ERR_EXIT, "err-exit"),
        (TD_CKSUM_MISMATCH, "td $1B cksum-mismatch (BNE.W taken)"),
        (TD_FMT_MISMATCH, "td $1B fmt!=$FF (BNE.W taken)"),
        (TD_TRK_MISMATCH, "td $1B track-mismatch (BNE.W taken)"),
        (GFX_LIB_INIT, "graphics.library Init entry"),
        (GFX_BLIT_ENABLE_1, "gfx SET BLITINT @ $FC5916"),
        (GFX_BLIT_ENABLE_2, "gfx SET BLITINT @ $FC5984"),
        (GFX_BLIT_ENABLE_3, "gfx SET BLITINT @ $FC641E"),
        (GFX_BLIT_ENABLE_4, "gfx SET BLITINT @ $FC6508"),
        (GFX_BLIT_ENABLE_5, "gfx SET BLITINT @ $FC6DE8"),
        (GFX_BLIT_ENABLE_6, "gfx SET BLITINT @ $FC6F18"),
        (TD_READ_DECODE_CALL, "td READ-decode call site $FEA552"),
        (TD_READ_DECODE_ENTRY, "td READ-decode entry $FEA932"),
        (TD_READ_DECODE_CB, "td READ-decode QBlit callback $FEA970"),
        (TD_READ_BLT0_WRITE, "td READ-decode BLTCON0=$1DD8 $FEA996"),
    ];
    let mut strap_hits = [0u64; 18];

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

    let total_frames = *checkpoints.last().expect("checkpoints not empty");
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
                if pc > max_chip_pc {
                    max_chip_pc = pc;
                }
                if pc < min_chip_pc {
                    min_chip_pc = pc;
                }
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
        if m.drive().motor_on() {
            saw_motor_on = true;
        }
        if m.drive().motor_spinning() {
            saw_motor_spinning = true;
        }
        if m.paula().disk_dma_pending() {
            saw_disk_dma_pending = true;
        }
        let steps = m.drive().step_event_counter();
        if steps > max_step_events {
            max_step_events = steps;
        }
        let cyl = m.drive().cylinder();
        if cyl > max_cylinder {
            max_cylinder = cyl;
        }

        if next_idx < checkpoints.len() && frame + 1 == checkpoints[next_idx] {
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
        let mut line = String::from("  header: ");
        line.push_str(&format!("${base:06X}: "));
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
        if a == 0x44 && b == 0x89 {
            sync_hits += 1;
        }
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

    // For each $4489 sync, decode the 8-byte header immediately
    // following it into (format, track, sector, sectors_until_gap).
    // The pair ($4489 $4489) takes 4 bytes; header odd-half starts 4
    // bytes after the FIRST sync word. So if a sync pair is at
    // offsets N and N+2, odd header is at N+4, even header at N+8.
    //
    // Real trackdisk at $FEAC62 expects: info[0] == $FF,
    // info[1] == track, info[3] = sectors_until_gap, and it uses
    // sectors_until_gap to compute how much to blit. If our DMA
    // buffer presents sectors in an unexpected order (e.g. first
    // sync is sector 10 rather than sector 0) the blit-count path
    // in trackdisk will double-blit into the wrong slots.
    let decode_mfm_byte = |odd: u8, even: u8| -> u8 { ((odd & 0x55) << 1) | (even & 0x55) };
    println!("Sync header decodes (pair_offset: [fmt trk sec stg]):");
    // sync_offsets has both halves of the pair ($4489 twice); group
    // them so we only print one header per sector.
    let mut pair_starts = Vec::new();
    let mut i = 0;
    while i < sync_offsets.len() {
        let start = sync_offsets[i];
        pair_starts.push(start);
        if i + 1 < sync_offsets.len() && sync_offsets[i + 1] == start + 2 {
            i += 2;
        } else {
            i += 1;
        }
    }
    for (pair_idx, sync_start) in pair_starts.iter().enumerate() {
        // sync pair occupies bytes [sync_start..sync_start+4).
        // Header odd at sync_start+4, even at sync_start+8.
        let odd_off = *sync_start + 4;
        let even_off = *sync_start + 8;
        let mut info = [0u8; 4];
        for k in 0..4u32 {
            let o = m.memory().read_chip_ram_byte(target + odd_off + k);
            let e = m.memory().read_chip_ram_byte(target + even_off + k);
            info[k as usize] = decode_mfm_byte(o, e);
        }
        println!(
            "  [{pair_idx:2}] @${sync_start:04X}: fmt=${:02X} trk=${:3} sec=${:2} stg=${:2}",
            info[0], info[1], info[2], info[3]
        );
    }

    // KS trackdisk blits from "gap_pos" = sync_pos - 4 into its internal
    // decode buffer. That means for sector 1 (sync at $0C9E in our DMA
    // log), Blit #2 source begins at $085A (sector 0 gap) and runs for
    // 10*1088 bytes. We want to verify:
    //   (a) chip RAM at sector 1's gap = $0C9A..$0C9D is all-AA (gap
    //       filler) — proves the track format is correct at the boundary.
    //   (b) chip RAM at sector 1's data byte 9 (odd and even halves)
    //       are both $AA — proves the DMA wrote zeros there.
    // If either is not $AA, our encoder or DMA is corrupt at this point.
    let sec1_gap_off = 0x0C9Au32;
    let sec1_sync_off = 0x0C9Eu32;
    let sec1_data_off = sec1_gap_off + 64; // 64 = sector data offset
    let sec1_b9_odd_off = sec1_data_off + 9;
    let sec1_b9_even_off = sec1_data_off + 9 + 512;
    let sec1_b9_10_11_odd = [
        m.memory().read_chip_ram_byte(target + sec1_data_off + 9),
        m.memory().read_chip_ram_byte(target + sec1_data_off + 10),
        m.memory().read_chip_ram_byte(target + sec1_data_off + 11),
    ];
    let sec1_b9_10_11_even = [
        m.memory()
            .read_chip_ram_byte(target + sec1_data_off + 9 + 512),
        m.memory()
            .read_chip_ram_byte(target + sec1_data_off + 10 + 512),
        m.memory()
            .read_chip_ram_byte(target + sec1_data_off + 11 + 512),
    ];
    println!(
        "sector-1 MFM in chip RAM @ ${:04X} gap, ${:04X} sync, ${:04X} data:",
        sec1_gap_off, sec1_sync_off, sec1_data_off
    );
    println!(
        "  gap bytes: {:02X} {:02X} {:02X} {:02X} (expect all $AA)",
        m.memory().read_chip_ram_byte(target + sec1_gap_off),
        m.memory().read_chip_ram_byte(target + sec1_gap_off + 1),
        m.memory().read_chip_ram_byte(target + sec1_gap_off + 2),
        m.memory().read_chip_ram_byte(target + sec1_gap_off + 3)
    );
    println!(
        "  byte 9-11 odd:  {:02X} {:02X} {:02X} @${:04X} (expect all $AA)",
        sec1_b9_10_11_odd[0], sec1_b9_10_11_odd[1], sec1_b9_10_11_odd[2], sec1_b9_odd_off
    );
    println!(
        "  byte 9-11 even: {:02X} {:02X} {:02X} @${:04X} (expect all $AA)",
        sec1_b9_10_11_even[0], sec1_b9_10_11_even[1], sec1_b9_10_11_even[2], sec1_b9_even_off
    );

    // Dump trackdisk's per-unit buffer header. At base+$3 is the
    // most-recent error code from the sector validation function:
    //   $00..$0A = success (sector_num of first found)
    //   $15 = no first sync found
    //   $16 = gap or sync mismatch in slot loop
    //   $17 = format/track/sector mismatch in slot loop
    //   $18 = header checksum mismatch
    //   $19 = data checksum mismatch
    //   $1A = no next sync found after Blit #1
    //   $1B = first-sync header / cksum mismatch
    // state[$4E] points to a buffer that's $684 bytes into the
    // chip-RAM region trackdisk allocates. The DMA target $2064 IS
    // state[$4E] + $684, so state[$4E] = $2064 - $684 = $19E0.
    // buffer[3] holds the most-recent validation error code, written
    // by `move.b d0, $3(a2)` at $FEA652.
    let buf_base = target - 0x684;
    println!("\nTrackdisk unit buffer header @ ${buf_base:06X} (state[$4E]):");
    let mut line = String::from("  header: ");
    for k in 0..16u32 {
        line.push_str(&format!(
            "{:02X} ",
            m.memory().read_chip_ram_byte(buf_base + k)
        ));
    }
    println!("{line}");
    let err = m.memory().read_chip_ram_byte(buf_base + 3);
    let err_label = match err {
        0..=10 => "success (sector_num)",
        0x15 => "$15 NO FIRST SYNC FOUND",
        0x16 => "$16 GAP/SYNC MISMATCH IN SLOT LOOP",
        0x17 => "$17 FORMAT/TRACK/SECTOR MISMATCH",
        0x18 => "$18 HDR CKSUM MISMATCH",
        0x19 => "$19 DATA CKSUM MISMATCH",
        0x1A => "$1A NO NEXT SYNC FOUND",
        0x1B => "$1B FIRST-SYNC HEADER/CKSUM",
        _ => "(unknown)",
    };
    println!("  buffer[3] (last validation result) = ${err:02X}  → {err_label}");

    // Sequence of writes to buffer[3].
    let watches = &rt.machine().debug_watch_writes;
    println!("  buffer[3] write sequence ({} total):", watches.len());
    for (cck, pc, addr, val, is_word) in watches.iter().take(20) {
        println!("    cck={cck:>9} pc=${pc:06X} addr=${addr:06X} val=${val:04X} word={is_word}");
    }
    if watches.len() > 20 {
        println!("    ... and {} more", watches.len() - 20);
        // Show last few too.
        for (cck, pc, addr, val, is_word) in watches.iter().rev().take(5).rev() {
            println!(
                "    cck={cck:>9} pc=${pc:06X} addr=${addr:06X} val=${val:04X} word={is_word}"
            );
        }
    }

    // The validation track check at $FEAD1C compares info[1] (track
    // byte from the decoded sync header) against $4B(a3) = the
    // expected track stored on the trackdisk unit struct (a3).
    // We don't easily know a3, but expected track for cyl 0 head 0
    // should be 0. Dump cyl/head and compare with what the first
    // sync's info shows. If the encoder produced track 0 but
    // trackdisk reads a different value, we have an encoder/decoder
    // disagreement at the per-sector header level.
    println!(
        "  drive cyl={} head={}  → expected info[1] = {}",
        m.drive().cylinder(),
        m.drive().head(),
        m.drive().cylinder() * 2 + m.drive().head()
    );

    // The validation reads the cksum from DMA buffer at gap_pos+8
    // onwards (info_odd, info_even, label_odd, label_even, then
    // hdr_cksum_odd, hdr_cksum_even). It XORs MFM longs of
    // info+label (40 bytes), masks with $5555_5555, compares against
    // decoded stored hdr_cksum at gap+$30.
    //
    // Compute this ourselves over the FIRST sync in the current DMA
    // buffer (at offset $0162-$4 = $015E gap-pos) and compare to
    // what we read from $015E+$30 = $018E (hdr_cksum_odd).
    let first_sync_gap = 0x015Eu32; // sector 10's gap (end of run)
    let info_pos = first_sync_gap + 8;
    let cksum_pos = first_sync_gap + 0x30;
    let mut info_label = [0u32; 10];
    for i in 0..10u32 {
        let mut w = 0u32;
        for k in 0..4u32 {
            w = (w << 8) | u32::from(m.memory().read_chip_ram_byte(target + info_pos + i * 4 + k));
        }
        info_label[i as usize] = w;
    }
    let mut xor: u32 = 0;
    for v in &info_label {
        xor ^= *v;
    }
    let computed = xor & 0x5555_5555;
    let cksum_odd = (0..4u32).fold(0u32, |a, k| {
        (a << 8) | u32::from(m.memory().read_chip_ram_byte(target + cksum_pos + k))
    });
    let cksum_even = (0..4u32).fold(0u32, |a, k| {
        (a << 8) | u32::from(m.memory().read_chip_ram_byte(target + cksum_pos + 4 + k))
    });
    let stored = ((cksum_odd & 0x5555_5555) << 1) | (cksum_even & 0x5555_5555);
    println!("  hdr cksum verify (FIRST sync at end of run, gap=${first_sync_gap:04X}):");
    println!("    info+label longs: {:08X?}", info_label);
    println!("    computed XOR & $55555555 = ${computed:08X}");
    println!(
        "    stored hdr_cksum: odd_long=${cksum_odd:08X} even_long=${cksum_even:08X} decoded=${stored:08X}"
    );
    println!(
        "    {}",
        if computed == stored {
            "✓ MATCH (validation should pass)"
        } else {
            "✗ MISMATCH (this is what triggers $1B)"
        }
    );

    // Verify cksum for ALL 11 syncs in the DMA buffer.
    println!("  cksum check across all 11 syncs in DMA buffer:");
    for sync_byte_off in &pair_starts {
        let gap_pos = (*sync_byte_off).wrapping_sub(4);
        let info_pos = gap_pos + 8;
        let cksum_pos = gap_pos + 0x30;
        let mut info_label = [0u32; 10];
        for i in 0..10u32 {
            let mut w = 0u32;
            for k in 0..4u32 {
                w = (w << 8)
                    | u32::from(m.memory().read_chip_ram_byte(target + info_pos + i * 4 + k));
            }
            info_label[i as usize] = w;
        }
        let xor: u32 = info_label.iter().fold(0u32, |a, v| a ^ v);
        let computed = xor & 0x5555_5555;
        let cksum_odd = (0..4u32).fold(0u32, |a, k| {
            (a << 8) | u32::from(m.memory().read_chip_ram_byte(target + cksum_pos + k))
        });
        let cksum_even = (0..4u32).fold(0u32, |a, k| {
            (a << 8) | u32::from(m.memory().read_chip_ram_byte(target + cksum_pos + 4 + k))
        });
        let stored = ((cksum_odd & 0x5555_5555) << 1) | (cksum_even & 0x5555_5555);
        let mark = if computed == stored { "OK" } else { "FAIL" };
        println!("    gap=${gap_pos:04X}: computed=${computed:08X} stored=${stored:08X}  {mark}");
    }

    // KEY: The trackdisk decode buffer is at state[$4E] + $680, and
    // the DMA buffer is at state[$4E] + $684 — i.e., the decode
    // buffer is 4 bytes BEFORE the DMA buffer. The blits copy in-place,
    // shifting MFM data 4 bytes earlier so each sector's gap+sync
    // lands at the slot start. Compute decode_buf address directly
    // and dump its key offsets:
    let decode_buf = target - 4; // state[$4E]+$680 = DSKPT target - 4
    println!("\nDecode buffer @ ${decode_buf:06X}:");
    // Dump first 16 bytes (slot 0 gap + sync + info odd).
    let mut line = String::from("  slot  0: ");
    for k in 0..16u32 {
        line.push_str(&format!(
            "{:02X} ",
            m.memory().read_chip_ram_byte(decode_buf + k)
        ));
    }
    println!("{line}");
    // Slot 2 (= sector 1's slot given first sync was sector 10):
    let slot2 = decode_buf + 2 * 1088;
    let mut line = String::from("  slot  2: ");
    for k in 0..16u32 {
        line.push_str(&format!(
            "{:02X} ",
            m.memory().read_chip_ram_byte(slot2 + k)
        ));
    }
    println!("{line}");
    // Dump sector 1 slot 2's data byte 9 odd/even halves.
    let slot2_d9_odd = slot2 + 64 + 9;
    let slot2_d9_even = slot2_d9_odd + 512;
    println!(
        "  slot 2 data byte 9 odd @${slot2_d9_odd:06X}: {:02X}  even @${slot2_d9_even:06X}: {:02X}",
        m.memory().read_chip_ram_byte(slot2_d9_odd),
        m.memory().read_chip_ram_byte(slot2_d9_even),
    );
    // Decode all 11 slots' info headers to confirm layout.
    println!("  decode_buf slot info headers:");
    for slot in 0..11u32 {
        let slot_base = decode_buf + slot * 1088;
        let odd = slot_base + 8;
        let even = slot_base + 12;
        let mut info = [0u8; 4];
        for k in 0..4u32 {
            let o = m.memory().read_chip_ram_byte(odd + k);
            let e = m.memory().read_chip_ram_byte(even + k);
            info[k as usize] = decode_mfm_byte(o, e);
        }
        // Also decode data bytes 9-11.
        let data_odd_base = slot_base + 64;
        let data_even_base = slot_base + 64 + 512;
        let b9 = decode_mfm_byte(
            m.memory().read_chip_ram_byte(data_odd_base + 9),
            m.memory().read_chip_ram_byte(data_even_base + 9),
        );
        let b10 = decode_mfm_byte(
            m.memory().read_chip_ram_byte(data_odd_base + 10),
            m.memory().read_chip_ram_byte(data_even_base + 10),
        );
        let b11 = decode_mfm_byte(
            m.memory().read_chip_ram_byte(data_odd_base + 11),
            m.memory().read_chip_ram_byte(data_even_base + 11),
        );
        println!(
            "    slot {slot:2} @${slot_base:06X}: fmt=${:02X} trk=${:3} sec=${:2} stg=${:2}  data[9..12]={b9:02X} {b10:02X} {b11:02X}",
            info[0], info[1], info[2], info[3]
        );
    }

    // Still scan chip RAM for backup — maybe trackdisk put decode_buf
    // elsewhere for cylinders we're not currently on.
    println!();
    println!("Search chip RAM for OTHER decode-buf-like patterns...");
    let mut decode_buf_candidates = Vec::new();
    for base in (0x0000..0x080000u32).step_by(2) {
        // Cheap screen first: look for $44894489 at base + 4.
        let s0 = m.memory().read_chip_ram_byte(base + 4);
        let s1 = m.memory().read_chip_ram_byte(base + 5);
        let s2 = m.memory().read_chip_ram_byte(base + 6);
        let s3 = m.memory().read_chip_ram_byte(base + 7);
        if s0 != 0x44 || s1 != 0x89 || s2 != 0x44 || s3 != 0x89 {
            continue;
        }
        // Second slot: sync at base + 1088 + 4 = base + 1092.
        let t0 = m.memory().read_chip_ram_byte(base + 1092);
        let t1 = m.memory().read_chip_ram_byte(base + 1093);
        let t2 = m.memory().read_chip_ram_byte(base + 1094);
        let t3 = m.memory().read_chip_ram_byte(base + 1095);
        if t0 != 0x44 || t1 != 0x89 || t2 != 0x44 || t3 != 0x89 {
            continue;
        }
        decode_buf_candidates.push(base);
        if decode_buf_candidates.len() >= 5 {
            break;
        }
    }
    println!("  candidates: {decode_buf_candidates:X?}");
    for buf_base in &decode_buf_candidates {
        // Decode the info header of each slot to see which sector is
        // in each position. If slot 2 really has sector 1, then
        // sector 1's data extraction should give us what's in the
        // bootblock — or reveal the corruption.
        println!("  decode_buf @${buf_base:06X}:");
        for slot in 0..11u32 {
            let slot_base = buf_base + slot * 1088;
            let odd = slot_base + 8;
            let even = slot_base + 12;
            let mut info = [0u8; 4];
            for k in 0..4u32 {
                let o = m.memory().read_chip_ram_byte(odd + k);
                let e = m.memory().read_chip_ram_byte(even + k);
                info[k as usize] = decode_mfm_byte(o, e);
            }
            // Decode data byte 9 from slot.
            let data_odd = slot_base + 64 + 9;
            let data_even = slot_base + 64 + 9 + 512;
            let o = m.memory().read_chip_ram_byte(data_odd);
            let e = m.memory().read_chip_ram_byte(data_even);
            let byte9 = decode_mfm_byte(o, e);
            println!(
                "    slot {slot:2}: fmt=${:02X} trk=${:3} sec=${:2} stg=${:2}  \
                 data[9]=${byte9:02X} (odd@${data_odd:06X}=${o:02X} \
                 even@${data_even:06X}=${e:02X})",
                info[0], info[1], info[2], info[3]
            );
        }
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
    println!(
        "Our decoder: recovered {} sectors from chip RAM",
        decoded.len()
    );
    for ds in &decoded {
        if ds.sector <= 1 {
            let sec_off = ds.sector as usize * 512;
            let adf_sec = &adf_bootblock[sec_off..sec_off + 512];
            let matches = ds.data.as_slice() == adf_sec;
            println!(
                "  track={} sector={} data matches ADF? {}",
                ds.track,
                ds.sector,
                if matches { "YES" } else { "NO" }
            );
        }
    }

    // Encode the track fresh from the ADF using our encoder, then
    // compare byte-for-byte with what DMA actually landed in chip
    // RAM. If these diverge, the bug is in the DMA/memory path, not
    // in the encoder itself.
    let adf_full = std::fs::read(home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
        .expect("re-read ADF");
    let track0 = &adf_full[0..11 * 512];
    let expected_mfm = encode_mfm_track(track0, 0, 11);
    assert_eq!(expected_mfm.len(), MFM_TRACK_BYTES);
    let mut encode_vs_dma_mismatches = 0u32;
    let mut first_mismatch: Option<(u32, u8, u8)> = None;
    for (i, &want) in expected_mfm.iter().enumerate().take(MFM_TRACK_BYTES) {
        let got = m.memory().read_chip_ram_byte(target + i as u32);
        if got != want {
            encode_vs_dma_mismatches += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some((i as u32, want, got));
            }
        }
    }
    println!(
        "encoder → chip RAM diff: {} / {} bytes, first {}",
        encode_vs_dma_mismatches,
        MFM_TRACK_BYTES,
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

    // Bit-level sync scan: look for the 32-bit pattern $44894489
    // at *every* bit offset within the track region of chip RAM,
    // not just byte-aligned word pairs. A trackdisk that scans
    // bit-by-bit would pick up any such match; a byte-aligned
    // scan (like ours) only sees 22. Extras would explain KS 1.3
    // finding a "false" sync inside sector 0's data and decoding
    // the following bytes as a spurious sector 1 header.
    let mut bit_aligned = 0usize;
    let mut byte_aligned = 0usize;
    for bit_off in 0..((MFM_TRACK_BYTES - 4) * 8) {
        let byte_off = bit_off / 8;
        let in_byte = bit_off % 8;
        // Assemble 40 bits starting at bit_off (we need 32 sync
        // bits plus enough to handle byte crossings). Read 5
        // consecutive chip-RAM bytes, compose into a u64, then
        // shift down to extract the 32-bit candidate.
        let mut acc: u64 = 0;
        for i in 0..5 {
            acc = (acc << 8)
                | u64::from(
                    m.memory()
                        .read_chip_ram_byte(target + (byte_off + i) as u32),
                );
        }
        // acc has byte[0] in bits 32..39, byte[4] in bits 0..7.
        // Shift right so the first candidate bit is at bit 31.
        let candidate = ((acc >> (8 - in_byte)) & 0xFFFF_FFFF) as u32;
        if candidate == 0x4489_4489 {
            bit_aligned += 1;
            if in_byte == 0 {
                byte_aligned += 1;
            }
        }
    }
    println!(
        "$4489_$4489 bit-level matches: {bit_aligned} total, \
         {byte_aligned} byte-aligned"
    );

    // KS 1.3's sync scanner at $FEABCC enters "try to match
    // $4489-shifted" mode on ANY byte-aligned word matching
    // either $AAAA or $5555. $AAAA is expected (gap filler), but
    // $5555 should be rare in a well-formed track — every hit is
    // a candidate place where the sync-match table might fire on
    // a coincidental pattern.
    let mut aaaa_words = 0usize;
    let mut five_words = 0usize;
    for i in (0..(MFM_TRACK_BYTES - 1)).step_by(2) {
        let hi = m.memory().read_chip_ram_byte(target + i as u32);
        let lo = m.memory().read_chip_ram_byte(target + (i + 1) as u32);
        let w = ((hi as u16) << 8) | (lo as u16);
        if w == 0xAAAA {
            aaaa_words += 1;
        }
        if w == 0x5555 {
            five_words += 1;
        }
    }
    println!("byte-aligned $AAAA words: {aaaa_words}, $5555 words: {five_words}");

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
            if dos_hits.len() >= 4 {
                break;
            }
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
            line.push_str(&format!("{:02X} ", m.memory().read_chip_ram_byte(base + i)));
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
                Some((off, want, got)) => format!("off=${off:03X} want=${want:02X} got=${got:02X}"),
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
                if got != want {
                    bad += 1;
                }
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
                want_line.push_str(&format!("{:02X} ", adf_bootblock[off as usize]));
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
        m.cpu().regs.pc,
        m.intena(),
        m.intreq()
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
    if let Some(&(&hot_pc, _)) = pc_sorted.first() {
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
        cpu.regs.d[0],
        cpu.regs.d[1],
        cpu.regs.d[2],
        cpu.regs.a[0],
        cpu.regs.a[1],
        cpu.regs.a[6],
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
        println!("   {chip_pc_hits} PC samples, range ${min_chip_pc:06X}..${max_chip_pc:06X}");
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
    println!("   CIA-A ICR: status=${icr_status:02X} mask=${icr_mask:02X}");
    println!(
        "   Blit starts (BLTSIZE writes): {}",
        rt.machine().debug_blit_starts
    );
    // Look at all INTENA writes — see if BLIT (bit 6) ever gets enabled.
    let intena_log = &rt.machine().debug_intena_log;
    let blit_enables: Vec<_> = intena_log
        .iter()
        .filter(|(_, _, w, _, after)| (w & 0x8040) == 0x8040 && (after & 0x40) != 0)
        .collect();
    let blit_disables: Vec<_> = intena_log
        .iter()
        .filter(|(_, _, w, before, after)| {
            (w & 0x0040) != 0 && (w & 0x8000) == 0 && (before & 0x40) != 0 && (after & 0x40) == 0
        })
        .collect();
    println!(
        "   INTENA BLIT-bit changes: enables={} disables={} total writes={}",
        blit_enables.len(),
        blit_disables.len(),
        intena_log.len()
    );
    for (cck, pc, w, b, a) in blit_enables.iter().take(3) {
        println!("     enable cck={cck:>9} pc=${pc:06X} w=${w:04X} b=${b:04X} a=${a:04X}");
    }
    println!(
        "   Final intena=${:04X}, BLIT bit ({}): {}",
        rt.machine().intena(),
        if rt.machine().intena() & 0x40 != 0 {
            "set"
        } else {
            "CLEAR — gfx library never enabled BLITINT"
        },
        if rt.machine().intena() & 0x40 != 0 {
            "✓"
        } else {
            "✗"
        }
    );

    // Show all blits whose dest pointer falls in the bootblock buffer
    // range ($604C..$6850). DESC mode adjusts dst by length, so check
    // both raw dpt and dpt-len.
    println!("   Blits writing to bootblock buffer ($604C..$6850):");
    let bb_lo = 0x604Cu32;
    let bb_hi = 0x6850u32;
    let log = &rt.machine().debug_blit_log;
    for (cck, pc, c0, c1, apt, bpt, _cpt, dpt, sz) in log {
        // For DESC mode (BLTCON1 bit 1 set), dst is the END pointer.
        // For ascending, dst is the start. Either way, the write
        // range covers a length of ((sz>>6)*(sz&$3F)*2) bytes.
        let height = (sz >> 6) as u32;
        let width = (sz & 0x3F) as u32;
        let len_bytes =
            if width == 0 { 64 } else { width } * 2 * if height == 0 { 1024 } else { height };
        let desc = (c1 & 0x02) != 0;
        let (dlo, dhi) = if desc {
            (dpt.wrapping_sub(len_bytes), *dpt)
        } else {
            (*dpt, dpt.wrapping_add(len_bytes))
        };
        if dhi > bb_lo && dlo < bb_hi {
            println!(
                "     cck={cck:>9} pc=${pc:06X} c0=${c0:04X} c1=${c1:04X} apt=${apt:08X} bpt=${bpt:08X} dpt=${dpt:08X} size=${sz:04X} dest=${dlo:06X}..${dhi:06X}"
            );
        }
    }

    // Bucket all INTENA writes by value to see what's being written.
    let mut intena_writes: std::collections::HashMap<u16, u32> = std::collections::HashMap::new();
    for (_, _, val, _, _) in intena_log {
        *intena_writes.entry(*val).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = intena_writes.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    println!("   INTENA write values (top 12):");
    for (val, count) in sorted.iter().take(12) {
        let setclr = if *val & 0x8000 != 0 { "SET" } else { "CLEAR" };
        let blit_bit = if *val & 0x40 != 0 {
            " ⚠ has BLIT bit"
        } else {
            ""
        };
        println!("     ${val:04X} ({setclr}) × {count}{blit_bit}");
    }
    // Count ANY write that touches bit 6 (BLIT) — set or clear.
    let touches_blit: u32 = intena_log
        .iter()
        .filter(|(_, _, val, _, _)| *val & 0x40 != 0)
        .count() as u32;
    println!(
        "   INTENA writes touching BLIT bit (any direction): {}",
        touches_blit
    );

    // Show all blit logs that look like trackdisk's B→D copy
    // (BLTCON0 = $05CC). Group by PC so we can spot the QBlit
    // dispatcher (which calls the same blit setup repeatedly).
    let log = &rt.machine().debug_blit_log;
    for (label, want_c0) in [
        ("$05CC trackdisk B→D copy", 0x05CCu16),
        ("$1DD8 trackdisk READ MFM decode", 0x1DD8u16),
        ("$1DB1 trackdisk WRITE MFM decode", 0x1DB1),
        ("$2D8C trackdisk MFM decode (alt shift)", 0x2D8C),
    ] {
        let blits: Vec<_> = log
            .iter()
            .filter(|(_, _, c0, _, _, _, _, _, _)| *c0 == want_c0)
            .collect();
        println!("   Blits with BLTCON0={label}: {}", blits.len());
        for (cck, pc, c0, c1, apt, bpt, _cpt, dpt, sz) in blits.iter().take(6) {
            println!(
                "     cck={cck:>9} pc=${pc:06X} c0=${c0:04X} c1=${c1:04X} apt=${apt:08X} bpt=${bpt:08X} dpt=${dpt:08X} size=${sz:04X}"
            );
        }
        if blits.len() > 6 {
            println!("     ... and {} more", blits.len() - 6);
        }
    }
    // Also dump the unique BLTCON0 values seen across all blits.
    let mut seen: std::collections::HashMap<u16, u32> = std::collections::HashMap::new();
    for (_, _, c0, _, _, _, _, _, _) in log {
        *seen.entry(*c0).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = seen.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    println!("   Unique BLTCON0 values (top 10):");
    for (c0, count) in sorted.iter().take(10) {
        println!("     ${c0:04X}: {count} times");
    }
    Ok(())
}
