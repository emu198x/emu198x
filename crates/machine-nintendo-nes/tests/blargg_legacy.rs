//! Harness for *older* blargg/blargg-derived NES test ROMs that
//! predate the `$6000` result shell.
//!
//! These ROMs (e.g. `cpu_dummy_reads`) report results only on-screen
//! and over a bit-banged serial line on controller port 2 — there is
//! no `$6000` status byte or `DE B0 61` signature, so the standard
//! [`blargg_ppu`](super) harness sees them as an endless "running"
//! state. They share `common/ascii.chr`, whose tiles are laid out in
//! ASCII order, so the nametable holds the printed text directly:
//! `tests_passed` writes the literal string `"Passed"`.
//!
//! The harness runs the ROM until `"Passed"` appears in CIRAM (early
//! out — the common case) or a tick ceiling is hit, then asserts. A
//! genuine failure never prints `"Passed"`; the assertion message
//! dumps the on-screen text (the failing sub-test name) for triage.
//!
//! ROMs resolve under `assets/test-suites/nes-test-roms/` exactly as
//! in `blargg_ppu.rs`. Run with:
//!
//! ```sh
//! cargo test -p machine-nintendo-nes --test blargg_legacy -- --ignored
//! ```

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;
use std::path::PathBuf;

/// Tick ceiling before declaring a hang. These are short CPU tests;
/// `cpu_dummy_reads` prints its result within ~15M ticks. The ceiling
/// is only reached on a genuine failure (no `"Passed"` ever printed).
const MAX_TICKS: u64 = 80_000_000;

/// How often to scan CIRAM for the result string (in ticks). Scanning
/// every tick would dominate runtime; once per frame (~30k ticks) is
/// plenty since the text is written once and then static.
const SCAN_PERIOD: u64 = 30_000;

fn blargg_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("NES_BLARGG_ROOT") {
        let d = PathBuf::from(p);
        if d.is_dir() {
            return Some(d);
        }
    }
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

/// Decode CIRAM (2 KiB nametable RAM) to printable ASCII, joining
/// rows with spaces. The `ascii.chr` font maps tile index to ASCII,
/// so the byte value *is* the character.
fn ciram_text(nes: &Nes) -> String {
    nes.ppu
        .nametable_ram()
        .iter()
        .map(|&b| {
            if (0x20..=0x7E).contains(&b) {
                b as char
            } else {
                ' '
            }
        })
        .collect()
}

/// Outcome of running one legacy on-screen test ROM.
enum Legacy {
    Passed,
    /// Never printed "Passed" within the tick ceiling; carries the
    /// on-screen text (collapsed whitespace) for diagnostics.
    NoPass(String),
}

fn run_legacy(rom_path: &PathBuf) -> Result<Legacy, String> {
    let bytes = std::fs::read(rom_path).map_err(|e| format!("read {rom_path:?}: {e}"))?;
    let parsed = parse_ines(&bytes).map_err(|e| format!("parse {rom_path:?}: {e}"))?;
    let mut nes = Nes::new(parsed.mapper);

    let mut scan_at = SCAN_PERIOD;
    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        if nes.master_clock() >= scan_at {
            scan_at += SCAN_PERIOD;
            if ciram_text(&nes).contains("Passed") {
                return Ok(Legacy::Passed);
            }
        }
    }

    let text: String = ciram_text(&nes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Legacy::NoPass(text))
}

/// Look up one legacy ROM and run it, asserting it prints "Passed".
fn run_or_skip(rel: &str) {
    let Some(root) = blargg_root() else {
        eprintln!("blargg root not found; skipping {rel}");
        return;
    };
    let rom = root.join(rel);
    if !rom.is_file() {
        eprintln!("legacy ROM not present at {rom:?}; skipping");
        return;
    }
    match run_legacy(&rom).unwrap_or_else(|e| panic!("legacy run failed: {e}")) {
        Legacy::Passed => eprintln!("legacy test {rel} passed"),
        Legacy::NoPass(text) => {
            panic!("legacy test {rel} did not print \"Passed\"; screen: {text:?}")
        }
    }
}

// ────────────────────────────────────────────────────────────────
//  cpu_dummy_reads
//
//  Tests the dummy reads the 6502 performs before the real access on
//  LDA/STA with (zp,X), (zp),Y and abs,X — specifically the spurious
//  read at the un-fixed address when an index crosses a page boundary
//  (abs,X / (zp),Y) and the always-present dummy on the indexed
//  modes. The ROM detects each dummy read through its side effect on
//  the mirrored $2002 (a $2002 read clears the VBL flag), so passing
//  proves the CPU reads the right phantom address at the right time.
// ────────────────────────────────────────────────────────────────

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn cpu_dummy_reads() {
    run_or_skip("cpu_dummy_reads/cpu_dummy_reads.nes");
}
