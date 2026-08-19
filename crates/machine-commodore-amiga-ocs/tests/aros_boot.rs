//! AROS m68k boots — the Amiga's free-firmware evidence.
//!
//! Commodore's Kickstart cannot be distributed, so the Amiga had no
//! boot evidence of the kind the C64 and 800XL have. [AROS](https://aros.org)
//! is an AmigaOS reimplementation under the APL, redistributable, and it
//! boots this emulator — but only since the extended-ROM window was
//! implemented (#1022).
//!
//! ## Why one ROM was never going to be enough
//!
//! AROS m68k spans **two** ROM windows: `aros-amiga-m68k-rom.bin` at
//! `$F80000` and `aros-amiga-m68k-ext.bin` at `$E00000`. Supplying only the
//! first runs half an operating system, and it behaves like one — the CPU
//! stays in ROM and renders a flat field forever.
//!
//! The control test below is that state, kept deliberately: it is what
//! #1022 looked like, and it is the difference the second window makes.
//!
//! ## Fixtures
//!
//! Provisioned from `EMU198X_ROMS_ROOT`, joining `commodore-amiga/`. The
//! images are APL-licensed and redistributable, but are not staged in the
//! corpora store yet, so this skips in CI rather than failing.

use std::path::PathBuf;

use machine_commodore_amiga_ocs::{AmigaOcs, ExtendedRomWindow, RamConfig};

const PAL_FRAME_TICKS: u64 = 3_546_895 / 50;

fn rom_dir() -> Option<PathBuf> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    Some(root.join("commodore-amiga"))
}

fn booted(with_extended: bool) -> Option<AmigaOcs> {
    let dir = rom_dir()?;
    let rom = std::fs::read(dir.join("aros-amiga-m68k-rom.bin")).ok()?;
    let ext = std::fs::read(dir.join("aros-amiga-m68k-ext.bin")).ok()?;

    let mut amiga = AmigaOcs::with_ram_config(
        rom,
        RamConfig {
            chip_kb: 512,
            slow_kb: 512,
            fast_kb: 0,
        },
    );
    if with_extended {
        amiga.install_extended_rom(ExtendedRomWindow::E00000, ext);
    }
    // Settled by 1500 frames: the same frame comes out at 3000.
    for _ in 0..(1500 * PAL_FRAME_TICKS) {
        amiga.tick();
    }
    Some(amiga)
}

/// Distinct colours in the frame, and how many rows hold anything other
/// than the dominant one. A machine painting a flat field scores 1 and 0;
/// a machine drawing a screen scores neither.
fn frame_shape(amiga: &AmigaOcs) -> (usize, usize) {
    let frame = amiga.denise().framebuffer();
    let (width, _) = amiga.denise().framebuffer_size();
    let mut counts = std::collections::HashMap::new();
    for &pixel in frame {
        *counts.entry(pixel).or_insert(0usize) += 1;
    }
    let dominant = counts
        .iter()
        .max_by_key(|&(_, n)| *n)
        .map(|(&colour, _)| colour)
        .unwrap_or(0);
    let rows = frame
        .chunks(width as usize)
        .filter(|row| row.iter().any(|&pixel| pixel != dominant))
        .count();
    (counts.len(), rows)
}

#[test]
#[ignore = "needs AROS at <EMU198X_ROMS_ROOT>/commodore-amiga/aros-amiga-m68k-{rom,ext}.bin"]
fn aros_boots_when_both_rom_halves_are_fitted() {
    let Some(amiga) = booted(true) else {
        emu198x_test_skip::skip!("AROS m68k not staged at <EMU198X_ROMS_ROOT>/commodore-amiga/");
    };

    let (colours, rows) = frame_shape(&amiga);
    // 15 colours over 106 rows in practice. The thresholds are far below
    // that and far above the flat field the control produces.
    assert!(
        colours > 4,
        "AROS should render a screen, not a flat field; got {colours} distinct colours"
    );
    assert!(
        rows > 50,
        "and it should cover the display, not a band; got {rows} rows with content"
    );
}

/// Only the `$F80000` half fitted — half an operating system.
///
/// This is exactly the state #1022 described, and it is kept as a test
/// because it is the evidence that the extended-ROM window is what changed:
/// same image, same RAM, same frame count, and a flat field instead of a
/// screen.
#[test]
#[ignore = "needs AROS at <EMU198X_ROMS_ROOT>/commodore-amiga/aros-amiga-m68k-{rom,ext}.bin"]
fn the_main_rom_alone_renders_a_flat_field() {
    let Some(amiga) = booted(false) else {
        emu198x_test_skip::skip!("AROS m68k not staged at <EMU198X_ROMS_ROOT>/commodore-amiga/");
    };

    let (colours, rows) = frame_shape(&amiga);
    assert!(
        colours <= 2,
        "without its extended ROM AROS cannot draw a screen; got {colours} colours"
    );
    // Not zero: the same two-row blanking band a real Kickstart leaves
    // black is still there. What is absent is a drawn screen.
    assert!(
        rows < 10,
        "and nothing but the blanking band should vary; got {rows} rows"
    );
}
