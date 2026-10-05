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
#[ignore = "FIXTURE: needs Oric Atmos / Oric-1 ROM (16 KB) — run with --ignored"]
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

/// Type `text` on the matrix, a key at a time, holding each for a few frames
/// so the ROM's keyboard scan sees it. Positions are the Atmos matrix
/// (`runtime-oric-atmos` `key_to_matrix`).
fn type_text(sys: &mut OricAtmos, text: &str) {
    for c in text.chars() {
        let (col, row) = match c {
            'C' => (2, 7),
            'E' => (6, 3),
            'H' => (6, 1),
            'I' => (5, 1),
            'K' => (3, 0),
            'O' => (5, 2),
            'P' => (5, 3),
            'R' => (1, 2),
            'S' => (6, 6),
            'T' => (1, 1),
            'X' => (0, 6),
            '1' => (0, 5),
            '2' => (2, 6),
            '4' => (2, 3),
            '9' => (3, 1),
            ',' => (4, 1),
            '\n' => (7, 5),
            other => panic!("no matrix position for {other:?}"),
        };
        sys.press_key(col, row);
        for _ in 0..4 {
            sys.run_frame();
        }
        sys.release_key(col, row);
        for _ in 0..4 {
            sys.run_frame();
        }
    }
}

/// The ROM's own `HIRES`, `TEXT` and a BASIC `POKE` of a 60 Hz attribute
/// drive the ULA mode register and the frame length (#341).
///
/// `HIRES` and `TEXT` write `$1E` / `$1A` to `$BFDF` and then overwrite it,
/// so the mode only survives if the ULA latches it. `POKE 49119,24` leaves a
/// TEXT 60 Hz attribute on screen, and every frame after is 264 lines.
#[test]
#[ignore = "FIXTURE: needs Oric Atmos / Oric-1 ROM (16 KB) — run with --ignored"]
fn rom_switches_hires_and_refresh_rate_through_the_ula() {
    let Some(path) = rom_path() else {
        panic!(
            "Oric ROM not found — set EMU198X_ORIC_ATMOS_ROM or place atmos.rom \
             / oric1.rom at ~/.emu198x/roms/oric-atmos/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    let mut sys = OricAtmos::new(rom, OricModel::Atmos);
    for _ in 0..200 {
        sys.run_frame();
    }
    assert_eq!(sys.ula_mode() & 0x06, 0x02, "cold start leaves TEXT, 50 Hz");

    type_text(&mut sys, "HIRES\n");
    for _ in 0..50 {
        sys.run_frame();
    }
    assert_ne!(
        sys.peek(0xBFDF),
        0x1E,
        "the ROM has overwritten its attribute"
    );
    assert_eq!(
        sys.ula_mode() & 0x06,
        0x06,
        "HIRES, 50 Hz latched in the ULA"
    );
    assert_eq!(sys.run_frame(), 312 * 64);

    type_text(&mut sys, "TEXT\n");
    for _ in 0..50 {
        sys.run_frame();
    }
    assert_eq!(sys.ula_mode() & 0x06, 0x02, "back to TEXT, 50 Hz");

    type_text(&mut sys, "POKE49119,24\n");
    for _ in 0..50 {
        sys.run_frame();
    }
    assert_eq!(sys.ula_mode() & 0x06, 0x00, "TEXT, 60 Hz");
    for _ in 0..5 {
        assert_eq!(sys.run_frame(), 264 * 64, "60 Hz frames are 264 lines");
    }
}
