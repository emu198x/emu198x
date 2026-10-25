//! LSPC2 tests: registers, interrupt timing, and line-by-line drawing from
//! synthetic graphics ROMs.

#![allow(clippy::unwrap_used)]

use super::*;

const FRAME: u32 = MASTER_PER_LINE * VTOTAL;

fn run(chip: &mut Lspc2, roms: &VideoRoms, clocks: u32) {
    for _ in 0..clocks {
        chip.tick(roms);
    }
}

/// Run to the start of `line`, clock 0.
fn run_to_line(chip: &mut Lspc2, roms: &VideoRoms, line: u32) {
    while chip.line() != line || chip.line_clock != 0 {
        chip.tick(roms);
    }
}

fn vram_write(chip: &mut Lspc2, address: u16, data: u16) {
    chip.write_register(0, address);
    chip.write_register(1, data);
}

/// Set pixel `(x, y)` of sprite tile `code` in an interleaved C-ROM image.
fn set_sprite_pixel(rom: &mut [u8], code: u32, x: u32, y: u32, value: u8) {
    let address = (code << 8) | (y << 4) | x;
    let offset =
        ((address & !0xff) >> 1) | (((address & 0x8) ^ 0x8) << 3) | ((address & 0xf0) >> 2);
    let bit = address & 7;
    for (plane, byte) in [0, 2, 1, 3].into_iter().enumerate() {
        let target = &mut rom[(offset + byte) as usize];
        *target &= !(1 << bit);
        *target |= ((value >> plane) & 1) << bit;
    }
}

/// A C-ROM whose tile 1 is solid colour 1 with a colour 2 column on its
/// left edge, and whose tile 2 has row `r` in colour `r` (1-15, row 0 blank).
fn sprite_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x400];
    for y in 0..16 {
        for x in 0..16 {
            set_sprite_pixel(&mut rom, 1, x, y, if x == 0 { 2 } else { 1 });
            set_sprite_pixel(&mut rom, 2, x, y, y as u8);
        }
    }
    rom
}

/// An S-ROM whose tile 1 has pixel `x` of every line in colour `x + 1`.
fn fix_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x40];
    for line in 0..8 {
        for (pair, offset) in [0x10, 0x18, 0x00, 0x08].into_iter().enumerate() {
            let left = (pair * 2 + 1) as u8;
            rom[0x20 + offset + line] = left | ((left + 1) << 4);
        }
    }
    rom
}

/// Palette 1: colour n is a distinct opaque value per n.
fn set_palette(chip: &mut Lspc2) {
    chip.write_palette(0xfff, 0x0000);
    for n in 1..16 {
        chip.write_palette(0x10 + n, n * 0x0111);
    }
}

fn pen(chip: &Lspc2, n: u16) -> u32 {
    palette::argb(chip.read_palette(0x10 + n), chip.shadow)
}

fn pixel(chip: &Lspc2, x: usize, y: u32) -> u32 {
    chip.framebuffer[(y - VBEND) as usize * WIDTH + x]
}

/// Place sprite `n` at screen `x`, `line`, `rows` tiles tall, full size,
/// every tile code `code` with palette 1.
fn place_sprite(chip: &mut Lspc2, n: u16, x: u32, line: u32, rows: u16, code: u16) {
    for tile in 0..rows.min(32) {
        vram_write(chip, (n << 6) | (tile << 1), code);
        vram_write(chip, (n << 6) | (tile << 1) | 1, 0x0100);
    }
    vram_write(chip, 0x8000 | n, 0x0fff);
    let y = (0x200 - line) & 0x1ff;
    vram_write(chip, 0x8200 | n, ((y as u16) << 7) | rows);
    vram_write(chip, 0x8400 | n, (x as u16) << 7);
}

#[test]
fn palette_levels_follow_the_resistor_network() {
    assert_eq!(palette::argb(0x7fff, false), 0xffff_ffff);
    assert_eq!(palette::argb(0x0000, false), 0xff00_0000);
    // Red only, high bits: bits 11-8 and the shared low bit 14.
    assert_eq!(palette::argb(0x4f00, false), 0xffff_0000);
    let dark = palette::argb(0xffff, false);
    let shadow = palette::argb(0x7fff, true);
    let both = palette::argb(0xffff, true);
    let level = |c: u32| (c >> 16) & 0xff;
    assert!(level(dark) < 255 && level(dark) > 240, "dark {dark:08x}");
    assert!(level(shadow) < level(dark), "shadow {shadow:08x}");
    assert!(level(both) < level(shadow));
    for table in palette::LEVELS {
        assert!(table.windows(2).all(|w| w[0] <= w[1]));
    }
}

#[test]
fn palette_banks_are_separate() {
    let mut chip = Lspc2::new();
    chip.write_palette(5, 0x1234);
    chip.palette_bank = true;
    assert_eq!(chip.read_palette(5), 0);
    chip.write_palette(5, 0x4321);
    chip.palette_bank = false;
    assert_eq!(chip.read_palette(5), 0x1234);
    assert_eq!(chip.palette()[0x1005], 0x4321);
}

#[test]
fn vram_steps_by_the_modulo_and_keeps_bit_15() {
    let mut chip = Lspc2::new();
    chip.write_register(2, 0x20);
    chip.write_register(0, 0x7000);
    chip.write_register(1, 0xaaaa);
    chip.write_register(1, 0xbbbb);
    assert_eq!(chip.vram()[0x7000], 0xaaaa);
    assert_eq!(chip.vram()[0x7020], 0xbbbb);
    assert_eq!(chip.read_register(2), 0x20);

    chip.write_register(0, 0x7000);
    assert_eq!(chip.read_register(0), 0xaaaa, "the read is made at once");
    assert_eq!(chip.read_register(1), 0xaaaa);

    // Fast VRAM mirrors every 2K words and the step never leaves it.
    chip.write_register(2, 1);
    chip.write_register(0, 0x8fff);
    chip.write_register(1, 0x1111);
    assert_eq!(chip.vram()[0x87ff], 0x1111);
    chip.write_register(1, 0x2222);
    assert_eq!(chip.vram()[0x8000], 0x2222);

    // Slow VRAM wraps within 32K.
    chip.write_register(0, 0x7fff);
    chip.write_register(1, 0x3333);
    chip.write_register(1, 0x4444);
    assert_eq!(chip.vram()[0x7fff], 0x3333);
    assert_eq!(chip.vram()[0x0000], 0x4444);
}

#[test]
fn the_line_counter_runs_f8_to_1ff() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    assert_eq!(chip.read_register(3) >> 7, 0x100);
    run_to_line(&mut chip, &roms, 0xff);
    assert_eq!(chip.read_register(3) >> 7, 0x1ff);
    run_to_line(&mut chip, &roms, 0x100);
    assert_eq!(chip.read_register(3) >> 7, 0xf8);
    run_to_line(&mut chip, &roms, 0x107);
    assert_eq!(chip.read_register(3) >> 7, 0xff);
    run_to_line(&mut chip, &roms, 0);
    assert_eq!(chip.read_register(3) >> 7, 0x100);
    assert_eq!(chip.pixel(), 0);
    run(&mut chip, &roms, 40);
    assert_eq!(chip.pixel(), 10);
}

#[test]
fn reset_interrupt_is_pending_from_power_on() {
    let mut chip = Lspc2::new();
    assert_eq!(chip.irq_level(), 3);
    chip.write_register(6, 1);
    assert_eq!(chip.irq_level(), 0);
    chip.reset();
    assert_eq!(chip.irq_level(), 3);
}

#[test]
fn vblank_interrupt_rises_58_clocks_into_line_f0() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    chip.write_register(6, 1);
    run_to_line(&mut chip, &roms, VBSTART);
    run(&mut chip, &roms, 58);
    assert!(!chip.vblank_pending);
    run(&mut chip, &roms, 1);
    assert!(chip.vblank_pending);
    assert_eq!(chip.irq_level(), 1);
    chip.write_register(6, 4);
    assert_eq!(chip.irq_level(), 0);
    // Once a frame.
    run(&mut chip, &roms, FRAME - 1);
    assert!(!chip.vblank_pending);
    run(&mut chip, &roms, 1);
    assert!(chip.vblank_pending);
}

#[test]
fn display_position_timer_counts_pixels_and_repeats() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    chip.write_register(6, 7);
    // Enable, load on write, repeat; fire every 100 pixels.
    chip.write_register(3, 0x00b0);
    chip.write_register(4, 0);
    chip.write_register(5, 99);
    run(&mut chip, &roms, 399);
    assert!(!chip.raster_pending);
    run(&mut chip, &roms, 1);
    assert!(chip.raster_pending);
    assert_eq!(chip.irq_level(), 2);
    chip.write_register(6, 2);
    run(&mut chip, &roms, 399);
    assert!(!chip.raster_pending);
    run(&mut chip, &roms, 1);
    assert!(chip.raster_pending, "the timer reloads itself");

    // Without the enable bit it still counts but raises nothing.
    chip.write_register(6, 2);
    chip.write_register(3, 0x0020);
    chip.write_register(5, 9);
    run(&mut chip, &roms, 100);
    assert!(!chip.raster_pending);
}

#[test]
fn display_position_timer_reloads_at_vblank() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    chip.write_register(6, 7);
    chip.write_register(4, 0);
    chip.write_register(5, 0);
    chip.write_register(3, 0x0050);
    run_to_line(&mut chip, &roms, VBSTART);
    run(&mut chip, &roms, VBLANK_RELOAD_CLOCK + 3);
    assert!(!chip.raster_pending);
    run(&mut chip, &roms, 1);
    assert!(chip.raster_pending);
}

#[test]
fn auto_animation_steps_at_vertical_sync() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    chip.write_register(3, 0x0100); // a step every second frame
    run(&mut chip, &roms, FRAME * 4);
    assert_eq!(chip.read_register(3) & 7, 2);
    chip.write_register(3, 0x0000);
    run(&mut chip, &roms, FRAME * 3);
    assert_eq!(chip.read_register(3) & 7, 5);
}

#[test]
fn a_frame_completes_after_the_last_visible_line() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    run_to_line(&mut chip, &roms, VBSTART);
    assert!(!chip.take_frame_complete());
    run(&mut chip, &roms, 1);
    assert!(chip.take_frame_complete());
    assert!(!chip.take_frame_complete());
}

#[test]
fn the_background_is_the_last_colour_of_the_bank() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    chip.write_palette(0xfff, 0x7f00);
    run(&mut chip, &roms, FRAME);
    assert!(
        chip.framebuffer
            .iter()
            .all(|&p| p == palette::argb(0x7f00, false))
    );
    chip.shadow = true;
    run(&mut chip, &roms, FRAME);
    assert_eq!(chip.framebuffer[0], palette::argb(0x7f00, true));
}

#[test]
fn the_fix_layer_draws_8_by_8_tiles_with_pixel_0_clear() {
    let mut chip = Lspc2::new();
    let fix = fix_rom();
    let roms = VideoRoms {
        fix: &fix,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    // Column 3, row 4 (lines 32-39, screen lines 16-23): tile 1, palette 1.
    vram_write(&mut chip, 0x7000 | (3 << 5) | 4, 0x1001);
    run(&mut chip, &roms, FRAME);
    for x in 0..8 {
        assert_eq!(pixel(&chip, 24 + x, 32), pen(&chip, x as u16 + 1), "x {x}");
        assert_eq!(pixel(&chip, 24 + x, 39), pen(&chip, x as u16 + 1));
    }
    assert_eq!(pixel(&chip, 23, 32), chip.colour(0xfff));
    assert_eq!(pixel(&chip, 24, 31), chip.colour(0xfff));
    assert_eq!(pixel(&chip, 24, 40), chip.colour(0xfff));
}

#[test]
fn a_sprite_draws_its_tiles_down_the_strip() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 100, 50, 2, 2);
    run(&mut chip, &roms, FRAME);
    // Two copies of tile 2 stacked: row r is colour r, row 0 clear.
    for r in 1..16 {
        assert_eq!(pixel(&chip, 100, 50 + r), pen(&chip, r as u16), "row {r}");
        assert_eq!(pixel(&chip, 115, 66 + r), pen(&chip, r as u16));
    }
    assert_eq!(pixel(&chip, 100, 50), chip.colour(0xfff));
    assert_eq!(pixel(&chip, 116, 60), chip.colour(0xfff));
    assert_eq!(pixel(&chip, 100, 82), chip.colour(0xfff), "two tiles tall");
}

#[test]
fn flip_bits_mirror_a_tile() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 100, 50, 1, 1);
    place_sprite(&mut chip, 2, 140, 50, 1, 1);
    vram_write(&mut chip, (2 << 6) | 1, 0x0101); // horizontal flip
    place_sprite(&mut chip, 3, 180, 50, 1, 2);
    vram_write(&mut chip, (3 << 6) | 1, 0x0102); // vertical flip
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 100, 55), pen(&chip, 2));
    assert_eq!(pixel(&chip, 115, 55), pen(&chip, 1));
    assert_eq!(pixel(&chip, 140, 55), pen(&chip, 1));
    assert_eq!(pixel(&chip, 155, 55), pen(&chip, 2));
    assert_eq!(pixel(&chip, 180, 50), pen(&chip, 15));
    assert_eq!(pixel(&chip, 180, 64), pen(&chip, 1));
    assert_eq!(pixel(&chip, 180, 65), chip.colour(0xfff));
}

#[test]
fn chained_sprites_follow_to_the_right_at_their_own_width() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 60, 50, 1, 1);
    // Sprite 2 chains: its own Y and X are ignored.
    vram_write(&mut chip, 2 << 6, 1);
    vram_write(&mut chip, (2 << 6) | 1, 0x0100);
    vram_write(&mut chip, 0x8002, 0x07ff); // 8 pixels wide
    vram_write(&mut chip, 0x8202, 0x0040);
    vram_write(&mut chip, 0x8402, 0xffff);
    place_sprite(&mut chip, 3, 0, 0, 0, 0);
    vram_write(&mut chip, 0x8203, 0x0040); // chains on, shrink 15
    vram_write(&mut chip, 3 << 6, 1);
    vram_write(&mut chip, (3 << 6) | 1, 0x0100);
    run(&mut chip, &roms, FRAME);
    // Sprite 1 at 60-75, sprite 2 at 76-83 (8 pixels), sprite 3 from 84.
    assert_eq!(pixel(&chip, 76, 55), pen(&chip, 2));
    assert_eq!(pixel(&chip, 83, 55), pen(&chip, 1));
    assert_eq!(pixel(&chip, 84, 55), pen(&chip, 2));
    assert_eq!(pixel(&chip, 99, 55), pen(&chip, 1));
    assert_eq!(pixel(&chip, 100, 55), chip.colour(0xfff));
}

#[test]
fn horizontal_shrink_drops_pixels_by_the_table() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    for (n, shrink) in (0..16u16).enumerate() {
        place_sprite(&mut chip, n as u16 + 1, 20 * n as u32, 50, 1, 1);
        vram_write(&mut chip, 0x8001 + n as u16, (shrink << 8) | 0xff);
    }
    run(&mut chip, &roms, FRAME);
    for (n, shrink) in (0..16usize).enumerate() {
        let width = (0..20)
            .filter(|&dx| pixel(&chip, 20 * n + dx, 55) != chip.colour(0xfff))
            .count();
        assert_eq!(width, shrink + 1, "shrink {shrink}");
    }
}

#[test]
fn vertical_shrink_uses_the_zoom_table() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    // A table that shows only tile 0 line 5 for shrink $10.
    let mut zoom = vec![0u8; 0x10000];
    for line in 0..256 {
        zoom[(0x10 << 8) | line] = 0x05;
    }
    let roms = VideoRoms {
        sprites: &c,
        zoom: &zoom,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 100, 50, 1, 2);
    vram_write(&mut chip, 0x8001, 0x0f10);
    run(&mut chip, &roms, FRAME);
    for r in 0..16 {
        assert_eq!(pixel(&chip, 100, 50 + r), pen(&chip, 5));
    }

    // Without the ROM, shrink $7F shows every other line.
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    vram_write(&mut chip, 0x8001, 0x0f7f);
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 100, 51), pen(&chip, 2));
    assert_eq!(pixel(&chip, 100, 57), pen(&chip, 14));
}

#[test]
fn a_line_shows_at_most_96_sprites() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    // 100 one-pixel-wide sprites side by side on line 50.
    for n in 1..=100u16 {
        place_sprite(&mut chip, n, u32::from(n) * 3, 50, 1, 1);
        vram_write(&mut chip, 0x8000 | n, 0x00ff);
    }
    run_to_line(&mut chip, &roms, 51);
    let list = &chip.vram()[Lspc2::sprite_list_base(50)..];
    assert_eq!(list[0], 1);
    assert_eq!(list[95], 96);
    assert_eq!(list[96], 0);
    run(&mut chip, &roms, FRAME);
    let drawn = (1..=100)
        .filter(|&n| pixel(&chip, n * 3, 55) != chip.colour(0xfff))
        .count();
    assert_eq!(drawn, 96);
}

#[test]
fn sprites_draw_in_list_order_under_the_fix_layer() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let fix = fix_rom();
    let roms = VideoRoms {
        sprites: &c,
        fix: &fix,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 100, 48, 1, 2);
    place_sprite(&mut chip, 2, 100, 48, 1, 1);
    // Fix column 12 (x 96-103), row 6 (lines 48-55).
    vram_write(&mut chip, 0x7000 | (12 << 5) | 6, 0x1001);
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 101, 50), pen(&chip, 6), "fix over sprites");
    assert_eq!(pixel(&chip, 104, 50), pen(&chip, 1), "sprite 2 over 1");
}

#[test]
fn sprites_wrap_around_the_left_edge() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 0x1f8, 50, 1, 1);
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 0, 55), pen(&chip, 1));
    assert_eq!(pixel(&chip, 7, 55), pen(&chip, 1));
    assert_eq!(pixel(&chip, 8, 55), chip.colour(0xfff));
}

#[test]
fn auto_animation_replaces_the_low_code_bits() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    // Code 0 with 2-bit animation: shows tile 1 once the counter is 1.
    place_sprite(&mut chip, 1, 100, 50, 1, 0);
    vram_write(&mut chip, (1 << 6) | 1, 0x0104);
    run_to_line(&mut chip, &roms, VSSTART + 1);
    assert_eq!(chip.read_register(3) & 7, 1);
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 101, 55), pen(&chip, 1));
    chip.write_register(3, 0x0008); // disabled
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 101, 55), chip.colour(0xfff));
}

#[test]
fn state_survives_a_snapshot() {
    let mut chip = Lspc2::new();
    let roms = VideoRoms::default();
    set_palette(&mut chip);
    chip.write_register(3, 0x00b0);
    chip.write_register(5, 1000);
    run(&mut chip, &roms, 12345);
    let bytes = postcard::to_allocvec(&chip).unwrap();
    let mut copy: Lspc2 = postcard::from_bytes(&bytes).unwrap();
    run(&mut chip, &roms, FRAME);
    run(&mut copy, &roms, FRAME);
    assert_eq!(chip.framebuffer, copy.framebuffer);
    assert_eq!(chip.irq_level(), copy.irq_level());
    assert_eq!(chip.read_register(3), copy.read_register(3));
}

#[test]
fn the_level_tables_are_built_from_the_network() {
    assert_eq!(palette::levels(0.0), palette::LEVELS[0]);
    assert_eq!(palette::levels(1.0 / 8200.0), palette::LEVELS[1]);
    assert_eq!(palette::LEVELS[0][31], 255);
    assert_eq!(palette::LEVELS[0][0], 0);
}

#[test]
fn a_32_tile_sprite_continues_into_its_lower_half() {
    let mut chip = Lspc2::default();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    // Screen line 50 is sprite line 256: tile 16, line 0.
    place_sprite(&mut chip, 1, 100, (50 + 512 - 256) & 0x1ff, 32, 2);
    vram_write(&mut chip, (1 << 6) | (16 << 1), 1);
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 101, 49), pen(&chip, 15), "tile 15, line 15");
    assert_eq!(pixel(&chip, 101, 50), pen(&chip, 1), "tile 16");
}

#[test]
fn a_sprite_taller_than_32_tiles_repeats_mirrored() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 100, 50, 0x21, 2);
    vram_write(&mut chip, 0x8001, 0x0f7f);
    run(&mut chip, &roms, FRAME);
    // Shrunk to 128 lines, then mirrored.
    assert_eq!(pixel(&chip, 101, 50 + 127), pen(&chip, 14));
    assert_eq!(pixel(&chip, 101, 50 + 128), pen(&chip, 1));
}

#[test]
fn three_bit_auto_animation_and_off_screen_sprites() {
    let mut chip = Lspc2::new();
    let c = sprite_rom();
    let roms = VideoRoms {
        sprites: &c,
        ..VideoRoms::default()
    };
    set_palette(&mut chip);
    place_sprite(&mut chip, 1, 100, 50, 1, 0);
    vram_write(&mut chip, (1 << 6) | 1, 0x0108);
    // Between $140 and $1F0 a sprite is not drawn at all.
    place_sprite(&mut chip, 2, 0x150, 50, 1, 1);
    run_to_line(&mut chip, &roms, VSSTART + 1);
    run(&mut chip, &roms, FRAME);
    assert_eq!(pixel(&chip, 101, 55), pen(&chip, 1));
    let drawn = chip
        .framebuffer
        .iter()
        .filter(|&&p| p != chip.colour(0xfff))
        .count();
    assert_eq!(drawn, 16 * 16);
}
