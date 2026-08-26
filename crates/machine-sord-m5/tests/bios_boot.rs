//! Sord M5 boot smoke — Monitor ROM + cartridge through the Z80 CTC.
//!
//! The M5 Monitor ROM alone leaves the screen blank: it needs a cartridge
//! (BASIC-I is one) to paint anything. This test loads the Monitor ROM and
//! a cart, runs ~10 seconds of frames, and verifies the boot reaches a real
//! rendered screen.
//!
//! Crucially it exercises the **Z80 CTC** path: the BIOS programs the CTC,
//! the TMS9918A `/INT` line drives CTC channel 3, and the IM 2 interrupt it
//! raises is what carries the boot. Before the CTC was wired (and before the
//! CTC/VDP/PSG I/O ports were corrected) the machine stalled at VDP init on
//! an all-backdrop black screen.
//!
//! Gated `#[ignore]` because the Monitor ROM and carts are copyrighted and
//! not shipped in-tree.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-sord-m5 \
//!     --test bios_boot -- --ignored --nocapture
//! ```
//!
//! BIOS source (first match wins):
//!   1. `EMU198X_SORD_M5_BIOS` env var (full file path)
//!   2. `~/.emu198x/roms/sord-m5/sord-m5.rom`
//!
//! Cart source: `~/.emu198x/media/sord-m5/{basic-i.bin,dig-dug.bin}`.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sord_m5::{M5Region, SordM5, VDP_INT_CTC_CHANNEL};

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_SORD_M5_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/sord-m5/sord-m5.rom");
    p.exists().then_some(p)
}

fn cart() -> Option<Vec<u8>> {
    let home = env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".emu198x/media/sord-m5");
    for name in ["basic-i.bin", "dig-dug.bin"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(fs::read(&p).expect("read cart"));
        }
    }
    None
}

#[test]
#[ignore = "FIXTURE: needs Sord M5 BIOS + cart — run with --ignored"]
fn boots_through_ctc_to_a_rendered_screen() {
    let Some(path) = bios_path() else {
        panic!(
            "Sord M5 BIOS not found — set EMU198X_SORD_M5_BIOS or place sord-m5.rom \
             at ~/.emu198x/roms/sord-m5/"
        );
    };
    let bios = fs::read(&path).expect("read BIOS");
    assert_eq!(bios.len(), 8192, "BIOS must be exactly 8 KB");
    let cart = cart()
        .expect("need a Sord M5 cart (basic-i.bin or dig-dug.bin) in ~/.emu198x/media/sord-m5/");

    let mut sys = SordM5::new(bios, cart, M5Region::Ntsc);
    for _ in 0..600 {
        sys.run_frame();
    }

    // The BIOS must have programmed the VDP-interrupt CTC channel as an
    // interrupt-enabled counter — the frame interrupt vectors through it.
    let ctc = sys.ctc();
    assert!(
        ctc.running(VDP_INT_CTC_CHANNEL) && ctc.int_enabled(VDP_INT_CTC_CHANNEL),
        "BIOS should arm CTC channel {VDP_INT_CTC_CHANNEL} with interrupts enabled"
    );
    // IM 2 vector high byte (I register) is $70 on the M5.
    assert_eq!(sys.cpu().regs.i, 0x70, "IM 2 vector high byte");

    // The cart must paint a real screen, not the all-backdrop black of the
    // pre-CTC stall.
    let fb = sys.framebuffer();
    assert_eq!(
        fb.len() as u32,
        sys.framebuffer_width() * sys.framebuffer_height()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    let distinct: std::collections::HashSet<u32> = fb.iter().copied().collect();
    // The pre-CTC stall left the screen all-backdrop (0 non-zero, 1 colour).
    // BASIC-I's "Ready" prompt is sparse white-on-black (~209 px, 2 colours);
    // Dig Dug paints thousands of pixels in 8 colours. Either clears this.
    assert!(
        non_zero >= 128 && distinct.len() >= 2,
        "cart should render a real screen; got {non_zero} non-backdrop px, \
         {} distinct colours",
        distinct.len()
    );
}
