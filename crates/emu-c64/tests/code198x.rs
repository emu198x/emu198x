//! Code Like It's 198x integration tests.
//!
//! Verify that the C64 emulator can run actual lesson code from the Code198x
//! curriculum. Loads compiled PRG binaries and confirms visible output.

#![allow(clippy::cast_possible_truncation)]

use emu_c64::capture::save_screenshot;
use emu_c64::{C64, C64Config, C64Model};

const OUTPUT_DIR: &str = "../../test_output";

fn load_c64() -> Option<C64> {
    let kernal = std::fs::read("../../roms/kernal.rom").ok()?;
    let basic = std::fs::read("../../roms/basic.rom").ok()?;
    let chargen = std::fs::read("../../roms/chargen.rom").ok()?;
    Some(C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    }))
}

fn ensure_output_dir() {
    let _ = std::fs::create_dir_all(OUTPUT_DIR);
}

fn code198x_path(relative: &str) -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../Code198x/code-samples")
        .join(relative);
    if base.exists() {
        Some(base)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Starfield Unit 1: Ship sprite on screen
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires C64 ROMs and Code198x repo
fn test_starfield_unit01() {
    let prg_path = match code198x_path(
        "commodore-64/game-01-starfield/unit-01/starfield.prg",
    ) {
        Some(p) => p,
        None => {
            eprintln!("Skipping: Code198x repo not found");
            return;
        }
    };

    let mut c64 = match load_c64() {
        Some(c) => c,
        None => {
            eprintln!("Skipping: C64 ROMs not found");
            return;
        }
    };

    ensure_output_dir();

    // Boot to READY. prompt first (~120 frames)
    for _ in 0..120 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Load the PRG — writes bytes into RAM at the load address ($0801)
    let prg_data = std::fs::read(&prg_path).expect("read PRG");
    c64.load_prg(&prg_data).expect("load PRG");

    // Jump directly to the machine code entry point ($080D = 2061 decimal).
    // This bypasses BASIC's RUN/SYS — we set PC directly to the ML code.
    c64.cpu_mut().regs.pc = 0x080D;

    // Run frames for the program to execute and render
    for _ in 0..60 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Verify: border and background should be black ($00)
    let border = c64.bus().vic.read(0x20); // $D020
    let background = c64.bus().vic.read(0x21); // $D021
    assert_eq!(border & 0x0F, 0, "Starfield sets border to black");
    assert_eq!(background & 0x0F, 0, "Starfield sets background to black");

    // Verify: sprite 0 should be enabled
    let sprite_enable = c64.bus().vic.read(0x15); // $D015
    assert_eq!(
        sprite_enable & 0x01,
        0x01,
        "Starfield enables sprite 0"
    );

    // Verify: sprite 0 position should be near center-bottom
    let sprite_x = c64.bus().vic.read(0x00); // $D000
    let sprite_y = c64.bus().vic.read(0x01); // $D001
    assert!(sprite_x > 100, "Sprite X should be near center ({sprite_x})");
    assert!(sprite_y > 180, "Sprite Y should be near bottom ({sprite_y})");

    // Verify: sprite data at $2000 should have the ship pattern
    let sprite_byte = c64.bus().memory.ram_read(0x2000);
    assert_eq!(sprite_byte, 0x00, "First sprite byte should be $00");
    let sprite_byte_1 = c64.bus().memory.ram_read(0x2001);
    assert_eq!(sprite_byte_1, 0x18, "Second sprite byte should be $18");

    // Save screenshot
    let path = format!("{OUTPUT_DIR}/code198x_starfield_unit01.png");
    save_screenshot(&c64, path.as_ref()).expect("save screenshot");
    eprintln!("Saved {path}");
}
