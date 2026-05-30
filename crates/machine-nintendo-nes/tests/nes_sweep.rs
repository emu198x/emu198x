//! NES test-rom sweep across the directories we haven't wired into
//! specific harnesses yet. Loads each ROM, runs the standard blargg
//! `$6000` (DE B0 61) result protocol, and prints a pass / fail /
//! timeout table grouped by directory.
//!
//! Single `#[ignore]` test — run with:
//! `cargo test --release -p machine-nintendo-nes --test nes_sweep \
//!     -- --ignored --nocapture`

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;
use std::path::{Path, PathBuf};

/// Per-ROM master-clock ceiling. 150M ≈ 51 s emulated — generous
/// for every blargg CPU/APU test we've seen except `oam_stress`,
/// which already has its own harness with a higher cap. The
/// apu_reset / cpu_interrupts_v2 sub-tests can take ~40-50 s when
/// they include 100-ms reset-button delays as part of the test
/// sequence.
const MAX_TICKS: u64 = 150_000_000;

fn nes_test_roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

#[derive(Debug)]
enum Verdict {
    Pass { ticks: u64 },
    Fail { code: u8, text: String, ticks: u64 },
    Timeout,
}

/// Delay between observing the `$81` "needs reset" status and
/// actually performing the soft reset. blargg's apu_reset tests
/// require ≥ 100 ms; at ~5.37 MHz master clock that's ~537 000
/// master ticks. We use 600 000 to stay comfortably above the
/// minimum.
const RESET_DELAY_TICKS: u64 = 600_000;

fn run_one(path: &Path) -> Result<Verdict, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let parsed = parse_ines(&bytes).map_err(|e| format!("parse: {e}"))?;
    let mut nes = Nes::new(parsed.mapper);

    let mut signature_seen = false;
    let mut tick_count: u64 = 0;
    while tick_count < MAX_TICKS {
        nes.tick();
        tick_count += 1;
        if !signature_seen
            && nes.peek(0x6001) == 0xDE
            && nes.peek(0x6002) == 0xB0
            && nes.peek(0x6003) == 0x61
        {
            signature_seen = true;
        }
        if signature_seen {
            let status = nes.peek(0x6000);
            if status == 0x81 {
                // Test is waiting for a soft reset. Let it settle
                // for the required ≥ 100 ms, then press the reset
                // button and keep running. The test continues
                // with its post-reset sub-test.
                for _ in 0..RESET_DELAY_TICKS {
                    nes.tick();
                    tick_count += 1;
                }
                nes.soft_reset();
                continue;
            }
            if status != 0x80 {
                if status == 0 {
                    return Ok(Verdict::Pass {
                        ticks: nes.master_clock(),
                    });
                }
                let mut text = Vec::new();
                for i in 0..1024u16 {
                    let b = nes.peek(0x6004 + i);
                    if b == 0 {
                        break;
                    }
                    text.push(b);
                }
                return Ok(Verdict::Fail {
                    code: status,
                    text: String::from_utf8_lossy(&text).into_owned(),
                    ticks: nes.master_clock(),
                });
            }
        }
    }
    Ok(Verdict::Timeout)
}

/// Prefer `<dir>/rom_singles/*.nes` for granular per-subtest results;
/// fall back to top-level `*.nes` when that subdir is absent.
fn enumerate_roms(dir: &Path) -> Vec<PathBuf> {
    let mut roms = Vec::new();
    let singles = dir.join("rom_singles");
    let source = if singles.is_dir() {
        singles
    } else {
        dir.to_path_buf()
    };
    if let Ok(rd) = std::fs::read_dir(&source) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|s| s == "nes") {
                roms.push(p);
            }
        }
    }
    roms.sort();
    roms
}

/// The 22 test-rom directories we haven't run anywhere else this
/// session. Source of truth for the sweep.
const SWEEP_DIRS: &[&str] = &[
    // CPU integration
    "blargg_nes_cpu_test5",
    "branch_timing_tests",
    "cpu_dummy_reads",
    "cpu_dummy_writes",
    "cpu_exec_space",
    "cpu_interrupts_v2",
    "cpu_reset",
    "cpu_timing_test6",
    "instr_misc",
    "instr_test-v3",
    "instr_test-v5",
    "instr_timing",
    "nes_instr_test",
    // APU
    "apu_test",
    "apu_mixer",
    "apu_reset",
    "blargg_apu_2005.07.30",
    "dmc_tests",
    "dmc_dma_during_read4",
    "dpcmletterbox",
    // PPU corners we missed
    "nmi_sync",
    "full_palette",
];

#[test]
#[ignore = "long survey; run with --release --ignored --nocapture"]
fn sweep() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping sweep");
        return;
    };

    let mut total = 0u32;
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut timed_out = 0u32;
    let mut paniced = 0u32;

    for dir_name in SWEEP_DIRS {
        let dir_path = root.join(dir_name);
        let roms = enumerate_roms(&dir_path);
        if roms.is_empty() {
            eprintln!("\n=== {dir_name} (no ROMs found) ===");
            continue;
        }
        eprintln!("\n=== {dir_name} ({}) ===", roms.len());
        for rom in roms {
            total += 1;
            let label = rom
                .strip_prefix(&dir_path)
                .unwrap_or(&rom)
                .display()
                .to_string();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_one(&rom)));
            match outcome {
                Ok(Ok(Verdict::Pass { ticks })) => {
                    passed += 1;
                    eprintln!("  PASS     {label:<32} ({ticks} ticks)");
                }
                Ok(Ok(Verdict::Fail { code, text, ticks })) => {
                    failed += 1;
                    let snippet: String =
                        text.lines().next().unwrap_or("").chars().take(80).collect();
                    eprintln!("  FAIL #{code:02X} {label:<32} ({ticks} ticks) — {snippet}");
                }
                Ok(Ok(Verdict::Timeout)) => {
                    timed_out += 1;
                    eprintln!("  ---T---  {label:<32} (no $6000 result in {MAX_TICKS} ticks)");
                }
                Ok(Err(e)) => {
                    paniced += 1;
                    eprintln!("  ERROR    {label:<32} — {e}");
                }
                Err(_) => {
                    paniced += 1;
                    eprintln!("  PANIC    {label:<32} (likely unsupported mapper)");
                }
            }
        }
    }

    eprintln!("\n=== SWEEP SUMMARY ===");
    eprintln!(
        "Total: {total}  Pass: {passed}  Fail: {failed}  Timeout: {timed_out}  Panic/load: {paniced}"
    );
}
