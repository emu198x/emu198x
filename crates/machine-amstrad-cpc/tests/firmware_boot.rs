//! The real CPC464 firmware boots.
//!
//! Gated `#[ignore]` because the firmware is copyrighted and not in the tree.
//! Assemble it as MAME does — the 16 KB OS followed by the 16 KB BASIC, which
//! TOSEC splits across `Operating Systems/` and `Applications/` — and place it
//! at `~/.emu198x/roms/amstrad-cpc/cpc464.rom`. The staged copy is byte-verified
//! against MAME's own SHA1 for `cpc464.rom`.
//!
//! ```text
//! cargo test --release -p machine-amstrad-cpc --test firmware_boot -- --ignored
//! ```
//!
//! What makes this a boot rather than "the CPU did not crash" is the palette.
//! Nothing in this crate knows the CPC's startup colours: the firmware chooses
//! them and writes them to the Gate Array through `INKR`. Finding pen 0 blue,
//! pen 1 bright yellow and a blue border means the OS reached its screen setup
//! and the Gate Array's register decode agreed with it — the CPC464's own
//! livery, arrived at without being told.

use std::env;
use std::fs;
use std::path::PathBuf;

use amstrad_gate_array::{BORDER_PEN, VideoMode};
use machine_amstrad_cpc::AmstradCpc;

/// Hardware colour codes the CPC464 firmware sets for its own boot screen.
const BLUE: u8 = 4;
const BRIGHT_YELLOW: u8 = 10;

fn firmware_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_CPC_ROM") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/amstrad-cpc/cpc464.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs the 32 KB CPC464 firmware — run with --ignored"]
fn the_firmware_reaches_its_boot_screen() {
    let Some(path) = firmware_path() else {
        panic!(
            "CPC464 firmware not found — set EMU198X_CPC_ROM or place cpc464.rom \
             (16 KB OS + 16 KB BASIC) at ~/.emu198x/roms/amstrad-cpc/"
        );
    };
    let firmware = fs::read(&path).expect("read firmware");
    let mut cpc = AmstradCpc::new(&firmware).expect("build machine");

    for _ in 0..150 {
        cpc.run_frame();
    }

    let ga = cpc.gate_array();

    assert_eq!(
        ga.mode(),
        VideoMode::Mode1,
        "the CPC boots into mode 1 — 320x200, four colours"
    );
    assert_eq!(ga.pen_code(0), BLUE, "paper is blue");
    assert_eq!(ga.pen_code(1), BRIGHT_YELLOW, "ink is bright yellow");
    assert_eq!(ga.pen_code(BORDER_PEN), BLUE, "border matches the paper");

    assert!(
        ga.lower_rom_enabled(),
        "the OS stays paged in at $0000 while it runs"
    );

    // The interrupt counter only advances if the CRTC is generating syncs and
    // the Gate Array is counting them, so a moved counter means the whole
    // CRTC → Gate Array → Z80 interrupt path is live.
    assert!(
        ga.interrupt_counter() > 0,
        "the interrupt counter should be running"
    );

    // Screen RAM defaults to $C000-$FFFF. The boot message is only a few
    // hundred bytes of a 16 KB screen, so this is a floor rather than a target.
    let drawn = (0xC000u32..=0xFFFF)
        .filter(|&a| cpc.peek(a as u16) != 0)
        .count();
    assert!(
        drawn > 500,
        "expected the boot message in screen RAM, found {drawn} non-zero bytes"
    );

    // And that the screen reaches the framebuffer. Only two colours can appear
    // on a boot screen — the firmware programmed pens 0 and 1 and the border,
    // and set all three to blue or bright yellow.
    let paper = ga.pen_rgb(0);
    let ink = ga.pen_rgb(1);
    let fb = cpc.framebuffer();
    assert_eq!(
        fb.len(),
        (cpc.framebuffer_width() * cpc.framebuffer_height()) as usize
    );

    let stray = fb.iter().filter(|&&px| px != paper && px != ink).count();
    assert_eq!(stray, 0, "the boot screen is blue and yellow, nothing else");

    // The banner is a few hundred characters of text on an otherwise empty
    // screen, so ink is a small minority — but present. A framebuffer that
    // never got its display fetches would be uniformly paper and pass every
    // check above this one.
    let ink_dots = fb.iter().filter(|&&px| px == ink).count();
    assert!(
        (2_000..40_000).contains(&ink_dots),
        "expected the banner's text, found {ink_dots} ink dots of {}",
        fb.len()
    );
}
