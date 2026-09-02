//! ANTIC's write-only registers read as `$FF` through the CPU bus.
//!
//! Ace of Aces (XEGS cartridge) enables its interrupts with
//! `LDA NMIEN / AND #$7F / STA NMIEN` and then `LDA NMIEN / ORA #$80 /
//! STA NMIEN`. NMIEN is write-only, so each read returns `$FF` and the
//! sequence ends with both NMIs enabled. Returning 0 leaves the VBI off
//! and the game waiting on a jiffy clock that never ticks.

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

#[test]
fn read_modify_write_on_nmien_ends_with_every_nmi_enabled() {
    let mut cart = vec![0xFFu8; 8192];
    let program: [u8; 31] = [
        0xAD, 0x0E, 0xD4, // lda $D40E
        0x8D, 0x00, 0x06, // sta $0600 — what the bus handed back
        0x29, 0x7F, // and #$7F
        0x8D, 0x0E, 0xD4, // sta $D40E
        0xAD, 0x0E, 0xD4, // lda $D40E
        0x09, 0x40, // ora #$40
        0x8D, 0x0E, 0xD4, // sta $D40E
        0xAD, 0x0E, 0xD4, // lda $D40E
        0x09, 0x80, // ora #$80
        0x8D, 0x0E, 0xD4, // sta $D40E
        0x4C, 0x1B, 0xA0, // done: jmp done
        0xEA,
    ];
    cart[..program.len()].copy_from_slice(&program);

    let mut machine = Atari800xl::new(None, None, Some(cart), Atari800xlRegion::Ntsc, false)
        .expect("cart-only boot");
    machine.run_frame();

    assert_eq!(machine.peek(0x0600), 0xFF);
    assert_eq!(machine.antic().nmien_value(), 0xFF);
}
