//! Fixture-free proof that the Atari 7800 boots.
//!
//! The 7800 declares no firmware, so a cartridge is all it needs and the
//! claim can be checked on every push.
//!
//! No display list is involved. With MARIA's DMA off — the power-on state
//! — every line is filled with `BACKGRND`, so writing that register is a
//! complete picture. Driving a display list would test the DMA engine,
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

fn uniform(machine: &Atari7800) -> Option<u32> {
    let first = *machine.framebuffer().first()?;
    machine
        .framebuffer()
        .iter()
        .all(|&pixel| pixel == first)
        .then_some(first)
}

#[test]
fn the_atari_7800_boots_a_cartridge_and_paints_its_background() {
    let rom = std::fs::read(cart())
        .unwrap_or_else(|err| panic!("synthetic cartridge should be committed: {err}"));
    let machine = booted(rom);

    assert_eq!(
        uniform(&machine),
        Some(EXPECTED),
        "the cartridge should have written BACKGRND; black means it never ran"
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
    assert_ne!(
        uniform(&machine),
        Some(EXPECTED),
        "a cartridge that writes no colour must not pass the boot assertion"
    );
}
