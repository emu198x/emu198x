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
        drive_rom: None,
        sid_model: emu_c64::config::SidModel::Sid6581,
        reu_size: None,
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
// SID Symphony Unit 1: three-track rhythm display with SID audio
//
// Uses screen RAM, colour RAM, SID registers, and CIA keyboard.
// No sprites needed — verifies character-mode rendering and SID init.
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires C64 ROMs and Code198x repo
fn test_sid_symphony_unit01() {
    let prg_path = match code198x_path(
        "commodore-64/game-01-sid-symphony/unit-01/symphony.prg",
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

    // Boot to READY. prompt (~120 frames)
    for _ in 0..120 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Load the PRG and jump to the ML entry point ($0810)
    let prg_data = std::fs::read(&prg_path).expect("read PRG");
    c64.load_prg(&prg_data).expect("load PRG");
    c64.cpu_mut().regs.pc = 0x0810;

    // Run frames for the program to set up the display
    for _ in 0..60 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Verify: border and background should be black
    let border = c64.bus().vic.peek(0x20) & 0x0F;
    let background = c64.bus().vic.peek(0x21) & 0x0F;
    assert_eq!(border, 0, "SID Symphony sets border to black");
    assert_eq!(background, 0, "SID Symphony sets background to black");

    // Verify: screen has the title "SID SYMPHONY" at row 0, col 13.
    // PETSCII screen codes: S=19, I=9, D=4, space=32, etc.
    let title_addr = 0x0400 + 13;
    let s_code = c64.bus().memory.ram_read(title_addr);
    let i_code = c64.bus().memory.ram_read(title_addr + 1);
    let d_code = c64.bus().memory.ram_read(title_addr + 2);
    assert_eq!(s_code, 19, "First char should be S (screen code 19), got {s_code}");
    assert_eq!(i_code, 9, "Second char should be I (screen code 9), got {i_code}");
    assert_eq!(d_code, 4, "Third char should be D (screen code 4), got {d_code}");

    // Verify: track lines are drawn (minus chars = screen code $2D = 45)
    // Track 1 at row 8, Track 2 at row 12, Track 3 at row 16
    let track1_char = c64.bus().memory.ram_read(0x0400 + 8 * 40 + 5);
    let track2_char = c64.bus().memory.ram_read(0x0400 + 12 * 40 + 5);
    let track3_char = c64.bus().memory.ram_read(0x0400 + 16 * 40 + 5);
    assert_eq!(track1_char, 0x2D, "Track 1 line should be minus char");
    assert_eq!(track2_char, 0x2D, "Track 2 line should be minus char");
    assert_eq!(track3_char, 0x2D, "Track 3 line should be minus char");

    // Verify: SID volume is set to maximum ($0F)
    // SID $D418 lower nibble = volume
    let sid_vol = c64.bus().memory.peek(0xD418) & 0x0F;
    assert_eq!(sid_vol, 0x0F, "SID volume should be max (15), got {sid_vol}");

    // Save screenshot
    let path = format!("{OUTPUT_DIR}/code198x_sid_symphony_unit01.png");
    save_screenshot(&c64, path.as_ref()).expect("save screenshot");
    eprintln!("Saved {path}");
}

// ---------------------------------------------------------------------------
// Starfield Unit 1: Ship sprite on screen
//
// Verifies the program sets up sprite 0 at (172, 220) with the ship pattern,
// and that the sprite is visibly rendered in the framebuffer.
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires C64 ROMs and Code198x repo
fn test_starfield_unit01_sprite() {
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

    // Boot, load PRG, jump to entry point
    for _ in 0..120 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }
    let prg_data = std::fs::read(&prg_path).expect("read PRG");
    c64.load_prg(&prg_data).expect("load PRG");
    c64.cpu_mut().regs.pc = 0x080D;
    for _ in 0..60 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Program executes: border=black, sprite 0 enabled at (172, 220)
    assert_eq!(c64.bus().vic.peek(0x20) & 0x0F, 0, "Border should be black");
    assert_eq!(c64.bus().vic.peek(0x15) & 0x01, 0x01, "Sprite 0 should be enabled");
    assert_eq!(c64.bus().vic.peek(0x00), 172, "Sprite 0 X should be 172");
    assert_eq!(c64.bus().vic.peek(0x01), 220, "Sprite 0 Y should be 220");

    // Sprite data at $2000 should be the ship pattern
    assert_eq!(c64.bus().memory.ram_read(0x2001), 0x18, "Sprite row 0 mid-byte");

    // Verify sprite pixels are visible in the framebuffer.
    // The ship has white (colour 1) pixels on a black (colour 0) background.
    // Sprite at X=172, Y=220 → fb position (196, 214).
    // Scan the sprite area for any non-black pixels.
    let fb = c64.bus().vic.framebuffer();
    let fb_w = c64.bus().vic.framebuffer_width() as usize;
    let sprite_colour = c64.bus().vic.peek(0x27) & 0x0F;
    eprintln!("Sprite 0 colour register: {sprite_colour}");

    let fb_sprite_x = 196usize; // 172 + 24
    let fb_sprite_y = 214usize; // 220 - 6 (FIRST_VISIBLE_LINE)
    let mut sprite_pixels = 0u32;
    for dy in 0..21usize {
        for dx in 0..24usize {
            let idx = (fb_sprite_y + dy) * fb_w + fb_sprite_x + dx;
            if idx < fb.len() && fb[idx] != 0xFF00_0000 {
                sprite_pixels += 1;
            }
        }
    }
    eprintln!("Non-black pixels in sprite area: {sprite_pixels}");
    assert!(
        sprite_pixels > 0,
        "Sprite should have visible (non-black) pixels in the framebuffer"
    );

    let path = format!("{OUTPUT_DIR}/code198x_starfield_unit01.png");
    save_screenshot(&c64, path.as_ref()).expect("save screenshot");
    eprintln!("Saved {path}");
}
