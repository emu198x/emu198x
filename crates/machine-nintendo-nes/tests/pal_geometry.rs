//! PAL frame geometry, asserted against the documented 2C07 numbers.
//!
//! ⚠ These exist because PAL support previously had **no gate on its
//! video side at all**. `pal_apu_tests` exercises the APU and says
//! nothing about scanline count, VBLANK length, or the dot skip; the
//! geometry was wired, self-consistent, and entirely unasserted.
//!
//! This is a geometry gate, not a video comparison. It proves the PAL
//! machine counts dots, lines and CPU cycles the way a 2C07 does. It
//! does not prove that what those dots contain is correct — that needs a
//! reference framebuffer, which is separate work.
//!
//! Reference values (NESdev, PPU frame timing):
//!
//! | | NTSC (2C02) | PAL (2C07) |
//! |---|---|---|
//! | dots per scanline | 341 | 341 |
//! | scanlines per frame | 262 | 312 |
//! | dots per frame | 89 342 | 106 392 |
//! | VBLANK scanlines | 241–260 (20) | 241–310 (70) |
//! | dots per CPU cycle | 3 | 3.2 |
//! | CPU cycles per frame | 29 780.67 | 33 247.5 |
//! | odd-frame dot skip | yes | no |

use format_nintendo_nes_ines::{Mapper, parse_ines};
use machine_nintendo_nes::{Nes, Region};
use std::path::PathBuf;

const DOTS_PER_SCANLINE: u64 = 341;
const NTSC_SCANLINES: u64 = 262;
const PAL_SCANLINES: u64 = 312;

/// Any ROM will do — geometry is a property of the machine, not the
/// cartridge. This one is NROM and boots without a mapper in the way.
fn load() -> Option<Box<dyn Mapper>> {
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home)
        .join("Projects/198x/assets/test-suites/nes-test-roms")
        .join("cpu_timing_test6/cpu_timing_test.nes");
    let bytes = std::fs::read(p).ok()?;
    Some(parse_ines(&bytes).ok()?.mapper)
}

/// Count dots between two crossings of `(scanline, dot) == (0, 0)`.
fn dots_per_frame(nes: &mut Nes) -> u64 {
    // Advance to a frame boundary first, so the count excludes boot.
    while !(nes.ppu.scanline() == 0 && nes.ppu.dot() == 0) {
        nes.tick();
    }
    let start = nes.master_clock();
    nes.tick();
    while !(nes.ppu.scanline() == 0 && nes.ppu.dot() == 0) {
        nes.tick();
    }
    nes.master_clock() - start
}

#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn pal_frame_is_312_scanlines() {
    let Some(mapper) = load() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let mut nes = Nes::new_with_region(mapper, Region::Pal);
    assert_eq!(
        nes.ppu.pre_render_line(),
        311,
        "PAL pre-render line is 311, making 312 scanlines"
    );
    let dots = dots_per_frame(&mut nes);
    assert_eq!(
        dots,
        DOTS_PER_SCANLINE * PAL_SCANLINES,
        "PAL frame is 341 × 312 = 106 392 dots"
    );
}

#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn ntsc_frame_is_262_scanlines() {
    let Some(mapper) = load() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let mut nes = Nes::new(mapper);
    assert_eq!(nes.ppu.pre_render_line(), 261);
    let dots = dots_per_frame(&mut nes);
    assert_eq!(
        dots,
        DOTS_PER_SCANLINE * NTSC_SCANLINES,
        "NTSC frame is 341 × 262 = 89 342 dots"
    );
}

/// ⚠ The whole point of disabling the dot skip on PAL. On NTSC an odd
/// frame with rendering enabled is 340 dots; the 2C07 has no short
/// frame, so **every** PAL frame must be the full 341 × 312.
///
/// This ROM leaves rendering enabled, which is the condition the skip
/// depends on — with rendering off, NTSC would not skip either and the
/// test would pass without proving anything.
#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn pal_never_skips_a_dot() {
    let Some(mapper) = load() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let mut nes = Nes::new_with_region(mapper, Region::Pal);
    // Let the ROM enable rendering before measuring.
    for _ in 0..30 {
        nes.run_frame();
    }
    let mut lengths = Vec::new();
    for _ in 0..8 {
        lengths.push(dots_per_frame(&mut nes));
    }
    assert!(
        lengths
            .iter()
            .all(|&d| d == DOTS_PER_SCANLINE * PAL_SCANLINES),
        "every PAL frame must be 106 392 dots, saw {lengths:?}"
    );
}

/// ⚠ The control for `pal_never_skips_a_dot`. If this ROM never enabled
/// rendering, NTSC would not skip either, and that test would pass while
/// proving nothing about PAL. Asserting that NTSC *does* produce a
/// 340-dot frame here is what gives the PAL assertion its meaning.
#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn ntsc_does_skip_a_dot_on_odd_frames() {
    let Some(mapper) = load() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let mut nes = Nes::new(mapper);
    for _ in 0..30 {
        nes.run_frame();
    }
    let mut lengths = Vec::new();
    for _ in 0..8 {
        lengths.push(dots_per_frame(&mut nes));
    }
    let short = DOTS_PER_SCANLINE * NTSC_SCANLINES - 1;
    assert!(
        lengths.contains(&short),
        "expected at least one 340-dot odd frame on NTSC with rendering \
         enabled, saw {lengths:?} — without one, pal_never_skips_a_dot \
         asserts nothing"
    );
}

/// PAL's VBLANK is 70 scanlines (241–310) against NTSC's 20 (241–260).
/// Measured by counting dots between the VBLANK flag rising and the
/// pre-render line clearing it.
#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn pal_vblank_spans_70_scanlines() {
    let Some(mapper) = load() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let mut nes = Nes::new_with_region(mapper, Region::Pal);
    for _ in 0..30 {
        nes.run_frame();
    }
    // Advance to the start of VBLANK.
    while !(nes.ppu.scanline() == 241 && nes.ppu.dot() == 1) {
        nes.tick();
    }
    let start = nes.master_clock();
    // Run to the pre-render line, where the flag clears.
    while !(nes.ppu.scanline() == 311 && nes.ppu.dot() == 1) {
        nes.tick();
    }
    let dots = nes.master_clock() - start;
    assert_eq!(
        dots,
        DOTS_PER_SCANLINE * 70,
        "PAL VBLANK spans scanlines 241-310 = 70 lines"
    );
}

/// The clock ratio itself: 16 master units per CPU cycle against 5 per
/// dot gives 3.2 dots per cycle, so a 106 392-dot frame contains
/// 33 247.5 CPU cycles. Measured over two frames to land on a whole
/// number and prove the half-cycle is real rather than rounded away.
#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn pal_cpu_runs_at_one_cycle_per_3_2_dots() {
    let Some(mapper) = load() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let mut nes = Nes::new_with_region(mapper, Region::Pal);
    while !(nes.ppu.scanline() == 0 && nes.ppu.dot() == 0) {
        nes.tick();
    }
    let (dot0, cyc0) = (nes.master_clock(), nes.cpu_cycle_count());
    for _ in 0..2 {
        nes.tick();
        while !(nes.ppu.scanline() == 0 && nes.ppu.dot() == 0) {
            nes.tick();
        }
    }
    let dots = nes.master_clock() - dot0;
    let cycles = nes.cpu_cycle_count() - cyc0;
    assert_eq!(dots, 2 * DOTS_PER_SCANLINE * PAL_SCANLINES);
    assert_eq!(
        cycles, 66_495,
        "two PAL frames are 212 784 dots / 3.2 = 66 495 CPU cycles"
    );
}
