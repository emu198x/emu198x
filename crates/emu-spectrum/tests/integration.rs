//! Integration tests for the ZX Spectrum 48K emulator.
//!
//! These tests boot the real 48K ROM and verify the emulator produces correct
//! video and audio output. Artefacts are saved to `test_output/` at the
//! repository root for visual inspection.

#![allow(clippy::cast_possible_truncation)]

use std::path::Path;

use emu_spectrum::capture::{save_audio, save_screenshot};
use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumModel};

/// Embedded 48K ROM.
const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

fn make_spectrum_48k() -> Spectrum {
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: ROM_48K.to_vec(),
    };
    Spectrum::new(&config)
}

fn make_spectrum_custom_rom(rom: &[u8]) -> Spectrum {
    let mut padded = vec![0u8; 0x4000];
    let len = rom.len().min(0x4000);
    padded[..len].copy_from_slice(&rom[..len]);
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: padded,
    };
    Spectrum::new(&config)
}

/// Output directory for test artefacts (repo root's test_output/).
const OUTPUT_DIR: &str = "../../test_output";

fn ensure_output_dir() {
    let _ = std::fs::create_dir_all(OUTPUT_DIR);
}

// ---------------------------------------------------------------------------
// Test 1: Boot the real 48K ROM and verify the display
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires real ROM
fn test_boot_48k() {
    ensure_output_dir();

    let mut spectrum = make_spectrum_48k();

    // Run ~100 frames (~2 seconds of emulated time)
    for _ in 0..100 {
        spectrum.run_frame();
    }

    // Verify border is white (7) -- the ROM sets this during init
    assert_eq!(
        spectrum.bus().ula.border_colour(),
        7,
        "Border should be white after boot"
    );

    // Verify the display file ($4000-$57FF) has non-zero pixel data
    // (the ROM renders "(c) 1982 Sinclair Research Ltd" and the cursor)
    let mut has_screen_data = false;
    for addr in 0x4000..=0x57FFu16 {
        if spectrum.bus().memory.peek(addr) != 0 {
            has_screen_data = true;
            break;
        }
    }
    assert!(
        has_screen_data,
        "Display file should contain non-zero bitmap data after boot"
    );

    // Save hero screenshot
    let path = Path::new(OUTPUT_DIR).join("spectrum_boot.png");
    save_screenshot(&spectrum, &path).expect("Failed to save screenshot");
    assert!(path.exists(), "Screenshot should exist");
    eprintln!("Saved boot screenshot to {}", path.display());
}

// ---------------------------------------------------------------------------
// Test 2: Border stripes -- ULA renders per-pixel border changes
// ---------------------------------------------------------------------------

#[test]
fn test_border_stripes() {
    ensure_output_dir();

    // Build a small Z80 ROM that cycles through border colours in a tight
    // loop. No interrupts needed -- DI and spin. Each OUT changes the border
    // colour visible on subsequent scanlines, creating horizontal stripes.
    //
    // $0000: DI
    // $0001: XOR A          ; start with colour 0
    // $0002: OUT ($FE),A    ; set border
    // $0004: INC A          ; next colour
    // $0005: AND 7          ; wrap to 0-7
    // $0007: LD B,$60       ; delay (~96 iterations x 13T = ~1248T ~ 5.6 lines)
    // $0009: DJNZ $0009     ; tight delay
    // $000B: JR $0002       ; loop forever
    let mut rom = vec![0x00u8; 0x4000];

    // $0000: DI
    rom[0x0000] = 0xF3;
    // $0001: XOR A
    rom[0x0001] = 0xAF;
    // $0002: OUT ($FE),A
    rom[0x0002] = 0xD3;
    rom[0x0003] = 0xFE;
    // $0004: INC A
    rom[0x0004] = 0x3C;
    // $0005: AND 7
    rom[0x0005] = 0xE6;
    rom[0x0006] = 0x07;
    // $0007: LD B,$60
    rom[0x0007] = 0x06;
    rom[0x0008] = 0x60;
    // $0009: DJNZ $0009
    rom[0x0009] = 0x10;
    rom[0x000A] = 0xFE;
    // $000B: JR $0002
    rom[0x000B] = 0x18;
    rom[0x000C] = 0xF5; // -11: next=$000D, target=$000D-11=$0002

    let mut spectrum = make_spectrum_custom_rom(&rom);

    // Run 3 frames
    for _ in 0..3 {
        spectrum.run_frame();
    }

    // Verify framebuffer has multiple distinct colours in the border area.
    // Sample the left border column (x=0) across all visible lines.
    let fb = spectrum.framebuffer();
    let width = spectrum.framebuffer_width() as usize;
    let mut colours = std::collections::HashSet::new();
    for y in 0..spectrum.framebuffer_height() as usize {
        let pixel = fb[y * width]; // x=0, left border column
        colours.insert(pixel);
    }

    assert!(
        colours.len() >= 3,
        "Border should have at least 3 distinct colours (got {}), proving border rendering works",
        colours.len()
    );

    // Save screenshot
    let path = Path::new(OUTPUT_DIR).join("spectrum_border_stripes.png");
    save_screenshot(&spectrum, &path).expect("Failed to save screenshot");
    assert!(path.exists());
    eprintln!(
        "Saved border stripes screenshot to {} ({} colours)",
        path.display(),
        colours.len()
    );
}

// ---------------------------------------------------------------------------
// Test 3: Beeper tone -- audio capture
// ---------------------------------------------------------------------------

#[test]
fn test_beeper_tone() {
    ensure_output_dir();

    // Build a ROM that generates a square wave on the beeper.
    //
    // $0000: DI
    // $0001: LD A,$10    -- beeper on (bit 4)
    // $0003: OUT ($FE),A -- write beeper
    // $0005: XOR $10     -- toggle bit 4
    // $0007: LD B,$40    -- delay
    // $0009: DJNZ $0009  -- tight delay loop
    // $000B: JR $0003    -- loop
    let mut rom = vec![0x00u8; 0x4000];

    // $0000: DI
    rom[0x0000] = 0xF3;
    // $0001: LD A,$10
    rom[0x0001] = 0x3E;
    rom[0x0002] = 0x10;
    // $0003: OUT ($FE),A
    rom[0x0003] = 0xD3;
    rom[0x0004] = 0xFE;
    // $0005: XOR $10
    rom[0x0005] = 0xEE;
    rom[0x0006] = 0x10;
    // $0007: LD B,$40
    rom[0x0007] = 0x06;
    rom[0x0008] = 0x40;
    // $0009: DJNZ $0009
    rom[0x0009] = 0x10;
    rom[0x000A] = 0xFE;
    // $000B: JR $0003
    rom[0x000B] = 0x18;
    rom[0x000C] = 0xF6; // -10: next=$000D, target=$000D-10=$0003

    let mut spectrum = make_spectrum_custom_rom(&rom);

    // Run 50 frames (~1 second at PAL 50 Hz) to produce a usable audio clip.
    let mut all_audio: Vec<[f32; 2]> = Vec::new();
    for _ in 0..50 {
        spectrum.run_frame();
        all_audio.extend_from_slice(&spectrum.take_audio_buffer());
    }

    // Verify audio buffer has samples
    assert!(
        !all_audio.is_empty(),
        "Audio buffer should have samples after 50 frames"
    );

    // Check left channel (beeper is mono, duplicated to both)
    let has_positive = all_audio.iter().any(|s| s[0] > 0.0);
    let has_negative = all_audio.iter().any(|s| s[0] < 0.0);
    assert!(
        has_positive && has_negative,
        "Audio should have both positive and negative samples (square wave)"
    );

    // Verify reasonable amplitude
    let max_abs = all_audio
        .iter()
        .map(|s| s[0].abs())
        .fold(0.0f32, f32::max);
    assert!(
        max_abs > 0.5,
        "Max amplitude should be > 0.5 for a beeper square wave, got {max_abs}"
    );

    // Save WAV (stereo)
    let path = Path::new(OUTPUT_DIR).join("spectrum_beeper_tone.wav");
    save_audio(&all_audio, &path).expect("Failed to save audio");
    assert!(path.exists());
    eprintln!(
        "Saved beeper tone to {} ({} samples, max amplitude {:.3})",
        path.display(),
        all_audio.len(),
        max_abs
    );
}
