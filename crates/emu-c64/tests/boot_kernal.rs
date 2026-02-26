//! C64 Kernal boot test — verify the machine boots to BASIC READY. prompt.

use emu_c64::{C64, C64Config, C64Model};
use std::fs;
use std::path::Path;

/// PETSCII codes for "READY."
const READY_PETSCII: [u8; 6] = [
    18, // R
    5,  // E
    1,  // A
    4,  // D
    25, // Y
    46, // .
];

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_boot_kernal() {
    let kernal = fs::read("../../roms/kernal.rom").expect("kernal.rom not found at roms/kernal.rom");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found at roms/basic.rom");
    let chargen =
        fs::read("../../roms/chargen.rom").expect("chargen.rom not found at roms/chargen.rom");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    println!("Reset: PC=${:04X}", c64.cpu().regs.pc);

    let max_frames = 200;
    let mut found_ready = false;

    for frame in 0..max_frames {
        let cycles = c64.run_frame();

        // Every 50 frames (~1s), print diagnostic
        if frame % 50 == 0 {
            println!(
                "Frame {frame}: PC=${:04X} cycles={cycles}",
                c64.cpu().regs.pc
            );
        }

        // Search screen memory ($0400-$07E7) for "READY."
        if find_ready_in_screen(&c64) {
            println!("READY. found at frame {frame}!");
            found_ready = true;

            // Run a few more frames so the VIC renders the complete screen
            for _ in 0..2 {
                c64.run_frame();
            }

            // Save screenshot
            let out_dir = Path::new("../../test_output");
            fs::create_dir_all(out_dir).ok();
            let screenshot_path = out_dir.join("c64_boot_ready.png");
            emu_c64::capture::save_screenshot(&c64, &screenshot_path)
                .expect("Failed to save screenshot");
            println!("Screenshot saved to {}", screenshot_path.display());
            break;
        }
    }

    assert!(found_ready, "C64 did not reach READY. prompt within {max_frames} frames");
}

/// Scan screen memory for the PETSCII sequence "READY."
fn find_ready_in_screen(c64: &C64) -> bool {
    let screen_start = 0x0400u16;
    let screen_end = 0x07E8u16;

    for addr in screen_start..screen_end {
        if addr + READY_PETSCII.len() as u16 > screen_end {
            break;
        }

        let mut matches = true;
        for (i, &expected) in READY_PETSCII.iter().enumerate() {
            let actual = c64.bus().memory.peek(addr + i as u16);
            if actual != expected {
                matches = false;
                break;
            }
        }

        if matches {
            return true;
        }
    }

    false
}
