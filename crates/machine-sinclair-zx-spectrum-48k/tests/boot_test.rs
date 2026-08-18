//! Integration boot test for the ZX Spectrum 48K.
//!
//! Loads the original Sinclair 48K ROM, runs ~4 seconds of CPU time (200
//! frames), and asserts that screen RAM contains the "© 1982 Sinclair
//! Research Ltd" boot banner pixels.
//!
//! The ROM comes from `EMU198X_SPECTRUM_48K_ROM` when set, and otherwise
//! from `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom` — the same
//! resolution `z80test.rs` uses, so CI can point every Spectrum test at
//! one provisioned copy.
//!
//! Still `#[ignore]`d, because not every developer has the ROM. That is a
//! statement about developers, not about the ROM: Amstrad permits the
//! Sinclair ROMs to be distributed, which is why CI can run this while
//! most other machines' boot tests can never leave a private disk.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use std::path::PathBuf;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";

fn rom_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(ROM_PATH_ENV) {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
}

#[test]
#[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn boot_to_basic_renders_screen_content() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!("neither {ROM_PATH_ENV} nor HOME is set — cannot locate 48K ROM");
    };
    if !path.exists() {
        // Previously an `eprintln!` and a bare `return`, which libtest
        // reports as `ok`. That is the shape that let the Dragon
        // golden-frame test pass for three months while comparing
        // nothing. A skip is recorded, and fails outright under
        // `EMU198X_STRICT_FIXTURES` — which is exactly what the nightly
        // and the ROM-provisioned CI step set.
        emu198x_test_skip::skip!("48K ROM not staged at {} ({ROM_PATH_ENV})", path.display());
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
