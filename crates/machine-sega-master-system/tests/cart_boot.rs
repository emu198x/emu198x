//! SMS cart boot smoke.
//!
//! Loads a single SMS cart from the user's local media directory and
//! verifies the title screen renders a non-trivial framebuffer within
//! 600 NTSC frames. Gated `#[ignore]` because cart ROMs are not
//! shipped in-tree.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-sega-master-system \
//!     --test cart_boot -- --ignored --nocapture
//! ```
//!
//! Cart source (first match wins):
//!   1. `EMU198X_SMS_CART` env var (full file path)
//!   2. First `.sms` / `.bin` file under `~/.emu198x/media/sega-master-system/`

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sega_master_system::{Sms, SmsVariant};

fn first_sms_cart(dir: &PathBuf) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("sms" | "SMS" | "bin" | "BIN")
            )
        })
        .collect();
    paths.sort();
    paths.into_iter().next()
}

fn cart_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_SMS_CART") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let media = PathBuf::from(home).join(".emu198x/media/sega-master-system");
    first_sms_cart(&media)
}

#[test]
#[ignore = "needs an SMS cart — run with --ignored"]
fn cart_boots_to_title_screen() {
    let Some(path) = cart_path() else {
        panic!(
            "no SMS cart found — set EMU198X_SMS_CART or place a .sms / .bin \
             file under ~/.emu198x/media/sega-master-system/"
        );
    };
    let cart = fs::read(&path).expect("read cart");
    // Strip 512-byte SMD header if present.
    let cart = if cart.len() % 0x4000 == 0x200 {
        cart[0x200..].to_vec()
    } else {
        cart
    };

    let mut sys = Sms::new(cart, SmsVariant::SmsNtsc);
    for _ in 0..600 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(
        fb.len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 16 {
            break;
        }
    }
    // SMS Mode 4 title screens routinely use 8-32 distinct colours.
    assert!(
        colours.len() >= 4,
        "title screen should have >= 4 distinct colours; got {} (cart: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 2048,
        "title screen should have >= 2048 non-backdrop pixels; got {non_zero}"
    );
}
