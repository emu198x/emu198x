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

#[test]
#[ignore = "needs BBC Micro MOS + BASIC ROMs — run with --ignored"]
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
    let screen: String = (0x7C00u16..0x8000)
        .map(|a| {
            let c = sys.peek(a);
            if (0x20..0x7f).contains(&c) {
                c as char
            } else {
                ' '
            }
        })
        .collect();
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
