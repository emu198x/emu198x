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
//! Gated `#[ignore]`: needs the BIOS, character ROM and a cart. Each is taken
//! from its env var if set, otherwise from `~/.emu198x/roms/mattel-aquarius/`
//! — the same two-tier lookup the other Aquarius fixture tests use, so this
//! runs in an ordinary `--ignored` sweep instead of demanding three variables
//! nothing else sets.
//!
//!   cargo test --release -p machine-mattel-aquarius --test cart_autostart_probe \
//!     -- --ignored --nocapture

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_zilog_z80::Z80Stepper;
use machine_mattel_aquarius::{Aquarius, AquariusRegion};

fn rom_dir() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/mattel-aquarius"))
}

/// The env var if it names a real file, else `name` in the shared ROM
/// directory.
fn rom_path(var: &str, name: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var(var) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let p = rom_dir()?.join(name);
    p.exists().then_some(p)
}

/// Carts carry their published title rather than a fixed filename, so fall
/// back to the `.bin` files in the ROM directory — the BIOS and character ROM
/// are `.rom`, which keeps the two apart. Sorted, so a directory holding
/// several picks the same one every run.
fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_AQUARIUS_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let mut hits: Vec<PathBuf> = fs::read_dir(rom_dir()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin")))
        .collect();
    hits.sort();
    hits.into_iter().next()
}

fn read(path: Option<PathBuf>, what: &str) -> Vec<u8> {
    let path = path.unwrap_or_else(|| {
        panic!("Aquarius {what} not found — see this file's header for where it is looked for")
    });
    fs::read(&path).unwrap_or_else(|e| panic!("read {} ({e})", path.display()))
}

fn cart_machine() -> Aquarius {
    let bios = read(rom_path("EMU198X_AQUARIUS_BIOS", "aquarius.rom"), "BIOS");
    let mut sys = Aquarius::new(bios, 0, AquariusRegion::Ntsc);
    sys.set_char_rom(read(
        rom_path("EMU198X_AQUARIUS_CHAR", "aquarius-char.rom"),
        "character ROM",
    ));
    sys.insert_cart(read(cart_path(), "cart"));
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
