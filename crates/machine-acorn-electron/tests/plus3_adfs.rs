//! The real Electron OS and ADFS 1.00 read a disc through the Plus 3.
//!
//! Gated `#[ignore]` because the ROMs are copyrighted and not in the tree.
//! Place the Electron's 16 KB OS, BASIC and the Plus 3's ADFS 1.00 in
//! `~/.emu198x/roms/acorn-electron/` as `os.rom`, `basic.rom` and `adfs.rom`,
//! or name them with `EMU198X_ELECTRON_OS`, `EMU198X_ELECTRON_BASIC` and
//! `EMU198X_ELECTRON_ADFS`.
//!
//! ```text
//! cargo test -p machine-acorn-electron --test plus3_adfs -- --ignored
//! ```
//!
//! The disc is built here, so no image is needed: a blank M-format ADFS disc
//! titled `EMU198X`, with one BASIC program, `HELLO`, written into its root
//! directory and first free sectors. The boot banner proves the OS found ADFS
//! in ROM 4, `*CAT` that ADFS read the free-space map and root directory, and
//! `CHAIN "HELLO"` that it found and loaded a file. Every byte crosses the
//! 1770 by polling, so a passing test is the controller, the drive latch, the
//! ROM latch and ADFS agreeing.

use std::env;
use std::fs;
use std::path::PathBuf;

use format_acorn_adfs::{Format, SECTOR_SIZE, blank};
use machine_acorn_electron::{AcornElectron, adfs_disk};

fn rom(env_var: &str, file: &str) -> Option<Vec<u8>> {
    let path = env::var(env_var).map(PathBuf::from).ok().or_else(|| {
        let home = env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join(".emu198x/roms/acorn-electron")
                .join(file),
        )
    })?;
    fs::read(path).ok()
}

/// `10 PRINT "DISC OK"`, tokenised, then the end-of-program marker.
const HELLO_PROGRAM: &[u8] = &[
    0x0D, 0x00, 0x0A, 0x0F, 0xF1, b' ', b'"', b'D', b'I', b'S', b'C', b' ', b'O', b'K', b'"', 0x0D,
    0xFF,
];

/// A blank M disc titled `EMU198X` holding `$.HELLO` in sector 7, the first
/// free sector, with the free-space map shrunk by one sector to match.
fn hello_disc() -> Vec<u8> {
    let mut image = blank(Format::M, "EMU198X");
    // The file's data.
    image[7 * SECTOR_SIZE..7 * SECTOR_SIZE + HELLO_PROGRAM.len()].copy_from_slice(HELLO_PROGRAM);
    // Its directory entry: name, load &FFFF0E00, exec &FFFF8023, length,
    // start sector 7, sequence 0. The name's first byte carries R, its second
    // W.
    let e = 2 * SECTOR_SIZE + 5;
    image[e..e + 6].copy_from_slice(b"HELLO\r");
    image[e] |= 0x80;
    image[e + 1] |= 0x80;
    image[e + 10..e + 14].copy_from_slice(&0xFFFF_0E00u32.to_le_bytes());
    image[e + 14..e + 18].copy_from_slice(&0xFFFF_8023u32.to_le_bytes());
    image[e + 18..e + 22].copy_from_slice(&(HELLO_PROGRAM.len() as u32).to_le_bytes());
    image[e + 22..e + 25].copy_from_slice(&[7, 0, 0]);
    // The free space now starts at sector 8 and is one sector shorter.
    image[0] = 8;
    let free = Format::M.total_sectors() - 8;
    image[SECTOR_SIZE] = (free & 0xFF) as u8;
    image[SECTOR_SIZE + 1] = (free >> 8) as u8;
    image[0xFF] = format_acorn_adfs::map_checksum(&image[..SECTOR_SIZE]);
    image[SECTOR_SIZE + 0xFF] =
        format_acorn_adfs::map_checksum(&image[SECTOR_SIZE..2 * SECTOR_SIZE]);
    image
}

fn hold(sys: &mut AcornElectron, frames: usize) {
    for _ in 0..frames {
        sys.run_frame();
    }
}

fn press(sys: &mut AcornElectron, (col, row): (usize, usize), shifted: bool) {
    if shifted {
        sys.press_key(13, 3);
        sys.run_frame();
    }
    sys.press_key(col, row);
    hold(sys, 3);
    sys.release_key(col, row);
    sys.run_frame();
    if shifted {
        sys.release_key(13, 3);
    }
    hold(sys, 3);
}

/// Type a command in capitals, as CAPS LOCK leaves the Electron at power-on.
/// Matrix positions from the runtime's keyboard table (MAME `LINE.0-13`).
fn type_text(sys: &mut AcornElectron, text: &str) {
    for c in text.chars() {
        let key = match c {
            '*' => ((2, 2), true),  // SHIFT + :
            '"' => ((11, 0), true), // SHIFT + 2
            ' ' => ((0, 3), false),
            '\r' => ((1, 2), false),
            'A' => ((12, 2), false),
            'C' => ((10, 3), false),
            'E' => ((10, 1), false),
            'H' => ((7, 2), false),
            'I' => ((5, 1), false),
            'L' => ((4, 2), false),
            'N' => ((7, 3), false),
            'O' => ((4, 1), false),
            'T' => ((8, 1), false),
            other => panic!("no key for {other:?} in this test's table"),
        };
        press(sys, key.0, key.1);
    }
}

/// The text on a 40-column screen (modes 3 and 6 put eight bytes per
/// character cell; mode 6 is the Electron's power-on mode), recognised against
/// the OS ROM's own font at `&C000`.
fn screen_text(sys: &AcornElectron, os: &[u8]) -> String {
    let start = usize::from(sys.screen_start());
    let mut text = String::new();
    for row in 0..25 {
        for col in 0..40 {
            let mut addr = start + (row * 40 + col) * 8;
            if addr >= 0x8000 {
                addr -= 0x2000; // mode 6's screen size
            }
            let cell: Vec<u8> = (0..8).map(|i| sys.peek((addr + i) as u16)).collect();
            let ch = (32u8..127)
                .find(|&c| {
                    let glyph = &os[usize::from(c - 32) * 8..usize::from(c - 32) * 8 + 8];
                    glyph == cell.as_slice()
                })
                .map_or('?', char::from);
            text.push(ch);
        }
        text.push('\n');
    }
    text
}

fn run_until(sys: &mut AcornElectron, os: &[u8], frames: usize, needle: &str) -> bool {
    for _ in 0..frames {
        if screen_text(sys, os).contains(needle) {
            return true;
        }
        sys.run_frame();
    }
    screen_text(sys, os).contains(needle)
}

#[test]
#[ignore = "FIXTURE: needs the Electron OS, BASIC and ADFS 1.00 ROMs — run with --ignored"]
fn adfs_boots_catalogues_a_disc_and_chains_a_program() {
    let (Some(os), Some(basic), Some(adfs)) = (
        rom("EMU198X_ELECTRON_OS", "os.rom"),
        rom("EMU198X_ELECTRON_BASIC", "basic.rom"),
        rom("EMU198X_ELECTRON_ADFS", "adfs.rom"),
    ) else {
        panic!(
            "needs os.rom, basic.rom and adfs.rom in ~/.emu198x/roms/acorn-electron/, \
             or EMU198X_ELECTRON_OS, EMU198X_ELECTRON_BASIC and EMU198X_ELECTRON_ADFS"
        );
    };
    let mut sys = AcornElectron::new(os.clone(), basic);
    sys.fit_plus3(adfs);
    sys.insert_disk(0, adfs_disk(&hello_disc(), true).expect("disc"));

    assert!(
        run_until(&mut sys, &os, 400, ">"),
        "the Electron never reached the BASIC prompt:\n{}",
        screen_text(&sys, &os)
    );
    let banner = screen_text(&sys, &os);
    eprintln!("boot screen:\n{banner}");
    assert!(
        banner.contains("Acorn ADFS"),
        "ADFS announced itself:\n{banner}"
    );

    type_text(&mut sys, "*CAT\r");
    assert!(
        run_until(&mut sys, &os, 600, "HELLO"),
        "*CAT did not list HELLO:\n{}",
        screen_text(&sys, &os)
    );
    let cat = screen_text(&sys, &os);
    eprintln!("after *CAT:\n{cat}");
    assert!(cat.contains("EMU198X"), "*CAT shows the title:\n{cat}");

    type_text(&mut sys, "CHAIN \"HELLO\"\r");
    assert!(
        run_until(&mut sys, &os, 600, "DISC OK"),
        "CHAIN \"HELLO\" did not run the program:\n{}",
        screen_text(&sys, &os)
    );
    eprintln!("after CHAIN:\n{}", screen_text(&sys, &os));
}
