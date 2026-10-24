//! The real C128 ROMs boot to BASIC 7.0 on the 40-column screen.
//!
//! Gated `#[ignore]`: name a directory holding VICE's C128 ROMs with
//! EMU198X_C128_ROM_DIR (`kernal-318020-05.bin`, `basiclo-318018-04.bin`,
//! `basichi-318019-04.bin` and `chargen-390059-01.bin`, from VICE's
//! `data/C128`).
//!
//! ```text
//! EMU198X_C128_ROM_DIR=... cargo test --release -p machine-commodore-c128 --test boot -- --ignored
//! ```

use std::path::PathBuf;

use machine_commodore_c128::{C128, C128Roms};

fn c128() -> C128 {
    let dir =
        PathBuf::from(std::env::var("EMU198X_C128_ROM_DIR").expect("set EMU198X_C128_ROM_DIR"));
    let read =
        |name: &str| std::fs::read(dir.join(name)).unwrap_or_else(|err| panic!("{name}: {err}"));
    let (kernal, basic_lo, basic_hi, chargen) = (
        read("kernal-318020-05.bin"),
        read("basiclo-318018-04.bin"),
        read("basichi-318019-04.bin"),
        read("chargen-390059-01.bin"),
    );
    C128::new(&C128Roms {
        kernal: &kernal,
        basic_lo: &basic_lo,
        basic_hi: &basic_hi,
        chargen: &chargen,
    })
    .expect("ROMs")
}

fn screen(c128: &C128) -> String {
    c128.screen_text().join("\n")
}

#[test]
#[ignore = "FIXTURE: needs the C128 ROMs — run with --ignored"]
fn the_c128_boots_to_basic_7() {
    let mut c128 = c128();
    let mut ready = false;
    for _ in 0..400 {
        c128.run_frame();
        if screen(&c128).contains("READY.") {
            ready = true;
            break;
        }
    }
    println!("{}", screen(&c128));
    assert!(
        ready,
        "no READY. after 400 frames; 8502 running: {}, PC {:04X}, Z80 PC {:04X}",
        c128.running_8502(),
        c128.cpu().regs.pc,
        c128.z80().regs.pc
    );
    assert!(screen(&c128).contains("BASIC V7.0"), "the BASIC 7.0 banner");
}

/// Type `text` on the C64 part of the keyboard, a key at a time.
fn type_text(c128: &mut C128, text: &str) {
    for c in text.chars() {
        let (row, col) = match c {
            'P' => (5, 1),
            'R' => (2, 1),
            'I' => (4, 1),
            'N' => (4, 7),
            'T' => (2, 6),
            ' ' => (7, 4),
            '2' => (7, 3),
            '+' => (5, 0),
            '\r' => (0, 1),
            other => panic!("no key for {other:?} in this test's table"),
        };
        c128.keyboard_mut().set_key(row, col, true);
        for _ in 0..3 {
            c128.run_frame();
        }
        c128.keyboard_mut().set_key(row, col, false);
        for _ in 0..3 {
            c128.run_frame();
        }
    }
}

#[test]
#[ignore = "FIXTURE: needs the C128 ROMs — run with --ignored"]
fn basic_7_reads_the_keyboard_and_the_vic_draws_it() {
    let mut c128 = c128();
    for _ in 0..200 {
        c128.run_frame();
        if screen(&c128).contains("READY.") {
            break;
        }
    }
    type_text(&mut c128, "PRINT 2+2\r");
    for _ in 0..20 {
        c128.run_frame();
    }
    let text = screen(&c128);
    println!("{text}");
    assert!(
        text.contains("PRINT 2+2"),
        "the keys reached the screen editor"
    );
    assert!(
        text.lines().any(|line| line.trim() == "4"),
        "BASIC printed 4"
    );
    // The VIC draws the text. The C128 writes it in the border's light green
    // on dark grey, so look inside the display window, lines 51-250 of the
    // PAL frame, for both colours.
    let frame = c128.framebuffer();
    let width = frame.len() / 312;
    let window = frame
        .chunks(width)
        .skip(60)
        .take(180)
        .flat_map(|line| &line[width / 2 - 150..width / 2 + 150]);
    let paper = frame[100 * width + width / 2];
    let ink = window.clone().filter(|&&pixel| pixel != paper).count();
    assert!(ink > 500, "only {ink} text pixels in the display window");
}
