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
#[ignore = "FIXTURE: needs an Atari 7800 cart — run with --ignored"]
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

    // The cart must get *past* its boot wait: native 7800 games spin on a
    // zero-page counter that only their NMI handler advances, and that NMI comes
    // from MARIA's DLI — which only fires once DMA is enabled (CTRL `DM` bits).
    // A rendered frame (several colours, many non-background pixels) is the
    // proof the display list is being walked and the interrupt path is live;
    // before the CTRL-bit fix this was a uniform black frame.
    let fb = sys.framebuffer();
    let colours: std::collections::HashSet<u32> = fb.iter().copied().collect();
    let non_bg = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        colours.len() >= 2 && non_bg >= 500,
        "screen never rendered ({} colours, {non_bg} non-background px) — \
         MARIA DMA/DLI likely not running",
        colours.len()
    );
}
