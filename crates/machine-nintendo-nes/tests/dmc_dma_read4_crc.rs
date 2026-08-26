//! `dmc_dma_during_read4`, gated on the ROM's own CRC.
//!
//! ⚠ These ROMs have **several legal outputs**. Their source headers say
//! so outright — `dma_2007_read.s` lists "33 44 or 44 55", and
//! `double_2007_read.s` lists five outputs "(depends on CPU-PPU
//! synchronization)". The number of extra `$2007` reads a DMC DMA
//! provokes turns on CPU-PPU alignment at reset, which is not fixed.
//!
//! That makes a screen diff against a reference capture the **wrong
//! mechanism**: a reference emulator lands on one draw from the legal
//! set, and comparing to it reports a difference that is not a defect.
//! It cost a wrong "candidate defect" claim to learn that here.
//!
//! Each ROM ends with `jsr print_crc` and its header lists every
//! acceptable checksum. Accepting any of them is what the hardware
//! actually licenses, so that is what these gates assert.
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test dmc_dma_read4_crc \
//!     -- --ignored
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

/// Frames to run before reading the printed CRC. The ROMs finish well
/// inside this; 300 is ~5 s emulated.
const FRAMES: u64 = 300;

fn root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

/// Whole-screen text, via the effective nametable so a mapper serving
/// its own nametables is not invisible.
fn screen_text(nes: &Nes) -> String {
    nes.effective_nametable()
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

/// Assert the ROM printed one of the checksums its source header lists.
fn expect_crc_in(name: &str, allowed: &[&str]) {
    let Some(root) = root() else {
        emu198x_test_skip::skip!("nes-test-roms not found; skipping {name}");
    };
    let path = root.join(format!("dmc_dma_during_read4/{name}.nes"));
    let Ok(bytes) = std::fs::read(&path) else {
        emu198x_test_skip::skip!("ROM not present at {path:?}; skipping");
    };
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    for _ in 0..FRAMES {
        nes.run_frame();
    }
    let text = screen_text(&nes);
    let words: Vec<&str> = text.split_whitespace().collect();
    // The CRC is the last 8-hex-digit token on screen.
    let printed = words
        .iter()
        .rev()
        .find(|w| w.len() == 8 && w.chars().all(|c| c.is_ascii_hexdigit()))
        .copied()
        .unwrap_or("<none>");
    assert!(
        allowed.contains(&printed),
        "{name}: printed CRC {printed} is not one of the documented {allowed:?}\n  screen: {}",
        words.join(" ")
    );
}

/// ⚠ Both listed CRCs are correct. Ours prints `5E3DF9C4`, Mesen2 lands
/// on the other one, and neither is more right — which is exactly why
/// this is gated on the set rather than on a golden.
#[test]
#[ignore = "FIXTURE: ROM run — requires test-suites/nes-test-roms"]
fn dma_2007_read_crc() {
    expect_crc_in("dma_2007_read", &["159A7A8F", "5E3DF9C4"]);
}

/// ⚠ Was failing at `D84F6815`, outside the documented set: two `$2007`
/// reads on consecutive CPU cycles were treated as independent, so the
/// second handed back a freshly latched buffer instead of repeating the
/// first read's value. Fixed in `ricoh-ppu-2c02`; now prints `85CFD627`.
#[test]
#[ignore = "FIXTURE: ROM run — requires test-suites/nes-test-roms"]
fn double_2007_read_crc() {
    expect_crc_in(
        "double_2007_read",
        &["85CFD627", "F018C287", "440EF923", "E52F41A5"],
    );
}
