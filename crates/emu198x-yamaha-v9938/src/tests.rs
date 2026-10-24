//! Unit tests: ports, status, every screen mode, both sprite modes and the
//! command engine, driven through the ports as an MSX2 drives them.

use super::*;

fn vdp() -> V9938 {
    V9938::new(VdpRegion::Ntsc)
}

fn reg(v: &mut V9938, n: u8, value: u8) {
    v.write_control(value);
    v.write_control(0x80 | n);
}

/// Write `bytes` from logical VRAM address `address` through `#98`.
fn vram_write(v: &mut V9938, address: u32, bytes: &[u8]) {
    reg(v, 14, (address >> 14) as u8);
    v.write_control(address as u8);
    v.write_control(((address >> 8) as u8 & 0x3F) | 0x40);
    for &b in bytes {
        v.write_data(b);
    }
}

fn set_palette(v: &mut V9938, index: u8, grb: u16) {
    reg(v, 16, index);
    v.write_palette(((grb >> 4) as u8 & 7) << 4 | (grb as u8 & 7));
    v.write_palette((grb >> 8) as u8 & 7);
}

/// Mode registers: (R#0, R#1) for each mode, display on.
fn set_mode(v: &mut V9938, mode: Mode) {
    let (m3m4m5, m1m2) = match mode {
        Mode::G1 => (0x00, 0x00),
        Mode::G2 => (0x02, 0x00),
        Mode::Mc => (0x00, 0x08),
        Mode::T1 => (0x00, 0x10),
        Mode::T2 => (0x04, 0x10),
        Mode::G3 => (0x04, 0x00),
        Mode::G4 => (0x06, 0x00),
        Mode::G5 => (0x08, 0x00),
        Mode::G6 => (0x0A, 0x00),
        Mode::G7 => (0x0E, 0x00),
        Mode::Undefined => (0x0C, 0x18),
    };
    reg(v, 0, m3m4m5);
    reg(v, 1, 0x40 | m1m2);
    assert_eq!(v.mode(), mode);
}

fn frame(v: &mut V9938) {
    v.run_frame();
    v.run_frame();
}

/// The colour drawn at low-resolution `x`, active line `y`.
fn pixel(v: &V9938, x: usize, y: usize) -> u32 {
    let width = v.framebuffer_width() as usize;
    let row = v.border_top() + y;
    v.framebuffer()[row * width + v.region.border_left() as usize + 2 * x]
}

/// The colour drawn at hi-res column `hx`, active line `y`.
fn hires(v: &V9938, hx: usize, y: usize) -> u32 {
    let width = v.framebuffer_width() as usize;
    let row = v.border_top() + y;
    v.framebuffer()[row * width + v.region.border_left() as usize + hx]
}

fn colour(v: &V9938, index: usize) -> u32 {
    V9938::grb_to_argb(v.palette()[index])
}

fn run_command(v: &mut V9938) {
    for _ in 0..200_000 {
        if !v.command_busy() {
            return;
        }
        v.tick();
    }
    panic!("command never finished");
}

/// Program the command registers, as a BIOS routine would.
#[allow(clippy::too_many_arguments)]
fn command(v: &mut V9938, sx: u16, sy: u16, dx: u16, dy: u16, nx: u16, ny: u16, clr: u8, arg: u8) {
    for (n, value) in [(32, sx), (34, sy), (36, dx), (38, dy), (40, nx), (42, ny)] {
        reg(v, n, value as u8);
        reg(v, n + 1, (value >> 8) as u8);
    }
    reg(v, 44, clr);
    reg(v, 45, arg);
}

// ---------------------------------------------------------------------
// Ports and registers
// ---------------------------------------------------------------------

#[test]
fn registers_are_masked_and_indirect_writes_advance() {
    let mut v = vdp();
    reg(&mut v, 0, 0xFF);
    assert_eq!(v.register(0), 0x7E);
    reg(&mut v, 24, 0xFF);
    assert_eq!(v.register(24), 0, "R#24 does not exist on the V9938");
    // Bits 7 and 6 both set is not a register write.
    v.write_control(0x55);
    v.write_control(0xC2);
    assert_eq!(v.register(2), 0);

    reg(&mut v, 17, 32);
    v.write_indirect(0x12);
    v.write_indirect(0x01);
    assert_eq!((v.register(32), v.register(33)), (0x12, 0x01));
    assert_eq!(v.register(17), 34);
    reg(&mut v, 17, 0x80 | 36);
    v.write_indirect(0x05);
    v.write_indirect(0x06);
    assert_eq!(v.register(36), 0x06, "AII holds the register");
}

#[test]
fn the_palette_takes_two_bytes_and_advances() {
    let mut v = vdp();
    set_palette(&mut v, 3, 0x527);
    set_palette(&mut v, 4, 0x700);
    assert_eq!(v.palette()[3], 0x527);
    assert_eq!(v.palette()[4], 0x700);
    assert_eq!(v.register(16), 5);
    assert_eq!(V9938::grb_to_argb(0x070), 0xFFFF_0000, "red");
    assert_eq!(V9938::grb_to_argb(0x700), 0xFF00_FF00, "green");
}

#[test]
fn vram_access_reads_ahead_and_the_pointer_carries_only_in_v9938_modes() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G4);
    vram_write(&mut v, 0x1_3FFF, &[0xAA, 0xBB]);
    assert_eq!(v.vram()[0x1_3FFF], 0xAA);
    assert_eq!(v.vram()[0x1_4000], 0xBB, "carried into R#14");
    assert_eq!(v.register(14), 5);

    set_mode(&mut v, Mode::G1);
    vram_write(&mut v, 0x3FFF, &[0x11, 0x22]);
    assert_eq!(v.vram()[0x0000], 0x22, "wraps at 16 KB in a TMS9918 mode");

    // Read set-up fetches ahead.
    v.write_control(0xFF);
    v.write_control(0x3F);
    reg(&mut v, 14, 0);
    v.write_control(0xFF);
    v.write_control(0x3F);
    assert_eq!(v.read_data(), 0x11);
    assert_eq!(v.read_data(), 0x22);
}

#[test]
fn planar_modes_interleave_the_two_banks() {
    assert_eq!(V9938::planar(0), 0);
    assert_eq!(V9938::planar(1), 0x1_0000);
    assert_eq!(V9938::planar(2), 1);
    assert_eq!(V9938::planar(0xFFFF), 0x1_7FFF);
    let mut v = vdp();
    set_mode(&mut v, Mode::G7);
    vram_write(&mut v, 0, &[1, 2, 3]);
    assert_eq!(v.vram()[0], 1);
    assert_eq!(v.vram()[0x1_0000], 2);
    assert_eq!(v.vram()[1], 3);
}

// ---------------------------------------------------------------------
// Status and interrupts
// ---------------------------------------------------------------------

#[test]
fn the_frame_interrupt_sets_f_and_clears_on_read() {
    let mut v = vdp();
    reg(&mut v, 1, 0x60);
    while v.line() != 192 {
        v.tick();
    }
    assert!(v.interrupt);
    reg(&mut v, 15, 0);
    assert_eq!(v.read_status() & 0x80, 0x80);
    assert!(!v.interrupt);
    assert_eq!(v.read_status() & 0x80, 0);
}

#[test]
fn the_line_interrupt_fires_on_the_scrolled_line() {
    let mut v = vdp();
    reg(&mut v, 0, 0x10);
    reg(&mut v, 19, 50);
    reg(&mut v, 23, 20);
    while !v.interrupt {
        v.tick();
    }
    assert_eq!(v.line(), 30, "line 30 scrolled by 20 is 50");
    assert_eq!(v.dot(), 257);
    reg(&mut v, 15, 1);
    assert_eq!(v.read_status() & 1, 1);
    assert!(!v.interrupt);
}

#[test]
fn status_2_reports_blanking_and_s_1_has_no_id() {
    let mut v = vdp();
    assert_eq!(v.peek_status(1) & 0x3E, 0, "the V9938's ID is 0");
    assert_eq!(v.peek_status(2) & 0x60, 0, "drawing the first pixel");
    while v.dot() < 300 {
        v.tick();
    }
    assert_eq!(v.peek_status(2) & 0x20, 0x20, "HR in the border");
    while v.line() < 200 {
        v.tick();
    }
    assert_eq!(v.peek_status(2) & 0x40, 0x40, "VR below the picture");
    assert_eq!(v.peek_status(10), 0xFF);
    let eo = v.peek_status(2) & 2;
    frame(&mut v);
    assert_eq!(v.peek_status(2) & 2, eo, "EO toggles each frame");
}

#[test]
fn pal_frames_are_313_lines_and_212_line_mode_shows_more() {
    let mut v = V9938::new(VdpRegion::Pal);
    assert_eq!(v.lines_per_frame(), 313);
    assert_eq!(v.framebuffer_height(), 288);
    reg(&mut v, 9, 0x80 | 0x02);
    assert_eq!(v.active_lines(), 212);
    let mut lines = 0;
    while !v.tick() {
        if v.dot() == 0 {
            lines += 1;
        }
    }
    assert_eq!(lines + 1, 313);
}

// ---------------------------------------------------------------------
// Screen modes
// ---------------------------------------------------------------------

#[test]
fn graphic_1_draws_tiles_with_shared_colours() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G1);
    reg(&mut v, 2, 0x06); // names at 0x1800
    reg(&mut v, 3, 0x80); // colours at 0x2000
    reg(&mut v, 4, 0x00); // patterns at 0
    reg(&mut v, 7, 0x01);
    vram_write(&mut v, 8, &[0xF0; 8]); // tile 1
    vram_write(&mut v, 0x1800, &[1]);
    vram_write(&mut v, 0x2000, &[0x6C]);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 6));
    assert_eq!(pixel(&v, 4, 0), colour(&v, 12));
}

#[test]
fn graphic_2_and_3_take_a_colour_per_tile_row() {
    for mode in [Mode::G2, Mode::G3] {
        let mut v = vdp();
        set_mode(&mut v, mode);
        reg(&mut v, 2, 0x06);
        reg(&mut v, 3, 0xFF); // colours at 0x2000, full mask
        reg(&mut v, 4, 0x03); // patterns at 0, full mask
        reg(&mut v, 8, 0x02); // sprites off
        vram_write(&mut v, 0x1800 + 32 * 8, &[2]); // third 1, tile 2
        vram_write(&mut v, 0x0800 + 2 * 8, &[0x80, 0x00]);
        vram_write(&mut v, 0x2800 + 2 * 8, &[0x4A, 0x5B]);
        frame(&mut v);
        assert_eq!(pixel(&v, 0, 64), colour(&v, 4));
        assert_eq!(pixel(&v, 1, 64), colour(&v, 10));
        assert_eq!(pixel(&v, 0, 65), colour(&v, 11));
    }
}

#[test]
fn multicolour_draws_four_by_four_blocks() {
    let mut v = vdp();
    set_mode(&mut v, Mode::Mc);
    reg(&mut v, 2, 0x06);
    reg(&mut v, 4, 0x00);
    vram_write(&mut v, 0x1800, &[3]);
    vram_write(&mut v, 3 * 8, &[0x9D, 0x27]);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 9));
    assert_eq!(pixel(&v, 4, 3), colour(&v, 13));
    assert_eq!(pixel(&v, 0, 4), colour(&v, 2));
}

#[test]
fn text_1_has_forty_columns_and_a_border() {
    let mut v = vdp();
    set_mode(&mut v, Mode::T1);
    reg(&mut v, 2, 0x00);
    reg(&mut v, 4, 0x01); // patterns at 0x800
    reg(&mut v, 7, 0xF4);
    vram_write(&mut v, 0x0001, &[5]); // column 1
    vram_write(&mut v, 0x0800 + 5 * 8, &[0xFC]);
    frame(&mut v);
    assert_eq!(pixel(&v, 7, 0), colour(&v, 4), "left padding");
    assert_eq!(pixel(&v, 14, 0), colour(&v, 15), "column 1, first pixel");
    assert_eq!(pixel(&v, 13, 0), colour(&v, 4), "column 0 is blank");
}

#[test]
fn text_2_has_eighty_columns_and_blinks() {
    let mut v = vdp();
    set_mode(&mut v, Mode::T2);
    reg(&mut v, 2, 0x03); // names at 0
    reg(&mut v, 3, 0x27); // blink table at 0x800
    reg(&mut v, 10, 0x00);
    reg(&mut v, 4, 0x02); // patterns at 0x1000
    reg(&mut v, 7, 0xF4);
    reg(&mut v, 12, 0x1A);
    vram_write(&mut v, 0x0001, &[7]); // column 1
    vram_write(&mut v, 0x1000 + 7 * 8, &[0x80]);
    vram_write(&mut v, 0x0800, &[0x40]); // column 1 blinks
    reg(&mut v, 13, 0x10); // on, never off
    frame(&mut v);
    // Column 1 starts at hi-res 16 + 6.
    assert_eq!(hires(&v, 22, 0), colour(&v, 1), "blink foreground");
    assert_eq!(hires(&v, 23, 0), colour(&v, 10), "blink background");
    reg(&mut v, 13, 0x00);
    frame(&mut v);
    let mut v2 = vdp();
    set_mode(&mut v2, Mode::T2);
    assert_eq!(hires(&v2, 0, 0), hires(&v2, 1, 0));
}

#[test]
fn graphic_4_is_a_nibble_bitmap_and_scrolls() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G4);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 9, 0x80);
    reg(&mut v, 8, 0x02);
    vram_write(&mut v, 10 * 128 + 2, &[0x3E]);
    frame(&mut v);
    assert_eq!(pixel(&v, 4, 10), colour(&v, 3));
    assert_eq!(pixel(&v, 5, 10), colour(&v, 14));
    // Page 1 at 0x8000.
    reg(&mut v, 2, 0x3F);
    vram_write(&mut v, 0x8000, &[0x90]);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 9));
    // Scroll up by 10: line 0 shows VRAM line 10 of page 0.
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 23, 10);
    frame(&mut v);
    assert_eq!(pixel(&v, 4, 0), colour(&v, 3));
    // 212 lines drawn.
    vram_write(&mut v, (211 + 10) * 128, &[0x70]);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 211), colour(&v, 7));
}

#[test]
fn colour_0_is_the_backdrop_unless_tp_is_set() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G4);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 7, 0x05);
    reg(&mut v, 8, 0x02);
    set_palette(&mut v, 0, 0x777);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 5));
    reg(&mut v, 8, 0x22);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 0));
}

#[test]
fn graphic_5_is_four_colours_across_512() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G5);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 8, 0x22);
    vram_write(&mut v, 0, &[0b00_01_10_11]);
    frame(&mut v);
    assert_eq!(hires(&v, 0, 0), colour(&v, 0));
    assert_eq!(hires(&v, 1, 0), colour(&v, 1));
    assert_eq!(hires(&v, 2, 0), colour(&v, 2));
    assert_eq!(hires(&v, 3, 0), colour(&v, 3));
}

#[test]
fn graphic_6_is_planar_nibbles_across_512() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G6);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 8, 0x02);
    vram_write(&mut v, 256 + 1, &[0x9A]); // line 1, hi-res 2 and 3
    frame(&mut v);
    assert_eq!(hires(&v, 2, 1), colour(&v, 9));
    assert_eq!(hires(&v, 3, 1), colour(&v, 10));
}

#[test]
// The literals group GGG RRR BB, the colour byte's fields.
#[allow(clippy::unusual_byte_groupings)]
fn graphic_7_colours_are_direct() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G7);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 8, 0x22);
    vram_write(&mut v, 5, &[0b111_000_00, 0b000_111_00, 0b000_000_11]);
    frame(&mut v);
    assert_eq!(pixel(&v, 5, 0), 0xFF00_FF00);
    assert_eq!(pixel(&v, 6, 0), 0xFFFF_0000);
    assert_eq!(pixel(&v, 7, 0), 0xFF00_00FF);
    // The backdrop is R#7 as a colour byte.
    reg(&mut v, 7, 0b000_111_00);
    reg(&mut v, 8, 0x02);
    frame(&mut v);
    assert_eq!(pixel(&v, 100, 100), 0xFFFF_0000);
}

#[test]
fn display_off_and_undefined_modes_show_the_backdrop() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G4);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 7, 0x09);
    vram_write(&mut v, 0, &[0x33]);
    reg(&mut v, 1, 0x00);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 9));
    set_mode(&mut v, Mode::Undefined);
    frame(&mut v);
    assert_eq!(pixel(&v, 0, 0), colour(&v, 9));
}

#[test]
fn a_mid_line_backdrop_change_splits_the_line() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G4);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 8, 0x02);
    reg(&mut v, 7, 0x02);
    frame(&mut v);
    while !(v.line() == 50 && v.dot() == 100) {
        v.tick();
    }
    reg(&mut v, 7, 0x0D);
    while v.line() != 51 {
        v.tick();
    }
    assert_eq!(pixel(&v, 50, 50), colour(&v, 2));
    assert_eq!(pixel(&v, 150, 50), colour(&v, 13));
}

// ---------------------------------------------------------------------
// Sprites
// ---------------------------------------------------------------------

/// Sprite mode 1 on G1: attributes at 0x1B00, patterns at 0x3800.
fn sprite_mode_1() -> V9938 {
    let mut v = vdp();
    set_mode(&mut v, Mode::G1);
    reg(&mut v, 5, 0x36);
    reg(&mut v, 6, 0x07);
    vram_write(&mut v, 0x3800, &[0xFF; 8]);
    v
}

#[test]
fn mode_1_allows_four_sprites_a_line_and_flags_the_fifth() {
    let mut v = sprite_mode_1();
    let mut attributes = Vec::new();
    for i in 0..5u8 {
        attributes.extend_from_slice(&[9, i * 20, 0, 8]);
    }
    attributes.push(208);
    vram_write(&mut v, 0x1B00, &attributes);
    frame(&mut v);
    assert_eq!(pixel(&v, 60, 10), colour(&v, 8));
    assert_ne!(pixel(&v, 80, 10), colour(&v, 8), "the fifth is not drawn");
    reg(&mut v, 15, 0);
    let s0 = v.read_status();
    assert_eq!(s0 & 0x40, 0x40);
    assert_eq!(s0 & 0x1F, 4);
}

#[test]
fn mode_1_collisions_set_c() {
    let mut v = sprite_mode_1();
    vram_write(&mut v, 0x1B00, &[9, 10, 0, 8, 9, 14, 0, 9, 208]);
    frame(&mut v);
    assert_eq!(pixel(&v, 14, 10), colour(&v, 8), "earlier sprite in front");
    assert_eq!(v.peek_status(0) & 0x20, 0x20);
}

/// Sprite mode 2 on G4: attributes at 0x7600, colours at 0x7400.
fn sprite_mode_2() -> V9938 {
    let mut v = vdp();
    set_mode(&mut v, Mode::G4);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 5, 0xEF);
    reg(&mut v, 11, 0x00);
    reg(&mut v, 6, 0x0F);
    vram_write(&mut v, 0x7800, &[0xFF; 8]);
    v
}

#[test]
fn mode_2_allows_eight_a_line_with_a_colour_per_line() {
    let mut v = sprite_mode_2();
    let mut attributes = Vec::new();
    for i in 0..9u8 {
        attributes.extend_from_slice(&[19, i * 20, 0, 0]);
    }
    attributes.push(216);
    vram_write(&mut v, 0x7600, &attributes);
    let mut colours = Vec::new();
    for _ in 0..9 {
        let mut c = [3u8; 16];
        c[1] = 6;
        colours.extend_from_slice(&c);
    }
    vram_write(&mut v, 0x7400, &colours);
    frame(&mut v);
    assert_eq!(pixel(&v, 140, 20), colour(&v, 3));
    assert_eq!(pixel(&v, 140, 21), colour(&v, 6), "row 1's colour");
    assert_ne!(pixel(&v, 160, 20), colour(&v, 3), "the ninth is not drawn");
    assert_eq!(v.peek_status(0) & 0x5F, 0x40 | 8);
}

#[test]
fn mode_2_cc_combines_and_ec_shifts() {
    let mut v = sprite_mode_2();
    vram_write(
        &mut v,
        0x7600,
        &[19, 40, 0, 0, 19, 44, 0, 0, 19, 100, 0, 0, 216],
    );
    let mut colours = vec![0x01u8; 16];
    colours.extend_from_slice(&[0x42; 16]); // CC, colour 2
    colours.extend_from_slice(&[0x85; 16]); // EC, colour 5
    vram_write(&mut v, 0x7400, &colours);
    frame(&mut v);
    assert_eq!(pixel(&v, 40, 20), colour(&v, 1));
    assert_eq!(pixel(&v, 44, 20), colour(&v, 3), "1 | 2 where they overlap");
    assert_ne!(pixel(&v, 50, 20), colour(&v, 2), "CC alone is not shown");
    assert_eq!(pixel(&v, 68, 20), colour(&v, 5), "shifted 32 left");
    assert_eq!(v.peek_status(0) & 0x20, 0, "CC does not collide");
}

#[test]
fn mode_2_collision_reports_coordinates_and_spd_disables() {
    let mut v = sprite_mode_2();
    vram_write(&mut v, 0x7600, &[19, 40, 0, 0, 19, 42, 0, 0, 216]);
    vram_write(&mut v, 0x7400, &[0x01; 32]);
    frame(&mut v);
    reg(&mut v, 15, 3);
    assert_eq!(v.read_status(), 42 + 12);
    reg(&mut v, 15, 5);
    assert_eq!(v.read_status(), 20 + 8);
    reg(&mut v, 8, 0x02);
    frame(&mut v);
    assert_eq!(pixel(&v, 40, 20), v.backdrop_colour_for_test());
}

#[test]
fn graphic_7_sprites_use_their_fixed_palette() {
    let mut v = vdp();
    set_mode(&mut v, Mode::G7);
    reg(&mut v, 2, 0x1F);
    reg(&mut v, 5, 0xF7); // attributes at 0xFA00
    reg(&mut v, 11, 0x01);
    reg(&mut v, 6, 0x1E); // patterns at 0xF000
    vram_write(&mut v, 0xFA00, &[19, 40, 0, 0, 216]);
    vram_write(&mut v, 0xF800, &[0x0C; 16]);
    vram_write(&mut v, 0xF000, &[0xFF; 8]);
    frame(&mut v);
    assert_eq!(pixel(&v, 40, 20), V9938::grb_to_argb(0x700));
}

impl V9938 {
    fn backdrop_colour_for_test(&self) -> u32 {
        Self::grb_to_argb(self.palette[usize::from(self.regs[7] & 0x0F)])
    }
}

// ---------------------------------------------------------------------
// Command engine
// ---------------------------------------------------------------------

fn bitmap(mode: Mode) -> V9938 {
    let mut v = vdp();
    set_mode(&mut v, mode);
    reg(&mut v, 2, 0x1F);
    v
}

#[test]
fn hmmv_fills_bytes_and_writes_back_its_position() {
    let mut v = bitmap(Mode::G4);
    command(&mut v, 0, 0, 10, 5, 20, 3, 0x37, 0);
    reg(&mut v, 46, 0xC0);
    assert!(v.command_busy());
    assert_eq!(v.peek_status(2) & 1, 1, "CE");
    run_command(&mut v);
    for y in 5..8 {
        for x in (10..30).step_by(2) {
            assert_eq!(v.vram()[y * 128 + x / 2], 0x37);
        }
    }
    assert_eq!(v.vram()[5 * 128 + 4], 0);
    assert_eq!(v.vram()[8 * 128 + 5], 0);
    assert_eq!(v.register(38), 8, "DY after the last line");
    assert_eq!(v.register(46) & 0xF0, 0);
}

#[test]
fn lmmv_applies_the_logical_operation() {
    let mut v = bitmap(Mode::G4);
    vram_write(&mut v, 0, &[0x81]);
    command(&mut v, 0, 0, 0, 0, 2, 1, 0x06, 0);
    reg(&mut v, 46, 0x82); // LMMV OR
    run_command(&mut v);
    assert_eq!(v.vram()[0], 0xE7);
    command(&mut v, 0, 0, 0, 0, 2, 1, 0x00, 0);
    reg(&mut v, 46, 0x88); // LMMV TIMP with colour 0: nothing
    run_command(&mut v);
    assert_eq!(v.vram()[0], 0xE7);
    command(&mut v, 0, 0, 0, 0, 1, 1, 0x05, 0);
    reg(&mut v, 46, 0x84); // NOT
    run_command(&mut v);
    assert_eq!(v.vram()[0] >> 4, 0x0A);
}

#[test]
fn lmmm_and_hmmm_copy_rectangles_leftward() {
    let mut v = bitmap(Mode::G4);
    vram_write(&mut v, 0, &[0x12, 0x34]);
    command(&mut v, 3, 0, 103, 50, 4, 1, 0, 0x04); // DIX left
    reg(&mut v, 46, 0x90);
    run_command(&mut v);
    assert_eq!(&v.vram()[50 * 128 + 50..50 * 128 + 52], &[0x12, 0x34]);

    command(&mut v, 0, 0, 0, 100, 4, 1, 0, 0);
    reg(&mut v, 46, 0xD0);
    run_command(&mut v);
    assert_eq!(&v.vram()[100 * 128..100 * 128 + 2], &[0x12, 0x34]);
}

#[test]
fn ymmm_copies_rows_to_the_edge() {
    let mut v = bitmap(Mode::G4);
    vram_write(&mut v, 10 * 128 + 120, &[0xAB; 8]);
    command(&mut v, 0, 10, 240, 20, 0, 2, 0, 0);
    reg(&mut v, 46, 0xE0);
    run_command(&mut v);
    assert_eq!(v.vram()[20 * 128 + 127], 0xAB);
    assert_eq!(v.vram()[20 * 128 + 119], 0);
}

#[test]
fn line_draws_from_the_long_side() {
    let mut v = bitmap(Mode::G7);
    command(&mut v, 0, 0, 10, 10, 8, 4, 0xFF, 0);
    reg(&mut v, 46, 0x70);
    run_command(&mut v);
    fn at(v: &V9938, x: u16, y: u16) -> u8 {
        v.vram()[V9938::planar(u32::from(y) * 256 + u32::from(x))]
    }
    assert_eq!(at(&v, 10, 10), 0xFF);
    assert_eq!(at(&v, 18, 14), 0xFF);
    let lit = (0..30)
        .flat_map(|x| (0..30).map(move |y| (x, y)))
        .filter(|&(x, y)| at(&v, x, y) == 0xFF)
        .count();
    assert_eq!(lit, 9);
    // Y major.
    command(&mut v, 0, 0, 50, 0, 6, 0, 0x11, 0x01);
    reg(&mut v, 46, 0x70);
    run_command(&mut v);
    assert_eq!(at(&v, 50, 6), 0x11);
}

#[test]
fn pset_point_and_srch() {
    let mut v = bitmap(Mode::G4);
    command(&mut v, 0, 0, 77, 9, 0, 0, 0x0B, 0);
    reg(&mut v, 46, 0x50);
    run_command(&mut v);
    command(&mut v, 77, 9, 0, 0, 0, 0, 0, 0);
    reg(&mut v, 46, 0x40);
    run_command(&mut v);
    reg(&mut v, 15, 7);
    assert_eq!(v.read_status(), 0x0B);

    command(&mut v, 10, 9, 0, 0, 0, 0, 0x0B, 0);
    reg(&mut v, 46, 0x60);
    run_command(&mut v);
    assert_eq!(v.peek_status(2) & 0x10, 0x10);
    assert_eq!(v.peek_status(8), 77);
    // Searching left for anything other than 0 from x=50 reaches the edge.
    command(&mut v, 50, 9, 0, 0, 0, 0, 0x00, 0x06);
    reg(&mut v, 46, 0x60);
    run_command(&mut v);
    assert_eq!(v.peek_status(2) & 0x10, 0);
}

#[test]
fn hmmc_and_lmmc_take_data_through_r44_and_lmcm_returns_it() {
    let mut v = bitmap(Mode::G4);
    command(&mut v, 0, 0, 0, 30, 4, 1, 0x12, 0);
    reg(&mut v, 46, 0xF0);
    for byte in [0x34] {
        while v.peek_status(2) & 0x80 == 0 {
            v.tick();
        }
        reg(&mut v, 44, byte);
    }
    run_command(&mut v);
    assert_eq!(&v.vram()[30 * 128..30 * 128 + 2], &[0x12, 0x34]);

    command(&mut v, 0, 0, 0, 31, 2, 1, 0x07, 0);
    reg(&mut v, 46, 0xB0);
    while v.peek_status(2) & 0x80 == 0 {
        v.tick();
    }
    reg(&mut v, 44, 0x09);
    run_command(&mut v);
    assert_eq!(v.vram()[31 * 128], 0x79);

    command(&mut v, 0, 31, 0, 0, 2, 1, 0, 0);
    reg(&mut v, 46, 0xA0);
    reg(&mut v, 15, 7);
    let mut got = Vec::new();
    while got.len() < 2 {
        v.tick();
        if v.peek_status(2) & 0x80 != 0 {
            got.push(v.read_status());
        }
    }
    assert_eq!(got, vec![0x07, 0x09]);
}

#[test]
fn commands_take_time_and_stop_aborts() {
    let mut v = bitmap(Mode::G4);
    command(&mut v, 0, 0, 0, 0, 256, 100, 0x55, 0);
    reg(&mut v, 46, 0xC0);
    let mut dots = 0u32;
    while v.command_busy() {
        v.tick();
        dots += 1;
    }
    // 12800 bytes at 46 master clocks, plus 100 line ends at 58.
    let expected = (112 + 12_800 * 46 + 100 * 58) / 4;
    // Within 0.1%: the last unit's leftover budget and the line-end charge
    // after the final line are rounding, not timing.
    assert!(
        dots.abs_diff(expected) * 1000 < expected,
        "{dots} dots, expected {expected}"
    );

    command(&mut v, 0, 0, 0, 0, 256, 100, 0x00, 0);
    reg(&mut v, 46, 0xC0);
    for _ in 0..100 {
        v.tick();
    }
    reg(&mut v, 46, 0x00);
    assert!(!v.command_busy());
    assert_eq!(v.vram()[99 * 128], 0x55, "stopped before the end");
}

#[test]
fn graphic_5_and_6_commands_address_their_pixels() {
    let mut v = bitmap(Mode::G5);
    command(&mut v, 0, 0, 5, 0, 1, 1, 0x03, 0);
    reg(&mut v, 46, 0x50);
    run_command(&mut v);
    assert_eq!(v.vram()[1], 0b00_11_00_00);

    let mut v = bitmap(Mode::G6);
    command(&mut v, 0, 0, 3, 2, 1, 1, 0x0C, 0);
    reg(&mut v, 46, 0x50);
    run_command(&mut v);
    assert_eq!(v.vram()[V9938::planar(2 * 256 + 1)], 0x0C);
}
