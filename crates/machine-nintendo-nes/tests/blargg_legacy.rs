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

/// How one legacy ROM is driven and graded.
///
/// Defaults describe the common case: no buttons, and the `$6000`-era
/// shell's mixed-case `"Passed"`.
struct LegacyRun<'a> {
    /// Controller-1 buttons held for the whole run (bits per
    /// `Nes::set_controller1`).
    ///
    /// ⚠ Held, not pressed. A ROM that selects a mode from the pad
    /// reads it during init, and releasing before that read drops
    /// silently back to the default mode — which looks exactly like a
    /// pass on the mode you thought you selected.
    held: u8,
    /// Token the ROM prints on success. blargg's older `console.a`
    /// shell prints upper-case `"PASSED"`, not `"Passed"`.
    pass_token: &'a str,
    /// Per-run tick ceiling. Overridden for ROMs whose runtime depends
    /// on the mode selected — `cpu_timing_test` takes ~55M ticks over
    /// the official set but ~88M over the undocumented one, so a single
    /// global ceiling would report the wider mode as a failure while
    /// showing its banner correctly.
    max_ticks: u64,
}

impl Default for LegacyRun<'_> {
    fn default() -> Self {
        Self {
            held: 0,
            pass_token: "Passed",
            max_ticks: MAX_TICKS,
        }
    }
}

fn run_legacy(rom_path: &PathBuf, opts: &LegacyRun<'_>) -> Result<Legacy, String> {
    let bytes = std::fs::read(rom_path).map_err(|e| format!("read {rom_path:?}: {e}"))?;
    let parsed = parse_ines(&bytes).map_err(|e| format!("parse {rom_path:?}: {e}"))?;
    let mut nes = Nes::new(parsed.mapper);
    nes.set_controller1(opts.held);

    let mut scan_at = SCAN_PERIOD;
    while nes.master_clock() < opts.max_ticks {
        nes.tick();
        if nes.master_clock() >= scan_at {
            scan_at += SCAN_PERIOD;
            if ciram_text(&nes).contains(opts.pass_token) {
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

/// Look up one legacy ROM and run it, asserting it prints its pass token.
fn run_or_skip_with(rel: &str, opts: &LegacyRun<'_>) {
    let Some(root) = blargg_root() else {
        emu198x_test_skip::skip!("blargg root not found; skipping {rel}");
    };
    let rom = root.join(rel);
    if !rom.is_file() {
        emu198x_test_skip::skip!("legacy ROM not present at {rom:?}; skipping");
    }
    match run_legacy(&rom, opts).unwrap_or_else(|e| panic!("legacy run failed: {e}")) {
        Legacy::Passed => eprintln!("legacy test {rel} passed"),
        Legacy::NoPass(text) => {
            panic!(
                "legacy test {rel} did not print {:?}; screen: {text:?}",
                opts.pass_token
            )
        }
    }
}

/// Look up one legacy ROM and run it, asserting it prints "Passed".
fn run_or_skip(rel: &str) {
    run_or_skip_with(rel, &LegacyRun::default());
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

// ────────────────────────────────────────────────────────────────
//  cpu_timing_test6
//
//  Instruction timing for every official and undocumented opcode
//  except the branches and the 12 HLTs, in both the normal and
//  page-crossing cases. The ROM selects its instruction set from
//  controller 1 during init:
//
//      (nothing)  official only
//      A          official + $EB + the unofficial NOPs
//      B          official + all undocumented
//
//  ⚠ The sweep boots this ROM with no buttons, so it only ever
//  covered the official set. The two held-button modes below are the
//  undocumented-opcode timing coverage, and they are the point of
//  wiring it here rather than leaving it to the sweep.
//
//  ⚠ Each gate asserts the ROM's own mode banner as well as PASSED.
//  Without that, a button-hold that failed to register would drop the
//  ROM back to official-only and still print PASSED — a green test
//  proving nothing about the set it claims to cover.
//
//  It reports on-screen only (no $6000, no result byte) using the
//  older `console.a` shell, which prints in UPPER CASE. Its
//  `readme.txt` carries blargg's authoritative cycle tables.
// ────────────────────────────────────────────────────────────────

const CPU_TIMING_ROM: &str = "cpu_timing_test6/cpu_timing_test.nes";

/// Assert the ROM printed both its mode banner and `PASSED`, so the
/// gate cannot pass on a mode it did not actually run.
fn cpu_timing_mode(held: u8, banner: &str) {
    let Some(root) = blargg_root() else {
        emu198x_test_skip::skip!("blargg root not found; skipping {CPU_TIMING_ROM}");
    };
    let rom = root.join(CPU_TIMING_ROM);
    if !rom.is_file() {
        emu198x_test_skip::skip!("legacy ROM not present at {rom:?}; skipping");
    }
    let opts = LegacyRun {
        held,
        pass_token: "PASSED",
        // The undocumented set settles at ~88M ticks; allow headroom.
        max_ticks: 130_000_000,
    };
    match run_legacy(&rom, &opts).unwrap_or_else(|e| panic!("legacy run failed: {e}")) {
        Legacy::Passed => {}
        Legacy::NoPass(text) => {
            panic!("cpu_timing_test ({banner}) did not print \"PASSED\"; screen: {text:?}")
        }
    }
    // Re-run only far enough to read the banner back. Cheap relative to
    // the ~16 s test itself, and it is the only thing standing between
    // this gate and a false green.
    let bytes = std::fs::read(&rom).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    nes.set_controller1(held);
    while nes.master_clock() < 20_000_000 && !ciram_text(&nes).contains(banner) {
        nes.tick();
    }
    assert!(
        ciram_text(&nes).contains(banner),
        "cpu_timing_test never showed the {banner:?} banner — the held button \
         ({held:#04X}) did not select the intended instruction set"
    );
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn cpu_timing_official() {
    cpu_timing_mode(0x00, "OFFICIAL INSTRUCTIONS ONLY");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn cpu_timing_official_plus_nops() {
    cpu_timing_mode(0x01, "OFFICIAL + NOP");
}

#[test]
#[ignore = "blargg ROM run — requires test-suites/nes-test-roms"]
fn cpu_timing_all_undocumented() {
    cpu_timing_mode(0x02, "OFFICIAL + UNDOCUMENTED");
}
