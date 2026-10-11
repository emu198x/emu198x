//! Display blanking suppresses pixels/collisions, but not the Mode 4 sprite
//! count. FluBBa's SMS VDP Test 1.31, test 54, observes overflow with R1.6 off.
//! https://www.smspower.org/Homebrew/SMSVDPTest-SMS
//! MAME 315_5124 select_sprites and Genesis Plus GX parse_satb_m4 agree.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};

fn register(vdp: &mut SegaVdp, index: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | index);
}

fn vram(vdp: &mut SegaVdp, address: u16, bytes: &[u8]) {
    vdp.write_control(address as u8);
    vdp.write_control(0x40 | (address >> 8) as u8);
    for &byte in bytes {
        vdp.write_data(byte);
        for _ in 0..40 {
            vdp.tick();
        }
    }
}

fn chips() -> Vec<SegaVdp> {
    let mut chips = Vec::new();
    for region in [VdpRegion::Ntsc, VdpRegion::Pal] {
        for variant in [VdpVariant::Sms1, VdpVariant::Sms2] {
            chips.push(SegaVdp::new(region, variant));
        }
    }
    chips.push(SegaVdp::new_game_gear());
    chips
}

fn setup(vdp: &mut SegaVdp, count: usize, y: u8, opaque: bool) {
    register(vdp, 0, 0x04);
    register(vdp, 1, 0x00); // Mode 4, display disabled.
    register(vdp, 5, 0xff); // SAT at $3f00, SMS1 masks disabled.
    register(vdp, 6, 0x03);
    let mut positions = vec![y; count];
    positions.push(0xd0);
    vram(vdp, 0x3f00, &positions);
    // All sprites overlap. An opaque pattern would collide if blanking
    // incorrectly allowed the pixel/collision stage to run.
    vram(vdp, 0x3f80, &[64, 1].repeat(count));
    vram(vdp, 0x0020, &[if opaque { 0xff } else { 0 }; 32]);
    // Make sprite ink different from the default black backdrop.
    vdp.write_control(62); // GG colour 31 low/high; SMS address wraps to 30/31.
    vdp.write_control(0xc0);
    for byte in [0xff, 0x0f] {
        vdp.write_data(byte);
        for _ in 0..40 {
            vdp.tick();
        }
    }
    while !vdp.tick() {}
    vdp.read_status();
}

fn scan_active(vdp: &mut SegaVdp) {
    for _ in 0..192 * 342 {
        vdp.tick();
    }
}

#[test]
fn ninth_sprite_overflows_while_blanked_without_pixels_or_collision() {
    for mut vdp in chips() {
        for opaque in [false, true] {
            setup(&mut vdp, 9, 63, opaque);
            scan_active(&mut vdp);
            let status = vdp.read_status();
            assert_eq!(status & 0x40, 0x40, "{:?}, opaque={opaque}", vdp.variant());
            assert_eq!(status & 0x20, 0, "blanked sprites must not collide");
            assert!(vdp.framebuffer().iter().all(|&pixel| pixel == 0xff00_0000));
            assert_eq!(vdp.read_status() & 0x60, 0, "status read clears flags");
        }
    }
}

#[test]
fn blanking_does_not_invent_overflow_without_nine_selected_sprites() {
    for mut vdp in chips() {
        for (count, y) in [(8, 63), (9, 210), (9, 0xd0)] {
            setup(&mut vdp, count, y, true);
            scan_active(&mut vdp);
            assert_eq!(vdp.read_status() & 0x60, 0, "count={count}, y={y}");
        }
    }
}

#[test]
fn enabled_sprites_still_collide_and_overflow() {
    for mut vdp in chips() {
        setup(&mut vdp, 9, 63, true);
        register(&mut vdp, 1, 0x40);
        scan_active(&mut vdp);
        assert_eq!(vdp.read_status() & 0x60, 0x60);
    }
}
