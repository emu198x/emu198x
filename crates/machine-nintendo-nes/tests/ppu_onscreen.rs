//! On-screen-result PPU test harness (blargg's 2005 devcart shell).
//!
//! Unlike the newer `$6000` text protocol (see `blargg_ppu.rs`), the
//! 2005-era sprite-0-hit, sprite-overflow, and PPU test ROMs report
//! their result by storing a code in a zero-page `result` variable,
//! printing "PASSED" / "FAILED #n" to the screen, beeping, and looping
//! forever. `result == 1` means pass; any other value is the per-test
//! failure code documented in each suite's `readme.txt`.
//!
//! The shell revision determines the `result` address: the 2005.10.05
//! sprite suites use `$00F8`, while the earlier 2005.09.15b PPU suite
//! uses `$00F0`. Each suite below names its own address.
//!
//! These ROMs carry no `DE B0 61` completion signature, so we detect
//! completion by watching `$00F8` settle: once the ROM stops changing
//! it (it has entered the report/forever loop), the value is final.
//! Reading the canonical `result` variable is more robust than OCR-ing
//! the rendered text — the on-screen text is merely a rendering of it.
//!
//! ROMs resolve the same way as `blargg_ppu.rs` (NES_BLARGG_ROOT or
//! `~/Projects/198x/assets/test-suites/nes-test-roms`). `#[ignore]` by
//! default; run with:
//! `cargo test -p machine-nintendo-nes --test ppu_onscreen -- --ignored --nocapture`

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;
use std::path::PathBuf;

/// `result` address for the 2005.10.05 sprite suites.
const RESULT_F8: u16 = 0x00F8;
/// `result` address for the earlier 2005.09.15b PPU suite.
const RESULT_F0: u16 = 0x00F0;
/// Frames the result must hold steady (after at least one change) before
/// we treat the ROM as finished. Inter-sub-test gaps are ~1-2 frames;
/// the report/forever loop freezes the value indefinitely.
const SETTLE_FRAMES: u32 = 45;
/// Upper bound so a hung ROM can't loop forever.
const MAX_FRAMES: u32 = 1200;

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

/// Outcome of grading one on-screen-result ROM.
#[derive(Debug)]
struct OnscreenResult {
    /// Final `result` byte. 1 = pass; otherwise the failure code.
    code: u8,
    /// Frames executed before the result settled.
    frames: u32,
}

/// Run a ROM until its `result` byte settles, then read it.
fn grade_onscreen(rom_path: &PathBuf, result_addr: u16) -> Result<OnscreenResult, String> {
    let bytes = std::fs::read(rom_path).map_err(|e| format!("read {rom_path:?}: {e}"))?;
    let parsed = parse_ines(&bytes).map_err(|e| format!("parse {rom_path:?}: {e}"))?;
    let mut nes = Nes::new(parsed.mapper);

    let mut prev = nes.peek(result_addr);
    let mut seen_change = false;
    let mut steady = 0u32;

    for frame in 0..MAX_FRAMES {
        nes.run_frame();
        let cur = nes.peek(result_addr);
        if cur == prev {
            steady += 1;
        } else {
            prev = cur;
            steady = 0;
            seen_change = true;
        }
        // The ROM always writes `result` (sub-test codes, then the final
        // value) before it can settle, so require an observed change to
        // avoid mistaking the power-on value for a finished run.
        if seen_change && steady >= SETTLE_FRAMES {
            return Ok(OnscreenResult {
                code: cur,
                frames: frame + 1,
            });
        }
    }

    Err(format!(
        "on-screen ROM {rom_path:?} result never settled within {MAX_FRAMES} frames \
         (last value {prev}, seen_change={seen_change})"
    ))
}

/// Grade one on-screen ROM and assert it passed (`result == 1`).
/// Skips (no-op) when the ROM is absent.
fn run_onscreen_or_skip(dir: &str, rom: &str, result_addr: u16) {
    let Some(root) = blargg_root() else {
        eprintln!("blargg root not found; skipping {dir}/{rom}");
        return;
    };
    let path = root.join(dir).join(rom);
    if !path.is_file() {
        eprintln!("ROM not present at {path:?}; skipping");
        return;
    }
    let result = grade_onscreen(&path, result_addr).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        result.code, 1,
        "on-screen test {dir}/{rom} failed with code #{} after {} frames \
         (see the suite readme.txt for the code meaning)",
        result.code, result.frames
    );
    eprintln!("{dir}/{rom} passed in {} frames", result.frames);
}

/// Declares one `#[ignore]` per-ROM pass assertion.
macro_rules! onscreen_test {
    ($name:ident, $dir:expr, $rom:expr, $addr:expr) => {
        #[test]
        #[ignore = "requires local nes-test-roms; run with --ignored"]
        fn $name() {
            run_onscreen_or_skip($dir, $rom, $addr);
        }
    };
}

// ── sprite_hit_tests_2005.10.05 (result @ $F8) — all pass; these are
//    regression guards for our sprite-0-hit emulation. ──
const SPRITE_HIT_DIR: &str = "sprite_hit_tests_2005.10.05";
onscreen_test!(
    sprite_hit_01_basics,
    SPRITE_HIT_DIR,
    "01.basics.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_02_alignment,
    SPRITE_HIT_DIR,
    "02.alignment.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_03_corners,
    SPRITE_HIT_DIR,
    "03.corners.nes",
    RESULT_F8
);
onscreen_test!(sprite_hit_04_flip, SPRITE_HIT_DIR, "04.flip.nes", RESULT_F8);
onscreen_test!(
    sprite_hit_05_left_clip,
    SPRITE_HIT_DIR,
    "05.left_clip.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_06_right_edge,
    SPRITE_HIT_DIR,
    "06.right_edge.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_07_screen_bottom,
    SPRITE_HIT_DIR,
    "07.screen_bottom.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_08_double_height,
    SPRITE_HIT_DIR,
    "08.double_height.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_09_timing_basics,
    SPRITE_HIT_DIR,
    "09.timing_basics.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_10_timing_order,
    SPRITE_HIT_DIR,
    "10.timing_order.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_hit_11_edge_timing,
    SPRITE_HIT_DIR,
    "11.edge_timing.nes",
    RESULT_F8
);

// ── blargg_ppu_tests_2005.09.15b (result @ $F0) — palette_ram /
//    vbl_clear_time / vram_access pass; sprite_ram (#7, $4014 DMA wrap)
//    is a real OAM-DMA gap. ──
const PPU_2005_DIR: &str = "blargg_ppu_tests_2005.09.15b";
onscreen_test!(
    ppu2005_palette_ram,
    PPU_2005_DIR,
    "palette_ram.nes",
    RESULT_F0
);
onscreen_test!(
    ppu2005_vbl_clear_time,
    PPU_2005_DIR,
    "vbl_clear_time.nes",
    RESULT_F0
);
onscreen_test!(
    ppu2005_vram_access,
    PPU_2005_DIR,
    "vram_access.nes",
    RESULT_F0
);
onscreen_test!(
    ppu2005_sprite_ram,
    PPU_2005_DIR,
    "sprite_ram.nes",
    RESULT_F0
);
// Passes because the PPU powers up with the canonical palette table
// this ROM checks against (see `Ppu::new_with_pre_render_line`).
onscreen_test!(
    ppu2005_power_up_palette,
    PPU_2005_DIR,
    "power_up_palette.nes",
    RESULT_F0
);

// ── sprite_overflow_tests (result @ $F8) — all pass; regression guards
//    for the sprite-overflow flag's set/clear timing (basics, details,
//    timing, obscure, emulator). ──
const OVERFLOW_DIR: &str = "sprite_overflow_tests";
onscreen_test!(
    sprite_overflow_1_basics,
    OVERFLOW_DIR,
    "1.Basics.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_overflow_2_details,
    OVERFLOW_DIR,
    "2.Details.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_overflow_3_timing,
    OVERFLOW_DIR,
    "3.Timing.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_overflow_4_obscure,
    OVERFLOW_DIR,
    "4.Obscure.nes",
    RESULT_F8
);
onscreen_test!(
    sprite_overflow_5_emulator,
    OVERFLOW_DIR,
    "5.Emulator.nes",
    RESULT_F8
);

/// SURVEY (not an assertion): grade every on-screen ROM in the three
/// 2005 suites and print code + settle frames, so the result protocol
/// and completion detection can be eyeballed before wiring assertions.
#[test]
#[ignore = "survey: requires local nes-test-roms; run with --ignored --nocapture"]
fn survey_onscreen_suites() {
    let Some(root) = blargg_root() else {
        emu198x_test_skip::skip!("blargg root not found; skipping survey");
    };

    let suites: &[(&str, u16, &[&str])] = &[
        (
            "sprite_hit_tests_2005.10.05",
            RESULT_F8,
            &[
                "01.basics.nes",
                "02.alignment.nes",
                "03.corners.nes",
                "04.flip.nes",
                "05.left_clip.nes",
                "06.right_edge.nes",
                "07.screen_bottom.nes",
                "08.double_height.nes",
                "09.timing_basics.nes",
                "10.timing_order.nes",
                "11.edge_timing.nes",
            ],
        ),
        (
            "sprite_overflow_tests",
            RESULT_F8,
            &[
                "1.Basics.nes",
                "2.Details.nes",
                "3.Timing.nes",
                "4.Obscure.nes",
                "5.Emulator.nes",
            ],
        ),
        (
            "blargg_ppu_tests_2005.09.15b",
            RESULT_F0,
            &[
                "palette_ram.nes",
                "power_up_palette.nes",
                "sprite_ram.nes",
                "vbl_clear_time.nes",
                "vram_access.nes",
            ],
        ),
    ];

    let mut total = 0;
    let mut passed = 0;
    for (dir, result_addr, roms) in suites {
        eprintln!("\n=== {dir} (result @ ${result_addr:04X}) ===");
        for rom in *roms {
            let path = root.join(dir).join(rom);
            if !path.is_file() {
                eprintln!("  {rom:<24} MISSING");
                continue;
            }
            total += 1;
            match grade_onscreen(&path, *result_addr) {
                Ok(r) => {
                    let verdict = if r.code == 1 {
                        passed += 1;
                        "PASS".to_string()
                    } else {
                        format!("FAIL #{}", r.code)
                    };
                    eprintln!("  {rom:<24} {verdict:<10} (settled @ {} frames)", r.frames);
                }
                Err(e) => eprintln!("  {rom:<24} ERROR: {e}"),
            }
        }
    }
    eprintln!("\n=== on-screen suites: {passed}/{total} passing ===");
}
