use format_sinclair_zx81_p::Zx81Image;
use machine_sinclair_zx81::{Zx81, Zx81Key};
use std::{env, fs};

fn read_row(sys: &Zx81, row: usize) -> String {
    let fb = sys.framebuffer();
    let width = sys.framebuffer_width() as usize;
    let i = u16::from(sys.cpu().regs.i);
    let top = 32 + row * 8;

    (0..32)
        .map(|col| {
            let left = 32 + col * 8;
            let cell: Vec<u8> = (0..8)
                .map(|r| {
                    (0..8).fold(0u8, |b, bit| {
                        if fb[(top + r) * width + left + bit] == 0xFF00_0000 {
                            b | (0x80 >> bit)
                        } else {
                            b
                        }
                    })
                })
                .collect();
            (0..64u16)
                .find_map(|code| {
                    let base = (i & 0xFE) << 8 | code << 3;
                    let glyph: Vec<u8> = (0..8).map(|k| sys.peek_memory(base + k)).collect();
                    let inverse: Vec<u8> = glyph.iter().map(|b| !b).collect();
                    (glyph == cell || inverse == cell).then(|| character(code as u8))
                })
                .unwrap_or('?')
        })
        .collect()
}

/// The ZX81's character set, as far as this needs it.
fn character(code: u8) -> char {
    match code {
        0 => ' ',
        28..=37 => (b'0' + (code - 28)) as char,
        38..=63 => (b'A' + (code - 38)) as char,
        _ => '.',
    }
}

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

/// A real `.p` loads off the cassette line, and the ROM says so.
///
/// The whole path in one test: parse the image, encode it as the pulse train
/// the ROM's own `SAVE` produces, thread it, and let `LOAD` read it.
///
/// The byte comparison is deliberately not exact. `LOAD` recomputes the
/// pointers it just read -- `DF_CC`, `VARS`, `E_LINE`, `CH_ADD`, `STKBOT`,
/// `STKEND` -- so a byte-perfect match would mean the ROM had *not* run.
///
/// The report line is deliberately not asserted either. A ZX81 program
/// resumes at the line held in `NXTLIN`, so most games run the moment they
/// land and never show a report at all. Measured across four images:
///
/// | Image | Bytes | Bottom line |
/// |---|---|---|
/// | `THE-DICE.P` | 234/249 | `0/0` |
/// | `ZZZ-UNK-2.p` | 422/446 | `0/0` |
/// | `ZZZ-UNK-3.p` | 459/473 | blank -- auto-ran |
/// | `3D Defender` | 11,794/11,857 | blank -- auto-ran |
///
/// An earlier draft asserted `0/0` and failed on the two that auto-run, which
/// is the test being wrong rather than the loader.
#[test]
#[ignore = "needs a ZX81 ROM and a .p image — set EMU198X_ZX81_P and run with --ignored"]
fn a_real_image_loads_from_the_cassette_line() {
    let Ok(rom_path) = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
    else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let Ok(rom) = fs::read(&rom_path) else {
        emu198x_test_skip::skip!("ZX81 ROM not staged at {rom_path}");
    };
    let Ok(image_path) = env::var("EMU198X_ZX81_P") else {
        emu198x_test_skip::skip!("no .p image — set EMU198X_ZX81_P to one");
    };
    let raw = fs::read(&image_path).expect("read .p");

    let image = Zx81Image::parse(&raw).expect("a valid .p");
    let pulses = image.to_pulses(&[0x26]);

    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..400 {
        machine.run_frame();
    }

    // LOAD "" — J gives the LOAD keyword, shift+P a quote.
    tap(&mut machine, Zx81Key::J);
    shifted(&mut machine, Zx81Key::P);
    shifted(&mut machine, Zx81Key::P);
    assert_eq!(
        read_row(&machine, 23).trim_end(),
        "LOAD ..L",
        "the editor should be holding LOAD \"\" before the tape rolls"
    );

    machine.insert_tape(&pulses);
    machine.press_key(Zx81Key::Newline);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(Zx81Key::Newline);

    let mut frames = 0;
    while machine.tape_remaining() > 0 && frames < 20_000 {
        machine.run_frame();
        frames += 1;
    }
    assert_eq!(machine.tape_remaining(), 0, "the tape should run out");
    for _ in 0..200 {
        machine.run_frame();
    }

    let program = image.program();
    let matched = program
        .iter()
        .enumerate()
        .filter(|(n, b)| machine.peek_memory(0x4009 + *n as u16) == **b)
        .count();
    assert!(
        matched * 100 / program.len() >= 90,
        "only {matched} of {} bytes survived the round trip; bottom line reads {:?}",
        program.len(),
        read_row(&machine, 23).trim_end(),
    );
    println!(
        "{matched}/{} bytes, bottom line {:?}",
        program.len(),
        read_row(&machine, 23).trim_end()
    );
}
