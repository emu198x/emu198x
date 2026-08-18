//! Fixture-free proof that the Game Boy boots.
//!
//! The Game Boy declares no firmware, so a cartridge is all it needs and
//! the claim can be checked on every push.
//!
//! Its PPU emits indexed shades rather than colours, so the assertion is
//! on a shade: `BGP` powers up at zero, mapping every background index to
//! the lightest shade. The cartridge sets it to `$FF`, mapping every index
//! to the darkest. A machine that never ran the cartridge stays light.

use std::path::PathBuf;

use machine_nintendo_game_boy::GameBoy;

/// The darkest shade the DMG can show.
const DARKEST: u8 = 3;

fn cart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/nintendo/synthetic-cart/game-boy.gb")
}

fn booted(rom: Vec<u8>) -> GameBoy {
    let (_header, mut machine) =
        GameBoy::from_rom(rom).expect("the synthetic cartridge header should parse");
    for _ in 0..5 {
        machine.run_frame();
    }
    machine
}

fn uniform(machine: &GameBoy) -> Option<u8> {
    let first = *machine.framebuffer().first()?;
    machine
        .framebuffer()
        .iter()
        .all(|&pixel| pixel == first)
        .then_some(first)
}

#[test]
fn the_game_boy_boots_a_cartridge_and_sets_its_palette() {
    let rom = std::fs::read(cart())
        .unwrap_or_else(|err| panic!("synthetic cartridge should be committed: {err}"));
    let machine = booted(rom);

    assert_eq!(
        machine.framebuffer().len(),
        160 * 144,
        "the DMG displays 160x144"
    );
    assert_eq!(
        uniform(&machine),
        Some(DARKEST),
        "the cartridge should have set BGP to $FF; the lightest shade means \
         it never ran"
    );
}

/// The check is only worth having if it can fail.
#[test]
fn a_cartridge_that_sets_no_palette_does_not_look_like_a_boot() {
    let mut rom = std::fs::read(cart()).expect("cartridge should be committed");
    // Replace the entry point's jump with an immediate spin, so the machine
    // starts, runs, and touches nothing.
    rom[0x0101] = 0x18;
    rom[0x0102] = 0xFE;
    rom[0x0103] = 0x00;
    let machine = booted(rom);
    assert_ne!(
        uniform(&machine),
        Some(DARKEST),
        "a cartridge that sets no palette must not pass the boot assertion"
    );
}
