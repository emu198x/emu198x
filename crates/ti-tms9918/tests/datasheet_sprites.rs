//! The four-sprites-per-line rule, checked against the chip's own data manual.
//!
//! TMS9918A/9928A/9929A data manual (November 1982), §2.3.3 and §2.4.5, held at
//! `reference/by-topic/vdp-tms9918/`. Eight machines share this chip, so a
//! sprite rule that is wrong here is wrong on all of them.
//!
//! §2.4.5:
//!
//! > There is a maximum limit of four sprites that can be displayed on one
//! > horizontal line. If this rule is violated, the four highest-priority
//! > sprites on the line are displayed normally. The fifth and subsequent
//! > sprites are not displayed on that line. Furthermore, the fifth-sprite bit
//! > in the VDP status register is set to a 1, and the number of the violating
//! > fifth sprite is loaded into the status register.
//!
//! # On the status register's bit positions
//!
//! `reference/by-topic/vdp-tms9918/tms9918a-reference.md` records that the OCR
//! of Figure 2-3 is garbled, and that F / 5S / C at bits 7 / 6 / 5 with the
//! fifth-sprite number in bits 4-0 is the standard layout, consistent with the
//! §2.3 prose but not legible in the figure itself. These tests inherit that
//! caveat: the *positions* are convention, the *rules* below are the manual's
//! own words.

use ti_tms9918::{PALETTE, Tms9918, VdpRegion};

const SAT: u16 = 0x1000;
const SPRITE_PATTERNS: u16 = 0x1800;
/// The line the sprites are put on. Any active line does.
const LINE: usize = 100;
const WHITE: u8 = 0x0F;

fn reg(vdp: &mut Tms9918, index: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | index);
}

/// A VDP in Graphics I with `count` 8x8 sprites side by side on [`LINE`].
fn with_sprites(count: usize) -> Tms9918 {
    let mut vdp = Tms9918::new(VdpRegion::Ntsc);
    reg(&mut vdp, 0, 0x00);
    // Display enabled; SIZE and MAG clear, so 8x8 unmagnified.
    reg(&mut vdp, 1, 0x40);
    reg(&mut vdp, 2, 0x00); // name table  $0000
    reg(&mut vdp, 3, 0x80); // colour table $2000
    reg(&mut vdp, 4, 0x01); // pattern gen  $0800
    #[allow(clippy::cast_possible_truncation)]
    reg(&mut vdp, 5, (SAT / 0x80) as u8);
    #[allow(clippy::cast_possible_truncation)]
    reg(&mut vdp, 6, (SPRITE_PATTERNS / 0x800) as u8);
    reg(&mut vdp, 7, 0x01); // backdrop black, so a sprite pixel is unambiguous

    // Sprite pattern 0: solid.
    for row in 0..8u16 {
        vdp.write_vram(SPRITE_PATTERNS + row, 0xFF);
    }

    // Sprites 0..count on the same line, spaced so none overlap.
    for i in 0..count {
        let entry = SAT + (i as u16) * 4;
        #[allow(clippy::cast_possible_truncation)]
        {
            vdp.write_vram(entry, (LINE - 1) as u8); // display line = Y + 1
            vdp.write_vram(entry + 1, 20 + (i as u8) * 24); // X
        }
        vdp.write_vram(entry + 2, 0x00); // pattern name
        vdp.write_vram(entry + 3, WHITE); // colour, early clock off
    }
    // Terminate the sprite list.
    vdp.write_vram(SAT + (count as u16) * 4, 0xD0);
    vdp
}

fn frame(vdp: &mut Tms9918) {
    for _ in 0..262 {
        vdp.tick_scanline();
    }
}

/// Is sprite `i` drawn on [`LINE`]?
fn sprite_visible(vdp: &Tms9918, i: usize) -> bool {
    let width = vdp.framebuffer_width() as usize;
    let left = VdpRegion::Ntsc.border_left() as usize;
    let top = VdpRegion::Ntsc.border_top() as usize;
    let x = 20 + i * 24 + 3; // a few pixels into the sprite
    vdp.framebuffer()[(top + LINE) * width + left + x] == PALETTE[WHITE as usize]
}

/// Four fit, and all four are drawn.
#[test]
fn four_sprites_share_a_line() {
    let mut vdp = with_sprites(4);
    frame(&mut vdp);
    for i in 0..4 {
        assert!(
            sprite_visible(&vdp, i),
            "sprite {i} of four should be drawn"
        );
    }
    assert_eq!(
        vdp.read_status() & 0x40,
        0,
        "four sprites is the limit, not a violation of it"
    );
}

/// The fifth is not, and the four highest-priority ones still are.
#[test]
fn a_fifth_sprite_is_dropped_and_the_first_four_survive() {
    let mut vdp = with_sprites(5);
    frame(&mut vdp);
    for i in 0..4 {
        assert!(
            sprite_visible(&vdp, i),
            "sprite {i} is one of the four highest-priority and should still draw"
        );
    }
    assert!(
        !sprite_visible(&vdp, 4),
        "the fifth sprite should not be displayed on that line"
    );
}

/// And it reports itself: the flag, and its own number.
#[test]
fn the_fifth_sprite_reports_its_number() {
    let mut vdp = with_sprites(5);
    frame(&mut vdp);
    let status = vdp.read_status();
    assert_eq!(status & 0x40, 0x40, "the fifth-sprite flag should be set");
    assert_eq!(
        status & 0x1F,
        4,
        "the number of the *violating* sprite is the fifth one, index 4"
    );
}

/// Reading the status register clears it. (§2.3.3: "cleared to a 0 after the
/// status register is read".)
#[test]
fn reading_the_status_clears_the_fifth_sprite_flag() {
    let mut vdp = with_sprites(5);
    frame(&mut vdp);
    assert_eq!(vdp.read_status() & 0x40, 0x40);
    assert_eq!(
        vdp.read_status() & 0x40,
        0,
        "a second read should find the flag cleared by the first"
    );
}

/// The flag needs the frame flag clear, which is the manual's own condition.
///
/// §2.3.3, through the OCR's substitution of `55` for `5S`:
///
/// > The 5S status flag in the status register is set to a 1 whenever there
/// > are five or more sprites on a horizontal line (lines 0 to 192) **and the
/// > frame flag is equal to a 0**.
///
/// So a program that never reads the status register — leaving F set from the
/// previous frame — stops being told about sprite overflow. That is a real
/// state to be in: the manual elsewhere advises reading the status register
/// only in the interrupt handler, so a machine running with interrupts
/// disabled sits with F latched.
///
/// The first frame here carries four sprites, which is legal, and ends with F
/// set. Nothing reads it. The second frame carries five.
#[test]
fn the_fifth_sprite_flag_needs_the_frame_flag_clear() {
    let mut vdp = with_sprites(4);
    frame(&mut vdp);
    // Deliberately not read: F stays set into the next frame.

    // Add the fifth sprite and move the terminator down.
    vdp.write_vram(SAT + 4 * 4, (LINE - 1) as u8);
    vdp.write_vram(SAT + 4 * 4 + 1, 20 + 4 * 24);
    vdp.write_vram(SAT + 4 * 4 + 2, 0x00);
    vdp.write_vram(SAT + 4 * 4 + 3, WHITE);
    vdp.write_vram(SAT + 5 * 4, 0xD0);

    frame(&mut vdp);

    let status = vdp.read_status();
    assert_eq!(
        status & 0x80,
        0x80,
        "the frame flag should still be set — this test is worthless if it is not"
    );
    assert_eq!(
        status & 0x40,
        0,
        "with the frame flag set, five sprites on a line must not set the \
         fifth-sprite flag (§2.3.3)"
    );
}
