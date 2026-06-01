//! Diagnostic probe for `ppu_read_buffer/test_ppu_read_buffer.nes`.
//!
//! The sweep grader reports the ROM as TIMEOUT (never sets
//! `$6000 = $80`). Earlier diagnostics noted PC sitting at `$EBD8`
//! for ~25M ticks then bouncing to `$FFA9` (NMI/IRQ vicinity).
//! This probe samples PC over the first ~30M ticks and reports the
//! hottest PCs so we can see what loop is spinning.
//!
//! Not part of the normal sweep — `#[ignore]`d. Run with:
//!
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test ppu_read_buffer_probe \
//!     -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

fn nes_test_roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_ppu_read_buffer() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let rom_path = root.join("ppu_read_buffer/test_ppu_read_buffer.nes");
    let bytes = std::fs::read(&rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    let mut pc_hits: BTreeMap<u16, u64> = BTreeMap::new();
    let mut transitions: Vec<(u64, u16)> = Vec::new();
    let mut last_pc: u16 = 0xFFFF;
    let mut sample_at: u64 = 0;
    const SAMPLE_PERIOD: u64 = 12; // ~once per CPU cycle
    const MAX_TICKS: u64 = 30_000_000;
    const REPORT_AT: &[u64] = &[
        1_000_000, 5_000_000, 10_000_000, 15_000_000, 20_000_000, 25_000_000, 30_000_000,
    ];
    let mut report_cursor = 0;

    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        if nes.master_clock() >= sample_at {
            sample_at += SAMPLE_PERIOD;
            let pc = nes.cpu.regs.pc;
            *pc_hits.entry(pc).or_insert(0) += 1;
            if pc != last_pc && transitions.len() < 200 {
                // Only record interesting transitions — when the
                // high byte changes or we cross a 256-byte page
                // boundary backwards (suggests a branch).
                if (pc & 0xFF00) != (last_pc & 0xFF00) || pc < last_pc.saturating_sub(0x20) {
                    transitions.push((nes.master_clock(), pc));
                }
                last_pc = pc;
            }
        }

        if report_cursor < REPORT_AT.len() && nes.master_clock() >= REPORT_AT[report_cursor] {
            let mark = REPORT_AT[report_cursor];
            report_cursor += 1;
            println!(
                "=== ticks={mark} PC=${:04X} $6000={:02X} scanline={}",
                nes.cpu.regs.pc,
                nes.peek(0x6000),
                nes.ppu.scanline()
            );
        }
    }

    let total_samples: u64 = pc_hits.values().sum();
    println!("\nTotal samples: {total_samples}");
    let mut top: Vec<(u16, u64)> = pc_hits.into_iter().collect();
    top.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    println!("\nHottest PCs (top 20):");
    for (pc, count) in top.iter().take(20) {
        let pct = (*count as f64 / total_samples as f64) * 100.0;
        println!("  ${pc:04X}: {count:>7} ({pct:5.2}%)");
    }

    println!("\nFirst 40 page transitions / backward branches:");
    for (tick, pc) in transitions.iter().take(40) {
        println!("  tick={tick:>10}  PC=${pc:04X}");
    }

    println!(
        "\nFinal: PC=${:04X} $6000={:02X} ppu.scanline={} ppu.dot={}",
        nes.cpu.regs.pc,
        nes.peek(0x6000),
        nes.ppu.scanline(),
        nes.ppu.dot()
    );
}

/// Once stalled, capture the value our $2002 read actually returns
/// to the CPU. We hook by checking the CPU's data_in immediately
/// after each tick where PC was at $EBD5 (the BIT $2002 opcode
/// fetch). After 4 cycles of BIT, the read result lands in
/// data_in — we report what it was, alongside the live status
/// shadow + scanline + dot.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn what_the_cpu_actually_reads_from_2002() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let rom_path = root.join("ppu_read_buffer/test_ppu_read_buffer.nes");
    let bytes = std::fs::read(&rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    // Run until stalled.
    let mut consecutive_in_loop: u64 = 0;
    while nes.master_clock() < 30_000_000 {
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if (0xEBD5..=0xEBDA).contains(&pc) {
            consecutive_in_loop += 1;
            if consecutive_in_loop > 100_000 {
                break;
            }
        } else {
            consecutive_in_loop = 0;
        }
    }
    if consecutive_in_loop <= 100_000 {
        println!("Stall not reached; PC=${:04X}", nes.cpu.regs.pc);
        return;
    }
    println!(
        "Stalled at tick={}, PC=${:04X}. Watching $2002 reads…",
        nes.master_clock(),
        nes.cpu.regs.pc
    );

    // Now run for ~2M ticks tracking every $2002 read.
    // The CPU reads $2002 on cycle 4 of BIT $2002, which lands at
    // CPU addr=$2002 with rw=true. We detect by sampling addr/rw
    // each tick and recording when (addr,rw) transitioned to
    // (something with addr=0x2002, rw=true) — but the CPU latches
    // the resulting data_in on the cycle the read completes, so we
    // also record data_in right after.
    // Cover ~3 frames so we see VBL crossings.
    let stop_at = nes.master_clock() + 350_000;
    let mut reads: Vec<Read2002> = Vec::new();
    // Capture PRE-tick state on each iteration so we can show what
    // the PPU looked like at the moment the bus op fired (step 1
    // happens before ppu.run advances the PPU in step 2).
    while nes.master_clock() < stop_at {
        let pre_scanline = nes.ppu.scanline();
        let pre_dot = nes.ppu.dot();
        let pre_status = nes.ppu.status();
        let pre_pc = nes.cpu.regs.pc;
        let pre_addr = nes.cpu.addr;
        let pre_rw = nes.cpu.rw;
        nes.tick();
        // A bus op on $2002 fires when the master tick was a CPU
        // tick AND the CPU's pre-tick state targeted $2002 with rw.
        // `pre_addr == 0x2002 && pre_rw` identifies that — bus op
        // happens at the start of the master tick from the CPU's
        // perspective.
        if pre_addr == 0x2002 && pre_rw {
            reads.push(Read2002 {
                tick: nes.master_clock(),
                scanline: nes.ppu.scanline(),
                dot: nes.ppu.dot(),
                status: nes.ppu.status(),
                data_in: nes.cpu.data_in,
                pc: pre_pc,
                pre_scanline,
                pre_dot,
                pre_status,
            });
        }
    }

    println!("\nTotal $2002 bus-op cycles captured: {}", reads.len());
    println!("\nFirst 40 captures:");
    println!(
        "  {:>10}  pc      pre(sln,dot,ST)        post(sln,dot,ST)   data_in",
        "tick"
    );
    for r in reads.iter().take(40) {
        println!(
            "  {:>10}  ${:04X}  ({:3},{:3},${:02X})   →  ({:3},{:3},${:02X})   ${:02X}",
            r.tick,
            r.pc,
            r.pre_scanline,
            r.pre_dot,
            r.pre_status,
            r.scanline,
            r.dot,
            r.status,
            r.data_in,
        );
    }

    println!("\nFirst 20 captures in VBL window OR with bit-7 in data_in:");
    let mut printed = 0;
    for r in &reads {
        if (241..=260).contains(&r.pre_scanline) || (r.data_in & 0x80) != 0 {
            println!(
                "  {:>10}  ${:04X}  ({:3},{:3},${:02X})   →  ({:3},{:3},${:02X})   ${:02X}",
                r.tick,
                r.pc,
                r.pre_scanline,
                r.pre_dot,
                r.pre_status,
                r.scanline,
                r.dot,
                r.status,
                r.data_in,
            );
            printed += 1;
            if printed >= 20 {
                break;
            }
        }
    }
}

/// Run for the full 200M-tick sweep budget and report where the
/// CPU actually ends up. The initial boot wait-loop at $EBD5/$EBD8
/// IS exited correctly when VBL fires — so any TIMEOUT must be
/// from a later stall.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn long_run_pc_distribution() {
    let Some(root) = nes_test_roms_root() else {
        return;
    };
    let rom_path = root.join("ppu_read_buffer/test_ppu_read_buffer.nes");
    let bytes = std::fs::read(&rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    let mut pc_hits: BTreeMap<u16, u64> = BTreeMap::new();
    const MAX_TICKS: u64 = 200_000_000;
    const SAMPLE_PERIOD: u64 = 12;
    let mut sample_at: u64 = 0;
    const REPORT_AT: &[u64] = &[
        10_000_000,
        25_000_000,
        50_000_000,
        100_000_000,
        150_000_000,
        200_000_000,
    ];
    let mut report_cursor = 0;
    let mut six000_hit = false;
    while nes.master_clock() < MAX_TICKS {
        nes.tick();
        if nes.master_clock() >= sample_at {
            sample_at += SAMPLE_PERIOD;
            *pc_hits.entry(nes.cpu.regs.pc).or_insert(0) += 1;
        }
        if !six000_hit && nes.peek(0x6000) == 0x80 {
            six000_hit = true;
            println!(
                "=== $6000 = $80 hit at tick={} (test signalling start)",
                nes.master_clock()
            );
        }
        if report_cursor < REPORT_AT.len() && nes.master_clock() >= REPORT_AT[report_cursor] {
            let mark = REPORT_AT[report_cursor];
            report_cursor += 1;
            println!(
                "=== ticks={mark} PC=${:04X} $6000=${:02X} scanline={}",
                nes.cpu.regs.pc,
                nes.peek(0x6000),
                nes.ppu.scanline()
            );
        }
    }

    let total: u64 = pc_hits.values().sum();
    let mut top: Vec<(u16, u64)> = pc_hits.into_iter().collect();
    top.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
    println!("\nHottest 25 PCs over 200M ticks:");
    for (pc, count) in top.iter().take(25) {
        let pct = (*count as f64 / total as f64) * 100.0;
        println!("  ${pc:04X}: {count:>9} ({pct:5.2}%)");
    }
    println!(
        "\nFinal: PC=${:04X} $6000=${:02X} scanline={}",
        nes.cpu.regs.pc,
        nes.peek(0x6000),
        nes.ppu.scanline()
    );

    // Decode nametable as ASCII so a sub-test failure message
    // surfaces. Test ROM's halt loop at $EBE2 means whatever the
    // test wrote to the screen before failure is preserved.
    println!("\nNametable text (first nametable, 32x30 tiles):");
    let nt = nes.ppu.nametable_ram();
    for row in 0..30 {
        let start = row * 32;
        let line: String = nt[start..start + 32]
            .iter()
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect();
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            println!("  {trimmed}");
        }
    }
}

/// After observing $2002 returning $93 (bit 7 set) yet the loop
/// continuing, check what the CPU's N flag actually becomes after
/// the BIT $2002 cycle 4. Run to the stall, then single-step until
/// the next $2002 read returns bit-7 set, then dump registers.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn one_loop_iteration_with_vbl_set() {
    let Some(root) = nes_test_roms_root() else {
        return;
    };
    let rom_path = root.join("ppu_read_buffer/test_ppu_read_buffer.nes");
    let bytes = std::fs::read(&rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    let mut consecutive_in_loop: u64 = 0;
    while nes.master_clock() < 30_000_000 {
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if (0xEBD5..=0xEBDA).contains(&pc) {
            consecutive_in_loop += 1;
            if consecutive_in_loop > 100_000 {
                break;
            }
        } else {
            consecutive_in_loop = 0;
        }
    }
    println!("Stalled at tick={}", nes.master_clock());

    // Advance until next $2002 bus op (pre_addr was $2002 + rw)
    // returns a value with bit 7 set in data_in.
    let mut prev_data_in = nes.cpu.data_in;
    while nes.master_clock() < 32_000_000 {
        let pre_addr = nes.cpu.addr;
        let pre_rw = nes.cpu.rw;
        nes.tick();
        if pre_addr == 0x2002 && pre_rw
            && nes.cpu.data_in & 0x80 != 0
            && prev_data_in & 0x80 == 0
        {
            break;
        }
        prev_data_in = nes.cpu.data_in;
    }
    println!(
        "Bit-7 read seen at tick={} PC=${:04X} A=${:02X} P=${:02X} (N={}, Z={})  data_in=${:02X}",
        nes.master_clock(),
        nes.cpu.regs.pc,
        nes.cpu.regs.a,
        nes.cpu.regs.p,
        if nes.cpu.regs.p & 0x80 != 0 { '1' } else { '0' },
        if nes.cpu.regs.p & 0x02 != 0 { '1' } else { '0' },
        nes.cpu.data_in,
    );

    println!("\nNext 30 master ticks (PC / P / addr / rw / data_in):");
    for _ in 0..30 {
        nes.tick();
        println!(
            "  tick={} PC=${:04X} P=${:02X} (N={}) addr=${:04X} rw={} data_in=${:02X}",
            nes.master_clock(),
            nes.cpu.regs.pc,
            nes.cpu.regs.p,
            if nes.cpu.regs.p & 0x80 != 0 { '1' } else { '0' },
            nes.cpu.addr,
            nes.cpu.rw,
            nes.cpu.data_in,
        );
    }
}

struct Read2002 {
    tick: u64,
    scanline: u16,
    dot: u16,
    status: u8,
    data_in: u8,
    pc: u16,
    pre_scanline: u16,
    pre_dot: u16,
    pre_status: u8,
}

/// Once stalled in the BIT $2002 / BPL loop, sample $2002 and the
/// rendering state at intervals to confirm whether the VBlank flag
/// ever rises during the wait.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn vblank_flag_during_stall() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    let rom_path = root.join("ppu_read_buffer/test_ppu_read_buffer.nes");
    let bytes = std::fs::read(&rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    // Run until the stall PC range $EBD5-$EBDA has been live for
    // 100k consecutive ticks. Then sample for the next ~2M ticks.
    let mut consecutive_in_loop: u64 = 0;
    while nes.master_clock() < 30_000_000 {
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if (0xEBD5..=0xEBDA).contains(&pc) {
            consecutive_in_loop += 1;
            if consecutive_in_loop > 100_000 {
                break;
            }
        } else {
            consecutive_in_loop = 0;
        }
    }
    if consecutive_in_loop <= 100_000 {
        println!("Did not enter stall by 30M ticks; final PC=${:04X}", nes.cpu.regs.pc);
        return;
    }

    println!(
        "Stall confirmed at tick={}, PC=${:04X}. Sampling 60 times over 1.8M ticks.",
        nes.master_clock(),
        nes.cpu.regs.pc
    );
    println!("\nReading via nes.ppu.status() / .nmi_occurred() / .ctrl() / .mask()");
    println!("— these do NOT clear the latch (unlike a $2002 read).");
    println!();
    println!(
        "  {:>10}  {:>4}  {:>3}  ST  nmi_occ  CTRL  MASK  edges_seen",
        "tick", "scan", "dot"
    );

    let mut rising_edges = 0u32;
    let mut prev_nmi_occurred = nes.ppu.nmi_occurred();
    for _ in 0..60 {
        let target = nes.master_clock() + 30_000;
        while nes.master_clock() < target {
            nes.tick();
            let now = nes.ppu.nmi_occurred();
            if now && !prev_nmi_occurred {
                rising_edges += 1;
            }
            prev_nmi_occurred = now;
        }
        println!(
            "  {:>10}  {:>4}  {:>3}  ST=${:02X}  nmi_occ={}  CTRL=${:02X}  MASK=${:02X}  edges={}",
            nes.master_clock(),
            nes.ppu.scanline(),
            nes.ppu.dot(),
            nes.ppu.status(),
            nes.ppu.nmi_occurred(),
            nes.ppu.ctrl(),
            nes.ppu.mask(),
            rising_edges,
        );
    }
}
