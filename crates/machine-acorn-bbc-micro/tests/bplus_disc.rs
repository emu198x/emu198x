//! The real BBC B+ OS and Acorn 1770 DFS read a disc through the WD1770.
//!
//! Gated `#[ignore]` because the ROMs are copyrighted and not in the tree.
//! Place the B+'s 16 KB OS 2.00, BASIC 2 and Acorn DFS 2.26 in
//! `~/.emu198x/roms/acorn-bbc-micro/` as `bplus-os.rom`, `basic.rom` and
//! `dfs-2.26.rom`, or name them with EMU198X_BBC_BPLUS_OS, EMU198X_BBC_BASIC
//! and EMU198X_BBC_DFS. (MAME's `bpos2.ic71` holds the OS and BASIC together;
//! its two 16 KB halves are the first two files, in that order.)
//!
//! ```text
//! cargo test --release -p machine-acorn-bbc-micro --test bplus_disc -- --ignored
//! ```
//!
//! The disc is built here, so no image is needed: a DFS catalogue in sectors 0
//! and 1 of track 0 and one text file. `*CAT` proves the DFS read the
//! catalogue, and `*TYPE HELLO` that it found and read the file. Every byte
//! crosses the 1770 by NMI, so a passing test is the controller, its interrupt
//! wiring and the drive latch agreeing with the DFS.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_bbc_micro::BbcMicro;

fn rom(env_var: &str, file: &str) -> Option<Vec<u8>> {
    let path = env::var(env_var).map(PathBuf::from).ok().or_else(|| {
        let home = env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join(".emu198x/roms/acorn-bbc-micro")
                .join(file),
        )
    })?;
    fs::read(path).ok()
}

/// A 40-track single-sided DFS disc titled `EMU198X` holding `$.HELLO`, a
/// text file that reads `DISC OK`.
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
#[ignore = "FIXTURE: needs the B+ OS 2.00, BASIC and Acorn DFS 2.26 ROMs — run with --ignored"]
fn the_b_plus_dfs_catalogues_and_types_a_file_from_disc() {
    let (Some(os), Some(basic), Some(dfs)) = (
        rom("EMU198X_BBC_BPLUS_OS", "bplus-os.rom"),
        rom("EMU198X_BBC_BASIC", "basic.rom"),
        rom("EMU198X_BBC_DFS", "dfs-2.26.rom"),
    ) else {
        panic!(
            "needs bplus-os.rom, basic.rom and dfs-2.26.rom in \
             ~/.emu198x/roms/acorn-bbc-micro/, or EMU198X_BBC_BPLUS_OS, \
             EMU198X_BBC_BASIC and EMU198X_BBC_DFS"
        );
    };
    let mut sys = BbcMicro::new_b_plus(os);
    sys.insert_rom(15, basic);
    sys.insert_rom(14, dfs);
    sys.insert_disk(0, BbcMicro::dfs_disk(&hello_ssd(), 1, false));

    assert!(
        run_until(&mut sys, 300, "BASIC"),
        "the B+ never reached BASIC: {}",
        mode7_text(&sys)
    );
    assert!(
        mode7_text(&sys).contains("Acorn 1770 DFS"),
        "the DFS announced itself: {}",
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
