//! A typed space must remain a space in the program the real BASIC stores.

use std::{env, fs, path::PathBuf};

use machine_acorn_atom::{AcornAtom, AtomKey};

fn tap(machine: &mut AcornAtom, key: AtomKey) {
    machine.press_key(key);
    for _ in 0..5 {
        machine.run_frame();
    }
    machine.release_key(key);
    for _ in 0..8 {
        machine.run_frame();
    }
}

#[test]
#[ignore = "FIXTURE: needs a 24 KB Acorn Atom combined ROM — run with --ignored"]
fn typed_spaces_survive_basic_program_entry() {
    let path = env::var_os("EMU198X_ATOM_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/acorn-atom/atom.rom")
        });
    let rom = fs::read(path).expect("read Atom firmware");
    assert_eq!(rom.len(), 0x6000);
    for ram_size in [2560, 32768] {
        let mut machine = AcornAtom::new(rom.clone(), ram_size);
        for _ in 0..150 {
            machine.run_frame();
        }
        // The literal input is `10 PRINT 3` followed by RETURN. Neither the
        // expected bytes nor the keyboard actions use matrix coordinates.
        for key in [
            AtomKey::Num1,
            AtomKey::Num0,
            AtomKey::Space,
            AtomKey::P,
            AtomKey::R,
            AtomKey::I,
            AtomKey::N,
            AtomKey::T,
            AtomKey::Space,
            AtomKey::Num3,
            AtomKey::Return,
        ] {
            tap(&mut machine, key);
        }
        // MOS chooses the upper/lower text area; do not pin either model's
        // address. The stored line begins CR, big-endian line number, text.
        let page = u16::from(machine.peek(18)) << 8;
        let actual: Vec<u8> = (page + 3..page + 12)
            .map(|address| machine.peek(address))
            .collect();
        assert_eq!(
            actual, b" PRINT 3\r",
            "typed text changed with {ram_size} bytes of RAM"
        );
    }
}
