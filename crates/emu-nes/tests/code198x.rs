//! Code Like It's 198x integration tests.
//!
//! Verify that the NES emulator can run actual lesson code from the Code198x
//! curriculum. Loads compiled .nes ROMs and confirms visible output.

#![allow(clippy::cast_possible_truncation)]

use emu_nes::capture::save_screenshot;
use emu_nes::{Nes, NesConfig};

const OUTPUT_DIR: &str = "../../test_output";

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
// Dash Unit 1: Running figure sprite on screen
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires Code198x repo
fn test_dash_unit01() {
    let rom_path = match code198x_path(
        "nintendo-entertainment-system/game-01-dash/unit-01/dash.nes",
    ) {
        Some(p) => p,
        None => {
            eprintln!("Skipping: Code198x repo not found");
            return;
        }
    };

    ensure_output_dir();

    let rom_data = std::fs::read(&rom_path).expect("read ROM");
    let config = NesConfig { rom_data };
    let mut nes = Nes::new(&config).expect("create NES");

    // Run 30 frames — enough for reset sequence (two vblank waits),
    // palette load, sprite setup, and NMI to OAM DMA the sprite
    for _ in 0..30 {
        nes.run_frame();
    }

    // Verify: OAM should contain the player sprite at the expected position.
    // OAM byte 0 = Y position (PLAYER_Y = 120)
    // OAM byte 1 = tile number (PLAYER_TILE = 1)
    // OAM byte 3 = X position (PLAYER_X = 124)
    let sprite_y = nes.bus().ppu.read_oam(0);
    let sprite_tile = nes.bus().ppu.read_oam(1);
    let sprite_x = nes.bus().ppu.read_oam(3);

    assert_eq!(sprite_y, 120, "Sprite Y should be 120 (got {sprite_y})");
    assert_eq!(sprite_tile, 1, "Sprite tile should be 1 (got {sprite_tile})");
    assert_eq!(sprite_x, 124, "Sprite X should be 124 (got {sprite_x})");

    // Verify: framebuffer has non-black pixels (the sprite should be visible)
    let fb = nes.bus().ppu.framebuffer();
    let non_black = fb.iter().filter(|&&p| p != 0 && p != 0xFF000000).count();
    assert!(
        non_black > 10,
        "Framebuffer should have non-black pixels from the sprite (found {non_black})"
    );

    // Save screenshot
    let path = format!("{OUTPUT_DIR}/code198x_dash_unit01.png");
    save_screenshot(&nes, path.as_ref()).expect("save screenshot");
    eprintln!("Saved {path}");
}
