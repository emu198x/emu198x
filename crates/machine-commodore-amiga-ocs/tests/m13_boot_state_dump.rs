//! M13 diagnostic: dump the post-boot state after the 8520 MICROHZ
//! fix and CIA-B empty-drive defaults. We want to see:
//!  - INTENA / INTREQ / DMACON
//!  - CIA-B PRA effective value (disk pins)
//!  - Disk register writes the ROM made during boot
//!
//! Ignored by default — it's for investigation, not a pass/fail gate.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
#[ignore]
fn pc_histogram_over_last_20_frames() {
    // Is the boot genuinely idling in a small loop, or wandering?
    // Run for 400 frames, then sample PC once per tick for 20 frames
    // and build a histogram of the distinct PCs seen.
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
    }
    let mut hist: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut prev = amiga.cpu().regs.pc;
    for _ in 0..(20 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc != prev {
            *hist.entry(pc).or_insert(0) += 1;
            prev = pc;
        }
    }
    let mut entries: Vec<_> = hist.into_iter().collect();
    entries.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    eprintln!(
        "=== Top 20 PCs seen over 20 frames ({} distinct) ===",
        entries.len()
    );
    for (pc, count) in entries.iter().take(20) {
        eprintln!("  ${pc:08X}  {count:>6}");
    }
    let min_pc = entries
        .iter()
        .map(|(pc, _)| *pc)
        .min()
        .expect("at least one PC sampled");
    let max_pc = entries
        .iter()
        .map(|(pc, _)| *pc)
        .max()
        .expect("at least one PC sampled");
    eprintln!(
        "\nPC range: ${min_pc:08X} – ${max_pc:08X} (span ${:X})",
        max_pc - min_pc
    );
}

#[test]
#[ignore]
fn dump_boot_state() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    for _ in 0..(300 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    eprintln!("=== Post-boot state (300 frames) ===");
    eprintln!("INTENA   = ${:04X}", amiga.intena() & 0x7FFF);
    eprintln!("INTREQ   = ${:04X}", amiga.intreq() & 0x7FFF);
    eprintln!("DMACON   = ${:04X}", amiga.dmacon() & 0x7FFF);

    // CIA-B PRA effective output — disk pins.
    let pa = amiga.cia_b().peek(0);
    eprintln!(
        "\nCIA-B PA = ${pa:02X} (bits: /RDY={} /TK0={} /WPRO={} /CHNG={})",
        (pa >> 5) & 1,
        (pa >> 4) & 1,
        (pa >> 3) & 1,
        (pa >> 2) & 1
    );

    eprintln!(
        "\n=== Disk register writes ({} total) ===",
        amiga.debug_dsk_log.len()
    );
    for (cck, pc, reg, val) in amiga.debug_dsk_log.iter() {
        let f = cck / 70824;
        let name = match reg {
            0x020 => "DSKPTH",
            0x022 => "DSKPTL",
            0x024 => "DSKLEN",
            0x026 => "DSKDAT",
            0x07E => "DSKSYNC",
            _ => "DSK??",
        };
        eprintln!("  frame~{f:<3}  pc=${pc:08X}  {name} (${reg:03X}) = ${val:04X}");
    }
}
