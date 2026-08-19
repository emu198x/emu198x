//! Integration boot test for the Sinclair-branded grey +2.
//!
//! The grey +2 is electrically identical to the 128K — same Amstrad
//! 7K010E ULA, same paging, same dual 16 KiB ROM layout. It only
//! differs from the 128K in the boot banner ("Amstrad Consumer
//! Electronics plc" vs "Sinclair Research Ltd"). Loads the +2 ROMs from
//! `~/.emu198x/roms/amstrad-zx-spectrum-plus2/{plus2-0,plus2-1}.rom`,
//! runs ~4 seconds of CPU time (200 frames), and asserts that screen
//! RAM contains the boot menu pixels.
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_plus2::SpectrumPlus2;
use std::path::PathBuf;

// `EMU198X_ROMS_ROOT` overrides the firmware root so CI can provision one
// staging directory and every machine's test find its own ROMs inside it.
// `$HOME/.emu198x/roms` remains the developer default.
fn rom_dir() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("EMU198X_ROMS_ROOT") {
        return Some(PathBuf::from(root).join("amstrad-zx-spectrum-plus2"));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/amstrad-zx-spectrum-plus2"))
}

#[test]
#[ignore = "requires local +2 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus2/{plus2-0,plus2-1}.rom"]
fn boot_to_menu_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate +2 ROMs");
    };
    let rom0 = dir.join("plus2-0.rom");
    let rom1 = dir.join("plus2-1.rom");
    if !rom0.exists() || !rom1.exists() {
        emu198x_test_skip::skip!("+2 ROMs not found at {}", dir.display());
    }

    let mut machine = SpectrumPlus2::new();
    machine
        .memory
        .load_rom0(&rom0)
        .expect("ROM 0 should load (+2 editor)");
    machine
        .memory
        .load_rom1(&rom1)
        .expect("ROM 1 should load (48K BASIC)");

    for _ in 0..200 {
        machine.run_frame();
    }

    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "+2 should boot to menu with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
