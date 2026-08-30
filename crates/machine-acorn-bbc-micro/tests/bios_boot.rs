//! BBC Micro BIOS boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_bbc_micro::BbcMicro;

fn os_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_BBC_OS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/os.rom");
    p.exists().then_some(p)
}

fn basic_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_BBC_BASIC") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/basic.rom");
    p.exists().then_some(p)
}

fn font_path() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/saa5050.rom");
    p.exists().then_some(p)
}

fn tap(sys: &mut BbcMicro, col: usize, row: usize) {
    sys.press_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
}

fn mode7_text(sys: &BbcMicro) -> String {
    (0x7C00u16..0x8000)
        .map(|address| {
            let byte = sys.peek(address);
            if (0x20..0x7F).contains(&byte) {
                byte as char
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "FIXTURE: needs BBC Micro MOS + BASIC ROMs — run with --ignored"]
fn os_boots_to_basic_banner() {
    let Some(path) = os_path() else {
        panic!(
            "BBC MOS ROM not found — set EMU198X_BBC_OS or place os.rom \
             at ~/.emu198x/roms/acorn-bbc-micro/"
        );
    };
    let os = fs::read(&path).expect("read OS");
    assert_eq!(os.len(), 0x4000, "MOS ROM must be 16 KB");

    let mut sys = BbcMicro::new(os);
    let Some(basic) = basic_path() else {
        panic!(
            "BBC BASIC ROM not found — set EMU198X_BBC_BASIC or place basic.rom \
             at ~/.emu198x/roms/acorn-bbc-micro/"
        );
    };
    let basic = fs::read(&basic).expect("read BASIC");
    assert_eq!(basic.len(), 0x4000, "BASIC ROM must be 16 KB");
    sys.insert_rom(15, basic);

    for _ in 0..200 {
        sys.run_frame();
    }

    // Reaching the banner exercises the whole power-on path, including the
    // keyboard scan: the MOS drives a key code onto System VIA PA0-6 and reads
    // PA7 for each key. Until PA7 was wired to the key matrix it read a stuck
    // "key held", so the MOS never finished init, never ran CLI, and never
    // printed anything. A booted machine writes `BBC Computer 32K` into the
    // MODE 7 screen RAM at $7C00 (teletext alphanumerics are plain ASCII).
    let screen = mode7_text(&sys);
    assert!(
        screen.contains("BBC Computer"),
        "expected the BBC banner in MODE 7 screen RAM; got: {:?}",
        screen.trim()
    );
    assert!(
        sys.rom_bank() > 0,
        "OS should have selected a language ROM; got bank {}",
        sys.rom_bank()
    );
}

#[test]
#[ignore = "FIXTURE: needs BBC Micro MOS + BASIC ROMs — run with --ignored"]
fn boots_to_basic_prompt() {
    // Regression for the 6850 ACIA interrupt storm: open-bus $FE08 reads (0xFF,
    // status bit 7 set) made the MOS service a phantom serial interrupt every
    // IRQ and never clear the System VIA 100 Hz timer, starving BASIC before it
    // printed its `>` prompt. With the ACIA modelled, BASIC reaches the prompt.
    let (Some(os), Some(basic)) = (os_path(), basic_path()) else {
        panic!("needs os.rom + basic.rom at ~/.emu198x/roms/acorn-bbc-micro/");
    };
    let mut sys = BbcMicro::new(fs::read(&os).expect("read OS"));
    sys.insert_rom(15, fs::read(&basic).expect("read BASIC"));
    for _ in 0..200 {
        sys.run_frame();
    }
    let screen = mode7_text(&sys);
    assert!(
        screen.contains("BASIC"),
        "expected BASIC startup text; got: {:?}",
        screen.trim()
    );
    assert!(
        screen.contains('>'),
        "expected the BASIC `>` prompt (ACIA storm regression); got: {:?}",
        screen.trim()
    );
}

#[test]
#[ignore = "FIXTURE: needs BBC Micro MOS + BASIC ROMs — run with --ignored"]
fn keyboard_types_a_basic_expression_and_prints_the_result() {
    let (Some(os), Some(basic)) = (os_path(), basic_path()) else {
        panic!("needs os.rom + basic.rom at ~/.emu198x/roms/acorn-bbc-micro/");
    };
    let mut sys = BbcMicro::new(fs::read(&os).expect("read OS"));
    sys.insert_rom(15, fs::read(&basic).expect("read BASIC"));
    for _ in 0..200 {
        sys.run_frame();
    }

    // Type `PRINT 9/3` through the physical 10×8 keyboard matrix. All of its
    // characters are unshifted, keeping this test focused on the MOS scan and
    // debounce path rather than symbol/modifier mapping.
    for (col, row) in [
        (7, 3), // P
        (3, 3), // R
        (5, 2), // I
        (5, 5), // N
        (3, 2), // T
        (2, 6), // SPACE
        (6, 2), // 9
        (8, 6), // /
        (1, 1), // 3
        (9, 4), // RETURN
    ] {
        tap(&mut sys, col, row);
    }
    for _ in 0..20 {
        sys.run_frame();
    }

    let screen = mode7_text(&sys);
    assert!(
        screen.contains("PRINT 9/3"),
        "expected the typed expression to echo; got: {screen:?}"
    );
    assert!(
        screen.contains("PRINT 9/3 3 >"),
        "expected BASIC to evaluate 9/3 and return to the prompt; got: {screen:?}"
    );
}

#[test]
#[ignore = "FIXTURE: needs BBC MOS + BASIC + SAA5050 ROMs — run with --ignored"]
fn mode7_renders_the_banner() {
    let (Some(os), Some(basic), Some(font)) = (os_path(), basic_path(), font_path()) else {
        panic!(
            "needs os.rom + basic.rom + saa5050.rom at ~/.emu198x/roms/acorn-bbc-micro/ \
             (saa5050.rom is the 960-byte SAA5050 character ROM)"
        );
    };
    let mut sys = BbcMicro::new(fs::read(&os).expect("read OS"));
    sys.insert_rom(15, fs::read(&basic).expect("read BASIC"));
    sys.set_teletext_font(fs::read(&font).expect("read font"));
    for _ in 0..200 {
        sys.run_frame();
    }

    // With the SAA5050 font in place, MODE 7 draws the banner as white text on
    // black. The screen is mostly black with a few thousand white pixels of
    // "BBC Computer 32K" / "BASIC"; before the SAA5050 model it was entirely
    // black.
    let fb = sys.framebuffer();
    let white = fb.iter().filter(|&&px| px == 0xFFFF_FFFF).count();
    let black = fb.iter().filter(|&&px| px == 0xFF00_0000).count();
    assert!(
        black > fb.len() * 3 / 4,
        "MODE 7 background should be predominantly black; got {black}"
    );
    assert!(
        (200..40_000).contains(&white),
        "expected the banner as white teletext pixels; got {white}"
    );
}
