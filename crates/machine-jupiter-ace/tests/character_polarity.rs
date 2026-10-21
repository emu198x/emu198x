//! Jupiter Cantab, User's Manual (1982), ch12: codes 0–127 are white
//! on black; code+128 inverts those dots. MAME cantab/jupace.cpp emits
//! each glyph bit directly into its monochrome palette (0 black, 1 white).
use machine_jupiter_ace::JupiterAce;
use machine_jupiter_ace::{FB_HEIGHT, FB_WIDTH};

#[test]
fn normal_and_inverse_cells_preserve_the_manuals_dot_polarity() {
    let mut machine = JupiterAce::new(vec![0x76; 8192], 0).expect("original HALT fixture");
    let rows = [0x81, 0x42, 0x24, 0x18, 0x10, 0x24, 0x42, 0x80];
    for (row, bits) in rows.iter().copied().enumerate() {
        machine.poke(0x2c08 + row as u16, bits);
    }
    machine.poke(0x2400, 1);
    machine.poke(0x2401, 129);
    machine.run_frame();
    let frame = machine.framebuffer();
    let width = FB_WIDTH as usize;
    for (row, bits) in rows.iter().copied().enumerate() {
        for column in 0..8 {
            let at = ((FB_HEIGHT as usize - 192) / 2 + row) * width
                + (FB_WIDTH as usize - 256) / 2
                + column;
            let white = bits & (0x80 >> column) != 0;
            assert_eq!(frame[at], if white { 0xffcfcfcf } else { 0xff000000 });
            assert_eq!(frame[at + 8], if white { 0xff000000 } else { 0xffcfcfcf });
            assert_eq!(frame[at + 16], 0xff000000, "blank neighbour stays black");
        }
    }
    assert_eq!(frame[0], 0xff000000, "inactive border stays black");
}
