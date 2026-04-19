//! Pixel-exact golden-frame tests against FS-UAE captures.
//!
//! Each test boots a specific Amiga + Kickstart configuration to a known
//! frame count and asserts the framebuffer matches the corresponding
//! reference image in `tests/golden/`. Any deviation is a failure.
//!
//! Tests are `#[ignore]` because they depend on:
//!   - real Kickstart ROMs at `~/.emu198x/roms/commodore-amiga/kick*.rom`
//!   - golden PNGs in `tests/golden/`
//! Each test skips silently (with a stderr note) when its ROM or golden
//! is missing — never silently passes.
//!
//! Run with:
//!   cargo test -p machine-commodore-amiga \
//!       --test kickstart_golden_frames -- --ignored --nocapture
//!
//! See `wiki/processes/golden-image-capture.md` for capture procedure.

mod support;

use std::path::PathBuf;

use machine_commodore_amiga::Amiga;

use support::{assert_matches_golden, render_for_golden, FSUAE_H, FSUAE_W};

/// Frames to run before sampling. Matches the invariant-test boot point
/// where Kickstart 1.3 has reached the insert-disk screen.
const BOOT_FRAMES: u64 = 250;

fn rom_path(name: &str) -> PathBuf {
    let home = std::env::var("HOME").expect("HOME is set");
    PathBuf::from(home).join(".emu198x/roms/commodore-amiga").join(name)
}

fn golden_path(name: &str) -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root.join("tests/golden").join(name)
}

/// Skip helper. Returns Some((rom_bytes, golden_path)) when both exist,
/// else None and prints a skip note to stderr.
fn require_assets(rom_name: &str, golden_name: &str) -> Option<(Vec<u8>, PathBuf)> {
    let rom = rom_path(rom_name);
    let golden = golden_path(golden_name);
    if !rom.exists() {
        eprintln!("skipping: ROM missing at {}", rom.display());
        return None;
    }
    if !golden.exists() {
        eprintln!("skipping: golden missing at {}", golden.display());
        return None;
    }
    let bytes = std::fs::read(&rom).expect("read ROM");
    Some((bytes, golden))
}

fn run_and_compare(amiga: &mut Amiga, frames: u64, golden: &PathBuf) {
    for _ in 0..frames {
        amiga.run_frame();
    }
    let (pixels, w, h) = render_for_golden(amiga);
    assert_eq!(w, FSUAE_W, "FS-UAE-cropped width should be {FSUAE_W}");
    assert_eq!(h, FSUAE_H, "FS-UAE-cropped height should be {FSUAE_H}");

    let result = assert_matches_golden(&pixels, w, h, golden);
    if !result.matches {
        let fd = result
            .first_diff
            .as_ref()
            .map(|d| {
                format!(
                    " first diff at ({},{}): actual=${:08X} expected=${:08X}",
                    d.x, d.y, d.actual, d.expected
                )
            })
            .unwrap_or_default();
        panic!(
            "golden mismatch at {}: {} pixels differ (max channel delta {}).{}\n\
             Inspect <golden_stem>.actual.png and <golden_stem>.diff.png next to the golden.",
            golden.display(),
            result.differing_pixels,
            result.max_channel_delta,
            fd,
        );
    }
}

// ── A500 + KS 1.3 + 512K chip + 512K slow ──────────────────────────
//
// Currently the only configuration that boots reliably. The runtime
// hardcodes this configuration as a workaround for the chip-only bug.

#[test]
#[ignore]
fn golden_a500_ks13_512k_chip_512k_slow_frame250() {
    let Some((rom, golden)) =
        require_assets("kick13.rom", "a500-ks13-512k-chip-512k-slow-frame250.png")
    else {
        return;
    };
    let mut amiga = Amiga::new_with_slow_ram(rom, 512 * 1024);
    run_and_compare(&mut amiga, BOOT_FRAMES, &golden);
}

// ── A500 + KS 1.3 + 512K chip only ─────────────────────────────────
//
// Real A500 hardware. Currently fails to reach insert-disk because of
// the bug tracked in `wiki/decisions/amiga-chip-only-boot-failure.md`.
// The test will fail until the chip-only boot path is fixed — that's
// the point. Visually identical to the chip+slow golden per the FS-UAE
// captures, so once boot is fixed this will share its visible state.

#[test]
#[ignore]
fn golden_a500_ks13_512k_chip_frame250() {
    let Some((rom, golden)) = require_assets("kick13.rom", "a500-ks13-512k-chip-frame250.png")
    else {
        return;
    };
    let mut amiga = Amiga::new(rom);
    run_and_compare(&mut amiga, BOOT_FRAMES, &golden);
}

// ── A1000 + KS 1.2 + 512K chip ─────────────────────────────────────
//
// Same OCS chipset, different ROM. KS 1.2 (V33) has minor differences
// in the boot sequence vs 1.3 (V34) but reaches the same iconic
// insert-disk screen. This proves our emulator is robust to ROM-level
// variation, not just locked to one specific Kickstart binary.

#[test]
#[ignore]
fn golden_a1000_ks12_512k_chip_frame250() {
    let Some((rom, golden)) = require_assets("kick12.rom", "a1000-ks12-512k-chip-frame250.png")
    else {
        return;
    };
    // A1000 originally shipped with 256K chip RAM, but most got expanded
    // to 512K via the front expansion port, which is what the FS-UAE
    // capture uses. No slow RAM.
    let mut amiga = Amiga::new(rom);
    run_and_compare(&mut amiga, BOOT_FRAMES, &golden);
}

// ── ECS / AGA variants (all skipped until chipset support lands) ───
//
// The captures are wired up so they activate automatically as soon as
// the corresponding chipset variant of the machine crate is created.
// Each test documents exactly what's missing to enable it.

/// A600 + KS 2.05 + 1MB chip — ECS chipset (Fat Agnus, ECS Denise),
/// Gayle bridge for IDE. KS V37 series.
#[test]
#[ignore]
fn golden_a600_ks205_1m_chip_frame250() {
    eprintln!(
        "skipping: A600 / ECS / 1MB chip RAM not yet implemented \
         (this crate is OCS A500 only). Capture: \
         tests/golden/a600-ks205-1m-chip-frame250.png"
    );
}

/// A500+ + KS 2.04 + 1MB chip — ECS chipset, no Gayle (trapdoor still
/// available for slow RAM). The hardware bridge between OCS A500 and
/// the A600/A1200 line. KS V37.175.
#[test]
#[ignore]
fn golden_a500plus_ks204_1m_chip_frame250() {
    eprintln!(
        "skipping: A500+ / ECS / 1MB chip RAM not yet implemented \
         (this crate is OCS A500 only). Capture: \
         tests/golden/a500+-ks204-1m-chip-frame250.png"
    );
}

/// A1200 + KS 3.0 + 2MB chip — AGA chipset (Alice + Lisa), Gayle, no
/// Akiko. KS V39.106.
#[test]
#[ignore]
fn golden_a1200_ks30_2m_chip_frame250() {
    eprintln!(
        "skipping: A1200 / AGA / 2MB chip RAM not yet implemented \
         (this crate is OCS A500 only). Capture: \
         tests/golden/a1200-ks30-2m-chip-frame250.png"
    );
}

/// A1200 + KS 3.1 + 2MB chip — AGA chipset, KS V40.068. The most
/// commonly emulated A1200 configuration.
#[test]
#[ignore]
fn golden_a1200_ks31_2m_chip_frame250() {
    eprintln!(
        "skipping: A1200 / AGA / 2MB chip RAM not yet implemented \
         (this crate is OCS A500 only). Capture: \
         tests/golden/a1200-ks31-2m-chip-frame250.png"
    );
}
