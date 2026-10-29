//! The real Master 128 MOS 3.20 boots, and its DFS reads a disc through the
//! WD1770.
//!
//! Gated `#[ignore]` because the ROMs are copyrighted and not in the tree.
//! Name the directory holding MOS 3.20's ROMs with EMU198X_MASTER_ROM_DIR:
//! `mos.rom`, then `dfs.rom`, `viewsht.rom`, `edit.rom`, `basic4.rom`,
//! `adfs.rom`, `view.rom` and `terminal.rom` for banks 9 to 15, as b2 ships
//! them in `etc/roms/M128/3.20`.
//!
//! ```text
//! EMU198X_MASTER_ROM_DIR=... cargo test --release -p machine-acorn-bbc-master-128 --test master_disc -- --ignored
//! ```
//!
//! The CMOS clock powers up configured for BASIC and the DFS in MODE 7, so
//! the test reads the screen at `&7C00` as the B+ test does.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_bbc_master_128::BbcMicro;

/// The MOS 3.20 ROM set, bank by bank.
const BANKS: [(usize, &str); 7] = [
    (9, "dfs.rom"),
    (10, "viewsht.rom"),
    (11, "edit.rom"),
    (12, "basic4.rom"),
    (13, "adfs.rom"),
    (14, "view.rom"),
    (15, "terminal.rom"),
];

fn master() -> BbcMicro {
    let dir = env::var("EMU198X_MASTER_ROM_DIR")
        .map(PathBuf::from)
        .expect("set EMU198X_MASTER_ROM_DIR to the MOS 3.20 ROM directory");
    let read = |name: &str| fs::read(dir.join(name)).unwrap_or_else(|err| panic!("{name}: {err}"));
    let mut sys = BbcMicro::new(read("mos.rom"));
    for (bank, name) in BANKS {
        sys.insert_rom(bank, read(name));
    }
    sys
}

fn hello_ssd() -> Vec<u8> {
    let mut image = vec![0u8; 40 * 10 * 256];
    let contents = b"DISC OK\r";
    // Sector 0: the first eight characters of the title, then one name per
    // file (seven characters and the directory).
    image[..8].copy_from_slice(b"EMU198X ");
    image[8..16].copy_from_slice(b"HELLO  $");
    // Sector 1: the rest of the title, the cycle number, eight bytes per file,
    // and the disc's 400 sectors (&190) split across two bytes.
    let s1 = 256;
    image[s1..s1 + 4].copy_from_slice(b"    ");
    image[s1 + 5] = 8;
    image[s1 + 6] = 0x01;
    image[s1 + 7] = 0x90;
    // The file's load and exec addresses, its length and its start sector, 2.
    image[s1 + 8..s1 + 16].copy_from_slice(&[
        0x00,
        0x19,
        0x00,
        0x19,
        contents.len() as u8,
        0x00,
        0x00,
        0x02,
    ]);
    image[2 * 256..2 * 256 + contents.len()].copy_from_slice(contents);
    image
}

fn press(sys: &mut BbcMicro, (col, row): (usize, usize), shifted: bool) {
    if shifted {
        sys.press_key(0, 0);
        sys.run_frame();
    }
    sys.press_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(col, row);
    sys.run_frame();
    if shifted {
        sys.release_key(0, 0);
    }
    for _ in 0..3 {
        sys.run_frame();
    }
}

/// Type a command in capitals, as CAPS LOCK leaves the BBC at power-on.
fn type_command(sys: &mut BbcMicro, text: &str) {
    for c in text.chars() {
        let key = match c {
            '*' => ((8, 4), true), // SHIFT + :
            ' ' => ((2, 6), false),
            '\r' => ((9, 4), false),
            'A' => ((1, 4), false),
            'C' => ((2, 5), false),
            'E' => ((2, 2), false),
            'H' => ((4, 5), false),
            'L' => ((6, 5), false),
            'O' => ((6, 3), false),
            'P' => ((7, 3), false),
            'T' => ((3, 2), false),
            'Y' => ((4, 4), false),
            other => panic!("no key for {other:?} in this test's table"),
        };
        press(sys, key.0, key.1);
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

fn run_until(sys: &mut BbcMicro, frames: usize, needle: &str) -> bool {
    for _ in 0..frames {
        if mode7_text(sys).contains(needle) {
            return true;
        }
        sys.run_frame();
    }
    mode7_text(sys).contains(needle)
}

#[test]
#[ignore = "FIXTURE: needs the Master 128 MOS 3.20 ROM set — run with --ignored"]
fn the_master_boots_to_basic() {
    let mut sys = master();
    assert!(
        run_until(&mut sys, 300, "BASIC"),
        "the Master never reached BASIC: {}",
        mode7_text(&sys)
    );
    assert!(
        mode7_text(&sys).contains("Acorn MOS"),
        "the MOS announced itself: {}",
        mode7_text(&sys)
    );
}

#[test]
#[ignore = "FIXTURE: needs the Master 128 MOS 3.20 ROM set — run with --ignored"]
fn the_master_dfs_catalogues_and_types_a_file_from_disc() {
    let mut sys = master();
    sys.insert_disk(0, BbcMicro::dfs_disk(&hello_ssd(), 1, false));
    assert!(
        run_until(&mut sys, 300, "BASIC"),
        "the Master never reached BASIC: {}",
        mode7_text(&sys)
    );
    type_command(&mut sys, "*CAT\r");
    assert!(
        run_until(&mut sys, 300, "HELLO"),
        "*CAT did not list HELLO: {}",
        mode7_text(&sys)
    );
    assert!(mode7_text(&sys).contains("EMU198X"), "*CAT shows the title");
    type_command(&mut sys, "*TYPE HELLO\r");
    assert!(
        run_until(&mut sys, 300, "DISC OK"),
        "*TYPE HELLO did not print the file: {}",
        mode7_text(&sys)
    );
}
