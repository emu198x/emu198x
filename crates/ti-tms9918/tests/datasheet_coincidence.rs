//! The sprite coincidence flag, from §2.3.2 and §2.4.5 of the data manual
//! (November 1982), held at `reference/by-topic/vdp-tms9918/`.
//!
//! §2.3.2, with the OCR's `(el` for `(C)` and `0016` for the `$D0` terminator:
//!
//! > The C status flag in the status register is set to a 1 if two or more
//! > sprites coincide. Coincidence occurs if any two sprites on the screen have
//! > one overlapping pixel. Transparent colored sprites, as well as those that
//! > are partially or completely off the screen, are also considered. Sprites
//! > beyond the Sprite Attribute Table terminator are not considered.
//!
//! And §2.4.5 narrows it:
//!
//! > Only those sprites that are active on the display will cause the
//! > coincidence flag to set.
//!
//! Those two are not in conflict. A sprite suppressed by the four-per-line
//! limit is not active and does not collide; a sprite that is active but drawn
//! past the edge of the screen still does.
//!
//! Eight machines share this chip.

use ti_tms9918::{Tms9918, VdpRegion};

const SAT: u16 = 0x1000;
const SPG: u16 = 0x1800;
const LINE: u8 = 100;
const WHITE: u8 = 0x0F;
const COINCIDENCE: u8 = 0x20;

fn reg(vdp: &mut Tms9918, index: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | index);
}

fn base() -> Tms9918 {
    let mut vdp = Tms9918::new(VdpRegion::Ntsc);
    reg(&mut vdp, 0, 0x00);
    reg(&mut vdp, 1, 0x40); // display on, 8x8, unmagnified
    reg(&mut vdp, 2, 0x00);
    reg(&mut vdp, 3, 0x80);
    reg(&mut vdp, 4, 0x01);
    #[allow(clippy::cast_possible_truncation)]
    reg(&mut vdp, 5, (SAT / 0x80) as u8);
    #[allow(clippy::cast_possible_truncation)]
    reg(&mut vdp, 6, (SPG / 0x800) as u8);
    reg(&mut vdp, 7, 0x01);
    for row in 0..8u16 {
        vdp.write_vram(SPG + row, 0xFF); // solid pattern 0
    }
    vdp
}

/// `(x, colour, early_clock)` per sprite, all on the same line.
fn sprites(vdp: &mut Tms9918, list: &[(u8, u8, bool)]) {
    for (i, &(x, colour, early)) in list.iter().enumerate() {
        let entry = SAT + (i as u16) * 4;
        vdp.write_vram(entry, LINE - 1);
        vdp.write_vram(entry + 1, x);
        vdp.write_vram(entry + 2, 0x00);
        vdp.write_vram(entry + 3, colour | if early { 0x80 } else { 0x00 });
    }
    vdp.write_vram(SAT + (list.len() as u16) * 4, 0xD0);
}

fn frame_status(vdp: &mut Tms9918) -> u8 {
    for _ in 0..262 {
        vdp.tick_scanline();
    }
    vdp.read_status()
}

/// Two overlapping sprites set it; two apart do not.
#[test]
fn overlapping_sprites_coincide() {
    let mut apart = base();
    sprites(&mut apart, &[(40, WHITE, false), (80, WHITE, false)]);
    assert_eq!(
        frame_status(&mut apart) & COINCIDENCE,
        0,
        "sprites that do not touch should not coincide"
    );

    let mut overlapping = base();
    sprites(&mut overlapping, &[(40, WHITE, false), (44, WHITE, false)]);
    assert_eq!(
        frame_status(&mut overlapping) & COINCIDENCE,
        COINCIDENCE,
        "one overlapping pixel is enough"
    );
}

/// "Transparent colored sprites ... are also considered."
#[test]
fn transparent_sprites_still_coincide() {
    let mut vdp = base();
    sprites(&mut vdp, &[(40, 0, false), (44, 0, false)]);
    assert_eq!(
        frame_status(&mut vdp) & COINCIDENCE,
        COINCIDENCE,
        "colour 0 draws nothing but still collides"
    );
}

/// "Sprites beyond the Sprite Attribute Table terminator are not considered."
#[test]
fn sprites_past_the_terminator_do_not_coincide() {
    let mut vdp = base();
    // Terminator first, then two sprites that would otherwise overlap.
    vdp.write_vram(SAT, 0xD0);
    for (i, x) in [40u8, 44].iter().enumerate() {
        let entry = SAT + ((i + 1) as u16) * 4;
        vdp.write_vram(entry, LINE - 1);
        vdp.write_vram(entry + 1, *x);
        vdp.write_vram(entry + 2, 0x00);
        vdp.write_vram(entry + 3, WHITE);
    }
    assert_eq!(
        frame_status(&mut vdp) & COINCIDENCE,
        0,
        "nothing past the $D0 terminator is considered"
    );
}

/// §2.4.5: a sprite suppressed by the four-per-line limit is not active, so it
/// does not collide with the four that are.
#[test]
fn the_fifth_sprite_on_a_line_does_not_coincide() {
    let mut vdp = base();
    // Four spread out, then a fifth sitting exactly on the first.
    sprites(
        &mut vdp,
        &[
            (10, WHITE, false),
            (60, WHITE, false),
            (110, WHITE, false),
            (160, WHITE, false),
            (10, WHITE, false),
        ],
    );
    let status = frame_status(&mut vdp);
    assert_eq!(status & 0x40, 0x40, "five on a line is still an overflow");
    assert_eq!(
        status & COINCIDENCE,
        0,
        "but the suppressed fifth is not active, so it cannot coincide"
    );
}

/// "...as well as those that are partially or completely off the screen, are
/// also considered."
///
/// Early clock subtracts 32 from X, so an X of 0 with the bit set puts an 8-wide
/// sprite entirely at columns -32 to -25 — off the left edge, never generated
/// into a visible pixel. Two of them overlap exactly, and the manual says that
/// still counts: the VDP "checks each pixel position for coincidence during the
/// generation of the pixel regardless of where it is located on the screen".
#[test]
fn sprites_off_the_screen_still_coincide() {
    let mut vdp = base();
    sprites(&mut vdp, &[(0, WHITE, true), (0, WHITE, true)]);
    assert_eq!(
        frame_status(&mut vdp) & COINCIDENCE,
        COINCIDENCE,
        "two sprites overlapping off the left edge still coincide (§2.3.2)"
    );
}

/// Cleared by reading the status register.
#[test]
fn reading_the_status_clears_coincidence() {
    let mut vdp = base();
    sprites(&mut vdp, &[(40, WHITE, false), (44, WHITE, false)]);
    assert_eq!(frame_status(&mut vdp) & COINCIDENCE, COINCIDENCE);
    assert_eq!(
        vdp.read_status() & COINCIDENCE,
        0,
        "the first read should have cleared it"
    );
}
