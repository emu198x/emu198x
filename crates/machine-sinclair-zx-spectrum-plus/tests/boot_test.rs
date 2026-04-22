//! Integration boot test for the ZX Spectrum +2A / +2B / +3.
//!
//! Loads the four +3 ROMs from
//! `~/.emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{0,1,2,3}.rom`,
//! runs ~4 seconds of CPU time (200 frames), and asserts that the +3's
//! disk-or-tape menu has rendered enough non-zero pixels to constitute
//! boot.
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use machine_sinclair_zx_spectrum_plus::{Model, SpectrumPlus};
use common_sinclair_zx_spectrum::memory::MemoryBus;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/amstrad-zx-spectrum-plus3"))
}

#[test]
#[ignore = "requires local +3 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{0,1,2,3}.rom"]
fn plus3_boots_to_menu_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        eprintln!("HOME not set — cannot locate +3 ROMs");
        return;
    };
    let roms = [
        dir.join("plus3-0.rom"),
        dir.join("plus3-1.rom"),
        dir.join("plus3-2.rom"),
        dir.join("plus3-3.rom"),
    ];
    if roms.iter().any(|p| !p.exists()) {
        eprintln!("+3 ROMs not found at {}", dir.display());
        return;
    }

    let mut machine = SpectrumPlus::new(Model::Plus3);
    for (i, path) in roms.iter().enumerate() {
        machine
            .memory
            .load_rom(i, path)
            .unwrap_or_else(|e| panic!("ROM {i} should load: {e}"));
    }

    for _ in 0..200 {
        machine.run_frame();
    }

    // The +3 boot menu paints a four-line cyan menu — well over 50 bytes
    // of non-zero attributes/pixels in screen RAM (bank 5 at $4000).
    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "+3 should boot to menu with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
