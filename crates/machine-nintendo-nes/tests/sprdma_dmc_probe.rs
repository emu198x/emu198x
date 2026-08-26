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
        emu198x_test_skip::skip!("nes-test-roms not found; skipping");
    };
    let path = root.join("sprdma_and_dmc_dma").join(name);
    let Ok(bytes) = std::fs::read(&path) else {
        emu198x_test_skip::skip!("missing {}", path.display());
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
        emu198x_test_skip::skip!("nes-test-roms not found; skipping");
    };
    let path = root.join("sprdma_and_dmc_dma").join(name);
    let Ok(bytes) = std::fs::read(&path) else {
        emu198x_test_skip::skip!("missing {}", path.display());
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
#[ignore = "DIAGNOSTIC: diagnostic: prints the DMA bus-op trace for diffing against Mesen2"]
fn trace_sprdma_and_dmc_dma() {
    trace("sprdma_and_dmc_dma.nes", 20, 560);
}

#[test]
#[ignore = "DIAGNOSTIC: diagnostic: prints the ROM's measured clock table"]
fn probe_sprdma_and_dmc_dma() {
    probe("sprdma_and_dmc_dma.nes");
}

#[test]
#[ignore = "DIAGNOSTIC: diagnostic: prints the ROM's measured clock table"]
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
#[ignore = "DIAGNOSTIC: diagnostic: lists DMA episodes bracketing the first OAM transfers"]
fn probe_dma_episodes_around_transfers() {
    let Some(root) = rom_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let path = root
        .join("sprdma_and_dmc_dma")
        .join("sprdma_and_dmc_dma.nes");
    let Ok(bytes) = std::fs::read(&path) else {
        emu198x_test_skip::skip!("missing {}", path.display());
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
#[ignore = "DIAGNOSTIC: diagnostic: $4015 re-arms interleaved with DMC sample fetches"]
fn probe_dmc_rearm_vs_fetch() {
    let Some(root) = rom_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let path = root
        .join("sprdma_and_dmc_dma")
        .join("sprdma_and_dmc_dma.nes");
    let Ok(bytes) = std::fs::read(&path) else {
        emu198x_test_skip::skip!("missing {}", path.display());
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
#[ignore = "DIAGNOSTIC: diagnostic: prints dmc_tests on-screen text"]
fn probe_dmc_tests_text() {
    let Some(root) = rom_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
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

/// ⚠ Measure, do not infer. The four `dmc_tests` were recorded as
/// having no readable result channel on the strength of their behaviour
/// (they never draw) and a readme they do not ship. The identical claim
/// about `test_ppu_read_buffer` turned out to be wrong: it wrote the
/// full `$6000` report all along and merely needed more time.
///
/// This checks the `$6000` protocol on all four, over a budget well past
/// the sweep's ceiling.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_dmc_tests_6000_protocol() {
    let Some(root) = rom_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    // ⚠ The last three are the CONTROL. blargg encodes a result code as
    // binary with a leading zero, low tone = 0 and high tone = 1, so a
    // pass (code 0) is ONE tone and codes 2 and 3 are THREE. The two
    // mmc3 ROMs fail by design with codes $02 and $03, and 1.Clocking
    // passes. If the counter cannot tell those apart it cannot read a
    // dmc_tests verdict either, and a one-tone reading would mean
    // nothing.
    for (name, path_rel) in [
        ("buffer_retained", "dmc_tests/buffer_retained.nes"),
        ("latency", "dmc_tests/latency.nes"),
        ("status", "dmc_tests/status.nes"),
        ("status_irq", "dmc_tests/status_irq.nes"),
        ("CTRL pass", "mmc3_irq_tests/1.Clocking.nes"),
        ("CTRL fail $03", "mmc3_irq_tests/5.MMC3_rev_A.nes"),
        ("CTRL fail $02", "mmc3_test_2/rom_singles/6-MMC3_alt.nes"),
    ] {
        let path = root.join(path_rel);
        let Ok(bytes) = std::fs::read(&path) else {
            println!("{name:<18} MISSING");
            continue;
        };
        let parsed = parse_ines(&bytes).expect("parse iNES");
        let mut nes = Nes::new(parsed.mapper);
        let mut sig_at: Option<u64> = None;
        let mut result: Option<(u64, u8)> = None;
        // 900M ticks — well past test_ppu_read_buffer's ~520M.
        while nes.master_clock() < 900_000_000 {
            nes.run_frame();
            if sig_at.is_none()
                && nes.peek(0x6001) == 0xDE
                && nes.peek(0x6002) == 0xB0
                && nes.peek(0x6003) == 0x61
            {
                sig_at = Some(nes.frame_count());
            }
            if sig_at.is_some() && result.is_none() {
                let st = nes.peek(0x6000);
                if st != 0x80 {
                    result = Some((nes.frame_count(), st));
                    break;
                }
            }
        }
        let text: String = (0x6004u16..0x6204)
            .map(|a| nes.peek(a))
            .take_while(|&b| b != 0)
            .map(|b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!(
            "{name:<18} sig={:?} $6000=${:02X} result={:?} bytes@6000..6008={:02X?} text={text:?}",
            sig_at,
            nes.peek(0x6000),
            result,
            (0x6000u16..0x6008).map(|a| nes.peek(a)).collect::<Vec<_>>()
        );
    }
}

/// Do the `dmc_tests` actually beep?
///
/// blargg's shell documents an audible result channel: "A byte is
/// reported as a series of tones. The code is in binary, with a low tone
/// for 0 and a high tone for 1, and with leading zeroes skipped. The
/// first tone is always a zero. A final code of 0 means passed."
/// (`ppu_open_bus/readme.txt`.) That is the ROM author's own protocol,
/// so decoding it would gate these four on a published expectation
/// rather than on Mesen2.
///
/// ⚠ The readme attributes the tones to NSF builds. These four are
/// `.nes`. Whether the .nes builds emit them at all is the question —
/// measured here rather than assumed in either direction.
///
/// Segments 48 kHz audio into bursts by RMS and estimates each burst's
/// pitch by zero-crossing rate.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_dmc_tests_audio() {
    let Some(root) = rom_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    const RATE: f32 = 48_000.0;
    // ⚠ The last three are the CONTROL. blargg encodes a result code as
    // binary with a leading zero, low tone = 0 and high tone = 1, so a
    // pass (code 0) is ONE tone and codes 2 and 3 are THREE. The two
    // mmc3 ROMs fail by design with codes $02 and $03, and 1.Clocking
    // passes. If the counter cannot tell those apart it cannot read a
    // dmc_tests verdict either, and a one-tone reading would mean
    // nothing.
    for (name, path_rel) in [
        ("buffer_retained", "dmc_tests/buffer_retained.nes"),
        ("latency", "dmc_tests/latency.nes"),
        ("status", "dmc_tests/status.nes"),
        ("status_irq", "dmc_tests/status_irq.nes"),
        ("CTRL pass", "mmc3_irq_tests/1.Clocking.nes"),
        ("CTRL fail $03", "mmc3_irq_tests/5.MMC3_rev_A.nes"),
        ("CTRL fail $02", "mmc3_test_2/rom_singles/6-MMC3_alt.nes"),
    ] {
        let path = root.join(path_rel);
        let Ok(bytes) = std::fs::read(&path) else {
            println!("{name:<18} MISSING");
            continue;
        };
        let parsed = parse_ines(&bytes).expect("parse iNES");
        let mut nes = Nes::new(parsed.mapper);
        let mut audio: Vec<f32> = Vec::new();
        for _ in 0..900 {
            nes.run_frame();
            audio.extend(nes.take_audio_buffer());
        }
        // Window RMS at 10 ms, then group loud windows into bursts.
        let win = (RATE * 0.01) as usize;
        let mut loud: Vec<bool> = Vec::new();
        for chunk in audio.chunks(win) {
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            loud.push(rms > 0.02);
        }
        let mut bursts: Vec<(usize, usize)> = Vec::new();
        let mut start: Option<usize> = None;
        for (i, &l) in loud.iter().enumerate() {
            match (l, start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    bursts.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            bursts.push((s, loud.len()));
        }
        let peak = audio.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let tones: Vec<String> = bursts
            .iter()
            .take(12)
            .map(|&(s, e)| {
                let seg = &audio[s * win..(e * win).min(audio.len())];
                let crossings = seg
                    .windows(2)
                    .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                    .count();
                let hz = crossings as f32 * RATE / (2.0 * seg.len() as f32);
                format!("{:.0}Hz/{}ms", hz, (e - s) * 10)
            })
            .collect();
        println!(
            "{name:<18} samples={} peak={peak:.4} bursts={} tones=[{}]",
            audio.len(),
            bursts.len(),
            tones.join(" ")
        );
    }
}

/// OAM DMA stall length, counted from the DMA bus-op trace rather than
/// inferred from an instruction span.
///
/// ⚠ The trace records READS only — the halt read, the alignment dummy
/// and 256 OAM source reads — because put-cycle writes go straight to
/// `ppu.oam_dma_write`. So the count of entries is not the stall; the
/// span between the first and last recorded cycle is, plus the final
/// write cycle.
///
/// NESdev: the `$4014` write suspends the CPU for 513 cycles, or 514 if
/// the write lands on a put cycle. `lib.rs` says so in two comments and
/// nothing has ever asserted it.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_oam_dma_stall_from_trace() {
    let Some(root) = rom_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    for _ in 0..200 {
        nes.run_frame();
    }
    nes.start_dma_trace();
    for _ in 0..26 {
        nes.run_frame();
    }
    let trace = nes.take_dma_trace();
    let episodes = split_episodes(&trace);
    println!("{} OAM episodes", episodes.len());
    for ep in episodes.iter().take(6) {
        let (Some(&(first, _)), Some(&(last, _))) = (ep.first(), ep.last()) else {
            continue;
        };
        println!(
            "  reads={:<4} first_cycle={first} last_cycle={last} span={} \
             (+1 final write cycle = stall {})",
            ep.len(),
            last - first + 1,
            last - first + 2
        );
    }
    println!("expected stall: 513 (write on a get cycle) or 514 (put cycle)");
}
