//! PPU integration tests — drive the full pipeline via `tick`.

use super::*;

const VRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;

fn blank_vram() -> Vec<u8> {
    vec![0; VRAM_SIZE]
}

fn blank_oam() -> Vec<u8> {
    vec![0; OAM_SIZE]
}

fn run_dots(ppu: &mut Ppu, vram: &[u8], oam: &[u8], dots: u32) {
    for _ in 0..dots {
        ppu.tick(vram, oam);
    }
}

// -- Timing -----------------------------------------------------------

#[test]
fn ly_increments_every_456_dots() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    run_dots(&mut ppu, &vram, &oam, 456);
    assert_eq!(ppu.ly, 1);
}

#[test]
fn ly_wraps_at_154() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    run_dots(&mut ppu, &vram, &oam, 456 * 154);
    assert_eq!(ppu.ly, 0);
}

#[test]
fn frame_ready_set_at_start_of_vblank() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    run_dots(&mut ppu, &vram, &oam, 456 * 144 + 1);
    assert_eq!(ppu.ly, 144);
    assert!(ppu.consume_frame_ready());
    assert!(!ppu.consume_frame_ready(), "consume clears the latch");
}

#[test]
fn vblank_irq_latches_once_per_frame() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    run_dots(&mut ppu, &vram, &oam, 456 * 144 + 1);
    assert!(ppu.consume_vblank_irq());
    assert!(!ppu.consume_vblank_irq());
    // Continue to the next VBlank.
    run_dots(&mut ppu, &vram, &oam, 456 * 154);
    assert!(ppu.consume_vblank_irq());
}

#[test]
fn mode_transitions_within_a_visible_scanline() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    assert_eq!(ppu.mode(), 2, "starts in OAM scan");
    run_dots(&mut ppu, &vram, &oam, 80);
    assert_eq!(ppu.mode(), 3, "after OAM scan, pixel transfer");
}

#[test]
fn lcd_off_freezes_timing() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    ppu.write_lcdc(0); // disable
    run_dots(&mut ppu, &vram, &oam, 1000);
    assert_eq!(ppu.ly, 0);
    assert_eq!(ppu.dot, 0);
}

#[test]
fn turning_lcd_back_on_resumes_from_zero() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    run_dots(&mut ppu, &vram, &oam, 100);
    ppu.write_lcdc(0);
    run_dots(&mut ppu, &vram, &oam, 1000);
    assert_eq!(ppu.ly, 0);
    ppu.write_lcdc(0x91);
    run_dots(&mut ppu, &vram, &oam, 456);
    assert_eq!(ppu.ly, 1);
}

// -- STAT register ---------------------------------------------------

#[test]
fn read_stat_reports_current_mode() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    assert_eq!(ppu.read_stat() & 0b11, 2);
    run_dots(&mut ppu, &vram, &oam, 80);
    assert_eq!(ppu.read_stat() & 0b11, 3);
}

#[test]
fn read_stat_reports_lyc_match() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    ppu.lyc = 5;
    run_dots(&mut ppu, &vram, &oam, 456 * 5);
    assert_eq!(ppu.ly, 5);
    assert_ne!(ppu.read_stat() & 0x04, 0, "LYC coincidence flag set");
}

#[test]
fn write_stat_only_keeps_writable_bits() {
    let mut ppu = Ppu::new();
    ppu.write_stat(0xFF);
    // Only bits 3-6 are writable; mode + LYC bits are computed.
    assert_eq!(ppu.read_stat() & stat::WRITABLE_MASK, stat::WRITABLE_MASK);
}

#[test]
fn stat_irq_fires_on_lyc_match_when_enabled() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    ppu.lyc = 5;
    ppu.write_stat(stat::LYC_ENABLE);
    let _ = ppu.consume_stat_irq();
    run_dots(&mut ppu, &vram, &oam, 456 * 5);
    assert_eq!(ppu.ly, 5);
    assert!(ppu.consume_stat_irq(), "STAT IRQ should fire on LYC match");
    assert!(!ppu.consume_stat_irq(), "consume clears the latch");
}

#[test]
fn stat_irq_does_not_fire_when_lyc_enable_clear() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    ppu.lyc = 5;
    let _ = ppu.consume_stat_irq();
    run_dots(&mut ppu, &vram, &oam, 456 * 6);
    assert!(!ppu.consume_stat_irq());
}

#[test]
fn stat_irq_fires_on_mode1_entry() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    ppu.write_stat(stat::MODE1_ENABLE);
    let _ = ppu.consume_stat_irq();
    run_dots(&mut ppu, &vram, &oam, 456 * 144 + 1);
    assert!(ppu.consume_stat_irq(), "STAT mode-1 IRQ at VBlank entry");
}

// -- Pixel rendering -------------------------------------------------

#[test]
fn renders_solid_index_one_when_tile_bytes_set_low_only() {
    // Tile 0 row 0: low=0xFF, high=0x00 → every pixel = index 1.
    let mut vram = blank_vram();
    vram[0] = 0xFF;
    vram[1] = 0x00;
    // Tile map at $9800 ($1800 in VRAM offset) defaults to all zeros
    // so every cell points at tile 0.

    let mut ppu = Ppu::new();
    let oam = blank_oam();
    ppu.lcdc = 0x91;
    ppu.bgp = 0xE4; // identity palette
    ppu.scx = 0;
    ppu.scy = 0;

    run_dots(&mut ppu, &vram, &oam, 456);

    let fb = ppu.framebuffer();
    assert_eq!(fb[0], 1, "leftmost pixel of line 0");
    assert_eq!(fb[79], 1);
    assert_eq!(fb[159], 1, "rightmost pixel of line 0");
}

#[test]
fn bg_disable_forces_color_zero() {
    let mut vram = blank_vram();
    vram[0] = 0xFF;
    vram[1] = 0x00;

    let mut ppu = Ppu::new();
    let oam = blank_oam();
    ppu.lcdc = 0x90; // LCD on, BG off
    ppu.bgp = 0xE4;

    run_dots(&mut ppu, &vram, &oam, 456);

    let fb = ppu.framebuffer();
    assert_eq!(fb[0], 0, "BG disabled pixels read as palette[0]");
}

#[test]
fn apply_palette_decodes_each_slot() {
    // BGP = 0b11_10_01_00 → identity: index N maps to shade N.
    assert_eq!(apply_palette(0xE4, 0), 0);
    assert_eq!(apply_palette(0xE4, 1), 1);
    assert_eq!(apply_palette(0xE4, 2), 2);
    assert_eq!(apply_palette(0xE4, 3), 3);
    // Inverse palette.
    let inverted = 0b0001_1011;
    assert_eq!(apply_palette(inverted, 0), 3);
    assert_eq!(apply_palette(inverted, 3), 0);
}

// -- Sprites ---------------------------------------------------------

#[test]
fn sprite_overlays_bg_when_priority_clear() {
    let mut vram = blank_vram();
    // BG tile 0: every pixel = index 1.
    vram[0] = 0xFF;
    vram[1] = 0x00;
    // Sprite tile 1: every pixel = index 3 (low = high = 0xFF).
    vram[16] = 0xFF;
    vram[17] = 0xFF;

    let mut oam = blank_oam();
    // Sprite at screen (0, 0) = OAM (16, 8). Tile 1, attr 0 (no
    // priority, OBP0).
    oam[0] = 16;
    oam[1] = 8;
    oam[2] = 1;
    oam[3] = 0;

    let mut ppu = Ppu::new();
    ppu.lcdc = 0x93; // LCD on, BG on, sprites on
    ppu.bgp = 0xE4;
    ppu.obp0 = 0xE4;

    run_dots(&mut ppu, &vram, &oam, 456);

    let fb = ppu.framebuffer();
    // First 8 pixels: sprite covers them (index 3 → shade 3).
    assert_eq!(fb[0], 3, "sprite pixel overrides BG");
    // Pixel 8: sprite ends, back to BG.
    assert_eq!(fb[8], 1, "BG resumes past the sprite");
}

#[test]
fn sprite_priority_lets_bg_show_when_attr_bit_7_set_and_bg_nonzero() {
    let mut vram = blank_vram();
    vram[0] = 0xFF;
    vram[1] = 0x00; // BG = index 1 everywhere
    vram[16] = 0xFF;
    vram[17] = 0xFF; // sprite = index 3

    let mut oam = blank_oam();
    oam[0] = 16;
    oam[1] = 8;
    oam[2] = 1;
    oam[3] = 0x80; // BG-priority bit set

    let mut ppu = Ppu::new();
    ppu.lcdc = 0x93;
    ppu.bgp = 0xE4;
    ppu.obp0 = 0xE4;

    run_dots(&mut ppu, &vram, &oam, 456);

    let fb = ppu.framebuffer();
    assert_eq!(fb[0], 1, "BG wins because BG index != 0");
}

// -- Window ---------------------------------------------------------

#[test]
fn window_disabled_does_not_advance_window_line() {
    let mut ppu = Ppu::new();
    let vram = blank_vram();
    let oam = blank_oam();
    ppu.lcdc = 0x91; // window disabled
    run_dots(&mut ppu, &vram, &oam, 456 * 50);
    assert_eq!(ppu.window_line, 0);
}

#[test]
fn window_enable_with_wy_zero_advances_window_line() {
    let mut vram = blank_vram();
    // Tile 0 row 0: low=0xFF, high=0x00 — index 1 everywhere.
    vram[0] = 0xFF;
    vram[1] = 0x00;
    let oam = blank_oam();

    let mut ppu = Ppu::new();
    ppu.lcdc = 0xB1; // LCD on, window enable, BG enable
    ppu.bgp = 0xE4;
    ppu.wx = 7; // window starts at x = 0
    ppu.wy = 0;

    run_dots(&mut ppu, &vram, &oam, 456 * 5);
    assert!(ppu.window_line >= 1, "window_line should have advanced");
}
