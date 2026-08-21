//! Fixture-free proof that the SG-1000 boots.
//!
//! Companion to the Master System and Game Gear tests in
//! `machine-sega-master-system`. The SG-1000 also declares no firmware, so
//! a cartridge is all it needs and the claim can be checked on every push.
//!
//! Its TMS9918 has no colour memory: the backdrop is an index into a
//! palette fixed in silicon, taken from the low nibble of register 7. The
//! power-on value selects entry 0, which is transparent; the cartridge
//! selects entry 15, which is white. A machine that never ran the
//! cartridge cannot produce white.

use std::path::PathBuf;

use machine_sega_sg_1000::{Sg1000, Sg1000Region};

/// TMS9918 palette entry 15.
const WHITE: u32 = 0xFFFF_FFFF;

fn cart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/sega/synthetic-cart/sg-1000.sg")
}

fn booted(rom: Vec<u8>) -> Sg1000 {
    let mut machine = Sg1000::new(rom, Sg1000Region::Ntsc);
    for _ in 0..5 {
        machine.run_frame();
    }
    machine
}

fn uniform(machine: &Sg1000) -> Option<u32> {
    let first = *machine.framebuffer().first()?;
    machine
        .framebuffer()
        .iter()
        .all(|&pixel| pixel == first)
        .then_some(first)
}

#[test]
fn the_sg_1000_boots_a_cartridge_and_paints_its_backdrop() {
    let rom = std::fs::read(cart())
        .unwrap_or_else(|err| panic!("synthetic cartridge should be committed: {err}"));
    let machine = booted(rom);

    // 280 x 240 is the NTSC window — 5.369318 MHz over 52.148 µs, and 240
    // lines. It read 288 while the horizontal border was a fixed 16 either
    // side of the active 256, which is 103% of what a set shows (#1054).
    assert_eq!(
        (machine.framebuffer_width(), machine.framebuffer_height()),
        (280, 240)
    );
    assert_eq!(
        uniform(&machine),
        Some(WHITE),
        "the cartridge should have selected palette entry 15; \
         transparent means it never ran"
    );
}

/// The check is only worth having if it can fail.
#[test]
fn an_empty_cartridge_does_not_look_like_a_boot() {
    let machine = booted(vec![0; 32 * 1024]);
    assert_ne!(
        uniform(&machine),
        Some(WHITE),
        "a cartridge that selects no backdrop must not pass the boot assertion"
    );
}
