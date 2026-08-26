//! PAL **video** gates: the PAL machine's screen against Mesen2's, with
//! Mesen2 forced to the 2C07.
//!
//! ⚠ Why this is separate from `pal_geometry.rs`. That file proves the
//! PAL machine *counts* the way a 2C07 does — 312 scanlines, a 70-line
//! VBLANK, no odd-frame dot skip, 3.2 dots per CPU cycle. It says
//! nothing about what those dots contain. This file compares content.
//!
//! ⚠ The blocker that kept this unbuilt: the PAL test ROMs carry **no
//! PAL flag in their iNES headers**, and Mesen2 defaults to
//! `ConsoleRegion::Auto`, so it read every one of them as NTSC. A
//! "PAL golden" captured that way is an NTSC capture with a PAL
//! filename — strictly worse than no capture, because it looks like
//! evidence. `main.cpp` now honours `EMU198X_MESEN_REGION=pal`, and
//! `region-check.lua` reports the region Mesen is *actually* running
//! (`region=Pal`, `clockRate=1662607`) rather than the one requested.
//!
//! ⚠ Structural state is not region-sensitive for every ROM. A ROM
//! whose region-dependence lives entirely in raster timing can write
//! the same nametable, palette RAM and OAM under both regions — and a
//! comparison of those would then pass identically on an NTSC machine,
//! proving nothing about PAL. `region_sensitivity_of_each_rom` measures
//! which ROMs discriminate before any of them is trusted, in the same
//! way `probe_pal_roms_discriminate` does for `pal_apu.rs`.
//!
//! ⚠ **Measured: all five candidates are region-blind.** Not one of
//! them writes a different nametable, palette RAM or OAM under PAL than
//! under NTSC. So this file carries no screen gate — the structural
//! mechanism cannot express the difference, and a gate built on it
//! would pass on an NTSC machine. That is a result, not a gap: it says
//! the remaining PAL video work needs a different instrument.
//!
//! ## The instrument it needs, and where it is blocked
//!
//! Comparing rendered pixels directly does not work either — two
//! emulators need not agree on the RGB of NES colour `$21`, and Mesen2
//! ships several palettes. But if both drew the same picture their
//! framebuffers agree *up to a bijection on colours*, so replacing each
//! pixel with the index of its colour's first appearance in raster
//! order cancels the palette entirely. `screen-pixels.lua` implements
//! that capture and fits inside Mesen's 500-row log.
//!
//! ⚠ It is blocked on Mesen2's headless PPU frame buffer reading back
//! ALL BLACK — one distinct colour across all 61 440 pixels, which is
//! indistinguishable from "the ROM drew nothing". Eliminated so far:
//! the `noVideo` flag to `InitializeEmu` (cleared, no change), the
//! `MaximumSpeed` emulation flag skipping frame rendering (cleared, no
//! change), Lua table 0- vs 1-based indexing (it is 1-based, and
//! `emu.getPixel` reads black too), and the Lua API itself
//! (`emu.takeScreenshot` returns a 258-byte PNG — a blank image).
//!
//! Next step: `NesConsole::GetPpuFrame` hands out
//! `_ppu->GetScreenBuffer(false)`; check which of the two output
//! buffers that selects and whether the headless build ever fills it,
//! rather than trying more flags from outside.
//!
//! Region forcing itself is DONE and verified: `EMU198X_MESEN_REGION=pal`
//! makes Mesen report `region=Pal` with `clockRate=1662607`
//! (`region-check.lua` reads back what Mesen is actually running rather
//! than what was asked for).
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test pal_screen -- --ignored
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::{Nes, Region};

/// Frame at which both sides sample, matching `SAMPLE_FRAME` in
/// `screen_goldens.rs` and `screen-state.lua`.
const SAMPLE_FRAME: u64 = 600;

/// PPU position at which both sides sample: scanline 240, dot 0 —
/// Mesen2's `endFrame`. See the header of `screen_goldens.rs` for why
/// this is a measured position and not a frame counter.
const SAMPLE_SCANLINE: u16 = 240;
const SAMPLE_DOT: u16 = 0;

/// Candidate PAL video ROMs. Region sensitivity is measured, not
/// assumed — see `region_sensitivity_of_each_rom`.
const CANDIDATES: &[&str] = &[
    "nmi_sync/demo_pal.nes",
    "window5/colorwin_pal.nes",
    "other/window_old_pal.nes",
    "other/window2_pal.nes",
    "nes15-1.0.0/nes15-PAL.nes",
];

fn roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

fn load(rel: &str, region: Region) -> Option<Nes> {
    let rom = roms_root()?.join(rel);
    let bytes = std::fs::read(rom).ok()?;
    let parsed = parse_ines(&bytes).ok()?;
    Some(Nes::new_with_region(parsed.mapper, region))
}

/// Advance to the Nth occurrence of the sample position.
fn run_to_sample_point(nes: &mut Nes, n: u64) {
    let mut seen = 0u64;
    // PAL frames are longer than NTSC ones, so the ceiling is sized for
    // the slower region rather than the faster.
    let ceiling = nes.master_clock() + 600_000_000;
    while nes.master_clock() < ceiling {
        nes.tick();
        if nes.ppu.scanline() == SAMPLE_SCANLINE && nes.ppu.dot() == SAMPLE_DOT {
            seen += 1;
            if seen == n {
                return;
            }
            nes.tick();
        }
    }
    panic!("never reached sample point {n} times");
}

/// Structural screen signature, in the same `NT`/`PAL`/`OAM` form the
/// Lua oracle emits. Palette mirroring is resolved; see
/// `screen_goldens.rs` for why that matters.
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

/// ⚠ **The control for every gate this file could carry.**
///
/// A structural comparison of a ROM whose screen state is identical
/// under both regions would pass just as happily on an NTSC machine.
/// That is a test which passes when the thing it names is broken, which
/// is not a gate. This reports, per ROM, whether NTSC and PAL differ —
/// and how much — so that only discriminating ROMs get gated.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn region_sensitivity_of_each_rom() {
    if roms_root().is_none() {
        emu198x_test_skip::skip!("nes-test-roms not found");
    }
    for rel in CANDIDATES {
        let (Some(mut ntsc), Some(mut pal)) = (load(rel, Region::Ntsc), load(rel, Region::Pal))
        else {
            println!("{rel:<34} MISSING");
            continue;
        };
        run_to_sample_point(&mut ntsc, SAMPLE_FRAME);
        run_to_sample_point(&mut pal, SAMPLE_FRAME);
        let a = screen_state(&ntsc);
        let b = screen_state(&pal);
        let differing: Vec<&str> = a
            .iter()
            .zip(b.iter())
            .filter(|(x, y)| x != y)
            .map(|(x, _)| &x[..6])
            .collect();
        if differing.is_empty() {
            println!("{rel:<34} region-BLIND (structural state identical NTSC vs PAL)");
        } else {
            println!(
                "{rel:<34} discriminates: {} of 32 lines differ ({})",
                differing.len(),
                differing.join(" ")
            );
        }
    }
}
