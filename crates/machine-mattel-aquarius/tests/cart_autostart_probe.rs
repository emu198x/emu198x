//! Aquarius cartridge boot regression tests.
//!
//! Two fixes are needed before a game cart boots, and one test guards each:
//!
//! 1. `cart_detect_reaches_cart_entry` — the BIOS cart-detect at $005C must
//!    reach the cart-valid branch ($007F → JP $E010), not fall to BASIC
//!    ($0089). It regressed under a fictitious per-frame NMI: the base Aquarius
//!    wires no periodic interrupt (per MAME `aquarius.cpp`, IRQ/NMI come only
//!    from the expansion port), and the Z80 NMI vector $0066 sits *inside* the
//!    detect loop, so a stray NMI corrupted the compare.
//!
//! 2. `cart_descrambles_and_renders` — once running, the cart's ROM is read
//!    through the Aquarius "software lock": the BIOS derives an 8-bit pattern
//!    from the cart checksum, writes it to port $FF, and every external-bus
//!    byte ($4000-$FFFF) is XORed with it. Without that XOR the cart entry
//!    ($E010) reads as garbage, the CPU reboots, and the screen stays blank.
//!    With it, the title screen draws.
//!
//! Gated `#[ignore]`: needs the BIOS, character ROM and a cart. Run with
//! EMU198X_AQUARIUS_BIOS / _CHAR / _CART set:
//!   cargo test --release -p machine-mattel-aquarius --test cart_autostart_probe \
//!     -- --ignored --nocapture

use std::env;
use std::fs;

use emu198x_zilog_z80::Z80Stepper;
use machine_mattel_aquarius::{Aquarius, AquariusRegion};

fn env_file(var: &str) -> Vec<u8> {
    let path = env::var(var).unwrap_or_else(|_| panic!("set {var}"));
    fs::read(&path).unwrap_or_else(|_| panic!("read {var} ({path})"))
}

fn cart_machine() -> Aquarius {
    let mut sys = Aquarius::new(env_file("EMU198X_AQUARIUS_BIOS"), 0, AquariusRegion::Ntsc);
    sys.set_char_rom(env_file("EMU198X_AQUARIUS_CHAR"));
    sys.insert_cart(env_file("EMU198X_AQUARIUS_CART"));
    sys
}

#[test]
#[ignore = "FIXTURE: needs Aquarius BIOS + char + cart"]
fn cart_detect_reaches_cart_entry() {
    let mut sys = cart_machine();

    // The boot beep (BEL printed at $004E) burns ~1.5M t-states before the
    // cart-detect at $005C runs, so give it a generous budget to get there.
    assert!(
        sys.run_until_pc(0x005C, 4_000_000),
        "boot never reached the cart-detect routine ($005C)"
    );

    // A valid cart reaches $007F (JP $E010); no cart falls to $0089 (BASIC).
    let mut verdict = None;
    for _ in 0..200 {
        match sys.cpu().regs.pc {
            0x007F => {
                verdict = Some(true);
                break;
            }
            0x0089 => {
                verdict = Some(false);
                break;
            }
            _ => sys.step_instruction(),
        };
    }
    assert_eq!(
        verdict,
        Some(true),
        "cart-detect should reach the cart-valid path ($007F → JP $E010), not BASIC ($0089)"
    );
}

#[test]
#[ignore = "FIXTURE: needs Aquarius BIOS + char + cart"]
fn cart_descrambles_and_renders() {
    let mut sys = cart_machine();
    // Boot past the beep, cart entry and title-draw. A descrambled cart writes
    // its title into the 40x24 char RAM at $3000; a non-descrambled one reboots
    // and leaves the screen all spaces.
    for _ in 0..300 {
        sys.run_frame();
    }
    let nonspace = (0..40 * 24u16)
        .filter(|&i| {
            let c = sys.peek(0x3000 + i);
            c != 0x20 && c != 0x00
        })
        .count();
    assert!(
        nonspace > 0,
        "cart drew nothing — screen RAM is blank (scrambler not applied?)"
    );
    println!("cart drew {nonspace} non-blank cells");
}
