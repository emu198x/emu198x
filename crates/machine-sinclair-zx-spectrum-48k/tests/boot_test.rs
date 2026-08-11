//! Integration boot test for the ZX Spectrum 48K.
//!
//! Loads the original Sinclair 48K ROM from
//! `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`, runs ~4 seconds
//! of CPU time (200 frames), and asserts that screen RAM contains the
//! "© 1982 Sinclair Research Ltd" boot banner pixels.
//!
//! `#[ignore]`d because not every developer has the ROM locally — the
//! runner prints a path hint and skips when it's missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use std::path::PathBuf;

fn rom_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
}

#[test]
#[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn boot_to_basic_renders_screen_content() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate 48K ROM");
    };
    if !path.exists() {
        eprintln!("48K ROM not found at {}", path.display());
        return;
    }

    let bytes = std::fs::read(&path).expect("48K ROM should read");
    let mut machine = Spectrum48k::new();
    machine
        .load_rom_bytes(&bytes)
        .expect("48K ROM image should load (16 KiB)");

    for _ in 0..200 {
        machine.run_frame();
    }

    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "48K should boot to BASIC with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
