//! Integration boot test for the ZX Spectrum +2A.
//!
//! +2A, +2B, and +3 share the same four 16 KiB ROMs ("©1982, 1986,
//! 1987 Amstrad Plc.") from the Amstrad +3 ROM set. Loads them from
//! `~/.emu198x/roms/amstrad-zx-spectrum-plus3/{plus3-0..3}.rom`, runs
//! ~5 seconds of CPU time (250 frames — the +3 boot sequence is
//! slower than the 128K), and asserts that screen RAM contains the
//! boot menu pixels.
//!
//! `#[ignore]`d because not every developer has the ROMs locally —
//! the runner prints a path hint and skips when they're missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use std::path::PathBuf;

// `EMU198X_ROMS_ROOT` overrides the firmware root so CI can provision one
// staging directory and every machine's test find its own ROMs inside it.
// `$HOME/.emu198x/roms` remains the developer default.
fn rom_dir() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("EMU198X_ROMS_ROOT") {
        return Some(PathBuf::from(root).join("amstrad-zx-spectrum-plus3"));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/amstrad-zx-spectrum-plus3"))
}

#[test]
#[ignore = "requires local +3 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus3/{plus3-0..3}.rom"]
fn boot_to_menu_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate +3 ROMs");
    };
    for i in 0..4 {
        let rom = dir.join(format!("plus3-{i}.rom"));
        if !rom.exists() {
            emu198x_test_skip::skip!("+3 ROMs not found at {}", dir.display());
        }
    }

    let mut machine = SpectrumPlus2A::new();
    for i in 0..4 {
        machine
            .memory
            .load_rom(i, &dir.join(format!("plus3-{i}.rom")))
            .unwrap_or_else(|e| panic!("ROM {i} should load: {e}"));
    }

    for _ in 0..250 {
        machine.run_frame();
    }

    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "+2A should boot to menu with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
