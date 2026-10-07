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

    // "BBC Computer 32K" is the first sixteen cells of its row, and a cell is
    // the sixteen pixels of a microsecond, so it is lit from the first cell
    // to the sixteenth: x 0-255. Drawn twelve pixels a cell and centred, it
    // was x 80-271 (#1623).
    let width = sys.framebuffer_width() as usize;
    let banner_row = (0..25)
        .find(|&row| {
            let start = 0x7C00 + row as u16 * 40;
            (0..12).all(|i| sys.peek(start + i) == b"BBC Computer"[usize::from(i)])
        })
        .expect("the banner row");
    let lit: Vec<usize> = (banner_row * 20..banner_row * 20 + 20)
        .flat_map(|y| (0..width).filter(move |&x| fb[y * width + x] != 0xFF00_0000))
        .collect();
    let (first, last) = (lit.iter().min(), lit.iter().max());
    assert!(
        first.is_some_and(|&x| x < 16),
        "the banner starts in the first cell: {first:?}"
    );
    assert!(
        last.is_some_and(|&x| (240..256).contains(&x)),
        "and ends in the sixteenth: {last:?}"
    );
}

/// The MOS's five-byte clock at `&0292`/`&0297` (two copies, swapped each
/// tick), most significant byte first. The live copy is the larger.
fn mos_time(sys: &BbcMicro) -> u64 {
    let read = |base: u16| (0..5).fold(0u64, |acc, i| (acc << 8) | u64::from(sys.peek(base + i)));
    read(0x0292).max(read(0x0297))
}

#[test]
#[ignore = "FIXTURE: needs BBC Micro MOS + BASIC ROMs — run with --ignored"]
fn mos_clock_counts_centiseconds() {
    // The MOS drives TIME from System VIA T1, loaded for a 10 ms period at
    // the VIA's 1 MHz clock. Ten emulated seconds must add 1000 centiseconds;
    // with the VIAs clocked at 2 MHz they added 2000.
    let (Some(os), Some(basic)) = (os_path(), basic_path()) else {
        panic!("needs os.rom + basic.rom at ~/.emu198x/roms/acorn-bbc-micro/");
    };
    let mut sys = BbcMicro::new(fs::read(&os).expect("read OS"));
    sys.insert_rom(15, fs::read(&basic).expect("read BASIC"));
    for _ in 0..100 {
        sys.run_frame();
    }
    let before = mos_time(&sys);
    for _ in 0..500 {
        sys.run_frame();
    }
    let elapsed = mos_time(&sys) - before;
    assert!(
        (998..=1002).contains(&elapsed),
        "500 frames (10 s) should add 1000 centiseconds to TIME; added {elapsed}"
    );
}

/// The key-matrix position of each character the display tests type.
fn key(c: char) -> (usize, usize) {
    match c {
        'M' => (5, 6),
        'O' => (6, 3),
        'D' => (2, 3),
        'E' => (2, 2),
        'V' => (3, 6),
        'U' => (5, 3),
        ' ' => (2, 6),
        '0' => (7, 2),
        '1' => (0, 3),
        '2' => (1, 3),
        '3' => (1, 1),
        '4' => (2, 1),
        '5' => (3, 1),
        '6' => (4, 3),
        '7' => (4, 2),
        '8' => (5, 1),
        '9' => (6, 2),
        ',' => (6, 6),
        ';' => (7, 5),
        '\n' => (9, 4),
        _ => panic!("no key for {c:?}"),
    }
}

fn type_line(sys: &mut BbcMicro, text: &str) {
    for c in text.chars() {
        let (col, row) = key(c);
        tap(sys, col, row);
    }
    for _ in 0..30 {
        sys.run_frame();
    }
}

fn booted() -> BbcMicro {
    let (Some(os), Some(basic), Some(font)) = (os_path(), basic_path(), font_path()) else {
        panic!("needs os.rom + basic.rom + saa5050.rom at ~/.emu198x/roms/acorn-bbc-micro/");
    };
    let mut sys = BbcMicro::new(fs::read(&os).expect("read OS"));
    sys.insert_rom(15, fs::read(&basic).expect("read BASIC"));
    sys.set_teletext_font(fs::read(&font).expect("read font"));
    for _ in 0..200 {
        sys.run_frame();
    }
    sys
}

/// One field's lines of the picture: the even framebuffer rows.
///
/// The framebuffer weaves both fields of the interlaced picture, so a change
/// in one field shows on its rows a field before it reaches the other's.
/// Watching one field's rows sees each change once, a frame at a time.
fn even_field(sys: &BbcMicro) -> Vec<u32> {
    let width = sys.framebuffer_width() as usize;
    sys.framebuffer()
        .chunks(width)
        .step_by(2)
        .flatten()
        .copied()
        .collect()
}

/// Run `frames` frames and report which pixels of the even field changed
/// against the first, as `(line, first x, last x + 1)` runs, and whether each
/// frame matched it.
fn changes_over(sys: &mut BbcMicro, frames: usize) -> (Vec<(usize, usize, usize)>, Vec<bool>) {
    let width = sys.framebuffer_width() as usize;
    sys.run_frame();
    let first = even_field(sys);
    let mut changed = vec![false; first.len()];
    let mut same = Vec::new();
    for _ in 0..frames {
        sys.run_frame();
        let fb = even_field(sys);
        let mut matches = true;
        for (i, (&now, &was)) in fb.iter().zip(&first).enumerate() {
            if now != was {
                changed[i] = true;
                matches = false;
            }
        }
        same.push(matches);
    }
    let mut runs = Vec::new();
    for (line, row) in changed.chunks(width).enumerate() {
        let mut x = 0;
        while x < width {
            if row[x] {
                let start = x;
                while x < width && row[x] {
                    x += 1;
                }
                runs.push((line, start, x));
            } else {
                x += 1;
            }
        }
    }
    (runs, same)
}

/// Lengths of the complete runs of equal values, dropping the partial first
/// and last.
fn half_periods(states: &[bool]) -> Vec<usize> {
    let mut lengths = Vec::new();
    let mut run = 1;
    for pair in states.windows(2) {
        if pair[0] == pair[1] {
            run += 1;
        } else {
            lengths.push(run);
            run = 1;
        }
    }
    lengths.into_iter().skip(1).collect()
}

#[test]
#[ignore = "FIXTURE: needs BBC MOS + BASIC + SAA5050 ROMs — run with --ignored"]
fn the_prompt_cursor_blinks_in_mode_7_and_in_a_bitmap_mode() {
    // At the `>` prompt the MOS leaves R10 = &72 in MODE 7 and &67 in
    // MODE 4: a 32-field blink, sixteen frames on and sixteen off (Advanced
    // User Guide §18.8). The only pixels that change are the cursor's: one
    // teletext cell's bottom line in MODE 7, one character's bottom line in
    // MODE 4. Before #384 the BBC drew no cursor, so nothing changed.
    let mut sys = booted();
    let (runs, same) = changes_over(&mut sys, 96);
    assert_eq!(runs.len(), 1, "MODE 7: one changing run; got {runs:?}");
    let (_, start, end) = runs[0];
    assert_eq!(end - start, 16, "MODE 7: one teletext cell wide");
    assert!(
        half_periods(&same).iter().all(|&n| n == 16),
        "MODE 7 blink: {same:?}"
    );

    type_line(&mut sys, "MODE 4\n");
    let (runs, same) = changes_over(&mut sys, 96);
    assert_eq!(runs.len(), 1, "MODE 4: one changing run; got {runs:?}");
    let (line, start, end) = runs[0];
    assert_eq!(end - start, 16, "MODE 4: one character wide");
    assert_eq!(line % 8, 7, "MODE 4: the character's bottom line");
    assert!(
        half_periods(&same).iter().all(|&n| n == 16),
        "MODE 4 blink: {same:?}"
    );
}

#[test]
#[ignore = "FIXTURE: needs BBC MOS + BASIC + SAA5050 ROMs — run with --ignored"]
fn a_flashing_colour_flashes_at_the_mos_rate() {
    // `VDU 19,1,8;0;` makes logical colour 1, the text, flashing black and
    // white. The MOS flips Video ULA control bit 0 from its vertical-sync
    // interrupt, 25 fields each way by default (*FX9 and *FX10; Advanced
    // User Guide §19.1.1). The text on the top row then goes from lit to
    // dark every half second. Before #384 the ULA ignored the flash bit.
    let mut sys = booted();
    type_line(&mut sys, "MODE 4\n");
    type_line(&mut sys, "VDU 19,1,8;0;\n");
    let width = sys.framebuffer_width() as usize;
    let lit: Vec<usize> = (0..150)
        .map(|_| {
            sys.run_frame();
            sys.framebuffer()[..16 * width]
                .iter()
                .filter(|&&px| px == 0xFFFF_FFFF)
                .count()
        })
        .collect();
    assert!(lit.iter().any(|&n| n > 100), "the top row lights: {lit:?}");
    assert!(lit.contains(&0), "and goes dark: {lit:?}");
    let shown: Vec<bool> = lit.iter().map(|&n| n > 0).collect();
    let halves = half_periods(&shown);
    assert!(halves.len() >= 3, "several flashes: {lit:?}");
    assert!(
        halves.iter().all(|&n| n == 25),
        "25-field halves: {halves:?}"
    );
}

/// The MODE 7 screen row whose first byte is `code`.
fn mode7_row_starting(sys: &BbcMicro, code: u8) -> u16 {
    (0..25)
        .find(|&row| sys.peek(0x7C00 + row * 40) == code)
        .unwrap_or_else(|| panic!("no row starts with {code:#04X}: {}", mode7_text(sys)))
}

/// The pixels of one framebuffer row of MODE 7 cell (`column`, `row`), for
/// `line` 0-19 of the cell.
fn mode7_cell_line(sys: &BbcMicro, column: usize, row: u16, line: usize) -> Vec<u32> {
    let width = sys.framebuffer_width() as usize;
    let y = usize::from(row) * 20 + line;
    let x = column * 16;
    sys.framebuffer()[y * width + x..y * width + x + 16].to_vec()
}

#[test]
#[ignore = "FIXTURE: needs BBC MOS + BASIC + SAA5050 ROMs — run with --ignored"]
fn mode7_draws_double_height_and_flashing_text_from_basic() {
    // Two lines of double height (code 141), as the MOS needs them, then a
    // flashing line (136). Double height draws each glyph line on both
    // fields of a cell twice as tall, so the cell's framebuffer rows come in
    // identical pairs; in normal height a diagonal makes them differ,
    // because character rounding draws the two lines of a dot row apart.
    // Flashing text is hidden for 16 fields in every 64; the even field's
    // rows show that as 16 frames hidden and 48 shown. Before #383, MODE 7
    // drew none of this.
    let mut sys = booted();
    type_line(
        &mut sys,
        "VDU 141,66,66,67,13,10,141,66,66,67,13,10,136,70,76,65,83,72,13,10\n",
    );
    let top = mode7_row_starting(&sys, 0x8D);
    assert_eq!(sys.peek(0x7C00 + (top + 1) * 40), 0x8D, "the second row");
    let flashing = mode7_row_starting(&sys, 0x88);

    for row in [top, top + 1] {
        let lines: Vec<Vec<u32>> = (0..20).map(|y| mode7_cell_line(&sys, 1, row, y)).collect();
        for pair in lines.chunks(2) {
            assert_eq!(
                pair[0], pair[1],
                "row {row}: double height repeats each line"
            );
        }
        assert!(
            lines.iter().flatten().any(|&px| px == 0xFFFF_FFFF),
            "row {row}: B is drawn"
        );
    }
    assert_ne!(
        (0..20)
            .map(|y| mode7_cell_line(&sys, 1, top, y))
            .collect::<Vec<_>>(),
        (0..20)
            .map(|y| mode7_cell_line(&sys, 1, top + 1, y))
            .collect::<Vec<_>>(),
        "the top and bottom halves differ"
    );
    // The banner's "C" (row 1, column 2) has diagonals at its corners.
    let rounded = (0..20)
        .step_by(2)
        .any(|y| mode7_cell_line(&sys, 2, 1, y) != mode7_cell_line(&sys, 2, 1, y + 1));
    assert!(
        rounded,
        "normal height: the fields draw different lines of C"
    );

    let lit = |sys: &BbcMicro, row: u16| {
        (0..20)
            .step_by(2)
            .any(|y| mode7_cell_line(sys, 1, row, y).contains(&0xFFFF_FFFF))
    };
    let mut shown = Vec::new();
    for _ in 0..150 {
        sys.run_frame();
        shown.push(lit(&sys, flashing));
        assert!(lit(&sys, top), "double-height text does not flash");
    }
    let mut runs = Vec::new();
    let mut run = 1;
    for pair in shown.windows(2) {
        if pair[0] == pair[1] {
            run += 1;
        } else {
            runs.push((pair[0], run));
            run = 1;
        }
    }
    let whole = runs.get(1..).unwrap_or_default();
    assert!(whole.len() >= 2, "the text flashes: {shown:?}");
    for &(on, frames) in whole {
        assert_eq!(frames, if on { 48 } else { 16 }, "{runs:?}");
    }
}
