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
///
/// 250M was tried — no extra tests flipped, because the remaining
/// 10 timeouts are stuck in tight infinite loops (DMC waiting on
/// DMA that doesn't fire, official.nes spinning at $8003-$8005)
/// rather than just slow. Time bumps don't help.
///
/// ⚠ That experiment could not have flipped `test_ppu_read_buffer`:
/// the ROM was on [`VISUAL_ROMS`] at the time, so raising the ceiling
/// never ran it. It needs ~520M ticks and is now in [`SLOW_ROMS`].
/// A ROM excluded from the sweep is excluded from the sweep's
/// experiments too — which is the trap that kept it ungraded.
///
/// Per-ROM tick budget. The slowest legitimate test we run
/// (oam_stress) finishes at ~152M ticks, so the budget needs to
/// be above that. Adds a little headroom for future tests.
const MAX_TICKS: u64 = 200_000_000;

/// ROMs that legitimately need longer than [`MAX_TICKS`], with the
/// budget each needs. Kept per-ROM rather than raising the global
/// ceiling: 173 of the 174 finish well inside it, and a blanket rise
/// would make every genuine infinite-loop timeout that much slower to
/// report.
const SLOW_ROMS: &[(&str, u64)] = &[
    // bisqwit's ppu_read_buffer runs its longest sub-test for ~666
    // frames behind a still image ("art is provided. Contemplate on
    // the art while the test is in progress"), and only then writes
    // its result. It reports `$6000 = $00` at ~520M ticks — over
    // twice the standard ceiling, and the sole reason it looked like
    // a ROM with no result protocol.
    ("test_ppu_read_buffer.nes", 700_000_000),
];

fn nes_test_roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

#[derive(Debug)]
enum Verdict {
    Pass {
        ticks: u64,
    },
    Fail {
        code: u8,
        text: String,
        ticks: u64,
    },
    Timeout,
    /// Visual demo with no programmatic result protocol — it draws
    /// to the screen for human inspection. Counted separately so
    /// it doesn't pollute the fail / timeout counts.
    Visual,
    /// No protocol the sweep can read, but covered by a named gate
    /// elsewhere.
    ///
    /// ⚠ Distinct from `Visual` on purpose. `Visual` means "nobody
    /// checks this", which is a standing invitation for a regression to
    /// go unnoticed. Once a ROM has a real gate, saying `Visual` is
    /// stale — but calling it `Pass` would claim the sweep graded it,
    /// which it cannot. This says what is true: the sweep has no
    /// channel here, and something else does.
    GatedExternally(&'static str),
}

/// ROM filenames that are visual demos (no programmatic result
/// channel). Matched on the leaf filename only; any test ROM
/// with one of these names is graded as `Verdict::Visual`.
/// ROMs with no sweep-readable protocol that ARE gated elsewhere, with
/// where. Checked before [`VISUAL_ROMS`], so moving a ROM here is what
/// promotes it out of "unexamined".
const GATED_EXTERNALLY: &[(&str, &str)] = &[
    // Structural screen comparison against Mesen2 goldens — nametable,
    // palette and OAM at scanline 240 dot 0. See the header of
    // tests/screen_goldens.rs for why that position and not a frame
    // counter.
    ("dma_4016_read.nes", "tests/screen_goldens.rs"),
    ("dpcmletterbox.nes", "tests/screen_goldens.rs"),
    ("mmc5exram.nes", "tests/screen_goldens.rs"),
    ("flowing_palette.nes", "tests/screen_goldens.rs"),
    ("full_palette.nes", "tests/screen_goldens.rs"),
    ("full_palette_smooth.nes", "tests/screen_goldens.rs"),
    ("mmc5test.nes", "tests/screen_goldens.rs"),
    ("demo_ntsc.nes", "tests/screen_goldens.rs"),
    ("demo_pal.nes", "tests/screen_goldens.rs"),
    ("count_errors.nes", "tests/screen_goldens.rs"),
    ("count_errors_fast.nes", "tests/screen_goldens.rs"),
    ("test_buttons.nes", "tests/screen_goldens.rs"),
    ("volumes.nes", "tests/screen_goldens.rs"),
    // Gated on the ROM's own published CRC set rather than a golden:
    // these two document several legal outputs, so a single reference
    // capture cannot arbitrate between them.
    ("dma_2007_read.nes", "tests/dmc_dma_read4_crc.rs"),
    ("double_2007_read.nes", "tests/dmc_dma_read4_crc.rs"),
    // ⚠ The four dmc_tests really do have no $6000 protocol — measured,
    // not inferred: no DE B0 61 signature and all zeroes at $6000-$6007
    // after 900M ticks. But they are not ungradeable. blargg's shell
    // beeps its result code, the encoding is published, and code 0
    // (passed) is a single tone where every non-zero code is two or
    // more. All four beep once.
    ("buffer_retained.nes", "tests/dmc_tests_audio.rs"),
    ("latency.nes", "tests/dmc_tests_audio.rs"),
    ("status.nes", "tests/dmc_tests_audio.rs"),
    ("status_irq.nes", "tests/dmc_tests_audio.rs"),
];

const VISUAL_ROMS: &[&str] = &[
    "demo_ntsc.nes",
    "demo_pal.nes",
    "flowing_palette.nes",
    "full_palette.nes",
    "full_palette_smooth.nes",
    "dpcmletterbox.nes",
    // blargg's dmc_tests + the read-side variants of
    // dmc_dma_during_read4 — these probe audio-side behaviour
    // (DMC IRQ timing, DMA-during-`$2007`/`$4016`-read collisions)
    // that the test framework can't reduce to a `$6000` result;
    // they never print "Passed" / "Failed" tokens, so the
    // multi-protocol grader gets no signal and times out.
    //
    // ⚠ The four dmc_tests write NOTHING to either nametable — 9 PPU
    // register writes each, three of them `$2001`, which is rendering
    // being switched on and off. They report by BEEPING. Measured in
    // both emulators; Mesen2 leaves the nametable untouched across
    // 2400 frames with power-on RAM zeroed. An earlier note here and
    // in the campaign record said they "draw tile indices against a
    // CHR font" — that was inferred, never measured, and is wrong.
    // No screen-based gate can work on them; gating needs audio.
    // The write-side variants (`dma_2007_write.nes`,
    // `read_write_2007.nes`) DO emit nametable text on the
    // observable cases and stay graded normally.
    "buffer_retained.nes",
    "latency.nes",
    "status.nes",
    "status_irq.nes",
    "dma_2007_read.nes",
    "dma_4016_read.nes",
    "double_2007_read.nes",
    // Damian Yerrick's volume_tests — plays calibrated tones for
    // human / oscilloscope inspection of inter-channel mixing
    // levels. No `$6000` protocol, no nametable verdict.
    "volumes.nes",
    // read_joy3: three of the four are observational rather than
    // pass/fail. `count_errors` and `count_errors_fast` print an X each
    // time a DMC fetch collides with a controller read and explicitly
    // note that "a conflict doesn't affect the result of read_joy",
    // so there is no verdict to grade; `test_buttons` needs a human
    // pressing buttons. `thorough_test.nes` DOES produce a verdict and
    // stays graded.
    "count_errors.nes",
    "count_errors_fast.nes",
    "test_buttons.nes",
    // MMC5. ⚠ Visual because they carry no result protocol — NOT because
    // they render nothing, which is what they appeared to do while the
    // grader read `ppu.nametable_ram()`. MMC5 serves nametables from
    // inside the mapper, so CIRAM is always empty for these; the grader
    // now reads `Nes::effective_nametable()` and their screens match
    // Mesen2 byte for byte. mmc5test/mmc5test_v2 draw with a custom
    // graphics font (no ASCII to match), and mmc5exram is a colour-bar
    // demo. The capability that matters — executing code from ExRAM — is
    // gated properly in tests/mmc5_screen.rs.
    "mmc5test.nes",
    "mmc5exram.nes",
];

/// Delay between observing the `$81` "needs reset" status and
/// actually performing the soft reset. blargg's apu_reset tests
/// require ≥ 100 ms; at ~5.37 MHz master clock that's ~537 000
/// master ticks. We use 600 000 to stay comfortably above the
/// minimum.
const RESET_DELAY_TICKS: u64 = 600_000;

/// Older blargg test ROMs (pre-`$6000` protocol — branch_timing,
/// cpu_dummy_reads, instr_timing, dmc_tests, …) report results
/// via a single byte at `$00F8` or `$00F0` that the test writes
/// continuously, then freezes once the test ends. 1 = pass,
/// other = fail with that code. The settle window must be long
/// enough to ride out the gap between intermediate sub-tests but
/// short enough that the harness still finishes per-ROM in
/// reasonable wall-clock time. vbl_nmi_timing/1.frame_basics has
/// a ~7.3M-tick gap between sub-tests, so 10M is the floor.
const SETTLE_TICKS: u64 = 10_000_000;

/// Scan the PPU's nametable RAM for the blargg shell's "Passed"
/// / "Failed" / "Error" final-status strings. Returns the
/// corresponding verdict the moment one is found.
///
/// blargg's `print_str` writes ASCII tile codes directly into the
/// nametable via `$2007`, so the final status text persists in
/// nametable RAM after the test's infinite `forever` loop kicks
/// in. This is the only programmatic signal for the visual-only
/// dmc_tests / dmc_dma_during_read4 / blargg_nes_cpu_test5
/// suites, which never write to `$6000` / `$F8` / `$F0`.
fn try_nametable_protocol(nes: &Nes) -> Option<Verdict> {
    // ⚠ Effective nametable, not `ppu.nametable_ram()`. A mapper may
    // serve $2000-$2FFF itself — MMC5 always does — leaving the
    // console's CIRAM empty and this whole channel blind.
    let nt = &nes.effective_nametable();
    // The blargg shell prints the entire exit string in one
    // newline-flanked block: e.g. `"\n\nPassed\n\n\n"`. Detect
    // only the clean tokens, not the bare "Error " (initial
    // banner / opcode-list text contains incidental matches).
    if find_ascii(nt, b"Passed") {
        return Some(Verdict::Pass {
            ticks: nes.master_clock(),
        });
    }
    if find_ascii(nt, b"Failed") {
        return Some(Verdict::Fail {
            code: 1,
            text: "Failed (detected in nametable)".into(),
            ticks: nes.master_clock(),
        });
    }
    // ⚠ blargg's *older* `console.a` shell (cpu_timing_test6) prints in
    // UPPER CASE and uses its own vocabulary, so the mixed-case tokens
    // above never match it. It has no `$6000` and no result byte at all —
    // the screen is its only channel, which is why the case difference was
    // enough to hand the verdict to a scratch byte in `$F0`.
    if find_ascii(nt, b"PASSED") {
        return Some(Verdict::Pass {
            ticks: nes.master_clock(),
        });
    }
    for token in [
        b"FAIL OP".as_slice(),
        b"UNKNOWN ERROR".as_slice(),
        b"BASIC TIMING WRONG".as_slice(),
    ] {
        if find_ascii(nt, token) {
            return Some(Verdict::Fail {
                code: 1,
                text: format!("{} (on-screen)", String::from_utf8_lossy(token)),
                ticks: nes.master_clock(),
            });
        }
    }
    // blargg_nes_cpu_test5 is a BUILD_MULTI build: it runs eleven
    // sub-tests, prints "All tests complete" either way, and marks each
    // PASSING sub-test with a `$00` tile at column 31.
    //
    // ⚠⚠ `$00FF` is NOT a result sentinel. It was read as one from
    // 2026-05-30 until Mesen2 — which runs these ROMs correctly — was
    // measured and found to end with `$00FF == 0xFF` as well. Grading on
    // it declared both ROMs failures for the whole campaign. The byte is
    // residue; the markers are the actual channel.
    //
    // ⚠ The marker for sub-test N sits one row BELOW N's name, so the
    // last one lands on the separator line and the first test's row is
    // always bare. That looks exactly like "test 01 failed", and it is
    // the second thing that made this ROM read as broken. Mesen2's
    // nametable is byte-identical, markers included.
    if find_ascii(nt, b"All tests complete") {
        let names = count_subtest_rows(nt);
        let marks = count_pass_markers(nt);
        if marks >= names && names > 0 {
            return Some(Verdict::Pass {
                ticks: nes.master_clock(),
            });
        }
        return Some(Verdict::Fail {
            code: 1,
            text: format!("multi-test build: {marks} pass markers for {names} sub-tests"),
            ticks: nes.master_clock(),
        });
    }
    None
}

/// Rows of the form `NN-name`, i.e. one per sub-test the multi-cart ran.
fn count_subtest_rows(nt: &[u8]) -> usize {
    (0..30)
        .filter(|row| {
            let r = &nt[row * 32..row * 32 + 32];
            r.windows(3)
                .any(|w| w[0].is_ascii_digit() && w[1].is_ascii_digit() && w[2] == b'-')
        })
        .count()
}

/// `$00` tiles at column 31 — the shell's per-sub-test pass marker.
fn count_pass_markers(nt: &[u8]) -> usize {
    (0..30).filter(|row| nt[row * 32 + 31] == 0x00).count()
}

fn find_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Power-on garbage at `$F8` is typically `$00`. Treat the
/// settle as meaningful only after we've observed at least one
/// non-zero value — otherwise a ROM that never touches `$F8`
/// would falsely score as "settled at 0".
fn try_settle_protocol(nes: &Nes, history: &SettleHistory) -> Option<u8> {
    if history.steady < SETTLE_TICKS {
        return None;
    }
    if !history.saw_nonzero {
        return None;
    }
    let _ = nes; // for symmetry with future per-ROM hooks
    Some(history.last)
}

#[derive(Default)]
struct SettleHistory {
    last: u8,
    steady: u64,
    saw_nonzero: bool,
}

impl SettleHistory {
    fn observe(&mut self, value: u8) {
        if value == self.last {
            self.steady = self.steady.saturating_add(1);
        } else {
            self.last = value;
            self.steady = 0;
        }
        if value != 0 {
            self.saw_nonzero = true;
        }
    }
}

fn run_one(path: &Path) -> Result<Verdict, String> {
    let mut budget = MAX_TICKS;
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if let Some((_, where_gated)) = GATED_EXTERNALLY.iter().find(|(n, _)| *n == name) {
            return Ok(Verdict::GatedExternally(where_gated));
        }
        if VISUAL_ROMS.contains(&name) {
            return Ok(Verdict::Visual);
        }
        if let Some((_, ticks)) = SLOW_ROMS.iter().find(|(n, _)| *n == name) {
            budget = *ticks;
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let parsed = parse_ines(&bytes).map_err(|e| format!("parse: {e}"))?;
    let mut nes = Nes::new(parsed.mapper);

    let mut signature_seen = false;
    let mut tick_count: u64 = 0;
    let mut hist_f8 = SettleHistory::default();
    let mut hist_f0 = SettleHistory::default();
    // Weakest channel: a non-`1` settled byte, held back until every
    // stronger channel has failed to decide by the tick ceiling.
    let mut settle_fallback: Option<(u16, u8)> = None;
    while tick_count < budget {
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
                // RAM is preserved through soft reset, so $6000 is
                // still $81 until the test code overwrites it.
                // Tick until that happens (bounded by a budget) so
                // we don't immediately re-enter this branch and
                // reset mid-sequence — every extra reset decrements
                // SP by 3 and breaks the test's reset-state CRC.
                let cooldown_budget: u64 = 2_000_000;
                let cooldown_end = tick_count + cooldown_budget;
                while tick_count < cooldown_end && nes.peek(0x6000) == 0x81 {
                    nes.tick();
                    tick_count += 1;
                }
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
        } else {
            // No `$6000` signature yet — track the older `$F8` and
            // `$F0` protocols in parallel. Either may settle first.
            hist_f8.observe(nes.peek(0x00F8));
            hist_f0.observe(nes.peek(0x00F0));
            // ⚠⚠ A settled byte is an INFERENCE ("this stopped changing"),
            // not a positive result the ROM declared. Only `1` -- the
            // protocol's own pass code -- is trusted to decide here. Any
            // other value is far more likely to be scratch residue that
            // simply went quiet, so it is remembered as a fallback and the
            // ROM keeps running to give a positive channel (on-screen text,
            // `$6000`) the chance to speak.
            //
            // Measured, not assumed: across the 155-ROM corpus the settle
            // channels decide 43 ROMs, 42 of them at exactly `0x01`. The one
            // non-`1` value ever produced was `cpu_timing_test.nes` at
            // `0x98` -- the low byte of a font-upload pointer left in `$F0`
            // by `console.a`, which the ROM never touches again. It graded a
            // PASSING ROM as a failure for the whole campaign.
            if let Some(code) = try_settle_protocol(&nes, &hist_f8) {
                if code == 1 {
                    return Ok(Verdict::Pass {
                        ticks: nes.master_clock(),
                    });
                }
                settle_fallback.get_or_insert((0x00F8u16, code));
            }
            if let Some(code) = try_settle_protocol(&nes, &hist_f0) {
                if code == 1 {
                    return Ok(Verdict::Pass {
                        ticks: nes.master_clock(),
                    });
                }
                settle_fallback.get_or_insert((0x00F0u16, code));
            }
            // PPU nametable grader is more expensive than a byte
            // peek, so sample it every NAMETABLE_POLL_INTERVAL
            // ticks rather than every tick.
            if tick_count.is_multiple_of(NAMETABLE_POLL_INTERVAL)
                && let Some(v) = try_nametable_protocol(&nes)
            {
                return Ok(v);
            }
        }
    }
    if let Some((addr, code)) = settle_fallback {
        return Ok(Verdict::Fail {
            code,
            text: format!("settled at ${addr:04X} = {code:#04X} (no positive channel spoke)"),
            ticks: nes.master_clock(),
        });
    }
    Ok(Verdict::Timeout)
}

const NAMETABLE_POLL_INTERVAL: u64 = 200_000;

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
    "volume_tests",
    "sprdma_and_dmc_dma",
    // PPU
    "nmi_sync",
    "full_palette",
    "blargg_ppu_tests_2005.09.15b",
    "ppu_vbl_nmi",
    "vbl_nmi_timing",
    "sprite_hit_tests_2005.10.05",
    "sprite_overflow_tests",
    "oam_read",
    "oam_stress",
    "ppu_open_bus",
    "ppu_read_buffer",
    // Mappers and input. Added 2026-08-10 while closing the gap between
    // the corpus on disk and what the sweep actually reached; see
    // UNSWEPT_DIRS below for what is deliberately still outside it.
    "mmc3_irq_tests",
    "mmc3_test_2",
    "read_joy3",
    "mmc5test",
    "mmc5test_v2",
    "exram",
];

/// Directories of the corpus the sweep deliberately does NOT enumerate,
/// each with the reason.
///
/// ⚠ This exists so "not swept" is a recorded decision rather than an
/// oversight. The campaign's goal is that every ROM is accounted for —
/// passing, or carrying a stated reason — and a directory nobody listed
/// is exactly the gap that let three false failures stand for months.
///
/// Asserted against the filesystem by [`every_directory_is_accounted_for`],
/// so a newly-added directory fails the suite until it is either swept or
/// explicitly excluded here.
const UNSWEPT_DIRS: &[(&str, &str)] = &[
    (
        "mmc3_test",
        "gated ROM-by-ROM in blargg_ppu.rs, which also records why 6-MMC6 \
         is excluded (an MMC3B core cannot pass both it and 5-MMC3)",
    ),
    (
        "pal_apu_tests",
        "PAL: needs Nes::new_with_region(.., Region::Pal), which the sweep \
         cannot express per-directory. Gated in tests/pal_apu.rs instead — \
         all ten pass, and seven of them discriminate by region",
    ),
    (
        "240pee",
        "240p test suite — calibration patterns for human/display inspection",
    ),
    ("blargg_litewall", "demo effect, no result protocol"),
    ("nes15-1.0.0", "a 15-puzzle game, not a test"),
    ("ny2011", "demo"),
    ("scanline", "visual scanline-timing demo"),
    ("scanline-a1", "visual scanline-timing demo"),
    ("scrolltest", "visual scrolling demo"),
    ("spritecans-2011", "demo"),
    ("stomper", "demo"),
    ("tutor", "demo"),
    ("window5", "visual colour-window demo (NTSC+PAL pair)"),
    (
        "PaddleTest3",
        "requires paddle peripheral input the sweep cannot supply",
    ),
    (
        "vaus-test",
        "requires Arkanoid Vaus controller input the sweep cannot supply",
    ),
    (
        "MMC1_A12",
        "interactive: draws with an offset font and asks the operator to \
         adjust a delay with the D-pad",
    ),
    (
        "m22chrbankingtest",
        "visual: displays a CHR bank grid for inspection",
    ),
    (
        "nrom368",
        "homebrew NROM-368 mapper-proposal demo (C source, music, tileset); \
         fail368.nes exists to SHOW the failure mode, not to be passed",
    ),
    (
        "tvpassfail",
        "display calibration for the physical TV; needs a human and the A \
         button to page through screens",
    ),
    (
        "other",
        "mixed bag of demos, homebrew games and one-off probes with no \
         common result protocol; nestest.nes among them is gated by \
         tests/nestest.rs",
    ),
];

#[test]
#[ignore = "long survey; run with --release --ignored --nocapture"]
fn sweep() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms not found; skipping sweep");
    };

    let mut total = 0u32;
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut timed_out = 0u32;
    let mut paniced = 0u32;
    let mut visual = 0u32;
    let mut gated = 0u32;
    // Keyed on (suite, rom) so the baseline can name a ROM unambiguously —
    // "2.Details.nes" exists in three suites.
    let mut observed: std::collections::BTreeMap<(String, String), String> =
        std::collections::BTreeMap::new();

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
                    observed.insert((dir_name.to_string(), label.clone()), "pass".into());
                    eprintln!("  PASS     {label:<32} ({ticks} ticks)");
                }
                Ok(Ok(Verdict::Fail { code, text, ticks })) => {
                    failed += 1;
                    observed.insert((dir_name.to_string(), label.clone()), "fail".into());
                    let snippet: String =
                        text.lines().next().unwrap_or("").chars().take(80).collect();
                    eprintln!("  FAIL #{code:02X} {label:<32} ({ticks} ticks) — {snippet}");
                }
                Ok(Ok(Verdict::Timeout)) => {
                    timed_out += 1;
                    observed.insert((dir_name.to_string(), label.clone()), "timeout".into());
                    eprintln!("  ---T---  {label:<32} (no $6000 result in {MAX_TICKS} ticks)");
                }
                Ok(Ok(Verdict::Visual)) => {
                    visual += 1;
                    observed.insert((dir_name.to_string(), label.clone()), "visual".into());
                    eprintln!("  VISUAL   {label:<32} (visual demo — no result protocol)");
                }
                Ok(Ok(Verdict::GatedExternally(where_gated))) => {
                    gated += 1;
                    observed.insert((dir_name.to_string(), label.clone()), "gated".into());
                    eprintln!("  GATED    {label:<32} (no sweep protocol; gated in {where_gated})");
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
        "Total: {total}  Pass: {passed}  Fail: {failed}  Timeout: {timed_out}  Gated: {gated}  Visual: {visual}  Panic/load: {paniced}"
    );

    // ⚠⚠ Gate on the declared baseline. Until this existed the sweep RAN the
    // whole corpus, printed a verdict per ROM, tallied them — and then passed
    // unconditionally. The suite was green whether every ROM passed or every
    // ROM failed, so the information it gathered was discarded at the last
    // line. That is why the NES core could sit at 135/5/15 with nobody able to
    // say so.
    assert_against_baseline(&observed);
}

/// Compare observed verdicts against the declared baseline.
///
/// ⚠ An EXACT match is required in both directions. A regression fails, and so
/// does an unannounced improvement: a ROM that starts passing must be recorded
/// in the baseline deliberately. Otherwise a fix silently shifts the reference
/// and the file stops describing anything anyone chose.
///
/// The baseline is tab-separated rather than JSON so the test parses it with
/// std alone — adding a JSON dependency to assert 155 lines is not a trade
/// worth making.
fn assert_against_baseline(observed: &std::collections::BTreeMap<(String, String), String>) {
    let path = match baseline_path() {
        Some(p) => p,
        None => {
            eprintln!("no declared baseline found; skipping the gate");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read declared sweep baseline");
    let mut declared = std::collections::BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut f = line.split('\t');
        let (Some(suite), Some(rom), Some(verdict)) = (f.next(), f.next(), f.next()) else {
            panic!("malformed baseline line: {line}");
        };
        declared.insert((suite.to_string(), rom.to_string()), verdict.to_string());
    }

    let mut diffs = Vec::new();
    for (key, got) in observed {
        match declared.get(key) {
            Some(want) if want == got => {}
            Some(want) => diffs.push(format!(
                "  {}/{}: declared {want}, observed {got}",
                key.0, key.1
            )),
            None => diffs.push(format!("  {}/{}: not in baseline ({got})", key.0, key.1)),
        }
    }
    for key in declared.keys() {
        if !observed.contains_key(key) {
            diffs.push(format!("  {}/{}: in baseline but not swept", key.0, key.1));
        }
    }
    assert!(
        diffs.is_empty(),
        "sweep diverged from the declared baseline ({} difference(s)):\n{}\n\n\
         If this is a deliberate fix, update {} in the same commit.",
        diffs.len(),
        diffs.join("\n"),
        path.display()
    );
}

/// Every directory in the corpus is either swept or explicitly excluded.
///
/// ⚠ Not `#[ignore]`d, unlike the sweep itself: it only reads directory
/// names, so it costs nothing and can run in the normal suite. That
/// matters — the gap this closes (99 `.nes` files in directories nobody
/// had listed) survived precisely because noticing it required someone
/// to go looking.
#[test]
fn every_directory_is_accounted_for() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let excluded: std::collections::BTreeSet<&str> = UNSWEPT_DIRS.iter().map(|(d, _)| *d).collect();
    let swept: std::collections::BTreeSet<&str> = SWEEP_DIRS.iter().copied().collect();

    let mut unaccounted = Vec::new();
    for entry in std::fs::read_dir(&root)
        .expect("read corpus root")
        .flatten()
    {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Only directories that actually contain ROMs need a decision.
        let has_roms = walk_has_nes(&entry.path());
        if has_roms && !swept.contains(name.as_str()) && !excluded.contains(name.as_str()) {
            unaccounted.push(name);
        }
    }
    unaccounted.sort();
    assert!(
        unaccounted.is_empty(),
        "test-rom directories are neither swept nor declared unswept: {unaccounted:?}\n\
         Add each to SWEEP_DIRS, or to UNSWEPT_DIRS with the reason."
    );

    // And the reverse: a declared exclusion that no longer exists is a
    // stale claim, which is its own kind of wrong record.
    let stale: Vec<&str> = UNSWEPT_DIRS
        .iter()
        .map(|(d, _)| *d)
        .filter(|d| !root.join(d).is_dir())
        .collect();
    assert!(
        stale.is_empty(),
        "UNSWEPT_DIRS names directories that no longer exist: {stale:?}"
    );
}

fn walk_has_nes(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if walk_has_nes(&p) {
                return true;
            }
        } else if p.extension().is_some_and(|s| s == "nes") {
            return true;
        }
    }
    false
}

/// Locate the declared baseline, relative to this crate.
fn baseline_path() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/nintendo/nes/blargg-survey/sweep-baseline-v1.tsv");
    p.is_file().then(|| p.clone())
}

/// Print the on-screen text of named ROMs, for triaging a timeout into
/// "visual-only", "needs input", or "genuinely stuck".
#[test]
#[ignore = "diagnostic: prints screen text for triage"]
fn probe_timeout_screens() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    for rel in [
        "mmc5test/mmc5test.nes",
        "mmc5test_v2/mmc5test.nes",
        "exram/mmc5exram.nes",
        "m22chrbankingtest/0-127.nes",
        "nrom368/test1.nes",
        "nrom368/fail368.nes",
    ] {
        let Ok(bytes) = std::fs::read(root.join(rel)) else {
            continue;
        };
        let Ok(parsed) = parse_ines(&bytes) else {
            continue;
        };
        let mut nes = Nes::new(parsed.mapper);
        while nes.master_clock() < 40_000_000 {
            nes.tick();
        }
        let nt = nes.effective_nametable();
        println!("\n═══ {rel} ═══");
        for row in 0..30 {
            let line: String = nt[row * 32..row * 32 + 32]
                .iter()
                .map(|&b| {
                    if (0x21..=0x7E).contains(&b) {
                        b as char
                    } else {
                        ' '
                    }
                })
                .collect();
            if !line.trim().is_empty() {
                println!("  {}", line.trim_end());
            }
        }
    }
}
