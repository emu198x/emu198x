//! Integration boot test for the ZX Spectrum 128K.
//!
//! Loads the original Sinclair 128K ROMs from
//! `~/.emu198x/roms/sinclair-zx-spectrum-128k/{128-0,128-1}.rom`,
//! runs ~4 seconds of CPU time (200 frames), and asserts that the
//! 128K's familiar "128 BASIC / Tape Loader / Tape Tester / Calculator"
//! menu has rendered enough non-zero pixels to constitute boot.
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/sinclair-zx-spectrum-128k"))
}

#[test]
#[ignore = "requires local 128K ROMs at ~/.emu198x/roms/sinclair-zx-spectrum-128k/{128-0,128-1}.rom"]
fn boot_to_menu_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate 128K ROMs");
    };
    let rom0 = dir.join("128-0.rom");
    let rom1 = dir.join("128-1.rom");
    if !rom0.exists() || !rom1.exists() {
        emu198x_test_skip::skip!("128K ROMs not found at {}", dir.display());
    }

    let mut machine = Spectrum128K::new();
    machine
        .memory
        .load_rom0(&rom0)
        .expect("ROM 0 should load (128 KB editor)");
    machine
        .memory
        .load_rom1(&rom1)
        .expect("ROM 1 should load (48K BASIC)");

    for _ in 0..200 {
        machine.run_frame();
    }

    // The 128K menu paints titles + four selectable entries — well over 50
    // bytes of non-zero attributes/pixels in the standard screen RAM bank.
    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "128K should boot to menu with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
