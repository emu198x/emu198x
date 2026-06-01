//! SG-1000 cartridge boot smoke.
//!
//! Loads a single SG-1000 / Othello Multivision cart from the user's
//! local directory and verifies the title screen renders to a
//! non-trivial framebuffer within 300 frames. Gated `#[ignore]`
//! because cart ROMs are not shipped with the repo.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-sega-sg-1000 \
//!     --test cart_boot -- --ignored --nocapture
//! ```
//!
//! Cartridge source (first match wins):
//!   1. `EMU198X_SG_1000_CART` env var (full file path)
//!   2. `~/.emu198x/media/sega-sg-1000/*.sg`
//!   3. `~/Downloads/*.sg` (matches the live-capture path)

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sega_sg_1000::{Sg1000, Sg1000Region};

fn first_sg_in(dir: &PathBuf) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sg")))
}

fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_SG_1000_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let media = PathBuf::from(&home).join(".emu198x/media/sega-sg-1000");
    if let Some(p) = first_sg_in(&media) {
        return Some(p);
    }
    let downloads = PathBuf::from(home).join("Downloads");
    first_sg_in(&downloads)
}

#[test]
#[ignore = "needs an SG-1000 / Othello Multivision .sg cart — run with --ignored"]
fn cart_boots_to_title_screen() {
    let Some(path) = cart_path() else {
        panic!(
            "no SG-1000 cart found — set EMU198X_SG_1000_CART or place a .sg \
             file under ~/.emu198x/media/sega-sg-1000/ or ~/Downloads/"
        );
    };
    let cart = fs::read(&path).expect("read cart");
    assert!(
        cart.len() <= 0xC000,
        "cart {} is {} bytes; SG-1000 ceiling is 48 KB",
        path.display(),
        cart.len()
    );

    let mut sys = Sg1000::new(cart, Sg1000Region::Ntsc);
    for _ in 0..300 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 256 * 192);
    // TMS9918A has no anti-aliasing; many carts use only 3-4 palette
    // entries on the title screen (backdrop + 1-2 foreground colours).
    // Require >= 2 to catch the "framebuffer stays all backdrop" failure
    // mode without false-rejecting minimalist title cards.
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 16 {
            break;
        }
    }
    assert!(
        colours.len() >= 2,
        "framebuffer should have >= 2 distinct colours; got {} (cart: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "title screen should have >= 1024 non-backdrop pixels; got {non_zero}"
    );
}
