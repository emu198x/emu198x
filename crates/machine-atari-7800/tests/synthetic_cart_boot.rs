//! Fixture-free proof that the Atari 7800 boots.
//!
//! The 7800 declares no firmware, so a cartridge is all it needs and the
//! claim can be checked on every push.
//!
//! No display list is involved. With MARIA's DMA off — the power-on state
//! — the active picture is filled with `BACKGRND`. CTRL.BC is also clear,
//! so the side borders stay black. Driving a display list would test the DMA engine,
//! which is a different claim from "this machine starts".
//!
//! MARIA shares the TIA's colour encoding and palette, so the shade here
//! is the same one the 2600 test expects. The chips differ; the claim
//! does not.

use std::path::PathBuf;

use machine_atari_7800::{Atari7800, Atari7800Region};

/// NTSC palette entry `$0E`, selected by writing `$1C` to `BACKGRND`.
const EXPECTED: u32 = 0xFFD4_D478;

fn cart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/atari/synthetic-cart/atari-7800.a78")
}

fn booted(rom: Vec<u8>) -> Atari7800 {
    let mut machine = Atari7800::new(rom, Atari7800Region::Ntsc)
        .expect("the synthetic cartridge should load as a 16 KB image");
    for _ in 0..5 {
        machine.run_frame();
    }
    machine
}

fn has_expected_background(machine: &Atari7800) -> bool {
    let region = atari_maria::MariaRegion::Ntsc;
    let width = region.framebuffer_width() as usize;
    let left = region.border_left() as usize;
    let right = left + atari_maria::ACTIVE_WIDTH as usize;
    let framebuffer = machine.framebuffer();
    framebuffer.len() == width * region.framebuffer_height() as usize
        && framebuffer.chunks_exact(width).all(|row| {
            row[..left].iter().all(|&pixel| pixel == 0xFF00_0000)
                && row[left..right].iter().all(|&pixel| pixel == EXPECTED)
                && row[right..].iter().all(|&pixel| pixel == 0xFF00_0000)
        })
}

#[test]
fn the_atari_7800_boots_a_cartridge_and_paints_its_background() {
    let rom = std::fs::read(cart())
        .unwrap_or_else(|err| panic!("synthetic cartridge should be committed: {err}"));
    let machine = booted(rom);

    assert!(
        has_expected_background(&machine),
        "the cartridge should paint BACKGRND across the active picture, with black side borders"
    );
}

/// The check is only worth having if it can fail.
#[test]
fn a_cartridge_that_writes_no_colour_does_not_look_like_a_boot() {
    let mut rom = std::fs::read(cart()).expect("cartridge should be committed");
    // Spin immediately, before BACKGRND is ever written.
    rom[0] = 0x4C;
    rom[1] = 0x00;
    rom[2] = 0xC0;
    let machine = booted(rom);
    assert!(
        !has_expected_background(&machine),
        "a cartridge that writes no colour must not pass the boot assertion"
    );
}
