//! Fixture-free proof that the Atari 2600 boots.
//!
//! The 2600 declares no firmware, so a cartridge is all it needs and the
//! claim can be checked on every push.
//!
//! Unlike the other machines here the 2600 has no framebuffer of its own:
//! the TIA paints whatever its colour registers hold as the beam sweeps,
//! so a program that writes `COLUBK` and spins is complete — hardware
//! paints every scanline after it. The TIA powers up with its colour
//! registers at zero, which is black, so a bright picture can only mean
//! the cartridge ran.
//!
//! The picture is not uniform: the TIA renders the 68-clock horizontal
//! blank as black on every line. The visible window is what gets checked.

use std::path::PathBuf;

use machine_atari_2600::{Atari2600, Atari2600Region};

/// NTSC palette entry `$0E`, selected by writing `$1C` to `COLUBK`.
const EXPECTED: u32 = 0xFFD4_D478;

fn cart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/atari/synthetic-cart/atari-2600.a26")
}

fn booted(rom: Vec<u8>) -> Atari2600 {
    let mut machine = Atari2600::new(rom, Atari2600Region::Ntsc)
        .expect("the synthetic cartridge should load as a 4 KB image");
    for _ in 0..5 {
        machine.run_frame();
    }
    machine
}

/// The single colour filling the visible window, if it is uniform.
fn visible_colour(machine: &Atari2600) -> Option<u32> {
    let stride = machine.framebuffer_width() as usize;
    let left = machine.hblank_clocks() as usize;
    let width = machine.visible_framebuffer_width() as usize;
    let first = machine.visible_first_line() as usize;
    let height = machine.visible_framebuffer_height() as usize;
    let framebuffer = machine.framebuffer();

    let mut seen = None;
    for row in first..(first + height).min(machine.framebuffer_height() as usize) {
        for column in left..left + width {
            let pixel = framebuffer[row * stride + column];
            match seen {
                None => seen = Some(pixel),
                Some(previous) if previous == pixel => {}
                Some(_) => return None,
            }
        }
    }
    seen
}

#[test]
fn the_atari_2600_boots_a_cartridge_and_paints_its_background() {
    let rom = std::fs::read(cart())
        .unwrap_or_else(|err| panic!("synthetic cartridge should be committed: {err}"));
    let machine = booted(rom);

    assert_eq!(
        visible_colour(&machine),
        Some(EXPECTED),
        "the cartridge should have written COLUBK; black means it never ran"
    );
}

/// The check is only worth having if it can fail.
#[test]
fn a_cartridge_that_writes_no_colour_does_not_look_like_a_boot() {
    let mut rom = std::fs::read(cart()).expect("cartridge should be committed");
    // Spin immediately, before COLUBK is ever written.
    rom[0] = 0x4C;
    rom[1] = 0x00;
    rom[2] = 0xF0;
    let machine = booted(rom);
    assert_ne!(
        visible_colour(&machine),
        Some(EXPECTED),
        "a cartridge that writes no colour must not pass the boot assertion"
    );
}
