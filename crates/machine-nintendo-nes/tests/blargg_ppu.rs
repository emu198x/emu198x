//! Blargg PPU test ROM harness.
//!
//! Runs Shay Green's canonical PPU regression bench through the NES
//! machine layer. Each ROM uses the standard "shell" protocol:
//!
//! - `$6000` holds the test result byte.
//!   - `0x80` = test in progress (default).
//!   - `0x00` = pass.
//!   - any other value = failure code (per-test).
//! - `$6001..=$6003` hold the signature `DE B0 61` once the test has
//!   produced a valid result. Reads before the signature is set are
//!   meaningless — the harness must wait for the signature.
//! - `$6004..` holds zero-terminated ASCII text output (test name,
//!   failure detail).
//!
//! Reference: https://www.nesdev.org/knowledge/Emulator_tests
//!
//! ROMs live under `assets/test-suites/nes-test-roms/` in the 198x
//! umbrella. The harness resolves them in this order:
//!
//! 1. `NES_BLARGG_ROOT` env var (directory containing the blargg subdirs).
//! 2. `assets/test-suites/nes-test-roms/`.
//!
//! If neither resolves, the per-ROM test is a no-op (skipped). When the
//! ROM is present, the test is `#[ignore]`-by-default — run with
//! `cargo test -p machine-nintendo-nes --test blargg_ppu -- --ignored`.

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;
use std::path::PathBuf;

/// Maximum master-clock ticks to give a blargg ROM before declaring a
/// hang. Most PPU tests complete within ~30M ticks (~5 s emulated),
/// but oam_stress runs ~30 s by design (it stresses OAM access for
/// tens of seconds), so the ceiling needs ~200M ticks of headroom.
const MAX_TICKS: u64 = 250_000_000;

/// Resolve the blargg ROM root directory.
fn blargg_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("NES_BLARGG_ROOT") {
        let d = PathBuf::from(p);
        if d.is_dir() {
            return Some(d);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
        if d.is_dir() {
            return Some(d);
        }
    }
    None
}

/// Outcome of running one blargg test ROM.
#[derive(Debug)]
struct BlarggResult {
    /// The final result byte at `$6000`. 0 = pass, non-zero = failure.
    code: u8,
    /// Zero-terminated text the ROM wrote at `$6004`.
    text: String,
    /// Master-clock ticks consumed.
    ticks: u64,
}

/// Run one blargg test ROM. Returns `Ok(BlarggResult)` if the test
/// signalled completion within `MAX_TICKS`, or `Err(...)` if it
/// failed to parse, the mapper is unsupported, or the test hung.
fn run_blargg(rom_path: &PathBuf) -> Result<BlarggResult, String> {
    let bytes = std::fs::read(rom_path).map_err(|e| format!("read {rom_path:?}: {e}"))?;
    let parsed = parse_ines(&bytes).map_err(|e| format!("parse {rom_path:?}: {e}"))?;
    let mut nes = Nes::new(parsed.mapper);

    let mut signature_seen = false;
    for _ in 0..MAX_TICKS {
        nes.tick();

        // Cheap signature check every tick — three byte reads.
        if !signature_seen
            && nes.peek(0x6001) == 0xDE
            && nes.peek(0x6002) == 0xB0
            && nes.peek(0x6003) == 0x61
        {
            signature_seen = true;
        }

        if signature_seen {
            let status = nes.peek(0x6000);
            if status != 0x80 {
                // Test complete. Drain the text buffer.
                let mut text = Vec::new();
                let mut addr: u16 = 0x6004;
                // Cap at 1 KiB of text to avoid runaway loops on
                // ROMs with a broken text buffer.
                for _ in 0..1024 {
                    let b = nes.peek(addr);
                    if b == 0 {
                        break;
                    }
                    text.push(b);
                    addr = addr.wrapping_add(1);
                }
                return Ok(BlarggResult {
                    code: status,
                    text: String::from_utf8_lossy(&text).into_owned(),
                    ticks: nes.master_clock(),
                });
            }
        }
    }

    Err(format!(
        "blargg ROM {rom_path:?} did not signal completion within {MAX_TICKS} ticks"
    ))
}

/// Look up one blargg ROM and run it, asserting pass.
fn run_or_skip(rel: &str) {
    let Some(root) = blargg_root() else {
        emu198x_test_skip::skip!("blargg root not found; skipping {rel}");
    };
    let rom = root.join(rel);
    if !rom.is_file() {
        emu198x_test_skip::skip!("blargg ROM not present at {rom:?}; skipping");
    }
    let result = run_blargg(&rom).unwrap_or_else(|e| panic!("blargg run failed: {e}"));
    assert_eq!(
        result.code, 0,
        "blargg test {rel} failed with code 0x{:02X} after {} ticks; text: {:?}",
        result.code, result.ticks, result.text
    );
    eprintln!(
        "blargg test {rel} passed in {} ticks; text: {:?}",
        result.ticks, result.text
    );
}

// ────────────────────────────────────────────────────────────────
//  ppu_vbl_nmi (10 sub-tests)
// ────────────────────────────────────────────────────────────────

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_01_vbl_basics() {
    run_or_skip("ppu_vbl_nmi/rom_singles/01-vbl_basics.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_02_vbl_set_time() {
    run_or_skip("ppu_vbl_nmi/rom_singles/02-vbl_set_time.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_03_vbl_clear_time() {
    run_or_skip("ppu_vbl_nmi/rom_singles/03-vbl_clear_time.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_04_nmi_control() {
    run_or_skip("ppu_vbl_nmi/rom_singles/04-nmi_control.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_05_nmi_timing() {
    run_or_skip("ppu_vbl_nmi/rom_singles/05-nmi_timing.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_06_suppression() {
    run_or_skip("ppu_vbl_nmi/rom_singles/06-suppression.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_07_nmi_on_timing() {
    run_or_skip("ppu_vbl_nmi/rom_singles/07-nmi_on_timing.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_08_nmi_off_timing() {
    run_or_skip("ppu_vbl_nmi/rom_singles/08-nmi_off_timing.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_09_even_odd_frames() {
    run_or_skip("ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_vbl_nmi_10_even_odd_timing() {
    run_or_skip("ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes");
}

// ────────────────────────────────────────────────────────────────
//  oam_read, oam_stress
// ────────────────────────────────────────────────────────────────

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn oam_read() {
    run_or_skip("oam_read/oam_read.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn oam_stress() {
    run_or_skip("oam_stress/oam_stress.nes");
}

// ────────────────────────────────────────────────────────────────
//  sprite_hit_tests / sprite_overflow_tests — WIRED in ppu_onscreen.rs
//
//  These 2005 suites use a different result protocol than the `$6000`
//  shell: a `result` byte at zero-page `$f8` (1 = pass), printed to
//  the screen and beeped, with no `DE B0 61` signature. They are
//  graded by the `tests/ppu_onscreen.rs` harness, which runs each ROM
//  until `$f8` settles into its report/forever loop and asserts the
//  value is 1. All 11 sprite_hit and all 5 sprite_overflow ROMs pass.
//
//  See `knowledge/decisions/nes-architecture-review.md` Seam 1 for
//  the broader plan.
// ────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────
//  mmc3_test (MMC3 scanline counter / IRQ)
//
//  Shay Green's MMC3 IRQ bench. Each numbered ROM is itself a
//  multi-sub-test that reports the first failing sub-test's code via
//  the standard `$6000` shell, so a code of 0 means every sub-test
//  in that ROM passed.
//
//  These exercise the PPU A12 line driving the mapper IRQ counter:
//  the counter is clocked by debounced A12 rising edges, both during
//  rendering fetches and when the CPU toggles A12 through `$2006`
//  during forced blank. The mapper's low-duration filter (Mesen's
//  `_a12LowClock`) rejects the rapid per-sprite toggles while
//  counting the one clean rise per scanline.
//
//  `6-MMC6` is intentionally NOT wired: it tests the MMC3 *revision
//  A* IRQ behaviour (don't re-fire when the latch reloads 0 after the
//  counter normally reached 0), which directly contradicts `5-MMC3`
//  (fire on every clock when the latch is 0). Both ROMs are mapper 4
//  / submapper 0 with identical headers, so they cannot be told
//  apart — real hardware splits on the specific chip die. Standard
//  MMC3B emulation passes `5-MMC3` and cannot also pass `6-MMC6`
//  without a per-ROM chip database (which Mesen has and we do not).
// ────────────────────────────────────────────────────────────────

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn mmc3_1_clocking() {
    run_or_skip("mmc3_test/1-clocking.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn mmc3_2_details() {
    run_or_skip("mmc3_test/2-details.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn mmc3_3_a12_clocking() {
    run_or_skip("mmc3_test/3-A12_clocking.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn mmc3_4_scanline_timing() {
    run_or_skip("mmc3_test/4-scanline_timing.nes");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn mmc3_5_mmc3() {
    run_or_skip("mmc3_test/5-MMC3.nes");
}

// ────────────────────────────────────────────────────────────────
//  ppu_read_buffer (Bisqwit's mammoth $2007 read-buffer suite)
//
//  ~80 sub-tests covering the $2007 read buffer (one-byte delay for
//  $0000-$3EFF, immediate for palette $3F00-$3FFF), sequential reads
//  with 1- and 32-byte increments, CIRAM/nametable mirroring, and
//  PPU address decoding. Reports the first failing sub-test through
//  the standard $6000 shell.
//
//  The ROM is assembled to CNROM (mapper 3) but writes its shell
//  block to $6000-$7FFF WRAM, which production CNROM boards lack —
//  our CNROM port carries the RAM for this reason (as NROM does).
// ────────────────────────────────────────────────────────────────

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn ppu_read_buffer() {
    run_or_skip("ppu_read_buffer/test_ppu_read_buffer.nes");
}
