//! Atari 800XL OS boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

fn os_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATARI_800XL_OS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/atari-800xl/atarixl.rom");
    p.exists().then_some(p)
}

fn basic_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATARI_800XL_BASIC") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/atari-800xl/ataribas.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs an Atari 800XL OS ROM (atarixl.rom) — run with --ignored"]
fn os_reaches_self_check() {
    let Some(path) = os_path() else {
        panic!(
            "Atari 800XL OS ROM not found — set EMU198X_ATARI_800XL_OS or place \
             atarixl.rom at ~/.emu198x/roms/atari-800xl/"
        );
    };
    let os = fs::read(&path).expect("read OS");
    assert_eq!(os.len(), 0x4000, "atarixl.rom must be 16 KB");

    let basic = basic_path().and_then(|p| fs::read(&p).ok());

    let mut sys =
        Atari800xl::new(Some(os), basic, None, Atari800xlRegion::Ntsc, true).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }

    // OS should have advanced past reset and the master clock should be the
    // expected NTSC clock count per frame x 200.
    assert!(sys.frame_count() >= 200);
    assert_eq!(
        sys.framebuffer().len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
}
