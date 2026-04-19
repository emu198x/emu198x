//! Hook writes to GfxBase->LOFlist field, frame-by-frame, in both
//! chip-only and chip+slow KS 1.3 boots.
//!
//! Per the cop1lc_write_log.rs investigation:
//!   - The graphics.library VBlank handler at PC=$FC6D6C reads
//!     GfxBase->LOFlist (offset $32 from GfxBase) into d0, then
//!     writes d0 to COP2LC.
//!   - In slow-RAM, by frame 230, GfxBase->LOFlist = $0000B888 (a
//!     freshly-allocated per-frame copper list).
//!   - In chip-only, GfxBase->LOFlist stays at $00000676 (= the
//!     bootstrap ExecBase pointer, used as a placeholder during init
//!     and never replaced).
//!
//! GfxBase address (= A1 in the VBL handler):
//!   - chip-only: $0000221E → LOFlist field at $0000221E+$32 = $00002250
//!   - slow-RAM:  $00C01E1E → LOFlist field at $00C01E1E+$32 = $00C01E50
//!
//! Goal: find PCs that write to LOFlist. Whoever writes the real
//! buffer address ($B888 in slow-RAM) is the missing operation that
//! never happens in chip-only.

use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

fn rom() -> Vec<u8> {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    std::fs::read(&path).expect("read kick13.rom")
}

fn run_with_watch(label: &str, slow_ram: usize, watch: (u32, u32)) {
    let mut amiga = if slow_ram == 0 {
        Amiga::new(rom())
    } else {
        Amiga::new_with_slow_ram(rom(), slow_ram)
    };
    amiga.debug_cpu_write_watch = Some(watch);

    eprintln!("===== {label}: watching ${:08X}..${:08X} =====", watch.0, watch.1);

    for _ in 0..250 {
        amiga.run_frame();
    }

    let log = &amiga.debug_cpu_write_log;
    if log.is_empty() {
        eprintln!("  (no writes to watched range)");
    } else {
        eprintln!("  {} writes captured (last 256):", log.len());
        for (pc, addr, size, val) in log.iter() {
            eprintln!("    pc=${pc:08X} → write {size} ${addr:06X} = ${val:08X}");
        }
    }
    eprintln!();
}

#[test]
#[ignore]
fn watch_loflist_writes_chip_only() {
    // GfxBase = $0000221E, LOFlist at +$32 = $00002250 (4 bytes)
    // Watch a small range around it.
    run_with_watch("chip-only / LOFlist range", 0, (0x002250, 0x002258));
}

#[test]
#[ignore]
fn watch_loflist_writes_with_slow_ram() {
    // GfxBase = $00C01E1E, LOFlist at +$32 = $00C01E50 (4 bytes)
    run_with_watch("slow-RAM / LOFlist range", 512 * 1024, (0xC01E50, 0xC01E58));
}

/// Also watch the SHFlist field (offset $36) just in case it's the one
/// being updated for non-interlaced displays.
#[test]
#[ignore]
fn watch_shflist_writes_chip_only() {
    // GfxBase = $0000221E, SHFlist at +$36 = $00002254 (4 bytes)
    run_with_watch("chip-only / SHFlist range", 0, (0x002254, 0x00225C));
}

#[test]
#[ignore]
fn watch_shflist_writes_with_slow_ram() {
    run_with_watch("slow-RAM / SHFlist range", 512 * 1024, (0xC01E54, 0xC01E5C));
}
