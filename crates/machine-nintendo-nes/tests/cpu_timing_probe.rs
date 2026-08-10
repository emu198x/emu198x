//! Diagnostic harness for `cpu_timing_test6/cpu_timing_test.nes`, the
//! remaining CPU-timing failure in the accuracy campaign (stage 3).
//!
//! The ROM reports on-screen only — no `$6000` shell — and its
//! `print_char` writes the ASCII byte straight to `$2007` against an
//! ASCII-ordered font, so the nametable holds the printed text
//! verbatim. On failure it prints the failing opcode, whether a page
//! cross was in play, and both clock counts:
//!
//! ```text
//! FAIL OP : xx
//! WITH PAGE CROSS      (only when the crossing case failed)
//! EMULATOR: n
//! CORRECT : n
//! ```
//!
//! That is everything needed to name the defect, so this probe dumps
//! the screen rather than asserting. `readme.txt` beside the ROM
//! carries blargg's full timing tables for both cases.
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test cpu_timing_probe \
//!     -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

/// The ROM's own readme says it takes up to 16 seconds of emulated
/// time. At ~5.37 MHz master clock that is ~86M ticks, so allow
/// headroom and stop early once the screen settles.
const MAX_TICKS: u64 = 200_000_000;

/// Scan cadence. The text is written once and then static, so once
/// per frame is ample and scanning every tick would dominate runtime.
const SCAN_PERIOD: u64 = 30_000;

fn nes_test_roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

/// Decode CIRAM to text, one line per nametable row, trailing blanks
/// trimmed. Same basis as `blargg_legacy::ciram_text`: the tile index
/// *is* the ASCII code.
fn screen_text(nes: &Nes) -> String {
    let nt = nes.ppu.nametable_ram();
    let mut out = String::new();
    for row in 0..30 {
        let raw = &nt[row * 32..row * 32 + 32];
        let line: String = raw
            .iter()
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect();
        let line = line.trim_end();
        if !line.is_empty() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[test]
#[ignore = "diagnostic: prints cpu_timing_test's on-screen result"]
fn probe_cpu_timing_test() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let path = root.join("cpu_timing_test6").join("cpu_timing_test.nes");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("missing {}", path.display());
        return;
    };
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    // Stop as soon as the ROM has printed a verdict; otherwise run to
    // the ceiling and dump whatever is on screen.
    let mut scan_at = SCAN_PERIOD;
    let mut settled_at = None;
    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        if nes.master_clock() >= scan_at {
            scan_at += SCAN_PERIOD;
            let t = screen_text(&nes);
            if t.contains("PASSED") || t.contains("FAIL OP") || t.contains("ERROR") {
                settled_at = Some(nes.master_clock());
                break;
            }
        }
    }

    eprintln!(
        "\n═══ cpu_timing_test.nes ({}) ═══\n{}",
        settled_at.map_or_else(
            || format!("no verdict by {MAX_TICKS} ticks"),
            |t| format!("verdict at {t} ticks")
        ),
        screen_text(&nes)
    );
    eprintln!("$00F0 = {:#04X}", nes.peek(0x00F0));
}
