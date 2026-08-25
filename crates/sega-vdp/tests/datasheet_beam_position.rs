//! Where the beam is on the framebuffer.
//!
//! A light gun or light pen senses the screen, and the screen includes the
//! border. So the beam has to be locatable everywhere a set shows it, not
//! only across the picture — and the border is where the two coordinate
//! systems part company, because `dot` and `scanline` both count from the
//! *picture* while the framebuffer counts from the top-left of the window.

use sega_vdp::{DOTS_PER_LINE, SegaVdp, VdpRegion, VdpVariant};

fn vdp(region: VdpRegion) -> SegaVdp {
    let mut vdp = SegaVdp::new(region, VdpVariant::Sms2);
    // Mode 4, display on.
    vdp.write_control(0x04);
    vdp.write_control(0x80);
    vdp.write_control(0x40);
    vdp.write_control(0x81);
    vdp
}

/// Step the beam to a given dot of a given line.
fn beam_to(vdp: &mut SegaVdp, line: u16, dot: u16) {
    while !vdp.tick_scanline() {} // settle at a frame boundary
    for _ in 0..u32::from(line) * u32::from(DOTS_PER_LINE) + u32::from(dot) {
        vdp.tick();
    }
}

/// Every position the beam reaches is inside the framebuffer, and the ones it
/// does not reach are the ones a set does not show. Over a whole frame the
/// beam should touch every framebuffer pixel exactly once — that is what makes
/// the mapping a mapping rather than an approximation.
#[test]
fn the_beam_covers_every_framebuffer_pixel_once_a_frame() {
    for region in [VdpRegion::Ntsc, VdpRegion::Pal] {
        let mut vdp = vdp(region);
        let (width, height) = (region.framebuffer_width(), region.framebuffer_height());
        let mut seen = vec![0u8; (width * height) as usize];

        while !vdp.tick_scanline() {}
        let dots = u32::from(vdp.lines_per_frame()) * u32::from(DOTS_PER_LINE);
        for _ in 0..dots {
            if let Some((x, y)) = vdp.beam_framebuffer_position() {
                assert!(
                    x < width && y < height,
                    "{region:?}: ({x}, {y}) is off the framebuffer"
                );
                seen[(y * width + x) as usize] += 1;
            }
            vdp.tick();
        }

        let missed = seen.iter().filter(|&&n| n == 0).count();
        let twice = seen.iter().filter(|&&n| n > 1).count();
        assert_eq!(missed, 0, "{region:?}: {missed} pixels never scanned");
        assert_eq!(
            twice, 0,
            "{region:?}: {twice} pixels scanned more than once"
        );
    }
}

/// The first active pixel is under the border, not at the origin — the border
/// is scanned before the picture on every line and every frame.
#[test]
fn the_first_active_dot_lands_under_the_border() {
    let region = VdpRegion::Ntsc;
    let mut vdp = vdp(region);
    beam_to(&mut vdp, 0, 0);
    assert_eq!(
        vdp.beam_framebuffer_position(),
        Some((region.border_left(), region.border_top(192)))
    );
}

/// The dots after the picture are the right border, on the same line.
#[test]
fn the_dots_after_the_picture_are_the_right_border() {
    let region = VdpRegion::Ntsc;
    let mut vdp = vdp(region);
    beam_to(&mut vdp, 10, 256);
    let (x, y) = vdp.beam_framebuffer_position().expect("right border");
    assert_eq!(x, region.border_left() + 256, "just past the picture");
    assert_eq!(y, region.border_top(192) + 10, "still on the same line");
}

/// The dots at the end of a line are the left border of the *next* one. This
/// is the part that cannot be got by adding an offset: the beam is drawing a
/// row of the framebuffer that our line numbering has not reached yet.
#[test]
fn the_dots_at_the_end_of_a_line_are_the_next_lines_left_border() {
    let region = VdpRegion::Ntsc;
    let mut vdp = vdp(region);
    beam_to(&mut vdp, 10, DOTS_PER_LINE - region.border_left() as u16);
    let (x, y) = vdp.beam_framebuffer_position().expect("left border");
    assert_eq!(x, 0, "the first column of the window");
    assert_eq!(y, region.border_top(192) + 11, "belonging to the next line");
}

/// The same wrap vertically: the tail of the frame is the top border of the
/// picture that follows it.
#[test]
fn the_lines_at_the_end_of_a_frame_are_the_top_border() {
    let region = VdpRegion::Ntsc;
    let mut vdp = vdp(region);
    let lines = vdp.lines_per_frame();
    beam_to(&mut vdp, lines - region.border_top(192) as u16, 0);
    let (_, y) = vdp.beam_framebuffer_position().expect("top border");
    assert_eq!(y, 0, "the first row of the window");
}

/// Blanking is nowhere. A gun cannot see the beam during it, so the position
/// has to be absent rather than clamped to an edge.
#[test]
fn the_beam_is_nowhere_during_blanking() {
    let region = VdpRegion::Ntsc;
    let mut vdp = vdp(region);
    // Between the right border and the left border of the next line.
    beam_to(&mut vdp, 10, 280);
    assert_eq!(vdp.beam_framebuffer_position(), None);
}

/// A Game Gear has no border to scan, so it has no framebuffer beam.
#[test]
fn a_game_gear_has_no_border_to_scan() {
    let mut gg = SegaVdp::new_game_gear();
    for _ in 0..1000 {
        assert_eq!(gg.beam_framebuffer_position(), None);
        gg.tick();
    }
}

/// A position past the right edge is off the framebuffer, not the start of
/// the next row.
///
/// The framebuffer is one flat slice, so an unchecked `y * width + x` turns a
/// column just past the edge into a real index a row down — and a light gun
/// aimed at the edge of the screen would read a pixel from the wrong line
/// while looking perfectly reasonable.
#[test]
fn a_position_past_the_edge_does_not_alias_onto_the_next_row() {
    let region = VdpRegion::Ntsc;
    let mut vdp = vdp(region);
    while !vdp.tick_scanline() {}
    let width = region.framebuffer_width();
    let height = region.framebuffer_height();

    assert!(vdp.framebuffer_pixel(width - 1, 0).is_some());
    assert!(
        vdp.framebuffer_pixel(width, 0).is_none(),
        "one past the right edge is off the screen"
    );
    assert!(
        vdp.framebuffer_pixel(width + 5, 0).is_none(),
        "and so is five past it, rather than five into the next row"
    );
    assert!(vdp.framebuffer_pixel(0, height - 1).is_some());
    assert!(vdp.framebuffer_pixel(0, height).is_none());
}
