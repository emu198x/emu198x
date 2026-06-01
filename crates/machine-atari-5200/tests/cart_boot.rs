//! Atari 5200 cart boot smoke.
//!
//! Cart-only boot is supported — without a 5200 BIOS, the cart's
//! `$BFFC/$BFFD` reset vector is taken via the `$FFFC` mirror and
//! many self-contained carts will still produce visible playfield.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_atari_5200::{Atari5200, Atari5200Region};

fn first_cart(dir: &PathBuf) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let ok_ext = matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("a52") | Some("A52") | Some("bin") | Some("BIN") | Some("car") | Some("CAR")
            );
            let size = fs::metadata(p).ok().map(|m| m.len()).unwrap_or(0);
            ok_ext && matches!(size, 4096 | 8192 | 16384 | 32768)
        })
        .collect();
    paths.sort();
    paths.into_iter().next()
}

fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATARI_5200_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".emu198x/media/atari-5200");
    first_cart(&dir)
}

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATARI_5200_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/atari-5200/5200.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs an Atari 5200 cart — run with --ignored"]
fn cart_boots_to_playfield() {
    let Some(path) = cart_path() else {
        panic!(
            "no Atari 5200 cart found — set EMU198X_ATARI_5200_CART or place \
             a 4 KB / 8 KB / 16 KB / 32 KB .a52 / .bin under ~/.emu198x/media/atari-5200/"
        );
    };
    let rom = fs::read(&path).expect("read cart");
    let bios = bios_path()
        .and_then(|p| fs::read(&p).ok())
        .unwrap_or_default();

    let mut sys = Atari5200::new(rom, bios, Atari5200Region::Ntsc).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 8 {
            break;
        }
    }
    assert!(
        colours.len() >= 2,
        "Atari 5200 boot should produce >= 2 distinct colours; got {} (cart: {})",
        colours.len(),
        path.display()
    );
}
