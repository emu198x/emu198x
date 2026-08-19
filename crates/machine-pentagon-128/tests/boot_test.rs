//! Integration boot test for the Pentagon 128.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_pentagon_128::Pentagon128;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/pentagon-128"))
}

#[test]
#[ignore = "requires local Pentagon ROMs at ~/.emu198x/roms/pentagon-128/pentagon-{0,1}.rom"]
fn boot_to_menu_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate Pentagon ROMs");
    };
    let rom0 = dir.join("pentagon-0.rom");
    let rom1 = dir.join("pentagon-1.rom");
    if !rom0.exists() || !rom1.exists() {
        emu198x_test_skip::skip!("Pentagon ROMs not found at {}", dir.display());
    }

    let mut machine = Pentagon128::new();
    machine.memory.load_rom0(&rom0).expect("ROM 0 should load");
    machine.memory.load_rom1(&rom1).expect("ROM 1 should load");

    for _ in 0..200 {
        machine.run_frame();
    }

    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "Pentagon should boot to menu with screen content (got {nonzero} non-zero bytes)"
    );
}
