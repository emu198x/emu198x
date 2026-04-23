//! Dump every Paula disk-register write during the fresh OCS boot
//! so we can decide what DSKBLK trigger semantics to give the
//! controller.
//!
//! Regs we're watching:
//!   $020/$022 DSKPT   disk DMA pointer (high/low)
//!   $024      DSKLEN  DMA length + DMAEN/WRITE bits
//!   $026      DSKDAT  raw MFM data (CPU only writes during DMA)
//!   $07E      DSKSYNC sync word (typically \$4489)
//!
//! `debug_dsk_log` entries are (cck, pc, reg_offset, value).

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

fn reg_name(offset: u16) -> &'static str {
    match offset {
        0x020 => "DSKPTH ",
        0x022 => "DSKPTL ",
        0x024 => "DSKLEN ",
        0x026 => "DSKDAT ",
        0x07E => "DSKSYNC",
        _ => "???    ",
    }
}

fn decode_dsklen(val: u16) -> String {
    let dmaen = (val & 0x8000) != 0;
    let write = (val & 0x4000) != 0;
    let length = val & 0x3FFF;
    let dir = if write { "WRITE" } else { "READ " };
    format!(
        "DMAEN={} dir={} length={length}",
        if dmaen { "1" } else { "0" },
        dir
    )
}

fn run(label: &str, mut amiga: AmigaOcs, frames: u64) {
    eprintln!("\n########## {label} — {frames} PAL frames ##########");

    for _ in 0..(frames * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let log = &amiga.debug_dsk_log;
    eprintln!("Total disk-register writes: {}", log.len());

    if log.is_empty() {
        eprintln!("(boot never touches the disk registers at all)");
        return;
    }

    // Per-register counts.
    let mut counts: std::collections::BTreeMap<u16, u64> = std::collections::BTreeMap::new();
    for &(_, _, reg, _) in log {
        *counts.entry(reg).or_insert(0) += 1;
    }
    eprintln!("\n=== Per-register write counts ===");
    for (reg, count) in &counts {
        eprintln!("  ${reg:03X} {} — {count} write(s)", reg_name(*reg));
    }

    // Full list if small; first + last if large.
    let show_all = log.len() <= 60;
    eprintln!(
        "\n=== {} ===",
        if show_all {
            "All writes".to_string()
        } else {
            format!("First 30 + last 10 writes (of {})", log.len())
        }
    );
    let show: Vec<_> = if show_all {
        log.to_vec()
    } else {
        log.iter()
            .take(30)
            .copied()
            .chain(std::iter::once((0, 0, 0, 0))) // sentinel for ellipsis
            .chain(log.iter().rev().take(10).rev().copied())
            .collect()
    };
    for (cck, pc, reg, val) in show {
        if cck == 0 && pc == 0 && reg == 0 && val == 0 {
            eprintln!("  ... ({} more) ...", log.len().saturating_sub(40));
            continue;
        }
        let note = if reg == 0x024 {
            decode_dsklen(val)
        } else {
            String::new()
        };
        eprintln!(
            "  cck={cck:>10} pc=${pc:08X}  ${reg:03X} {}  = ${val:04X}  {note}",
            reg_name(reg)
        );
    }

    // Look for the canonical "DSKLEN armed twice" pattern:
    // two consecutive DSKLEN writes with DMAEN set.
    let mut dmaen_streak = 0u8;
    let mut arm_events: Vec<(u64, u32, u16)> = Vec::new();
    for &(cck, pc, reg, val) in log {
        if reg == 0x024 {
            if (val & 0x8000) != 0 {
                dmaen_streak += 1;
                if dmaen_streak == 2 {
                    arm_events.push((cck, pc, val));
                }
            } else {
                dmaen_streak = 0;
            }
        }
    }
    if !arm_events.is_empty() {
        eprintln!("\n=== DSKLEN arm events (DMAEN written twice in a row) ===");
        for (cck, pc, val) in &arm_events {
            eprintln!(
                "  cck={cck:>10} pc=${pc:08X}  DSKLEN=${val:04X}  {}",
                decode_dsklen(*val)
            );
        }
    } else {
        eprintln!(
            "\nNo canonical double-arm DMAEN sequence observed — \
            trackdisk hasn't tried to read/write via the controller yet."
        );
    }
}

#[test]
#[ignore]
fn dump_paula_disk_writes_400_frames() {
    let Some(rom) = load_kickstart() else { return };
    run(
        "slow-RAM (512K chip + 512K slow)",
        AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024),
        400,
    );
    run("chip-only (512K chip)", AmigaOcs::new(rom), 400);
}
