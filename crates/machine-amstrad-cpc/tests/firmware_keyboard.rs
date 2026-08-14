//! The real firmware accepts typed keys and executes what was typed.
//!
//! Gated `#[ignore]` for the same reason as `firmware_boot`: the 32 KB CPC464
//! firmware is copyrighted and lives at `~/.emu198x/roms/amstrad-cpc/cpc464.rom`.
//!
//! ```text
//! cargo test --release -p machine-amstrad-cpc --test firmware_keyboard -- --ignored
//! ```
//!
//! `POKE` is the assertion rather than the screen. The CPC has no text buffer to
//! read back — its display is a bitmap — so checking that typing worked by
//! looking for glyphs would mean rendering the firmware's own font and matching
//! pixels. Poking a byte into RAM and reading it back proves the same thing
//! through a narrower door, and proves rather more besides: the matrix, the PPI
//! row select, the AY port A read, the firmware's 50 Hz key scan, its debounce,
//! the BASIC line editor, and the interpreter all have to work for that byte to
//! arrive.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_amstrad_cpc::AmstradCpc;

/// Somewhere in RAM an empty BASIC program will not touch. Below the screen at
/// `$C000` and well above BASIC's own workspace at the bottom of memory.
const TARGET: u16 = 0x4000;
const VALUE: u8 = 0xA5;

fn firmware_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_CPC_ROM") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/amstrad-cpc/cpc464.rom");
    p.exists().then_some(p)
}

/// Type one character, holding it long enough for the firmware's key scan to
/// see it and then releasing it long enough to register as a separate press.
/// The scan runs off the 50 Hz interrupt, so this is counted in frames.
fn type_char(cpc: &mut AmstradCpc, c: char) {
    assert!(cpc.press_char(c), "no key produces {c:?}");
    for _ in 0..4 {
        cpc.run_frame();
    }
    cpc.release_char(c);
    for _ in 0..4 {
        cpc.run_frame();
    }
}

#[test]
#[ignore = "needs the 32 KB CPC464 firmware — run with --ignored"]
fn typing_a_poke_at_the_basic_prompt_writes_to_memory() {
    let Some(path) = firmware_path() else {
        panic!(
            "CPC464 firmware not found — set EMU198X_CPC_ROM or place cpc464.rom \
             (16 KB OS + 16 KB BASIC) at ~/.emu198x/roms/amstrad-cpc/"
        );
    };
    let firmware = fs::read(&path).expect("read firmware");
    let mut cpc = AmstradCpc::new(&firmware).expect("build machine");

    // Reach the `Ready` prompt.
    for _ in 0..150 {
        cpc.run_frame();
    }
    assert_ne!(
        cpc.peek(TARGET),
        VALUE,
        "the target byte must start out different, or the test proves nothing"
    );

    // Decimal rather than `&4000`: Locomotive BASIC takes lower case keywords,
    // so this needs no Shift at all and exercises the plain matrix path.
    for c in format!("poke {},{}\r", TARGET, VALUE).chars() {
        type_char(&mut cpc, c);
    }

    // Let BASIC parse and run the line.
    for _ in 0..30 {
        cpc.run_frame();
    }

    assert_eq!(
        cpc.peek(TARGET),
        VALUE,
        "the typed POKE should have written ${VALUE:02X} to ${TARGET:04X}"
    );
}
