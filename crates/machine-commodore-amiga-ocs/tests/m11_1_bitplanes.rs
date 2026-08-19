//! M11.1: bitplane DMA fetch + decode.
//!
//! Per `knowledge/decisions/amiga-restart-plan.md`. The boot's strap module
//! displays the disk-and-hand graphic with 3 bitplanes lores. M11.1
//! adds the data path that lets that graphic actually render:
//!   - Bitplane base pointers (BPL1PT-BPL6PT, $DFF0E0-$DFF0F4)
//!   - Bitplane modulos (BPL1MOD $DFF108, BPL2MOD $DFF10A)
//!   - BPLCON0 BPU bits 14:12 select 0-6 active planes
//!   - Each visible line: fetch words from chip RAM, decode pixels,
//!     write to framebuffer, advance pointers by line + modulo.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn rgb12_to_argb(c12: u16) -> u32 {
    let r = ((c12 >> 8) & 0xF) as u32;
    let g = ((c12 >> 4) & 0xF) as u32;
    let b = (c12 & 0xF) as u32;
    let r8 = (r << 4) | r;
    let g8 = (g << 4) | g;
    let b8 = (b << 4) | b;
    0xFF00_0000 | (r8 << 16) | (g8 << 8) | b8
}

/// Set up a minimal one-bitplane display showing a horizontal striped
/// pattern. Returns the chip-RAM address where bitplane data lives.
fn setup_one_bitplane_stripes(amiga: &mut AmigaOcs) -> u32 {
    // Drop overlay so we can write to chip RAM at low addresses.
    amiga.poke_byte(0x00BFE201, 0x03);
    amiga.poke_byte(0x00BFE001, 0x02);

    // Bitplane data at $10000 (well above bootstrap area).
    let bpl_base = 0x0001_0000u32;
    // Fill each line with alternating $FFFF $0000 words →
    // alternating 16-pixel runs of color 1 and color 0.
    // Display window is 384 lores px = 24 words per line.
    let words_per_line = 24u32;
    let lines = 256u32;
    for line in 0..lines {
        for w in 0..words_per_line {
            let addr = bpl_base + line * words_per_line * 2 + w * 2;
            let val = if w & 1 == 0 { 0xFFFFu16 } else { 0x0000 };
            amiga.poke_word(addr, val);
        }
    }

    // Palette: color 0 = black, color 1 = white.
    amiga.poke_word(0x00DFF180, 0x0000);
    amiga.poke_word(0x00DFF182, 0x0FFF);

    // BPLCON0: BPU = 1, COLOR enable.
    amiga.poke_word(0x00DFF100, 0x1200);

    // Bitplane pointer 1 → bpl_base.
    amiga.poke_word(0x00DFF0E0, (bpl_base >> 16) as u16);
    amiga.poke_word(0x00DFF0E2, bpl_base as u16);

    // Bitplane modulo = 0 (each line follows the previous immediately).
    amiga.poke_word(0x00DFF108, 0);
    amiga.poke_word(0x00DFF10A, 0);

    // Display data fetch — full visible window. DDFSTRT/STOP control
    // the fetch hpos range; DIWSTRT/STOP control the visible window.
    amiga.poke_word(0x00DFF092, 0x0038); // DDFSTRT
    amiga.poke_word(0x00DFF094, 0x00D0); // DDFSTOP
    amiga.poke_word(0x00DFF08E, 0x2C81); // DIWSTRT
    amiga.poke_word(0x00DFF090, 0xF4C1); // DIWSTOP

    // Enable DMA: master + bitplane.
    amiga.poke_word(0x00DFF096, 0x8300);

    bpl_base
}

#[test]
fn one_bitplane_stripes_render() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    let _ = setup_one_bitplane_stripes(&mut amiga);

    // Run one PAL frame (master/4 ticks).
    for _ in 0..PAL_FRAME_TICKS {
        amiga.tick();
    }

    let fb = amiga.denise().framebuffer();
    let (w, _h) = amiga.denise().framebuffer_size();

    // With alternating-word data ($FFFF, $0000, $FFFF, ...), each
    // 16-bit lores word covers 32 displayed pixels (lores bits are
    // pixel-doubled horizontally). Expect runs of 32 white / 32 black.
    //
    // Cycle-accurate Denise has a fetch warm-up: for BPU=1 the first
    // BPL1 fetch lands at slot 6 of the first DDF block, the shift
    // register reloads at slot 7, and the first fetched pixel reaches
    // the framebuffer 7 CCKs into the DDF window. That's 7 CCKs * 2
    // lores pixels/CCK * 2 display pixels/lores = 28 display pixels
    // from DDFSTRT. The framebuffer itself is aligned to the PAL
    // Standard viewport (`h_start_cck = $2C`), so DDFSTRT=$38 lands
    // 12 CCKs = 48 display pixels into the framebuffer. Total x-
    // position of the first bitplane pixel = 48 + 28 = 76.
    let center_line = 200u32;
    let row_start = center_line * w;
    let white = rgb12_to_argb(0x0FFF);
    let black = rgb12_to_argb(0x0000);
    let viewport_h_px = (0x38u32 - 0x2C) * 4; // 48
    let warmup_px = viewport_h_px + 28;

    for word in 0..8 {
        let base_x = warmup_px + (word as u32) * 32;
        let expect = if word & 1 == 0 { white } else { black };
        // Sample mid-word to avoid any edge ambiguity.
        let sample_x = base_x + 16;
        let p = fb[(row_start + sample_x) as usize];
        assert_eq!(
            p, expect,
            "word {word} (x={sample_x}): expected ${expect:08X}, got ${p:08X}",
        );
    }
}
