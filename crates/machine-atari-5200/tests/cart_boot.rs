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
            // A headered .a52/.car dump is a whole number of 4 KB pages
            // plus its 16-byte header; the loader strips it.
            let raw = matches!(size, 4096 | 8192 | 16384 | 32768);
            let headered = matches!(size.wrapping_sub(16), 4096 | 8192 | 16384 | 32768);
            ok_ext && (raw || headered)
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
    let dir = PathBuf::from(home).join(".emu198x/roms/atari-5200");
    // The 5200 BIOS is a 2 KB ROM; accept any name (5200.rom, bios.rom,
    // "Atari 5200 BIOS (1982)(Atari).bin", …).
    let direct = dir.join("5200.rom");
    if direct.exists() {
        return Some(direct);
    }
    let mut hits: Vec<PathBuf> = fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| fs::metadata(p).map(|m| m.len()).ok() == Some(2048))
        .collect();
    hits.sort();
    hits.into_iter().next()
}

#[test]
#[ignore = "FIXTURE: needs an Atari 5200 cart — run with --ignored"]
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

    let has_bios = !bios.is_empty();
    let mut sys = Atari5200::new(rom, bios, Atari5200Region::Ntsc).expect("init");
    // With a BIOS, the ATARI logo runs for ~255 frames before the BIOS
    // hands off to the cart (JMP ($BFFE)); run well past that so the cart
    // itself is driving the display.
    for _ in 0..320 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    let colours: std::collections::HashSet<u32> = fb.iter().copied().collect();
    let non_bg = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();

    if has_bios {
        // The cart has booted past the BIOS handoff and is rendering its
        // own screen. This exercises the whole chain end-to-end: the
        // two-chip 16 KB cart decode (so the entry vector lands on real
        // code) and ANTIC's DMA view of cart ROM + the BIOS character set
        // (so the display list, which lives in cart ROM, actually renders).
        // Before either fix this frame was uniform black.
        assert!(
            colours.len() >= 4 && non_bg >= 1000,
            "Atari 5200 cart should render a real frame after the BIOS \
             handoff; got {} colours / {non_bg} non-background px (cart: {})",
            colours.len(),
            path.display()
        );
    } else {
        // Cart-only boot (no BIOS): the reset vector is taken from the
        // `$BFFC` mirror; not every cart self-starts without the BIOS, so
        // only require that the pipeline produced some output.
        assert!(
            colours.len() >= 2,
            "Atari 5200 cart-only boot should produce >= 2 distinct \
             colours; got {} (cart: {})",
            colours.len(),
            path.display()
        );
    }
}
