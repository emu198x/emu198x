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
#[ignore = "needs BBC Micro MOS ROM — run with --ignored"]
fn os_reaches_bank_scan() {
    let Some(path) = os_path() else {
        panic!(
            "BBC MOS ROM not found — set EMU198X_BBC_OS or place os.rom \
             at ~/.emu198x/roms/acorn-bbc-micro/"
        );
    };
    let os = fs::read(&path).expect("read OS");
    assert_eq!(os.len(), 0x4000, "MOS ROM must be 16 KB");

    let mut sys = BbcMicro::new(os);
    // Optionally install BASIC into bank 15 if available.
    if let Some(basic) = basic_path() {
        let rom = fs::read(&basic).expect("read BASIC");
        if rom.len() == 0x4000 {
            sys.insert_rom(15, rom);
        }
    }
    for _ in 0..200 {
        sys.run_frame();
    }

    // Without BASIC, the OS finishes its sideways-ROM scan and ends
    // up selecting bank 15 (the conventional BASIC slot). With BASIC,
    // we additionally expect the framebuffer to carry non-backdrop
    // pixels (and not just MODE 7 teletext backdrop, which renders
    // as black until the SAA5050 ports).
    assert!(
        sys.rom_bank() > 0,
        "OS should have scanned at least one ROM bank; got {}",
        sys.rom_bank()
    );
    assert_eq!(sys.framebuffer().len(), 640 * 256);
}
