//! Diagnostic: count CPU reads from each custom-register offset.
//! Helps identify which registers the boot is hammering — likely
//! candidates for needing real values rather than floating-bus.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::AmigaOcs;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn name_of(offset: u16) -> &'static str {
    match offset {
        0x000 => "BLTDDAT", 0x002 => "DMACONR", 0x004 => "VPOSR",
        0x006 => "VHPOSR", 0x008 => "DSKDATR", 0x00A => "JOY0DAT",
        0x00C => "JOY1DAT", 0x00E => "CLXDAT", 0x010 => "ADKCONR",
        0x012 => "POT0DAT", 0x014 => "POT1DAT", 0x016 => "POTGOR",
        0x018 => "SERDATR", 0x01A => "DSKBYTR", 0x01C => "INTENAR",
        0x01E => "INTREQR",
        _ => "(unnamed)",
    }
}

#[test]
#[ignore]
fn count_register_reads() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    for _ in 0..50_000_000u64 {
        amiga.tick_cck();
    }

    let mut entries: Vec<_> = amiga.debug_reg_read_counts.iter().collect();
    entries.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    eprintln!("Top 30 chipset register reads in 50M CCKs:");
    for (offset, count) in entries.iter().take(30) {
        eprintln!(
            "  ${:03X} ({:>8}) {} times",
            offset,
            name_of(**offset),
            count
        );
    }
}
