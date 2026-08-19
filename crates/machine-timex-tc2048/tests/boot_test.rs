//! Integration boot test for the Timex Computer 2048.
//!
//! Loads the TC2048 ROM from
//! `~/.emu198x/roms/timex-tc2048/tc2048.rom`, runs ~4 seconds of CPU
//! time (200 frames), and asserts that the boot screen has rendered
//! enough non-zero bytes into screen RAM to constitute "boot".
//!
//! `#[ignore]`d because not every developer has the ROM locally — the
//! runner prints a path hint and skips when it's missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_timex_tc2048::TimexTC2048;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/timex-tc2048"))
}

#[test]
#[ignore = "requires local TC2048 ROM at ~/.emu198x/roms/timex-tc2048/tc2048.rom"]
fn boot_to_basic_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate TC2048 ROM");
    };
    let rom = dir.join("tc2048.rom");
    if !rom.exists() {
        emu198x_test_skip::skip!("TC2048 ROM not found at {}", rom.display());
    }

    let mut machine = TimexTC2048::new();
    machine
        .memory
        .load_rom(&rom)
        .expect("TC2048 ROM should load");

    for _ in 0..200 {
        machine.run_frame();
    }

    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "TC2048 should boot with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
