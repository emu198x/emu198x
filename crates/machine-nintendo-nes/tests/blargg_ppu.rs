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
//! 2. `~/Projects/198x/assets/test-suites/nes-test-roms/`.
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
        eprintln!("blargg root not found; skipping {rel}");
        return;
    };
    let rom = root.join(rel);
    if !rom.is_file() {
        eprintln!("blargg ROM not present at {rom:?}; skipping");
        return;
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
//  sprite_hit_tests / sprite_overflow_tests — NOT WIRED
//
//  These test-rom suites use a DIFFERENT result-reporting protocol
//  than the standard blargg `$6000`/`$6004` shell. They write a
//  `result` byte at zero-page `$f8` and render pass/fail text to
//  the PPU framebuffer via a console subsystem (validation.a +
//  debug.a in the source tree). There is no PRG-RAM `$6000` status
//  byte or `DE B0 61` signature.
//
//  Detecting their results requires either: (a) framebuffer OCR, or
//  (b) reading the result byte at `$f8` and verifying the CPU has
//  entered the `forever` loop (the PC settles at a known address).
//  Both are doable but neither is plug-compatible with the existing
//  harness, so they're deferred. The 16 tests across sprite_hit_*
//  and sprite_overflow_* will land under a separate harness when
//  per-test PPU fidelity (sprite-0 hit pipeline, sprite overflow
//  scan) is being driven from CI.
//
//  See `knowledge/decisions/nes-architecture-review.md` Seam 1 for
//  the broader plan.
// ────────────────────────────────────────────────────────────────
