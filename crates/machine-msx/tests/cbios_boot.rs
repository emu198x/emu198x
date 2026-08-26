//! The MSX booting real firmware.
//!
//! `synthetic_firmware_boot.rs` beside this file proves the machine runs
//! code from its ROM socket, which is all that can be checked without a
//! BIOS. This is the stronger claim: an actual MSX BIOS cold-starts and
//! renders its own screen.
//!
//! The BIOS is [C-BIOS](https://github.com/cbios/cbios) — a clean-room MSX
//! BIOS under a BSD licence, written so that MSX emulators can ship
//! without a manufacturer's ROM. It is not Microsoft's BIOS, and a title
//! that depends on undocumented BIOS internals may behave differently. For
//! "does this machine start", that does not matter.
//!
//! Provisioned from the corpora store; `EMU198X_ROMS_ROOT` points at the
//! firmware root, and this joins `microsoft-msx/` onto it.

use std::path::PathBuf;

use machine_msx::{Msx, MsxRegion};

/// TMS9918 entry 4 — the blue C-BIOS boots to.
const MSX_BLUE: u32 = 0xFF54_55ED;

fn bios() -> Option<PathBuf> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    Some(root.join("microsoft-msx/cbios_main_msx1.rom"))
}

#[test]
#[ignore = "FIXTURE: needs C-BIOS at <EMU198X_ROMS_ROOT>/microsoft-msx/cbios_main_msx1.rom"]
fn cbios_cold_starts_and_renders_its_screen() {
    let Some(path) = bios() else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    if !path.exists() {
        emu198x_test_skip::skip!("C-BIOS not staged at {}", path.display());
    }

    let rom = std::fs::read(&path).expect("C-BIOS should read");
    let mut machine = Msx::new(rom, MsxRegion::Ntsc);
    for _ in 0..200 {
        machine.run_frame();
    }

    let framebuffer = machine.framebuffer();
    let background = framebuffer
        .iter()
        .filter(|&&pixel| pixel == MSX_BLUE)
        .count();
    let foreground = framebuffer.len() - background;

    // A machine that fetched but never rendered shows one flat colour, and
    // a machine that hung shows the power-on colour. Requiring *both* a
    // dominant blue and a real minority of drawn pixels excludes both.
    assert!(
        background * 100 / framebuffer.len() > 80,
        "C-BIOS should paint its blue screen; got {background} of {} pixels",
        framebuffer.len()
    );
    assert!(
        foreground > 500,
        "C-BIOS should draw text over that screen; only {foreground} pixels differ"
    );
}
