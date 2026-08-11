//! Structural screen gates for ROMs that report only by drawing.
//!
//! These ROMs have no `$6000` protocol and no result byte, so the sweep
//! can only file them as `visual` — which means they can neither pass
//! nor fail, and a regression in any of them is invisible.
//!
//! The way in is to stop trying to *read* the screen and start
//! *comparing* it. Tile indices, palette entries and sprite state are
//! what the PPU was told to draw; unlike rendered pixels they carry no
//! dependence on palette rendering or filters, so two correct emulators
//! must agree on them exactly. Mesen2 runs these ROMs correctly, so its
//! values are the oracle.
//!
//! ⚠ **Goldens are Mesen2's values, not ours.** Recording our own output
//! would freeze whatever we do today, bugs included, and prove only that
//! we are self-consistent. Regenerate with
//! `tools/mesen-nes-cross-check/screen-state.lua`.
//!
//! ⚠ Two things had to be true before any of this meant anything, and
//! both were established the hard way:
//!
//! * **The reference must reproduce itself.** Mesen2's NES default is
//!   `RamState::Random`; under it, two runs of the same ROM differed on
//!   every line. `main.cpp` now forces `RamPowerOnState = AllZeros`.
//! * **The ROM must actually draw.** An all-zero dump means "has not
//!   drawn yet" or "never draws", not "the screen is the answer". The
//!   four `dmc_tests` ROMs write nothing to either nametable ever — they
//!   report by beeping — and are deliberately absent here.
//!
//! ⚠ Five of these render through **palette RAM alone** and never touch
//! a nametable byte (`full_palette` ×3, `nmi_sync` ×2). A
//! nametable-only comparison would call them blank and pass them
//! vacuously, which is why the signature covers palette and OAM too.
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test screen_goldens \
//!     -- --ignored
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

/// Frame at which both emulators sample. Must match `TARGET_FRAME` in
/// `screen-state.lua`, which cannot read it from the environment because
/// Mesen sandboxes Lua's `os` library. Every ROM here first draws by
/// frame 4, so 600 is well clear of start-up.
const SAMPLE_FRAME: u64 = 600;

/// PPU position at which both emulators sample: scanline 240, dot 0.
///
/// ⚠⚠ This is the whole reason the first attempt failed, and it is not
/// a detail. Mesen2's `endFrame` callback fires at **scanline 240,
/// cycle 0** — the start of post-render, measured, not assumed. Our
/// `run_frame` returns 21 scanlines later, at the wrap to scanline 0,
/// by which point the NMI handler has already rewritten palette RAM for
/// the next frame.
///
/// These ROMs rewrite the palette many times per frame, so "the palette
/// at end of frame" means nothing unless both sides name the same PPU
/// position. Sampling by frame counter compared two different moments
/// and made 17 correct ROMs look wrong.
const SAMPLE_SCANLINE: u16 = 240;
const SAMPLE_DOT: u16 = 0;

/// Advance to the Nth occurrence of the sample position.
fn run_to_sample_point(nes: &mut Nes, n: u64) {
    let mut seen = 0u64;
    // Cap generously; 600 frames is ~54M master ticks on NTSC.
    let ceiling = nes.master_clock() + 400_000_000;
    while nes.master_clock() < ceiling {
        nes.tick();
        if nes.ppu.scanline() == SAMPLE_SCANLINE && nes.ppu.dot() == SAMPLE_DOT {
            seen += 1;
            if seen == n {
                return;
            }
            // Skip past this dot so the same position is not counted
            // repeatedly while the PPU sits on it.
            nes.tick();
        }
    }
    panic!("never reached sample point {n} times");
}

fn roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

fn golden_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/nintendo/nes/screen-goldens")
}

/// Render structural screen state in the same `NT`/`PAL`/`OAM` form the
/// Lua oracle emits, so the two are diffable line for line.
///
/// ⚠ Uses [`Nes::effective_nametable`], never `ppu.nametable_ram()`: a
/// mapper may serve `$2000-$2FFF` itself, and MMC5 always does, leaving
/// the console's CIRAM permanently empty.
fn screen_state(nes: &Nes) -> Vec<String> {
    let mut out = Vec::with_capacity(32);
    let nt = nes.effective_nametable();
    for row in 0..30 {
        let hex: String = nt[row * 32..row * 32 + 32]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect();
        out.push(format!("NT {row:02} {hex}"));
    }
    // ⚠ Palette mirroring must be resolved before comparing. `$3F10`,
    // `$3F14`, `$3F18` and `$3F1C` are mirrors of `$3F00/$04/$08/$0C`;
    // our PPU redirects writes at `mirror_palette_addr`, so the RAW
    // 32-byte array still holds power-on values in those four slots
    // while the PPU never reads them. Mesen2's memory dump resolves the
    // mirror, so an unresolved comparison reports four differences on
    // every ROM and none of them are real.
    let raw = nes.ppu.palette_ram();
    let pal: String = (0..32)
        .map(|i| {
            let src = if matches!(i, 0x10 | 0x14 | 0x18 | 0x1C) {
                i - 0x10
            } else {
                i
            };
            format!("{:02X}", raw[src])
        })
        .collect();
    out.push(format!("PAL {pal}"));
    let oam: String = nes.ppu.oam().iter().map(|b| format!("{b:02X}")).collect();
    out.push(format!("OAM {oam}"));
    out
}

fn check(rel: &str) {
    let Some(root) = roms_root() else {
        eprintln!("nes-test-roms not found; skipping {rel}");
        return;
    };
    let rom = root.join(rel);
    if !rom.is_file() {
        eprintln!("ROM not present at {rom:?}; skipping");
        return;
    }
    let key = rel.replace('/', "_").replace(".nes", "");
    let golden_path = golden_root().join(format!("{key}.txt"));
    let Ok(golden_text) = std::fs::read_to_string(&golden_path) else {
        panic!("missing golden {golden_path:?} — regenerate with screen-state.lua");
    };
    let golden: Vec<&str> = golden_text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .collect();
    assert!(
        !golden.is_empty(),
        "golden {golden_path:?} is empty — the capture failed rather than \
         the ROM drawing nothing"
    );

    let bytes = std::fs::read(&rom).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    run_to_sample_point(&mut nes, SAMPLE_FRAME);
    let observed = screen_state(&nes);

    assert_eq!(
        golden.len(),
        observed.len(),
        "{rel}: golden has {} lines, observed {}",
        golden.len(),
        observed.len()
    );
    // Report the first divergence with its line label, which localises a
    // defect to a screen region rather than dumping 32 rows of hex.
    for (g, o) in golden.iter().zip(observed.iter()) {
        assert_eq!(
            g, o,
            "{rel}: screen state diverges from Mesen2\n  mesen2:  {g}\n  emu198x: {o}"
        );
    }
}

macro_rules! screen_gate {
    ($name:ident, $rel:literal) => {
        #[test]
        #[ignore = "ROM run — requires test-suites/nes-test-roms"]
        fn $name() {
            check($rel);
        }
    };
}

// dma_2007_read: see KNOWN_DIVERGENCES below.
// screen_gate!(dma_2007_read, "dmc_dma_during_read4/dma_2007_read.nes");
screen_gate!(dma_4016_read, "dmc_dma_during_read4/dma_4016_read.nes");
// double_2007_read: see KNOWN DIVERGENCES below.
// screen_gate!(double_2007_read, "dmc_dma_during_read4/double_2007_read.nes");
screen_gate!(dpcmletterbox, "dpcmletterbox/dpcmletterbox.nes");
screen_gate!(mmc5exram, "exram/mmc5exram.nes");
screen_gate!(flowing_palette, "full_palette/flowing_palette.nes");
screen_gate!(full_palette, "full_palette/full_palette.nes");
screen_gate!(full_palette_smooth, "full_palette/full_palette_smooth.nes");
screen_gate!(mmc5test, "mmc5test/mmc5test.nes");
screen_gate!(mmc5test_v2, "mmc5test_v2/mmc5test.nes");
screen_gate!(demo_ntsc, "nmi_sync/demo_ntsc.nes");
screen_gate!(demo_pal, "nmi_sync/demo_pal.nes");
// test_ppu_read_buffer is NOT gated here, and needs no golden: it
// reports through blargg's $6000 protocol like any other shell ROM and
// is graded by the sweep. See KNOWN DIVERGENCES below.
screen_gate!(count_errors, "read_joy3/count_errors.nes");
screen_gate!(count_errors_fast, "read_joy3/count_errors_fast.nes");
screen_gate!(test_buttons, "read_joy3/test_buttons.nes");
screen_gate!(volumes, "volume_tests/volumes.nes");
// keyed to PPU position rather than endFrame.

/// Which frame of ours matches Mesen2's frame-600 palette.
///
/// ⚠ Diagnostic for the animated ROMs. `full_palette` rewrites its
/// palette every frame, so a one-frame offset between the two
/// emulators' notions of "frame 600" changes every byte of it while the
/// nametable — static — still matches. Reports the offset rather than
/// assuming one.
#[test]
#[ignore = "diagnostic: find frame alignment against the golden"]
fn probe_frame_alignment() {
    let Some(root) = roms_root() else { return };
    for rel in [
        "full_palette/full_palette.nes",
        "nmi_sync/demo_ntsc.nes",
        "volume_tests/volumes.nes",
    ] {
        let key = rel.replace('/', "_").replace(".nes", "");
        let Ok(g) = std::fs::read_to_string(golden_root().join(format!("{key}.txt"))) else {
            continue;
        };
        let want = g
            .lines()
            .find(|l| l.starts_with("PAL "))
            .unwrap_or("")
            .to_string();
        let bytes = std::fs::read(root.join(rel)).expect("read");
        let parsed = parse_ines(&bytes).expect("parse");
        let mut nes = Nes::new(parsed.mapper);
        let mut hit = None;
        for f in 1..=640u64 {
            nes.run_frame();
            if f >= 560 {
                let pal: String = nes
                    .ppu
                    .palette_ram()
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect();
                if format!("PAL {pal}") == want {
                    hit = Some(f);
                    break;
                }
            }
        }
        println!("  {rel:<38} matches golden at our frame {hit:?} (golden = Mesen frame 600)");
    }
}

// ────────────────────────────────────────────────────────────────
//  ⚠ KNOWN DIVERGENCES — three ROMs whose gates are withheld
//
//  These are NOT "can't be gated". The goldens are captured, the
//  comparison runs, and it reports a difference. They are withheld
//  because the difference looks like a real defect, and a permanently
//  red gate teaches people to ignore the suite.
//
//  1. dma_2007_read and double_2007_read — ⚠ NOT GATEABLE THIS WAY.
//     A single golden cannot gate a ROM with several legal outputs,
//     and these have several. From their own source headers:
//
//       dma_2007_read.s:   "33 44 or 44 55"
//                          crc "159A7A8F or 5E3DF9C4"
//       double_2007_read.s "(depends on CPU-PPU synchronization)"
//                          five listed outputs, four listed CRCs
//
//     ⚠ An earlier note here called these a candidate defect on the
//     strength of a uniform +1 against Mesen2. That was wrong for
//     dma_2007_read: ours prints "44 55", which is the SECOND
//     documented-correct answer. Mesen2 happens to land on the first.
//     Neither is more right; the outcome turns on CPU-PPU alignment at
//     reset, which the ROM says outright.
//
//     The lesson is about the oracle, not the emulator. A reference
//     emulator captures ONE draw from a set of legal behaviours. Before
//     treating a divergence from it as a defect, check whether the ROM
//     admits more than one answer — these say so in their first ten
//     lines.
//
//     The right gate is the ROM's own CRC check, which accepts any of
//     the legal outputs. That is a different mechanism and is not built.
//
//     ⚠ Still open: double_2007_read prints "33 44 55 66 77", which is
//     NOT among its five documented outputs (they begin 22, 02 or 32).
//     Worth investigating on its own terms — via the ROM's CRC, not a
//     screen diff.
//
//  2. test_ppu_read_buffer — RESOLVED, and it was never a defect. Two
//     separate mistakes stacked here.
//
//     The palette difference was a SAMPLING artifact. The ROM shows a
//     still image for 666 frames while its longest sub-test runs
//     ("Contemplate on the art while the test is in progress"). At
//     frame 600 Mesen was in the art phase and we had not entered it
//     yet, so two different phases of the same correct sequence were
//     compared. Our art-phase palette is byte-identical to Mesen's,
//     and so is the settled one; the nametable matched all along
//     because the text does not change across the boundary.
//
//     ⚠ The general rule this file already states for the sample
//     POSITION applies just as much to the sample FRAME: a comparison
//     is only meaningful once the screen has SETTLED. Check that a ROM
//     has stopped changing before capturing a golden for it —
//     tools/mesen-nes-cross-check/palette-phases.lua reports the
//     boundaries.
//
//     The second mistake was believing it had no result protocol. It
//     writes the full blargg $6000 report and ends "Passed"; the sweep
//     had simply timed out at MAX_TICKS, ~560 frames short of the
//     ~1450 it needs. It is graded by the sweep now, on the author's
//     own protocol, which beats any golden.
//
//     ⚠ Left open: reaching the art phase takes us 38 frames longer
//     than Mesen — 1 131 774 CPU cycles, identical at both phase
//     boundaries, with identical 666-frame duration
//     either side. Localised to one 31-iteration sub-test loop whose
//     cadence is a flat 12 frames for us and a repeating 12,10,10 for
//     Mesen. Cause unknown; CPU/PPU alignment was measured and
//     ACQUITTED (tests/ppu_read_buffer_probe.rs). The ROM passes in
//     both, so this is an accuracy question, not a verdict question.
//
//  Re-enable each `screen_gate!` above as its cause is resolved.
// ────────────────────────────────────────────────────────────────
