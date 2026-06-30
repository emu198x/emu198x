//! Real OS-driven cassette SAVE → LOAD round-trip for the Acorn Atom (#696).
//!
//! Boots the genuine Atom ROM, enters a one-line BASIC program, `SAVE`s it to
//! tape (the COS bit-bangs the 300-baud waveform on 8255 PC0/PC1, which the
//! machine captures via `take_tape_output`), wipes the program with `NEW`, then
//! `LOAD`s it back from the captured waveform and confirms it returns — proving a
//! SAVEd program round-trips through LOAD.
//!
//! Gated `#[ignore]`: needs a real Atom ROM at `~/.emu198x/roms/acorn-atom/atom.rom`
//! (or `EMU198X_ATOM_ROM`). The MAME-assembled romset rejects SAVE at the command
//! parser; a real `Atom_Basic` + `Atom_FloatingPoint` + `Atom_Kernel` image
//! (e.g. acornatom.nl's `acorn_roms.zip`, assembled BASIC→$C000, FP→$D000,
//! Kernel→$F000) has a working SAVE.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_atom::{AcornAtom, AtomKey};

fn rom() -> Vec<u8> {
    let path = env::var("EMU198X_ATOM_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env::var("HOME").expect("HOME")).join(".emu198x/roms/acorn-atom/atom.rom")
        });
    fs::read(&path).unwrap_or_else(|_| panic!("read {}", path.display()))
}

fn key_for(c: char) -> Option<(AtomKey, bool)> {
    use AtomKey::{
        A, B, C, Colon, D, E, F, G, H, I, J, K, L, M, N, Num0, Num1, Num2, Num3, Num4, Num5, Num6,
        Num7, Num8, Num9, O, P, Period, Q, R, Return, S, Space, T, U, V, W, X, Y, Z,
    };
    let key = match c.to_ascii_uppercase() {
        'A' => A,
        'B' => B,
        'C' => C,
        'D' => D,
        'E' => E,
        'F' => F,
        'G' => G,
        'H' => H,
        'I' => I,
        'J' => J,
        'K' => K,
        'L' => L,
        'M' => M,
        'N' => N,
        'O' => O,
        'P' => P,
        'Q' => Q,
        'R' => R,
        'S' => S,
        'T' => T,
        'U' => U,
        'V' => V,
        'W' => W,
        'X' => X,
        'Y' => Y,
        'Z' => Z,
        '0' => Num0,
        '1' => Num1,
        '2' => Num2,
        '3' => Num3,
        '4' => Num4,
        '5' => Num5,
        '6' => Num6,
        '7' => Num7,
        '8' => Num8,
        '9' => Num9,
        ' ' => Space,
        '\n' => Return,
        '.' => Period,
        '"' => return Some((Num2, true)),
        '*' => return Some((Colon, true)),
        _ => return None,
    };
    Some((key, false))
}

fn type_str(sys: &mut AcornAtom, text: &str) {
    for c in text.chars() {
        let (key, shift) = key_for(c).unwrap_or_else(|| panic!("no key for {c:?}"));
        if shift {
            sys.press_key(AtomKey::Shift);
        }
        sys.press_key(key);
        // Hold for three 20 ms fields so the COS keyboard scan debounces the key
        // across more than one scan (one field alone is a single scan and drops).
        for _ in 0..3 {
            sys.run_frame();
        }
        sys.release_key(key);
        if shift {
            sys.release_key(AtomKey::Shift);
        }
        for _ in 0..3 {
            sys.run_frame();
        }
    }
}

fn screen(sys: &AcornAtom) -> String {
    (0x8000u16..0x8200)
        .map(|a| match sys.peek(a) & 0x3f {
            c @ 0x00..=0x1f => (b'@' + c) as char,
            c => c as char,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tap(sys: &mut AcornAtom, key: AtomKey) {
    sys.press_key(key);
    sys.run_frame();
    sys.release_key(key);
    sys.run_frame();
}

#[test]
#[ignore = "needs a real Atom ROM — run with --ignored"]
fn os_saves_and_loads_a_program() {
    let mut sys = AcornAtom::new(rom(), 0x3000);
    for _ in 0..120 {
        sys.run_frame();
    }

    // Enter a one-line program.
    type_str(&mut sys, "10P.5\n");
    let _ = sys.take_tape_output();

    // SAVE it, answering the COS's RECORD TAPE prompt with a key.
    type_str(&mut sys, "SAVE\"Z\"\n");
    for _ in 0..10 {
        sys.run_frame();
    }
    assert!(
        screen(&sys).contains("RECORD TAPE"),
        "SAVE should prompt RECORD TAPE, got {:?}",
        screen(&sys)
    );
    tap(&mut sys, AtomKey::Space);
    for _ in 0..900 {
        sys.run_frame();
    }
    let captured = sys.take_tape_output();
    assert!(!captured.is_empty(), "SAVE wrote a waveform");

    // Wipe the program, then LOAD it back from the captured tape.
    type_str(&mut sys, "NEW\n");
    type_str(&mut sys, "LOAD\"Z\"\n");
    for _ in 0..6 {
        sys.run_frame();
    }
    tap(&mut sys, AtomKey::Space); // PLAY TAPE
    sys.insert_tape(captured); // mount the SAVEd waveform, rewound
    for _ in 0..900 {
        sys.run_frame();
    }

    type_str(&mut sys, "LIST\n");
    for _ in 0..20 {
        sys.run_frame();
    }
    assert!(
        screen(&sys).contains("LIST 10P.5"),
        "the SAVEd program round-trips through LOAD; screen: {:?}",
        screen(&sys)
    );
}

#[test]
#[ignore = "needs a real Atom ROM — run with --ignored"]
fn os_saves_to_uef_then_loads_the_uef_back() {
    use common_acorn_cassette::demodulate_blocks;
    use format_acorn_uef::{encode_blocks, parse};

    let mut sys = AcornAtom::new(rom(), 0x3000);
    for _ in 0..120 {
        sys.run_frame();
    }

    type_str(&mut sys, "10P.5\n");
    let _ = sys.take_tape_output();

    // SAVE, capturing the COS's 300-baud waveform.
    type_str(&mut sys, "SAVE\"Z\"\n");
    for _ in 0..10 {
        sys.run_frame();
    }
    tap(&mut sys, AtomKey::Space);
    for _ in 0..900 {
        sys.run_frame();
    }

    // Demodulate to blocks and write a .uef, exactly as `--save-tape` does. A
    // ~2 s carrier leader (4800 cycles) per block lets the COS re-acquire on LOAD.
    let blocks = demodulate_blocks(sys.take_tape_output());
    assert!(
        blocks.iter().any(|b| b.contains(&b'Z')),
        "the recovered blocks carry the filename Z: {blocks:02X?}"
    );
    let uef = encode_blocks(&blocks, 4800);

    // LOAD that .uef back through the COS (parse -> waveform -> COS reads it).
    type_str(&mut sys, "NEW\n");
    type_str(&mut sys, "LOAD\"Z\"\n");
    for _ in 0..6 {
        sys.run_frame();
    }
    tap(&mut sys, AtomKey::Space); // PLAY TAPE
    sys.insert_tape(parse(&uef).expect("written UEF parses").pulses);
    for _ in 0..900 {
        sys.run_frame();
    }

    type_str(&mut sys, "LIST\n");
    for _ in 0..20 {
        sys.run_frame();
    }
    assert!(
        screen(&sys).contains("LIST 10P.5"),
        "the SAVEd .uef LOADs back to the program; screen: {:?}",
        screen(&sys)
    );
}
