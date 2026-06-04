//! Oric Atmos BIOS boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_oric_atmos::{OricAtmos, OricModel};

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ORIC_ATMOS_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let home = PathBuf::from(home);
    // The binary's default location, then the older oric-atmos names.
    let candidates = [
        home.join(".emu198x/roms/oric/oric.rom"),
        home.join(".emu198x/roms/oric-atmos/atmos.rom"),
        home.join(".emu198x/roms/oric-atmos/oric1.rom"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[test]
#[ignore = "needs Oric Atmos / Oric-1 ROM (16 KB) — run with --ignored"]
fn rom_boots_to_initial_screen() {
    let Some(path) = rom_path() else {
        panic!(
            "Oric ROM not found — set EMU198X_ORIC_ATMOS_ROM or place atmos.rom \
             / oric1.rom at ~/.emu198x/roms/oric-atmos/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x4000, "ROM must be exactly 16 KB");

    let mut sys = OricAtmos::new(rom, OricModel::Atmos);
    for _ in 0..300 {
        sys.run_frame();
    }

    // The Atmos cold-starts to `ORIC EXTENDED BASIC V1.1` / `1983 TANGERINE`
    // / `Ready`. That text lands in the TEXT screen RAM at $BB80 as ASCII;
    // count printable letters/digits (codes $21-$7F, excluding the space and
    // the low serial-attribute control codes) to prove the banner rendered,
    // not merely that the machine ran.
    let printed = (0xBB80u16..0xBE00)
        .filter(|&a| {
            let c = sys.peek(a);
            (0x21..0x80).contains(&c)
        })
        .count();
    assert!(
        printed >= 30,
        "expected the BASIC banner in TEXT RAM; got {printed} printable cells (rom: {})",
        path.display()
    );

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 240 * 224);
}
