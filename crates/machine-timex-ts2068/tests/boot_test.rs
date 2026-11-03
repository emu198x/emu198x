//! Integration boot test for the Timex Sinclair 2068.
//!
//! Loads the TS2068 main ROM + EXROM from
//! `~/.emu198x/roms/timex-ts2068/{ts2068,exrom}.rom`, runs ~4 seconds
//! of CPU time (240 frames at 60 Hz NTSC), and asserts that the boot
//! screen contains both expected copyright banners. Complete ROM identities,
//! typed BASIC, native pixels and restore are checked by the runtime's
//! `timex_original_boot` tests.
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use common_timex_2068::MemoryBus;
use machine_timex_ts2068::TimexTS2068;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/timex-ts2068"))
}

#[test]
#[ignore = "FIXTURE: requires local TS2068 ROMs at ~/.emu198x/roms/timex-ts2068/{ts2068,exrom}.rom"]
fn boot_to_basic_renders_screen_content() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate TS2068 ROMs");
    };
    let main_rom = dir.join("ts2068.rom");
    let exrom = dir.join("exrom.rom");
    if !main_rom.exists() {
        emu198x_test_skip::skip!("TS2068 main ROM not found at {}", main_rom.display());
    }
    if !exrom.exists() {
        emu198x_test_skip::skip!("TS2068 EXROM not found at {}", exrom.display());
    }

    let mut machine = TimexTS2068::new();
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

    let rom = std::fs::read(&main_rom).expect("font ROM");
    assert_eq!(rom.len(), 16384);
    for (row, text) in [
        (21u16, "1982 Sinclair Research Ltd"),
        (23, "1983 Timex Computer Corp"),
    ] {
        let matches = (0..=32 - text.len()).any(|column| {
            text.bytes().enumerate().all(|(i, c)| {
                (0..8u16).all(|scan| {
                    let y = row * 8 + scan;
                    let address = 0x4000
                        | ((y & 0xc0) << 5)
                        | ((y & 7) << 8)
                        | ((y & 0x38) << 2)
                        | u16::try_from(column + i).expect("screen column");
                    machine.memory.read_screen(address)
                        == rom[0x3c00 + usize::from(c) * 8 + usize::from(scan)]
                })
            })
        });
        assert!(matches, "normal startup banner {text}");
    }
}
