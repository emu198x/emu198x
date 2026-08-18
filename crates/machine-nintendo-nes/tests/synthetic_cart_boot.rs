//! Fixture-free proof that the NES boots.
//!
//! The NES declares no firmware, so a cartridge is all it needs and the
//! claim can be checked on every push. Its other boot tests are
//! `#[ignore]`d because they want blargg's ROMs.
//!
//! The PPU powers up with palette index `$09` at `$3F00`. The cartridge
//! writes `$30` there and enables the background, so the screen can only
//! be near-white if the machine executed it.

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

/// PPU palette entry `$30`.
const EXPECTED: u32 = 0xFFFF_FEFF;

fn cart() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/nintendo/synthetic-cart/nes.nes")
}

fn booted(bytes: &[u8]) -> Nes {
    let parsed = parse_ines(bytes).expect("the synthetic cartridge should parse as iNES");
    let mut machine = Nes::new(parsed.mapper);
    for _ in 0..5 {
        machine.run_frame();
    }
    machine
}

fn uniform(machine: &Nes) -> Option<u32> {
    let first = *machine.framebuffer().first()?;
    machine
        .framebuffer()
        .iter()
        .all(|&pixel| pixel == first)
        .then_some(first)
}

#[test]
fn the_nes_boots_a_cartridge_and_paints_its_backdrop() {
    let bytes = std::fs::read(cart())
        .unwrap_or_else(|err| panic!("synthetic cartridge should be committed: {err}"));
    let machine = booted(&bytes);

    assert_eq!(
        uniform(&machine),
        Some(EXPECTED),
        "the cartridge should have written palette $30 to $3F00; \
         the power-on colour means it never ran"
    );
}

/// The check is only worth having if it can fail.
#[test]
fn a_cartridge_that_writes_no_palette_does_not_look_like_a_boot() {
    let mut bytes = std::fs::read(cart()).expect("cartridge should be committed");
    // Replace the program with `jmp $C000` — a machine that starts, spins,
    // and touches nothing.
    bytes[16] = 0x4C;
    bytes[17] = 0x00;
    bytes[18] = 0xC0;
    let machine = booted(&bytes);
    assert_ne!(
        uniform(&machine),
        Some(EXPECTED),
        "a cartridge that writes no palette must not pass the boot assertion"
    );
}
