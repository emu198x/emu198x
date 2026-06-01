//! Atari 7800 cart boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_atari_7800::{Atari7800, Atari7800Region};

fn first_cart(dir: &PathBuf) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("a78") | Some("A78") | Some("bin") | Some("BIN")
            )
        })
        .collect();
    paths.sort();
    paths.into_iter().next()
}

fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATARI_7800_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".emu198x/media/atari-7800");
    first_cart(&dir)
}

#[test]
#[ignore = "needs an Atari 7800 cart — run with --ignored"]
fn cart_boots_without_panic() {
    let Some(path) = cart_path() else {
        panic!(
            "no Atari 7800 cart found — set EMU198X_ATARI_7800_CART or place \
             a .a78 / .bin under ~/.emu198x/media/atari-7800/"
        );
    };
    let rom = fs::read(&path).expect("read cart");

    let mut sys = Atari7800::new(rom, Atari7800Region::Ntsc).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }

    // MARIA framebuffer is allocated once at construction and never zeroed.
    // A boot that drives the display-list at all will produce at least the
    // background colour across the frame; verify it's the right size.
    assert_eq!(
        sys.framebuffer().len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
    assert!(sys.frame_count() >= 200);
}
