//! C64 Kernal boot test — verify the machine boots to BASIC READY. prompt.

use emu_core::Bus;
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

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_sid_produces_audio() {
    let kernal =
        fs::read("../../roms/kernal.rom").expect("kernal.rom not found at roms/kernal.rom");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found at roms/basic.rom");
    let chargen =
        fs::read("../../roms/chargen.rom").expect("chargen.rom not found at roms/chargen.rom");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    // Boot past READY.
    for _ in 0..120 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Poke SID registers for a sawtooth tone via the bus (like a program would).
    // Voice 1: ~440 Hz sawtooth, instant attack, max sustain, volume 15.
    let sid_base = 0xD400u32;
    let freq: u16 = 7479; // 440 Hz
    c64.bus_mut().write(sid_base, (freq & 0xFF) as u8);       // Freq lo
    c64.bus_mut().write(sid_base + 1, (freq >> 8) as u8);     // Freq hi
    c64.bus_mut().write(sid_base + 5, 0x00);                  // AD: attack=0, decay=0
    c64.bus_mut().write(sid_base + 6, 0xF0);                  // SR: sustain=F, release=0
    c64.bus_mut().write(sid_base + 4, 0x21);                  // Sawtooth + gate on
    c64.bus_mut().write(sid_base + 0x18, 0x0F);               // Volume = 15

    // Run one frame to produce audio
    c64.run_frame();
    let audio = c64.take_audio_buffer();

    println!("SID audio buffer: {} samples", audio.len());

    assert!(!audio.is_empty(), "SID should produce audio samples");

    // Verify non-silent: at least some samples above noise floor
    let max_abs = audio.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    println!("Max absolute sample: {max_abs}");
    assert!(
        max_abs > 0.01,
        "SID audio should be non-silent with sawtooth playing, max={max_abs}"
    );

    // Save as WAV for manual verification
    let out_dir = Path::new("../../test_output");
    fs::create_dir_all(out_dir).ok();
    let audio_path = out_dir.join("c64_sid_tone.wav");
    emu_c64::capture::save_audio(&audio, &audio_path).expect("Failed to save audio");
    println!("Audio saved to {}", audio_path.display());
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
