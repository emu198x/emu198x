//! Integration boot test for the Timex Sinclair 2068.
//!
//! Loads the TS2068 main ROM + EXROM from
//! `~/.emu198x/roms/timex-ts2068/{ts2068,exrom}.rom`, runs ~4 seconds
//! of CPU time (240 frames at 60 Hz NTSC), and asserts that the boot
//! screen has rendered enough non-zero bytes into screen RAM to
//! constitute "boot".
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_timex_ts2068::{TimexModel, TimexTS2068};
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/timex-ts2068"))
}

#[test]
#[ignore = "requires local TS2068 ROMs at ~/.emu198x/roms/timex-ts2068/{ts2068,exrom}.rom"]
fn boot_to_basic_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        eprintln!("HOME not set — cannot locate TS2068 ROMs");
        return;
    };
    let main_rom = dir.join("ts2068.rom");
    let exrom = dir.join("exrom.rom");
    if !main_rom.exists() {
        eprintln!("TS2068 main ROM not found at {}", main_rom.display());
        return;
    }
    if !exrom.exists() {
        eprintln!("TS2068 EXROM not found at {}", exrom.display());
        return;
    }

    let mut machine = TimexTS2068::new(TimexModel::TS2068);
    machine
        .memory
        .load_rom(&main_rom)
        .expect("TS2068 main ROM should load");
    machine
        .memory
        .load_exrom(&exrom)
        .expect("TS2068 EXROM should load");

    // TS2068 is 60 Hz NTSC — 240 frames ≈ 4 seconds.
    for _ in 0..240 {
        machine.run_frame();
    }

    let nonzero: usize = (0x4000u16..0x5800)
        .filter(|&addr| machine.memory.read(addr) != 0)
        .count();

    assert!(
        nonzero > 50,
        "TS2068 should boot with screen content (got {nonzero} non-zero bytes in screen RAM)"
    );
}
