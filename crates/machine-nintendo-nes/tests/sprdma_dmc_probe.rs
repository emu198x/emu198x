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

/// List every DMA episode -- including the DMC-only ones the trace comparison
/// discards -- around the first OAM transfers, with each episode's first and
/// last cycle.
///
/// The ROM reports ~528 clocks for a region that must contain the 513/514-cycle
/// OAM transfer, and the transfer is known to match the reference exactly. That
/// leaves ~14 cycles of overhead to hold the discrepancy, and the only
/// alignment-sensitive thing that fits is a DMC-only DMA at 3 or 4 cycles.
#[test]
#[ignore = "diagnostic: lists DMA episodes bracketing the first OAM transfers"]
fn probe_dma_episodes_around_transfers() {
    let Some(root) = rom_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let path = root
        .join("sprdma_and_dmc_dma")
        .join("sprdma_and_dmc_dma.nes");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("missing {}", path.display());
        return;
    };
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    nes.start_dma_trace();

    let mut raw: Vec<(u64, u16, bool)> = Vec::new();
    let mut oam_seen = 0;
    while nes.master_clock() < MAX_TICKS && oam_seen < 3 {
        for _ in 0..30_000 {
            nes.tick();
        }
        raw.extend(nes.take_dma_trace());
        nes.start_dma_trace();
        oam_seen = raw
            .iter()
            .filter(|(_, a, _)| *a < 0x0800)
            .map(|(c, _, _)| c / 100_000)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
    }

    // Split at halts, keeping every episode this time.
    let mut episodes: Vec<Vec<(u64, u16)>> = Vec::new();
    let mut current: Vec<(u64, u16)> = Vec::new();
    for &(cyc, addr, is_halt) in &raw {
        if is_halt && !current.is_empty() {
            episodes.push(std::mem::take(&mut current));
        }
        current.push((cyc, addr));
    }
    episodes.push(current);

    for (i, ep) in episodes.iter().enumerate() {
        let is_oam = ep.iter().any(|(_, a)| *a < 0x0800);
        let first = ep[0].0;
        let last = ep[ep.len() - 1].0;
        let kind = if is_oam { "OAM+DMC" } else { "DMC-only" };
        eprintln!(
            "{i:03} {kind} start={first} end={last} span={} reads={}",
            last - first + 1,
            ep.len()
        );
        if i > 60 {
            break;
        }
    }
}

/// Interleave `$4015` re-arm writes with DMC sample fetches, both stamped with
/// the CPU cycle, to compare against Mesen2's `dmc-fetch-cycles.lua` output.
///
/// With `sample_length` 1 every fetch needs its own re-arm, so a re-arm landing
/// while the channel still has bytes remaining is silently ignored.
#[test]
#[ignore = "diagnostic: $4015 re-arms interleaved with DMC sample fetches"]
fn probe_dmc_rearm_vs_fetch() {
    let Some(root) = rom_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let path = root
        .join("sprdma_and_dmc_dma")
        .join("sprdma_and_dmc_dma.nes");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("missing {}", path.display());
        return;
    };
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    nes.start_dma_trace();

    let mut raw: Vec<(u64, u16, bool)> = Vec::new();
    let mut writes: Vec<u64> = Vec::new();
    while nes.master_clock() < MAX_TICKS && raw.len() < 4000 {
        for _ in 0..30_000 {
            nes.tick();
        }
        raw.extend(nes.take_dma_trace());
        writes.extend(nes.take_reg_4015_trace());
        nes.start_dma_trace();
    }

    // Episodes exclude the register-write markers; a DMC-only episode's last
    // read is its sample fetch.
    let mut out: Vec<(u64, char)> = Vec::new();
    let mut current: Vec<(u64, u16)> = Vec::new();
    let flush = |cur: &mut Vec<(u64, u16)>, out: &mut Vec<(u64, char)>| {
        if !cur.is_empty() && !cur.iter().any(|(_, a)| *a < 0x0800) {
            out.push((cur[cur.len() - 1].0, 'F'));
        }
        cur.clear();
    };
    for &c in &writes {
        out.push((c, 'W'));
    }
    for &(cyc, addr, is_halt) in &raw {
        if is_halt {
            flush(&mut current, &mut out);
        }
        current.push((cyc, addr));
    }
    flush(&mut current, &mut out);
    out.sort_by_key(|(c, _)| *c);
    for (cyc, kind) in out.iter().take(24) {
        eprintln!("{kind} {cyc}");
    }
}

/// Read the on-screen text of the `dmc_tests` ROMs.
///
/// These predate blargg's `$6000` result protocol and report only by drawing to
/// the screen, which is why the sweep files them as visual-only. Their text is
/// still in nametable RAM, so if it is legible they can become real gates --
/// `latency.nes` in particular exercises the DMC transfer-start timing.
#[test]
#[ignore = "diagnostic: prints dmc_tests on-screen text"]
fn probe_dmc_tests_text() {
    let Some(root) = rom_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    for name in ["latency.nes", "buffer_retained.nes", "status.nes"] {
        let path = root.join("dmc_tests").join(name);
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("missing {}", path.display());
            continue;
        };
        let parsed = parse_ines(&bytes).expect("parse iNES");
        let mut nes = Nes::new(parsed.mapper);
        for _ in 0..600 {
            nes.run_frame();
        }
        eprintln!("\n═══ {name} ═══\n{}", nametable_text(&nes));
    }
}
