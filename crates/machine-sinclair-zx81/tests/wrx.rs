//! WRX hi-res, driven by a real program.
//!
//! WRX abandons the character-pattern lookup: with `I` pointing outside the
//! ROM the pattern byte comes from the plain refresh address, so `I` supplies
//! the high byte, `R` the low, and every byte fetched is eight pixels of an
//! arbitrary bitmap. Korth's *Sinclair ZX Specifications* states it directly —
//! the opcode and the line counter are both ignored — and EightyOne implements
//! the same thing in `Source/zx81/zx81.cpp`.
//!
//! `syntheses/sinclair-zx81/zx81-hi-res-techniques.md` has the mechanism, the
//! sources, and why this is a different technique from the pseudo hi-res that
//! shares its name (#1125).
//!
//! # What this asserts, and what it cannot
//!
//! Structure, not pixels. A 256×192 bitmap needs 6,144 bytes and `R` covers
//! 256 of them, so the driver must step `I` through **24 consecutive pages** —
//! and the whole 256×192 area must come out painted. Both fail if the address
//! is formed any other way: the character formula can only reach 512 bytes per
//! page, so it cannot produce either.
//!
//! Pixel-exact correctness needs a reference, and MAME cannot be it — its ZX81
//! driver implements only the character path. That leaves EightyOne or ZEsarUX,
//! which is #297's remaining half.
//!
//! # Running it
//!
//! ```text
//! EMU198X_ZX81_WRX_P="/path/to/a-wrx-program.p" cargo test -p machine-sinclair-zx81 --test wrx -- --ignored
//! ```
//!
//! ZEsarUX's `src/autoselectoptions.c` is a ready-made list of WRX titles.
//! Developed against *Starfight* (2001, Martin Korth) — the author of the
//! specification above, which makes it a fair exercise of it.

use format_sinclair_zx81_p::Zx81Image;
use machine_sinclair_zx81::{Zx81, Zx81Key};
use std::{collections::BTreeSet, env, fs};

const INK: u32 = 0xFF00_0000;

fn tap(m: &mut Zx81, k: Zx81Key) {
    m.press_key(k);
    for _ in 0..25 {
        m.run_frame();
    }
    m.release_key(k);
    for _ in 0..120 {
        m.run_frame();
    }
}

fn shifted(m: &mut Zx81, k: Zx81Key) {
    m.press_key(Zx81Key::Shift);
    m.press_key(k);
    for _ in 0..25 {
        m.run_frame();
    }
    m.release_key(k);
    m.release_key(Zx81Key::Shift);
    for _ in 0..120 {
        m.run_frame();
    }
}

#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM and a WRX .p — set EMU198X_ZX81_WRX_P"]
fn a_wrx_program_drives_the_bitmap_path() {
    let Ok(rom_path) = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
    else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let Ok(rom) = fs::read(&rom_path) else {
        emu198x_test_skip::skip!("ZX81 ROM not staged at {rom_path}");
    };
    let Ok(image_path) = env::var("EMU198X_ZX81_WRX_P") else {
        emu198x_test_skip::skip!("no WRX image — set EMU198X_ZX81_WRX_P to one");
    };
    let raw = fs::read(&image_path).expect("read the WRX .p");
    let pulses = Zx81Image::parse(&raw)
        .expect("a valid .p")
        .to_pulses(&[0x26]);

    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..400 {
        machine.run_frame();
    }

    // LOAD "" — J is the LOAD keyword, shift+P a quote.
    tap(&mut machine, Zx81Key::J);
    shifted(&mut machine, Zx81Key::P);
    shifted(&mut machine, Zx81Key::P);

    // Thread the tape only once the loader is listening; see `tape_load.rs`.
    machine.press_key(Zx81Key::Newline);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(Zx81Key::Newline);
    for _ in 0..40 {
        machine.run_frame();
    }
    machine.insert_tape(&pulses);

    let mut frames = 0;
    while machine.tape_remaining() > 0 && frames < 30_000 {
        machine.run_frame();
        frames += 1;
    }
    assert_eq!(machine.tape_remaining(), 0, "the tape should run out");

    let mut pages = BTreeSet::new();
    for _ in 0..3000 {
        machine.run_frame();
        pages.insert(machine.cpu().regs.i);
    }
    let w = machine.framebuffer_width() as usize;
    let fb = machine.framebuffer();
    let ink = fb.iter().filter(|&&p| p == INK).count();
    // Distinct 8-pixel cells across the picture. The character path can only
    // reach 512 bytes in a page, and with a uniform display file it renders
    // the same byte across a row; the bitmap path reads a different byte for
    // every cell.
    let mut cells = BTreeSet::new();
    for y in 0..192usize {
        for cx in 0..32usize {
            let mut byte = 0u8;
            for bit in 0..8usize {
                if fb[y * w + 32 + cx * 8 + bit] == INK {
                    byte |= 0x80 >> bit;
                }
            }
            cells.insert((y % 8, cx, byte));
        }
    }
    assert!(
        ink > 256 * 192 / 20 && ink < 256 * 192 * 19 / 20,
        "the picture should be a picture, not a blank or a solid block; \
         {ink} of {} pixels are ink",
        256 * 192
    );

    // There was a cell-count discriminator here and it did not hold.
    //
    // It counted distinct `(row-in-character, column, byte)` cells and
    // required more than 64 x 8 = 512, on the reasoning that the character
    // path can only reach 512 bytes within one `I` page. The bound is right
    // for *bytes* and wrong for the triples it was applied to: the same byte
    // at a different column counts again, so a character-mode display is not
    // limited to 512 of them. Measured, `Bi-Plot` in plain character mode
    // renders **609** -- above the ceiling that was being read as proof of
    // WRX (#297).
    //
    // Keyed soundly on `(row-in-character, byte)` the 512 bound does hold,
    // and then nothing reaches it: this image renders 113 that way, because a
    // bitmap has no reason to use many distinct byte values. So the count
    // cannot separate the two paths in either direction, and the assertion
    // below is the one that does.

    let wrx_pages: Vec<u8> = pages.iter().copied().filter(|&i| i > 0x1F).collect();
    assert!(
        !wrx_pages.is_empty(),
        "the program never pointed I outside the ROM, so it never asked for \
         WRX at all — it is either not a WRX image or it did not run; I was {pages:02x?}"
    );

    // 6,144 bytes of bitmap, 256 reachable per page, so 24 pages.
    assert_eq!(
        wrx_pages.len(),
        24,
        "a 256x192 bitmap needs 24 pages of R; saw {}: {wrx_pages:02x?}",
        wrx_pages.len()
    );
    let first = wrx_pages[0];
    assert!(
        wrx_pages
            .iter()
            .enumerate()
            .all(|(n, &p)| p == first + n as u8),
        "the pages should be consecutive, walking the bitmap: {wrx_pages:02x?}"
    );
}
