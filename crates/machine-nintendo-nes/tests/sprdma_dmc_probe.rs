//! Diagnostic probe for `sprdma_and_dmc_dma`.
//!
//! The sweep reports these two ROMs as `FAIL #01 — "T+ Clocks (decimal)"`, but
//! truncates the text to 80 characters. The ROM prints a table of measured
//! clock counts, and the numbers are the whole diagnosis: they say by how many
//! cycles our sprite-DMA / DMC-DMA arbitration differs from hardware, which
//! points at a specific arm of `dma_cycle()` rather than at "DMA is wrong".
//!
//! Not a gate — it prints and passes. Run with:
//! `cargo test -p machine-nintendo-nes --test sprdma_dmc_probe -- --ignored --nocapture`

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;
use std::path::PathBuf;

const MAX_TICKS: u64 = 200_000_000;

fn rom_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

/// Pull printable ASCII runs out of nametable RAM.
///
/// blargg's shell writes ASCII tile codes straight into the nametable, so the
/// text the ROM "printed" is readable there after it halts.
fn nametable_text(nes: &Nes) -> String {
    let nt = nes.ppu.nametable_ram();
    let mut out = String::new();
    let mut run = String::new();
    for &b in nt {
        if (0x20..0x7f).contains(&b) {
            run.push(b as char);
        } else {
            if run.trim().len() >= 3 {
                out.push_str(run.trim());
                out.push('\n');
            }
            run.clear();
        }
    }
    if run.trim().len() >= 3 {
        out.push_str(run.trim());
    }
    out
}

fn probe(name: &str) {
    let Some(root) = rom_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let path = root.join("sprdma_and_dmc_dma").join(name);
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("missing {}", path.display());
        return;
    };
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        if nes.peek(0x6001) == 0xDE && nes.peek(0x6002) == 0xB0 && nes.peek(0x6003) == 0x61 {
            let status = nes.peek(0x6000);
            if status != 0x80 {
                eprintln!("\n═══ {name}: $6000 = 0x{status:02X} ═══");
                break;
            }
        }
    }
    eprintln!("{}", nametable_text(&nes));
}

#[test]
#[ignore = "diagnostic: prints the ROM's measured clock table"]
fn probe_sprdma_and_dmc_dma() {
    probe("sprdma_and_dmc_dma.nes");
}

#[test]
#[ignore = "diagnostic: prints the ROM's measured clock table"]
fn probe_sprdma_and_dmc_dma_512() {
    probe("sprdma_and_dmc_dma_512.nes");
}
