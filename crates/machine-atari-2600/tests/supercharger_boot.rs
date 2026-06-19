//! Starpath Supercharger (AR) fast-load boot smoke.
//!
//! Drives the real `Phaser Patrol.a26` single-load proto headless and checks
//! the title/attract screen renders. Gated on the media file (run with
//! `--ignored`): set `EMU198X_SUPERCHARGER_CART` or stage `Phaser Patrol.a26`
//! under `~/.emu198x/media/atari-2600/`.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

use machine_atari_2600::{Atari2600, Atari2600Region};

fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_SUPERCHARGER_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/media/atari-2600/Phaser Patrol.a26");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs the Supercharger Phaser Patrol proto — run with --ignored"]
fn phaser_patrol_fast_loads_and_renders() {
    let Some(path) = cart_path() else {
        panic!(
            "no Supercharger cart — set EMU198X_SUPERCHARGER_CART or place \
             'Phaser Patrol.a26' under ~/.emu198x/media/atari-2600/"
        );
    };
    let rom = fs::read(&path).expect("read cart");
    assert_eq!(rom.len() % 8448, 0, "Supercharger image is 8448 × N");

    let mut sys = Atari2600::new(rom, Atari2600Region::Ntsc).expect("init");
    sys.set_joystick_input(0xFF);
    sys.set_switch_input(0xFF);
    // The dummy BIOS fast-loads on the first frames; give the title a moment.
    for _ in 0..60 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    let colours: HashSet<u32> = fb.iter().copied().collect();
    assert!(
        colours.len() >= 4,
        "fast-loaded title should render several colours; got {} (cart: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 4096,
        "fast-loaded title should fill the screen; got {non_zero} non-backdrop pixels"
    );
}
