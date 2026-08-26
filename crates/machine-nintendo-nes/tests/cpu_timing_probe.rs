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

/// Controller-1 button held through boot, which is how the ROM selects
/// its instruction set. Bit layout per `Nes::set_controller1`.
const HELD_NONE: u8 = 0x00;
const HELD_A: u8 = 0x01;
const HELD_B: u8 = 0x02;

/// Run the ROM with `held` on controller 1 and return
/// `(verdict_ticks, screen_text)`.
///
/// ⚠ The button must be held for the whole run, not pressed once. The
/// ROM reads the pad during init to choose its instruction set, and a
/// release before that read silently drops it back to official-only —
/// which looks identical to a pass on the wider set.
fn run_with(root: &std::path::Path, held: u8) -> Option<(Option<u64>, String)> {
    let path = root.join("cpu_timing_test6").join("cpu_timing_test.nes");
    let bytes = std::fs::read(&path).ok()?;
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    nes.set_controller1(held);

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
    Some((settled_at, screen_text(&nes)))
}

#[test]
#[ignore = "DIAGNOSTIC: diagnostic: prints cpu_timing_test's on-screen result"]
fn probe_cpu_timing_test() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };

    // All three instruction-set selections the ROM's readme documents.
    // The default (nothing held) covers official instructions only, so
    // undocumented-opcode timing is untested unless a button is held.
    for (held, label) in [
        (HELD_NONE, "official only"),
        (HELD_A, "official + $EB + unofficial NOPs"),
        (HELD_B, "official + all undocumented"),
    ] {
        let Some((settled_at, text)) = run_with(&root, held) else {
            emu198x_test_skip::skip!("missing cpu_timing_test.nes");
        };
        eprintln!(
            "\n═══ cpu_timing_test.nes — {label} (held {held:#04X}) — {} ═══\n{text}",
            settled_at.map_or_else(
                || format!("no verdict by {MAX_TICKS} ticks"),
                |t| format!("verdict at {t} ticks")
            ),
        );
    }
}
