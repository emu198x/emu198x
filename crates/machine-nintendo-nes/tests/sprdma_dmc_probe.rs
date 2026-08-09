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

/// Print the DMA read-cycle sequence of the first few DMA episodes, in the
/// same shape as `tools/mesen-nes-cross-check/dma-trace.lua` prints from
/// Mesen2. The first position where the two disagree is the extra cycle.
///
/// Addresses identify each cycle's role: `$07xx` is an OAM transfer read,
/// `$Exxx`/`$Cxxx` a DMC sample fetch or the CPU's pending address for a
/// halt/dummy/alignment cycle.
fn trace(name: &str, episodes: usize, cycles: usize) {
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
    nes.start_dma_trace();

    // Most DMA episodes are DMC-only (3-4 cycles, no OAM reads). The Mesen
    // side logs only episodes a `$4014` write opened, so collect generously
    // and keep the ones that actually transfer sprites.
    // Accumulate across takes rather than replacing, so an episode straddling
    // a drain boundary is not lost.
    let mut raw: Vec<(u64, u16, bool)> = Vec::new();
    let mut settled = false;
    while nes.master_clock() < MAX_TICKS && !settled {
        for _ in 0..30_000 {
            nes.tick();
            if nes.peek(0x6001) == 0xDE
                && nes.peek(0x6002) == 0xB0
                && nes.peek(0x6003) == 0x61
                && nes.peek(0x6000) != 0x80
            {
                settled = true;
                break;
            }
        }
        raw.extend(nes.take_dma_trace());
        nes.start_dma_trace();
    }
    let all = split_episodes(&raw);
    // The measurements are at the END of the run; the Mesen side's ring buffer
    // retains its tail for the same reason.
    let collected: Vec<Vec<(u64, u16)>> = all.iter().rev().take(episodes).rev().cloned().collect();

    eprintln!("\n═══ {name} ═══ ({} episodes total)", all.len());
    for (i, ep) in collected.iter().enumerate() {
        eprintln!("=== episode {}", i + 1);
        // 16 to a row, matching the Lua side's packing so the two outputs diff
        // line for line.
        eprintln!("start_cycle {}", ep[0].0);
        for chunk in ep.iter().take(cycles).collect::<Vec<_>>().chunks(16) {
            let cells: Vec<String> = chunk.iter().map(|(_, a)| format!("{a:04X}")).collect();
            eprintln!("{}", cells.join(" "));
        }
    }
}

/// Split a trace at its halt cycles and keep only episodes that read the OAM
/// source page — the ones a `$4014` write opened.
fn split_episodes(trace: &[(u64, u16, bool)]) -> Vec<Vec<(u64, u16)>> {
    let mut out: Vec<Vec<(u64, u16)>> = Vec::new();
    let mut current: Vec<(u64, u16)> = Vec::new();
    for &(cyc, addr, is_halt) in trace {
        if is_halt && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        current.push((cyc, addr));
    }
    out.push(current);
    // OAM DMA sources a RAM page; DMC fetches and halt/dummy reads land in
    // cartridge space, so a RAM read is what distinguishes a sprite transfer.
    out.retain(|ep| ep.iter().any(|(_, a)| *a < 0x0800));
    out
}

#[test]
#[ignore = "diagnostic: prints the DMA bus-op trace for diffing against Mesen2"]
fn trace_sprdma_and_dmc_dma() {
    trace("sprdma_and_dmc_dma.nes", 20, 560);
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
