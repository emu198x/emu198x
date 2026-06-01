//! Atari 2600 cart boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_atari_2600::{Atari2600, Atari2600Region};

fn first_cart(dir: &PathBuf) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("a26") | Some("A26") | Some("bin") | Some("BIN")
            )
        })
        .collect();
    paths.sort();
    paths.into_iter().next()
}

fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATARI_2600_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".emu198x/media/atari-2600");
    first_cart(&dir)
}

#[test]
#[ignore = "needs an Atari 2600 cart — run with --ignored"]
fn cart_boots_to_playfield() {
    let Some(path) = cart_path() else {
        panic!(
            "no Atari 2600 cart found — set EMU198X_ATARI_2600_CART or place \
             a .a26 / .bin under ~/.emu198x/media/atari-2600/"
        );
    };
    let rom = fs::read(&path).expect("read cart");

    let mut sys = Atari2600::new(rom, Atari2600Region::Ntsc).expect("init");
    sys.set_joystick_input(0xFF);
    sys.set_switch_input(0xFF);
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
        colours.len() >= 3,
        "Atari 2600 boot should produce >= 3 distinct colours; got {} (cart: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "boot should have >= 1024 non-backdrop pixels; got {non_zero}"
    );
}
