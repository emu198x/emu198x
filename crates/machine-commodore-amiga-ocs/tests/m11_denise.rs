//! M11: Denise pixel pipeline — background only.
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. Adds Denise, the Amiga
//! display chip, with:
//!   - Framebuffer (PAL Standard 768×576, ARGB8888).
//!   - Raster position derived from Agnus's beam.
//!   - One pixel-output per CCK (lores: 2 pixels per CCK).
//!   - Palette lookup (read from chipset.color).
//!
//! M11 covers BACKGROUND ONLY — every visible pixel is COLOR00.
//! Bitplane fetch + decode follows in M11.1 if the boot demands.
//!
//! Display window: PAL Standard viewport from the archived
//! investigation: h_start_cck $2C, h_end_cck $EC, v_start_line $19,
//! v_end_line $139. = 192 CCKs × 288 lines = 384 lores px × 288 lines
//! at lores; line-doubled to 768×576 for 4:3 display.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_LINES, PAL_LINE_CCKS};

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn rgb12_to_argb(c12: u16) -> u32 {
    let r = ((c12 >> 8) & 0xF) as u32;
    let g = ((c12 >> 4) & 0xF) as u32;
    let b = (c12 & 0xF) as u32;
    // Replicate nibble to byte: $A → $AA.
    let r8 = (r << 4) | r;
    let g8 = (g << 4) | g;
    let b8 = (b << 4) | b;
    0xFF00_0000 | (r8 << 16) | (g8 << 8) | b8
}

#[test]
fn framebuffer_dimensions_are_pal_standard() {
    let Some(rom) = load_kickstart() else { return };
    let amiga = AmigaOcs::new(rom);
    let (w, h) = amiga.denise().framebuffer_size();
    assert_eq!(w, 768, "PAL Standard width should be 768 lores px (line-doubled)");
    assert_eq!(h, 576, "PAL Standard height should be 576 (line-doubled)");
    assert_eq!(amiga.denise().framebuffer().len(), (w * h) as usize);
}

#[test]
fn visible_pixels_match_color00() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Drop overlay so the boot's chipset writes (which we'll skip)
    // don't matter. Configure COLOR00 directly via poke.
    amiga.poke_byte(0x00BFE201, 0x03);
    amiga.poke_byte(0x00BFE001, 0x02);
    amiga.poke_word(0x00DFF180, 0x0F00); // COLOR00 = red

    // Set up a PAL-standard display window so Denise has somewhere
    // to render. Denise now reads these from the chipset registers
    // instead of using fixed constants.
    amiga.poke_word(0x00DFF08E, 0x2C81); // DIWSTRT — V=$2C, H=$81
    amiga.poke_word(0x00DFF090, 0x2CC1); // DIWSTOP — V=$2C (→$12C), H=$C1 (→$1C1)
    amiga.poke_word(0x00DFF092, 0x0038); // DDFSTRT — lores standard
    amiga.poke_word(0x00DFF094, 0x00D0); // DDFSTOP — lores standard

    // Run one full PAL frame.
    let frame_ccks = u64::from(PAL_LINE_CCKS) * u64::from(PAL_FRAME_LINES);
    for _ in 0..frame_ccks {
        amiga.tick_cck();
    }

    let fb = amiga.denise().framebuffer();
    let expected = rgb12_to_argb(0x0F00);
    // Sample a pixel near the center of the visible viewport. With
    // DIW V = [$2C, $12C), the displayed rows start at line $2C = 44.
    // Framebuffer y = (vpos - $2C) * 2, so vpos $90 (144, roughly
    // mid-screen) lands at framebuffer row ($90 - $2C) * 2 = $E0*2
    // = 200 / ... actually (144-44)*2 = 200. Sample there.
    let (w, _h) = amiga.denise().framebuffer_size();
    let center_idx = (200 * w + 384) as usize;
    assert_eq!(
        fb[center_idx], expected,
        "Center pixel should be COLOR00=$0F00 → ARGB ${expected:08X}, got ${:08X}",
        fb[center_idx]
    );
}
