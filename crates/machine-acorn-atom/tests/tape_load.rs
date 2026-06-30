//! Real OS-driven cassette load for the Acorn Atom (#371).
//!
//! Boots the genuine 24 KB Atom ROM, types `LOAD"INSTRUCTIONS"` on the
//! 8255-scanned keyboard (the `"` via SHIFT+2), plays a real UEF, and confirms
//! the COS software-decodes the raw waveform on PC5 and lands the program in
//! RAM. This is the Atom's equivalent of the byte-for-byte BBC/Electron proofs.
//!
//! Keyed to the staged Defender tape (filename `INSTRUCTIONS`, a BASIC program).
//! Gated `#[ignore]`: needs the ROM at `~/.emu198x/roms/acorn-atom/atom.rom` and
//! the tape at `~/.emu198x/media/acorn-atom/` (or set `ACORN_UEF`).

use std::env;
use std::fs;
use std::path::PathBuf;

use format_acorn_uef::parse;
use machine_acorn_atom::{AcornAtom, AtomKey};

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME set"))
}

fn rom() -> Vec<u8> {
    let path = home().join(".emu198x/roms/acorn-atom/atom.rom");
    fs::read(&path).unwrap_or_else(|_| panic!("read {}", path.display()))
}

fn uef() -> Vec<u8> {
    let path = env::var("ACORN_UEF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            home().join(".emu198x/media/acorn-atom/Defender (1982)(Micromania).uef")
        });
    fs::read(&path).unwrap_or_else(|_| panic!("read {}", path.display()))
}

fn tap(sys: &mut AcornAtom, key: AtomKey) {
    sys.press_key(key);
    // Hold for three 20 ms fields so the COS keyboard scan debounces the key
    // across more than one scan (one field alone is a single scan and drops).
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(key);
    for _ in 0..3 {
        sys.run_frame();
    }
}

/// Type `"` — SHIFT held while pressing the 2 key.
fn tap_quote(sys: &mut AcornAtom) {
    sys.press_key(AtomKey::Shift);
    sys.press_key(AtomKey::Num2);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(AtomKey::Num2);
    sys.release_key(AtomKey::Shift);
    for _ in 0..3 {
        sys.run_frame();
    }
}

fn ram_contains(sys: &AcornAtom, needle: &[u8]) -> bool {
    (0x0000u16..0x3000 - needle.len() as u16)
        .any(|base| (0..needle.len()).all(|i| sys.peek(base + i as u16) == needle[i]))
}

/// Decode the 32×16 screen to text. The Atom stores characters in MC6847
/// display codes: 0x00-0x1F are `@A-Z[\]^_`, 0x20-0x3F are ASCII space..`?`.
fn atom_screen(sys: &AcornAtom) -> String {
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

#[test]
#[ignore = "needs the Atom ROM + a UEF — run with --ignored"]
fn os_loads_a_real_tape() {
    // 12 KB RAM so the text space (~$2900) exists for the loaded program.
    let mut sys = AcornAtom::new(rom(), 0x3000);

    // Boot to the `>` prompt.
    for _ in 0..120 {
        sys.run_frame();
    }

    // Type LOAD"INSTRUCTIONS" then RETURN. The double-quote is SHIFT+2.
    use AtomKey::*;
    for k in [L, O, A, D] {
        tap(&mut sys, k);
    }
    tap_quote(&mut sys);
    for k in [I, N, S, T, R, U, C, T, I, O, N, S] {
        tap(&mut sys, k);
    }
    tap_quote(&mut sys);
    tap(&mut sys, Return);

    // The command echoed and the COS parsed it (it asks for the tape).
    let typed = atom_screen(&sys);
    assert!(
        typed.contains("LOAD\"INSTRUCTIONS\"") && typed.contains("PLAY TAPE"),
        "expected the command to echo and the COS to ask for the tape; got: {typed:?}"
    );

    // Now "play the tape" — mount it fresh so the COS reads from the leader (the
    // Atom has no motor relay, so a tape mounted earlier would have run on during
    // boot + typing and be past the first file by now). The COS waits at "PLAY
    // TAPE" for the space bar to begin reading.
    sys.insert_tape(parse(&uef()).expect("parse UEF").pulses);
    tap(&mut sys, Space);

    // The COS software-decodes the raw cassette waveform on PC5 (300-baud Kansas
    // City) and loads the first file. Scan RAM for a distinctive run from its
    // BASIC text ("GOS.a") to confirm the program reached memory.
    let needle = [0x47u8, 0x4F, 0x53, 0x2E, 0x61];
    let mut loaded = false;
    for _ in 0..200 {
        for _ in 0..20 {
            sys.run_frame();
        }
        if ram_contains(&sys, &needle) {
            loaded = true;
            break;
        }
    }

    assert!(
        loaded,
        "the tape program never reached RAM; screen: {:?}",
        atom_screen(&sys)
    );
}
