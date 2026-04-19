//! Track INTENA + DMACON + BPLCON0 + COP1LC at every frame during boot
//! for both chip-only and chip+slow configs. Print the frame number
//! whenever a watched bit changes.
//!
//! Goal: find where slow-RAM enables SOFTINT (bit 2) / DSKBLK (bit 3)
//! and where chip-only diverges.

use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

fn rom() -> Vec<u8> {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    std::fs::read(&path).expect("read kick13.rom")
}

fn watched_bits(intena: u16) -> String {
    let mut parts = Vec::new();
    if intena & 0x4000 != 0 { parts.push("MASTER"); }
    if intena & 0x2000 != 0 { parts.push("EXTER"); }
    if intena & 0x0020 != 0 { parts.push("VERTB"); }
    if intena & 0x0008 != 0 { parts.push("DSKBLK"); }
    if intena & 0x0004 != 0 { parts.push("SOFTINT"); }
    if parts.is_empty() { "(none)".to_string() } else { parts.join("|") }
}

fn run_and_trace(label: &str, slow_ram: usize) {
    let mut amiga = if slow_ram == 0 {
        Amiga::new(rom())
    } else {
        Amiga::new_with_slow_ram(rom(), slow_ram)
    };
    eprintln!("===== {} =====", label);
    let mut last_intena = 0u16;
    let mut last_dmacon = 0u16;
    let mut last_bplcon0 = 0u16;
    let mut last_cop1lc = 0u32;
    for frame in 0..400u32 {
        amiga.run_frame();
        let intena = amiga.paula.intena;
        let dmacon = amiga.agnus.dmacon;
        let bplcon0 = amiga.agnus.bplcon0;
        let cop1lc = amiga.copper.cop1lc;
        // Watch master IRQ (bit 14), SOFTINT (2), DSKBLK (3), VERTB (5) — and BPLEN (DMACON bit 8).
        let watched_changed = (intena & 0x402C) != (last_intena & 0x402C)
            || (dmacon & 0x0100) != (last_dmacon & 0x0100);
        let other_changed = dmacon != last_dmacon
            || bplcon0 != last_bplcon0
            || cop1lc != last_cop1lc;
        if watched_changed || (other_changed && frame % 25 == 0) {
            eprintln!(
                "  f{frame:3}: INTENA=${intena:04X} [{}] DMACON=${dmacon:04X} BPLCON0=${bplcon0:04X} COP1LC=${cop1lc:08X}",
                watched_bits(intena),
            );
        }
        last_intena = intena;
        last_dmacon = dmacon;
        last_bplcon0 = bplcon0;
        last_cop1lc = cop1lc;
    }
    eprintln!("  final: INTENA=${last_intena:04X} [{}]", watched_bits(last_intena));
    eprintln!();
}

#[test]
#[ignore]
fn intena_dmacon_progression_chip_only_vs_slow() {
    run_and_trace("chip-only", 0);
    run_and_trace("chip + 512K slow RAM", 512 * 1024);
}
