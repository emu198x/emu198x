//! Diagnostic: dump the chipset state OCS presents when Kickstart
//! 1.3 reaches the insert-disk screen. Pinpoints task #191 — the
//! OCS rewrite renders the screen as a plain white rectangle rather
//! than the hand-holding-disk graphic, and the diff mask in
//! `tests/goldens/a500-ks13-no-disk.diff.png` points at planes 1+
//! not contributing.
//!
//! This test is `#[ignore]` (needs the KS 1.3 ROM locally). Run with
//!   cargo test -p runtime-commodore-amiga --test diag_insert_disk_state \
//!       -- --ignored --nocapture

use std::error::Error;
use std::path::PathBuf;

use machine_commodore_amiga_ocs::AmigaOcs;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntime, Model};

fn load_ks13() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read KS 1.3"))
}

#[test]
#[ignore = "needs KS 1.3 ROM — run with --ignored"]
fn dump_dmacon_trajectory_from_reset() -> Result<(), Box<dyn Error>> {
    let Some(rom) = load_ks13() else {
        return Ok(());
    };
    let mut rt = AmigaRuntime::new(Model::A500OcsPalA501, rom)?;
    for _ in 0..(260u64 * A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }

    println!("=== DMACON write log (CPU writes that changed the register) ===");
    let log = &rt.machine().debug_dmacon_log;
    println!("  {} writes logged", log.len());
    for &(cck, pc, val, before, after) in log {
        let bplen = (after >> 8) & 1;
        let spren = (after >> 5) & 1;
        println!(
            "  CCK {:>10} PC=${:08X} val=${:04X}  ${:04X} -> ${:04X}  (BPLEN={} SPREN={})",
            cck, pc, val, before, after, bplen, spren
        );
    }
    println!();

    // Copper list writes over the entire boot. If LoadView ran
    // normally after BPLEN was cleared, we should see COP1LC/COP2LC
    // updates at roughly that timestamp. If we don't, LoadView
    // never made it to the "program copper list" step.
    println!("=== COP1LC writes ===");
    for &(cck, pc, val) in &rt.machine().debug_cop1lc_log {
        println!("  CCK {:>10} PC=${:08X}  COP1LC=${:08X}", cck, pc, val);
    }
    println!();
    println!("=== COP2LC writes ===");
    for &(cck, pc, val) in &rt.machine().debug_cop2lc_log {
        println!("  CCK {:>10} PC=${:08X}  COP2LC=${:08X}", cck, pc, val);
    }
    println!();
    Ok(())
}

#[test]
#[ignore = "needs KS 1.3 ROM — run with --ignored"]
fn dump_state_at_frame_250_a500_a501() -> Result<(), Box<dyn Error>> {
    let Some(rom) = load_ks13() else {
        return Ok(());
    };
    let mut rt = AmigaRuntime::new(Model::A500OcsPalA501, rom)?;
    for _ in 0..(250u64 * A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }

    // Track whether the copper is actually running in the steady
    // state — sample BPL1PT, BPLCON0, and DMACON across a few
    // consecutive frames. If the copper ran, BPL1PT should be
    // reset to the copper list's programmed value ($5AC2 per the
    // dump below) every frame. If not, it drifts or stays fixed.
    // Walk copper PC tick-by-tick through one whole PAL frame. Log
    // the PC whenever it changes — or after every N ticks to see
    // whether it's advancing at all.
    println!("=== copper PC trace through one frame starting after frame 250 ===");
    let mut last_pc = rt.machine().copper().pc;
    let mut last_stopped = rt.machine().copper().stopped;
    let mut last_waiting = rt.machine().copper().waiting;
    println!(
        "  initial: PC=${:08X} stopped={} waiting={}",
        last_pc, last_stopped as u8, last_waiting as u8
    );
    let mut change_count = 0u32;
    for tick in 0..A500_PAL_FRAME_TICKS {
        rt.machine_mut().tick();
        let c = rt.machine().copper();
        let a = rt.machine().agnus();
        if c.pc != last_pc || c.stopped != last_stopped || c.waiting != last_waiting {
            change_count += 1;
            if change_count <= 40 {
                println!(
                    "  tick {:>5}  vpos={:>3} hpos={:>3}  PC=${:08X} stopped={} waiting={} wait_target=${:04X}",
                    tick, a.vpos, a.hpos, c.pc, c.stopped as u8, c.waiting as u8, c.wait_target,
                );
            }
            last_pc = c.pc;
            last_stopped = c.stopped;
            last_waiting = c.waiting;
        }
    }
    println!("  total PC/state changes in one frame: {change_count}");
    println!();

    println!("=== per-frame copper / agnus drift across frames 250-257 ===");
    println!(
        " frame | cop.pc   stop wait wait_target wait_mask | vpos  hpos | BPL1PT    BPLCON0 DMACON"
    );
    for extra in 0..8 {
        let c = rt.machine().copper();
        let a = rt.machine().agnus();
        println!(
            "  {:>4} | ${:08X} {:>4} {:>4} ${:04X}       ${:04X}     | {:>4}  {:>4} | ${:08X} ${:04X}   ${:04X}",
            250 + extra,
            c.pc,
            c.stopped as u8,
            c.waiting as u8,
            c.wait_target,
            c.wait_mask,
            a.vpos,
            a.hpos,
            a.bpl_pt[0],
            a.bplcon0,
            a.dmacon,
        );
        for _ in 0..A500_PAL_FRAME_TICKS {
            rt.machine_mut().tick();
        }
    }
    println!();

    let m = rt.machine();
    let a = m.agnus();

    println!("=== chipset snapshot at frame 258 (A500+A501, KS 1.3) ===");
    println!();

    println!(
        "DMACON   = ${:04X}  (DMAEN={} BPLEN={} COPEN={} BLTEN={} DSKEN={} SPREN={})",
        a.dmacon,
        (a.dmacon >> 9) & 1,
        (a.dmacon >> 8) & 1,
        (a.dmacon >> 7) & 1,
        (a.dmacon >> 6) & 1,
        (a.dmacon >> 4) & 1,
        (a.dmacon >> 5) & 1,
    );
    println!();

    let bpu = (a.bplcon0 >> 12) & 0x7;
    let hires = (a.bplcon0 >> 15) & 1;
    let ham = (a.bplcon0 >> 11) & 1;
    let dblpf = (a.bplcon0 >> 10) & 1;
    let color = (a.bplcon0 >> 9) & 1;
    let lace = (a.bplcon0 >> 2) & 1;
    println!(
        "BPLCON0  = ${:04X}  (BPU={}  HIRES={}  HAM={}  DBLPF={}  COLOR={}  LACE={})",
        a.bplcon0, bpu, hires, ham, dblpf, color, lace,
    );

    println!("BPL1MOD  = {}  BPL2MOD = {}", a.bpl1mod, a.bpl2mod);
    println!();

    println!(
        "DDFSTRT  = ${:04X}  DDFSTOP = ${:04X}",
        a.ddfstrt, a.ddfstop
    );
    println!(
        "DIWSTRT  = ${:04X}  DIWSTOP = ${:04X}",
        a.diwstrt, a.diwstop
    );
    println!();

    println!("Bitplane pointers (current):");
    for (i, ptr) in a.bpl_pt.iter().enumerate().take(6) {
        println!("  BPL{}PT = ${:08X}", i + 1, ptr);
    }
    println!();

    println!("Colour palette (COLOR00..COLOR31):");
    for row in 0..4 {
        print!("  ${:02X}..${:02X}: ", row * 8, row * 8 + 7);
        for col in 0..8 {
            let idx = row * 8 + col;
            print!("{:04X} ", m.color(idx));
        }
        println!();
    }
    println!();

    println!(
        "Copper pointers: COP1LC=${:08X}  COP2LC=${:08X}",
        m.copper().cop1lc,
        m.copper().cop2lc,
    );
    println!();

    // Dump the copper lists. COP1LC strobes COPJMP2 early on, so
    // most of the real bitplane/sprite/DMACON programming lives in
    // the COP2LC list.
    dump_copper_list(m, "COP1LC", m.copper().cop1lc, 32);
    dump_copper_list(m, "COP2LC", m.copper().cop2lc, 64);

    // CPU state — where is the ROM sitting when we snapshot?
    let cpu = m.cpu();
    println!(
        "CPU: PC=${:08X}  SR=${:04X}  IPL={}",
        cpu.regs.pc, cpu.regs.sr, cpu.ipl,
    );
    println!();

    // Dump chip RAM at the bitplane pointers. If the copper list
    // is correct and the blitter drew the hand-disk graphic there,
    // these should contain line-art bit patterns. All-zero means
    // nobody ever wrote anything to the bitplanes.
    println!("=== chip RAM at BPL1PT ($5AC2), 8 lines of 20 bytes ===");
    for line in 0..8 {
        print!("  +{:03X}: ", line * 20);
        for byte in 0..20 {
            let addr = 0x5AC2u32 + line * 20 + byte;
            let b = m.read_chip_ram_byte(addr);
            print!("{b:02X} ");
        }
        println!();
    }
    println!();
    println!("=== chip RAM at BPL2PT ($7A02), 8 lines of 20 bytes ===");
    for line in 0..8 {
        print!("  +{:03X}: ", line * 20);
        for byte in 0..20 {
            let addr = 0x7A02u32 + line * 20 + byte;
            let b = m.read_chip_ram_byte(addr);
            print!("{b:02X} ");
        }
        println!();
    }
    println!();

    // Count the total number of CPU DMACON writes + BPL*DAT writes
    // logged. If CPU never wrote BPL*DAT, that rules out the CPU-
    // direct-bitplane theory too.
    println!("DMACON writes recorded: {}", m.debug_dmacon_log.len());
    println!();

    // Blitter state right now. Bitplanes are all zeros so either
    // (a) the blitter never ran in a way that wrote bitplane data,
    // (b) the destination pointer (DPT) doesn't land in the BPL
    //     window, or (c) the blit is producing zero output because
    //     the source data (APT/BPT/CPT) is itself zero or the
    //     minterm selects nothing.
    let last_cck = m
        .debug_cia_b_cr_log
        .last()
        .map(|(cck, _, _, _)| *cck)
        .unwrap_or(0);
    println!("=== blitter state at end (cck={last_cck}) ===");
    // Scan chip RAM for non-zero content in 256-byte pages. If the
    // blitter drew the hand-disk line art, we should see non-trivial
    // byte patterns somewhere — track which pages are non-empty.
    println!("=== chip RAM pages with non-zero bytes ===");
    let mut non_zero_pages = Vec::new();
    for page in 0..(512 * 1024 / 256) {
        let base = (page * 256) as u32;
        let mut any_non_zero = false;
        for off in 0..256 {
            if m.read_chip_ram_byte(base + off) != 0 {
                any_non_zero = true;
                break;
            }
        }
        if any_non_zero {
            non_zero_pages.push(base);
        }
    }
    println!(
        "  {} / {} pages non-empty",
        non_zero_pages.len(),
        512 * 1024 / 256
    );
    println!("  ALL non-empty page bases:");
    for page in &non_zero_pages {
        // Count non-zero bytes in the page as a cheap "density".
        let mut count = 0;
        for off in 0..256 {
            if m.read_chip_ram_byte(page + off) != 0 {
                count += 1;
            }
        }
        println!("    ${page:06X}  ({count}/256 non-zero bytes)");
    }
    println!();

    println!(
        "Total blit starts (BLTSIZE writes) since reset: {}",
        m.debug_blit_starts
    );
    println!();
    println!("Top 10 custom-register read counts:");
    let mut counts: Vec<(u16, u64)> = m
        .debug_reg_read_counts
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    counts.sort_by_key(|&(_, v)| std::cmp::Reverse(v));
    for &(reg, count) in counts.iter().take(10) {
        println!("  ${reg:03X}:  {count}");
    }
    println!();
    println!("  BLTCON0  = ${:04X}", a.bltcon0);
    println!("  BLTCON1  = ${:04X}", a.bltcon1);
    println!("  BLTSIZE  = ${:04X}", a.bltsize);
    println!("  BLTAPT   = ${:08X}   AMOD = {}", a.blt_apt, a.blt_amod);
    println!("  BLTBPT   = ${:08X}   BMOD = {}", a.blt_bpt, a.blt_bmod);
    println!("  BLTCPT   = ${:08X}   CMOD = {}", a.blt_cpt, a.blt_cmod);
    println!("  BLTDPT   = ${:08X}   DMOD = {}", a.blt_dpt, a.blt_dmod);
    Ok(())
}

fn dump_copper_list(m: &AmigaOcs, label: &str, base: u32, len_instrs: u32) {
    let base = base & 0x001F_FFFE;
    println!(
        "Copper list at {} (${:08X}), first {} instructions:",
        label, base, len_instrs
    );
    for i in 0..len_instrs {
        let addr = base + i * 4;
        let w0 = m.read_word(addr);
        let w1 = m.read_word(addr + 2);
        let desc = decode_copper_instr(w0, w1);
        println!("  +{:03X}  {:04X} {:04X}   {}", i * 4, w0, w1, desc);
    }
    println!();
}

/// Decode one 32-bit copper instruction for the printout. Mirrors
/// the HRM 3rd ed. Ch. 7 copper encoding.
fn decode_copper_instr(w0: u16, w1: u16) -> String {
    if w0 & 0x0001 == 0 {
        let reg = w0 & 0x01FE;
        format!("MOVE  #${:04X} -> reg ${:03X}", w1, reg)
    } else if w1 & 0x0001 == 0 {
        let vp = (w0 >> 8) & 0xFF;
        let hp = w0 & 0xFE;
        let ve = (w1 >> 8) & 0x7F;
        let he = w1 & 0xFE;
        format!(
            "WAIT  V=${:02X} H=${:02X}  mask V=${:02X} H=${:02X}",
            vp, hp, ve, he
        )
    } else {
        let vp = (w0 >> 8) & 0xFF;
        let hp = w0 & 0xFE;
        format!("SKIP  V=${:02X} H=${:02X}", vp, hp)
    }
}
