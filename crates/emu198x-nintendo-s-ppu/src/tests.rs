//! Behaviour the ares oracle does not reach: the pins, the ARGB output,
//! the direct accessors, snapshots, and a few register paths it never
//! scripts. The rendering itself is checked against ares in
//! `tests/ares_oracle.rs`.

use super::*;

/// Run until the PPU reaches line `v`, clock `h` (a multiple of 4).
fn run_to(ppu: &mut SPpu, v: u32, h: u32) {
    while ppu.vcounter() != v || ppu.hcounter() != h {
        ppu.tick();
    }
}

#[test]
fn a_line_is_341_dots_and_a_frame_262_lines() {
    let mut ppu = SPpu::new(Region::Ntsc);
    assert_eq!(ppu.region(), Region::Ntsc);
    let mut dots = 0;
    loop {
        ppu.tick();
        dots += 1;
        if ppu.hcounter() == 0 {
            break;
        }
    }
    assert_eq!(dots, 341);
    assert_eq!(ppu.vcounter(), 1);
    run_to(&mut ppu, 0, 0);
    assert!(ppu.field(), "the field toggles each frame");
    assert_eq!(ppu.frames(), 1);
    assert!(ppu.take_frame_ready());
    assert!(!ppu.take_frame_ready(), "reading clears it");
}

#[test]
fn ntsc_odd_fields_shorten_line_240() {
    let mut ppu = SPpu::new(Region::Ntsc);
    ppu.tick();
    run_to(&mut ppu, 0, 0);
    assert!(ppu.field());
    run_to(&mut ppu, 240, 0);
    assert_eq!(ppu.hperiod(), 1360);
    // The latched H counter has no long dots on the short line.
    run_to(&mut ppu, 240, 1312);
    assert_eq!(ppu.hdot(), 328);
    run_to(&mut ppu, 241, 1312);
    assert_eq!(ppu.hperiod(), 1364);
    assert_eq!(ppu.hdot(), 327, "dots 323 and 327 are 6 clocks long");
}

#[test]
fn pal_interlaced_frames_add_a_line_and_lengthen_one() {
    let mut ppu = SPpu::new(Region::Pal);
    ppu.write(0x33, 0x01);
    let mut lines = 0;
    let mut long_line = false;
    run_to(&mut ppu, 0, 4);
    loop {
        ppu.tick();
        long_line |= ppu.hperiod() == 1368;
        if ppu.hcounter() == 0 {
            lines += 1;
            if ppu.vcounter() == 0 {
                break;
            }
        }
    }
    assert_eq!(lines, 313, "the first field is even, so one line longer");
    assert!(ppu.interlaced());
    run_to(&mut ppu, 311, 0);
    assert!(ppu.field());
    assert_eq!(ppu.hperiod(), 1368);
    assert!(long_line || ppu.hperiod() == 1368);
}

#[test]
fn the_blanking_pins_follow_hvbjoy() {
    let mut ppu = SPpu::new(Region::Ntsc);
    assert!(ppu.hblank(), "H=0 is in H-blank");
    run_to(&mut ppu, 10, 400);
    assert!(!ppu.hblank());
    assert!(!ppu.vblank());
    run_to(&mut ppu, 10, 1096);
    assert!(ppu.hblank());
    run_to(&mut ppu, 225, 0);
    assert!(ppu.vblank());
    assert_eq!(ppu.vdisp(), 225);
    ppu.write(0x33, 0x04);
    assert_eq!(ppu.vdisp(), 240, "overscan moves V-blank");
    assert!(!ppu.vblank());
}

#[test]
fn the_latch_pin_latches_on_a_falling_edge_only() {
    let mut ppu = SPpu::new(Region::Ntsc);
    run_to(&mut ppu, 20, 400);
    ppu.set_ext_latch(true);
    assert_eq!(ppu.read(0x3f, 0) & 0x40, 0, "no edge, no latch");
    ppu.set_ext_latch(false);
    assert_eq!(ppu.read(0x3c, 0), 100);
    assert_eq!(ppu.read(0x3d, 0), 20);
    assert_eq!(
        ppu.read(0x3f, 0) & 0x40,
        0x40,
        "the pin is low, so always set"
    );
    // $2137 latches only while the pin is high.
    run_to(&mut ppu, 30, 400);
    assert_eq!(ppu.read(0x37, 0xab), 0xab, "returns the CPU's open bus");
    assert_eq!(ppu.read(0x3d, 0), 20);
    ppu.set_ext_latch(true);
    ppu.read(0x37, 0);
    ppu.read(0x3f, 0); // resets the low/high flip-flops
    assert_eq!(ppu.read(0x3d, 0), 30);
}

#[test]
fn stat78_reports_the_region() {
    let mut pal = SPpu::new(Region::Pal);
    assert_eq!(pal.read(0x3f, 0) & 0x1f, 0x13);
    let mut ntsc = SPpu::new(Region::Ntsc);
    assert_eq!(ntsc.read(0x3f, 0) & 0x1f, 0x03);
    assert_eq!(ntsc.read(0x3e, 0) & 0x0f, 0x01, "PPU1 version 1");
}

#[test]
fn argb_scales_with_brightness() {
    assert_eq!(level(15, 31), 255);
    assert_eq!(level(7, 31), 128);
    assert_eq!(level(0, 31), 4, "brightness 0 is dim, not black");
    assert_eq!(level(15, 0), 0);

    let mut ppu = SPpu::new(Region::Ntsc);
    ppu.poke_cgram(0, 0x7c1f);
    ppu.write(0x00, 0x0f);
    ppu.write(0x2c, 0x00);
    run_to(&mut ppu, 20, 0);
    let row = (19 + 8) * FRAME_WIDTH;
    assert_eq!(ppu.framebuffer()[row + 10], 0xFFFF_00FF, "magenta backdrop");
    assert_eq!(ppu.framebuffer_raw()[row + 10], 15 << 15 | 0x7c1f);
}

#[test]
fn the_accessors_read_and_poke_memory() {
    let mut ppu = SPpu::new(Region::Ntsc);
    ppu.poke_vram(0x8123, 0xbeef);
    assert_eq!(ppu.vram(0x0123), 0xbeef, "64 KB mirrors at 32K words");
    ppu.set_vram_128k(true);
    ppu.poke_vram(0x8123, 0x1234);
    assert_eq!(ppu.vram(0x8123), 0x1234);
    assert_eq!(ppu.vram(0x0123), 0xbeef);
    ppu.set_vram_128k(false);

    ppu.poke_cgram(5, 0xffff);
    assert_eq!(ppu.cgram(5), 0x7fff);

    ppu.poke_oam(0x000, 0x12);
    ppu.poke_oam(0x003, 0xff);
    ppu.poke_oam(0x200, 0x03);
    assert_eq!(ppu.oam(0x000), 0x12);
    assert_eq!(ppu.oam(0x003), 0xff);
    assert_eq!(ppu.oam(0x200), 0x03);
    assert_eq!(ppu.oam(0x220), 0x03, "the high table mirrors");
}

#[test]
fn ports_reach_memory_in_force_blank() {
    let mut ppu = SPpu::new(Region::Ntsc);
    assert!(ppu.force_blank());
    // VRAM, word increment after the high byte.
    ppu.write(0x15, 0x80);
    ppu.write(0x16, 0x00);
    ppu.write(0x17, 0x10);
    ppu.write(0x18, 0x34);
    ppu.write(0x19, 0x12);
    assert_eq!(ppu.vram(0x1000), 0x1234);
    ppu.write(0x16, 0x00);
    ppu.write(0x17, 0x10);
    assert_eq!(ppu.read(0x39, 0), 0x34);
    assert_eq!(ppu.read(0x3a, 0), 0x12);
    // Remapped addressing: mode 1 rotates the low 8 bits.
    ppu.write(0x15, 0x84);
    ppu.write(0x16, 0x21);
    ppu.write(0x17, 0x00);
    ppu.write(0x19, 0x56);
    assert_eq!(ppu.vram(0x0009) >> 8, 0x56, "$0021 maps to $0009");
    // CGRAM.
    ppu.write(0x21, 0x10);
    ppu.write(0x22, 0xff);
    ppu.write(0x22, 0xff);
    assert_eq!(ppu.cgram(0x10), 0x7fff);
    // OAM: the low table is written in pairs.
    ppu.write(0x02, 0x00);
    ppu.write(0x03, 0x00);
    ppu.write(0x04, 0x11);
    assert_eq!(ppu.oam(0), 0, "the first byte waits for its pair");
    ppu.write(0x04, 0x22);
    assert_eq!((ppu.oam(0), ppu.oam(1)), (0x11, 0x22));
    // Mode 7 writes go through the latch.
    ppu.write(0x1b, 0x00);
    ppu.write(0x1b, 0x01);
    ppu.write(0x1c, 0xff);
    ppu.write(0x1c, 0x02);
    assert_eq!(ppu.read(0x34, 0), 0x00);
    assert_eq!(ppu.read(0x35, 0), 0x02);
    assert_eq!(ppu.read(0x36, 0), 0x00);
    // A register no PPU drives returns the CPU's open bus.
    assert_eq!(ppu.read(0x00, 0x77), 0x77);
    // A write-only register PPU1 owns returns its last read.
    assert_eq!(ppu.read(0x04, 0x77), 0x00);
}

#[test]
fn a_snapshot_resumes_identically() {
    let mut ppu = SPpu::new(Region::Ntsc);
    ppu.poke_cgram(0, 0x1234);
    ppu.write(0x00, 0x0f);
    run_to(&mut ppu, 100, 500);
    let json = serde_json::to_string(&ppu).expect("serialise");
    let mut copy: SPpu = serde_json::from_str(&json).expect("restore");
    for _ in 0..5000 {
        ppu.tick();
        copy.tick();
    }
    assert_eq!(ppu.framebuffer_raw(), copy.framebuffer_raw());
    assert_eq!(ppu.vcounter(), copy.vcounter());
}
