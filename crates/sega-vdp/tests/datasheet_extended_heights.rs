//! Mode 4's 224 and 240-line displays.
//!
//! Four mode bits select the height: M2 and M4 are R0 bits 1 and 2, M3 and M1
//! are R1 bits 3 and 4. Genesis Plus GX builds `(reg[0] & 0x06) | (reg[1] &
//! 0x18)` and tests it for exact equality — `$0E` for 240 lines, `$16` for
//! 224, everything else 192 — so neither extended height is reachable without
//! M2, and neither is reachable with M1 and M3 set together.
//!
//! Changing the height changes four other things, in four different places:
//! R2 decodes differently, the name table grows, the $D0 sprite terminator
//! stops working, and the vertical scroll wraps at 256 rather than 224.

use sega_vdp::{ACTIVE_WIDTH, SegaVdp, VdpRegion, VdpVariant};

const BACKDROP: u8 = 0x30;

fn write_register(vdp: &mut SegaVdp, reg: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | (reg & 0x0F));
}

fn poke_vram(vdp: &mut SegaVdp, addr: u16, bytes: &[u8]) {
    vdp.write_control(addr as u8);
    vdp.write_control(((addr >> 8) as u8 & 0x3F) | 0x40);
    for &b in bytes {
        vdp.write_data(b);
    }
}

fn poke_cram(vdp: &mut SegaVdp, index: u8, value: u8) {
    vdp.write_control(index);
    vdp.write_control(0xC0);
    vdp.write_data(value);
}

fn solid_tile(colour: u8) -> [u8; 32] {
    let plane = |bit: u8| {
        if colour & (1 << bit) != 0 {
            0xFFu8
        } else {
            0x00
        }
    };
    let row = [plane(0), plane(1), plane(2), plane(3)];
    let mut tile = [0u8; 32];
    for r in 0..8 {
        tile[r * 4..r * 4 + 4].copy_from_slice(&row);
    }
    tile
}

/// The 315-5246's measured CRAM output levels — `%00BBGGRR` through the
/// table in `VdpVariant::output_levels`. Levels 0 and 3 happen to match a
/// bit-replicated expansion, so a test that only uses black and full scale
/// cannot tell the two apart; these helpers use the real table so that
/// coincidence is not load-bearing.
fn sms_argb(entry: u8) -> u32 {
    const LEVELS: [u32; 4] = [0, 89, 174, 255];
    let e = u32::from(entry) as usize;
    0xFF00_0000 | (LEVELS[e & 3] << 16) | (LEVELS[(e >> 2) & 3] << 8) | LEVELS[(e >> 4) & 3]
}

/// R1 bits that put the chip in a given height, alongside the M2 that both
/// extended modes need.
const fn mode_bits(height: u32) -> (u8, u8) {
    match height {
        224 => (0x06, 0x10), // R0 |= M4 + M2, R1 |= M1
        240 => (0x06, 0x08), // R0 |= M4 + M2, R1 |= M3
        _ => (0x04, 0x00),   // R0 |= M4
    }
}

fn blank(region: VdpRegion, height: u32) -> SegaVdp {
    let (reg0, reg1) = mode_bits(height);
    let mut vdp = SegaVdp::new(region, VdpVariant::Sms2);
    write_register(&mut vdp, 0, reg0);
    write_register(&mut vdp, 1, 0x40 | reg1);
    write_register(&mut vdp, 3, 0xFF);
    write_register(&mut vdp, 4, 0x07);
    write_register(&mut vdp, 7, 0x00);
    poke_cram(&mut vdp, 16, BACKDROP);
    vdp
}

/// Scan one whole frame, from a fresh frame boundary.
fn render_frame(vdp: &mut SegaVdp) {
    while !vdp.tick_scanline() {}
    while !vdp.tick_scanline() {}
}

// ---------------------------------------------------------------------------
// Selecting a height
// ---------------------------------------------------------------------------

#[test]
fn the_mode_bits_select_the_height_by_exact_match() {
    let cases = [
        // (R0 mode bits, R1 mode bits, height)
        (0x04u8, 0x00u8, 192), // Mode 4 alone
        (0x06, 0x10, 224),     // M4 + M2 + M1
        (0x06, 0x08, 240),     // M4 + M2 + M3
        (0x04, 0x10, 192),     // M1 without M2 does nothing
        (0x04, 0x08, 192),     // M3 without M2 does nothing
        (0x06, 0x18, 192),     // M1 and M3 together is not a height
        (0x06, 0x00, 192),     // M2 without either is not a height
    ];
    for (reg0, reg1, expected) in cases {
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms2);
        write_register(&mut vdp, 0, reg0);
        write_register(&mut vdp, 1, 0x40 | reg1);
        render_frame(&mut vdp);
        assert_eq!(
            vdp.active_height(),
            expected,
            "R0 mode bits {reg0:#04X} and R1 mode bits {reg1:#04X}"
        );
    }
}

/// Both heights are 315-5246 modes; the 315-5124 ignores the bits, on the
/// same gate as its address-bus masks.
#[test]
fn the_315_5124_has_no_extended_heights() {
    for height in [224u32, 240] {
        let (reg0, reg1) = mode_bits(height);
        let mut vdp = SegaVdp::new(VdpRegion::Ntsc, VdpVariant::Sms1);
        write_register(&mut vdp, 0, reg0);
        write_register(&mut vdp, 1, 0x40 | reg1);
        render_frame(&mut vdp);
        assert_eq!(
            vdp.active_height(),
            192,
            "the 315-5124 should stay at 192 lines whatever {height} asks for"
        );
    }
}

/// A Game Gear stays at 192 lines whatever the mode bits say.
///
/// This is a decision, not a fact about the silicon: the Game Gear carries a
/// 315-5246, so the bits presumably do something. But its window is a
/// physical 160x144 panel rather than a television's, no source here says
/// where that panel would sit on a taller raster, and no Game Gear software
/// is known to ask. The test exists so the choice is visible rather than
/// implicit.
#[test]
fn a_game_gear_stays_at_192_lines() {
    for height in [224u32, 240] {
        let (reg0, reg1) = mode_bits(height);
        let mut vdp = SegaVdp::new_game_gear();
        write_register(&mut vdp, 0, reg0);
        write_register(&mut vdp, 1, 0x40 | reg1);
        render_frame(&mut vdp);
        assert_eq!(vdp.active_height(), 192, "a Game Gear asked for {height}");
    }
}

/// "Viewport changes should be applied on next frame" — a height written
/// part-way down a frame does not take effect until the next one, the same
/// rule the vertical scroll follows.
#[test]
fn a_height_change_takes_effect_on_the_next_frame() {
    let mut vdp = blank(VdpRegion::Ntsc, 192);
    render_frame(&mut vdp);
    assert_eq!(vdp.active_height(), 192);

    // Part-way into the next frame, ask for 224.
    for _ in 0..100 {
        vdp.tick_scanline();
    }
    let (reg0, reg1) = mode_bits(224);
    write_register(&mut vdp, 0, reg0);
    write_register(&mut vdp, 1, 0x40 | reg1);
    assert_eq!(
        vdp.active_height(),
        192,
        "the frame being scanned keeps the height it started with"
    );

    while !vdp.tick_scanline() {}
    vdp.tick_scanline();
    assert_eq!(vdp.active_height(), 224, "the next frame picks it up");
}

// ---------------------------------------------------------------------------
// Where the picture sits
// ---------------------------------------------------------------------------

/// The framebuffer is the set's window and does not change size with the
/// mode — what changes is how much border is left around the picture. Every
/// height has to account for every line of its window.
#[test]
fn every_height_fills_its_window_exactly() {
    for region in [VdpRegion::Ntsc, VdpRegion::Pal] {
        for height in [192u32, 224, 240] {
            let mut vdp = blank(region, height);
            render_frame(&mut vdp);
            assert_eq!(
                vdp.framebuffer_height(),
                region.framebuffer_height(),
                "{region:?} at {height} lines: the window does not resize"
            );
            assert_eq!(
                region.border_top(height) + height + region.border_bottom(height),
                region.framebuffer_height(),
                "{region:?} at {height} lines does not account for every line"
            );
        }
    }
}

/// The taller the picture, the less border there is — and on NTSC at 240
/// lines there is none at all, which is why that mode is unusable on a 60 Hz
/// set. MAME's `315_5124.h` tables the scanned borders these come from.
#[test]
fn the_border_shrinks_by_what_the_picture_grows() {
    for (region, expected) in [
        (VdpRegion::Ntsc, [(192u32, 25u32), (224, 9), (240, 0)]),
        (VdpRegion::Pal, [(192, 51), (224, 35), (240, 27)]),
    ] {
        for (height, top) in expected {
            assert_eq!(
                region.border_top(height),
                top,
                "{region:?} at {height} lines"
            );
        }
        // A taller picture never gets a taller border.
        assert!(region.border_top(224) < region.border_top(192));
        assert!(region.border_top(240) < region.border_top(224));
    }
}

/// The picture lands where the border says it does, in every mode.
#[test]
fn the_first_active_line_lands_under_the_border() {
    for height in [192u32, 224, 240] {
        let region = VdpRegion::Pal;
        let mut vdp = blank(region, height);
        write_register(&mut vdp, 2, 0xFF);
        poke_cram(&mut vdp, 1, 0x03); // red
        poke_vram(&mut vdp, 0x0020, &solid_tile(1));
        let name_base: u16 = if height > 192 { 0x3700 } else { 0x3800 };
        for row in 0..32u16 {
            for col in 0..32u16 {
                poke_vram(&mut vdp, name_base + row * 64 + col * 2, &[0x01, 0x00]);
            }
        }
        render_frame(&mut vdp);

        let width = region.framebuffer_width();
        let row = |y: u32| {
            let start = (y * width + region.border_left()) as usize;
            vdp.framebuffer()[start]
        };
        let top = region.border_top(height);
        assert_eq!(
            row(top - 1),
            sms_argb(BACKDROP),
            "{height}: the line above the picture is border"
        );
        assert_eq!(
            row(top),
            sms_argb(0x03),
            "{height}: the picture starts on the line under it"
        );
        assert_eq!(
            row(top + height - 1),
            sms_argb(0x03),
            "{height}: and runs to its last line"
        );
    }
}

// ---------------------------------------------------------------------------
// What else the height changes
// ---------------------------------------------------------------------------

/// "If Mode 4 is used in the 224 or 240-line display mode, only bits 3 and 2
/// are used to calculate the table address, and an offset of $700 is added."
/// So R2 = $FF names $3800 at 192 lines and $3700 at 224.
#[test]
fn the_tall_modes_decode_register_2_differently() {
    for (height, base) in [(192u32, 0x3800u16), (224, 0x3700), (240, 0x3700)] {
        let mut vdp = blank(VdpRegion::Ntsc, height);
        write_register(&mut vdp, 2, 0xFF);
        poke_cram(&mut vdp, 1, 0x03);
        poke_vram(&mut vdp, 0x0020, &solid_tile(1));
        poke_vram(&mut vdp, base, &[0x01, 0x00]);
        render_frame(&mut vdp);

        let region = VdpRegion::Ntsc;
        let index = (region.border_top(height) * region.framebuffer_width() + region.border_left())
            as usize;
        assert_eq!(
            vdp.framebuffer()[index],
            sms_argb(0x03),
            "at {height} lines R2 = $FF should name {base:#06X}"
        );
    }
}

/// R2's bit 1 counts at 192 lines and does not in the tall modes, where only
/// bits 3 and 2 reach the address. The same register value therefore has to
/// name two different tables, and $3F00 is the one it names at 224.
#[test]
fn register_2_bit_1_is_ignored_by_the_tall_modes() {
    // $FD has bit 1 clear. At 192 lines that is bits 3-1 = 6, so $3000. At
    // 224 lines bits 3-2 are still 3, so $3700 — unchanged from $FF.
    for (reg2, height, base) in [
        (0xFDu8, 192u32, 0x3000u16),
        (0xFD, 224, 0x3700),
        (0xFF, 224, 0x3700),
    ] {
        let mut vdp = blank(VdpRegion::Ntsc, height);
        write_register(&mut vdp, 2, reg2);
        poke_cram(&mut vdp, 1, 0x03);
        poke_vram(&mut vdp, 0x0020, &solid_tile(1));
        poke_vram(&mut vdp, base, &[0x01, 0x00]);
        render_frame(&mut vdp);

        let region = VdpRegion::Ntsc;
        let index = (region.border_top(height) * region.framebuffer_width() + region.border_left())
            as usize;
        assert_eq!(
            vdp.framebuffer()[index],
            sms_argb(0x03),
            "R2 = {reg2:#04X} at {height} lines should name {base:#06X}"
        );
    }
}

/// "Y = $D0 in 192-line mode terminates the list [...] In 224/240-line modes
/// the terminator is disabled." A chip that kept treating it as a terminator
/// would truncate every sprite list in the tall modes.
#[test]
fn the_sprite_list_terminator_is_disabled_in_the_tall_modes() {
    for (height, second_sprite_drawn) in [(192u32, false), (224, true), (240, true)] {
        let mut vdp = blank(VdpRegion::Ntsc, height);
        write_register(&mut vdp, 5, 0xFF); // attribute table $3F00
        write_register(&mut vdp, 6, 0x03); // sprite patterns $0000
        poke_cram(&mut vdp, 16 + 5, 0x03);
        poke_vram(&mut vdp, 0x0020, &solid_tile(5));
        // Sprite 0 at y = $D0, sprite 1 just below it. In the tall modes $D0
        // is line 209 and both are ordinary sprites.
        poke_vram(&mut vdp, 0x3F00, &[0xD0, 0xD0, 0xC0]);
        poke_vram(&mut vdp, 0x3F80, &[0, 1, 16, 1]);
        render_frame(&mut vdp);

        let region = VdpRegion::Ntsc;
        let line = 209u32; // $D0 + 1
        if height <= line {
            continue; // the sprite is off the bottom of this mode
        }
        let index = ((region.border_top(height) + line) * region.framebuffer_width()
            + region.border_left()) as usize;
        let drawn = vdp.framebuffer()[index] == sms_argb(0x03);
        assert_eq!(
            drawn,
            second_sprite_drawn,
            "at {height} lines a Y of $D0 should {} the list",
            if second_sprite_drawn {
                "not end"
            } else {
                "end"
            }
        );
    }
}

/// "Vertical: wraps modulo 224 in 192/224-line modes, modulo 256 in 240-line
/// mode." The name table is 28 rows in the first two and 32 in the last.
#[test]
fn the_vertical_scroll_wraps_at_the_name_tables_height() {
    // Every name-table row draws a different colour, so the pixel names the
    // row outright. Alternating two tiles would not do: the two wraps put a
    // given scroll 28 rows apart, and 28 is even, so a two-colour scene reads
    // the same either way.
    const COLOURS: u16 = 15;
    for (height, wrap) in [(192u32, 224u32), (224, 224), (240, 256)] {
        let mut vdp = blank(VdpRegion::Ntsc, height);
        write_register(&mut vdp, 2, 0xFF);
        write_register(&mut vdp, 9, 232); // past 224, short of 256
        let name_base = if height > 192 { 0x3700u16 } else { 0x3800 };
        for row in 0..32u16 {
            let colour = 1 + (row % COLOURS) as u8;
            let tile = 1 + row;
            poke_cram(&mut vdp, colour, colour * 4);
            poke_vram(&mut vdp, tile * 32, &solid_tile(colour));
            for col in 0..32u16 {
                poke_vram(
                    &mut vdp,
                    name_base + row * 64 + col * 2,
                    &[(tile & 0xFF) as u8, (tile >> 8) as u8],
                );
            }
        }
        render_frame(&mut vdp);

        // Line 0 samples name-table row (232 % wrap) / 8 — row 1 if the wrap
        // is at 224, row 29 if it is at 256.
        let expected_row = (232 % wrap) / 8;
        let expected_colour = 1 + (expected_row % u32::from(COLOURS)) as u8;

        let region = VdpRegion::Ntsc;
        let index = (region.border_top(height) * region.framebuffer_width() + region.border_left())
            as usize;
        assert_eq!(
            vdp.framebuffer()[index],
            sms_argb(expected_colour * 4),
            "at {height} lines a scroll of 232 should wrap at {wrap} and land on row {expected_row}"
        );
    }
}

/// Raster code busy-waits on the V counter for a particular line, so across
/// the picture the counter has to name each one unambiguously. It does: every
/// mode's jump threshold sits past the last active line, so inside the
/// display the counter simply *is* the scanline. A threshold set too low
/// would start repeating values while the picture was still being drawn, and
/// a game waiting for line 220 would fire on 214.
#[test]
fn the_v_counter_names_every_line_of_the_picture() {
    for region in [VdpRegion::Ntsc, VdpRegion::Pal] {
        for height in [192u32, 224, 240] {
            let mut vdp = blank(region, height);
            render_frame(&mut vdp);
            while !vdp.tick_scanline() {} // settle on a frame boundary

            for line in 0..height {
                vdp.tick_scanline();
                assert_eq!(
                    u32::from(vdp.read_v_counter()),
                    line,
                    "{region:?} at {height} lines: the counter should read {line}"
                );
            }
        }
    }
}

/// The active area is still 256 pixels across in every height, so nothing
/// horizontal moves.
#[test]
fn the_tall_modes_do_not_change_the_width() {
    for height in [192u32, 224, 240] {
        let mut vdp = blank(VdpRegion::Ntsc, height);
        render_frame(&mut vdp);
        assert_eq!(
            vdp.framebuffer_width(),
            VdpRegion::Ntsc.framebuffer_width(),
            "the window is as wide at {height} lines as at any other"
        );
        assert!(
            VdpRegion::Ntsc.border_left() * 2 + ACTIVE_WIDTH <= VdpRegion::Ntsc.framebuffer_width(),
            "the active area and its side borders fit the window"
        );
    }
}
