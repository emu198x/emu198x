//! Diagnostic probe for `ppu_read_buffer/test_ppu_read_buffer.nes`.
//!
//! ⚠ RESOLVED: the ROM passes, and it always did. It writes the full
//! blargg `$6000` report and ends "Passed"; the sweep timed out because
//! the ROM needs ~520M ticks against a 200M ceiling — it runs a still
//! image for 666 frames while its longest sub-test works. It is graded
//! by the sweep now, via `SLOW_ROMS`.
//!
//! The older probes here date from believing the ROM hung: they sample
//! PC and the VBL-wait loop. Kept because they are the record of what
//! was checked, but read the header of `tests/screen_goldens.rs` for the
//! resolution before spending time on them.
//!
//! ⚠ Still open, but narrowed a long way. We reach the ROM's art phase
//! 38 frames later than Mesen2 — 1 131 774 CPU cycles, measured
//! identically at both ends of the art phase. It is entirely accounted
//! for by one 31-iteration sub-test loop that spends 92% of its time in
//! the VBlank wait at `$EBD5`:
//!
//! ```text
//! $EBCE  BIT $8E      ; skip the wait if the flag is set
//! $EBD0  BMI $EBDA
//! $EBD2  BIT $2002    ; clear the VBL flag
//! $EBD5  BIT $2002    ; wait for it to be set again
//! $EBD8  BPL $EBD5
//! $EBDA  RTS
//! ```
//!
//! What is IDENTICAL in both emulators, measured: the CPU cycles between
//! consecutive waits (24666, 30699, 28122, ...), the total per slow
//! iteration (exactly 357 366 cycles), the per-frame CPU cycle counts
//! (29781/29780 strictly alternating), and the OAM DMA stall (513).
//!
//! What DIFFERS: one wait's arrival dot. Entering at scanline 241 costs
//! one frame at cycle ≤ 67 and two at cycle 68. Mesen2 arrives at
//! 50/55/59/62/67/68 across iterations; we arrive at dot 70 every single
//! time. Mesen2's per-slot counts jitter by exactly ±7 — one poll of the
//! 7-cycle wait loop — and ours never jitter at all, so our loop is
//! phase-locked into a 12-frame period while Mesen2's visits 10, 11 and
//! 12.
//!
//! Acquitted along the way: CPU/PPU alignment
//! (`probe_cpu_cycles_per_frame`), frame length, and OAM DMA length.
//!
//! The open question is now specific: **what perturbs Mesen2's wait-loop
//! exit phase by one poll, when every cycle count either side of it
//! matches ours exactly?**
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
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_ppu_read_buffer() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
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
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn what_the_cpu_actually_reads_from_2002() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
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
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn long_run_pc_distribution() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
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
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn one_loop_iteration_with_vbl_set() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
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
        if pre_addr == 0x2002 && pre_rw && nes.cpu.data_in & 0x80 != 0 && prev_data_in & 0x80 == 0 {
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
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn vblank_flag_during_stall() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
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
        println!(
            "Did not enter stall by 30M ticks; final PC=${:04X}",
            nes.cpu.regs.pc
        );
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

/// Record every change to palette RAM from reset, with the CPU PC that
/// caused it. 960 nametable bytes and 256 OAM bytes agree with Mesen
/// while 32 palette bytes do not, so the question is whether the values
/// written are wrong or the addresses are.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_palette_write_trace() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut prev = nes.ppu.palette_ram().to_vec();
    let mut events = 0u32;
    let mut last_reported_clock = 0u64;
    // The ROM halts well inside 600 frames; walk to the sample point.
    let mut seen = 0u64;
    loop {
        nes.tick();
        let now = nes.ppu.palette_ram();
        if now != prev.as_slice() {
            events += 1;
            if events <= 60 || events.is_multiple_of(200) {
                let changed: Vec<String> = now
                    .iter()
                    .zip(prev.iter())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(i, (a, b))| format!("$3F{i:02X}: {b:02X}->{a:02X}"))
                    .collect();
                println!(
                    "#{events:<5} clk={:<10} (+{:<8}) PC=${:04X} sl={:<3} dot={:<3}  {}",
                    nes.master_clock(),
                    nes.master_clock() - last_reported_clock,
                    nes.cpu.regs.pc,
                    nes.ppu.scanline(),
                    nes.ppu.dot(),
                    changed.join(" ")
                );
                last_reported_clock = nes.master_clock();
            }
            prev = now.to_vec();
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            seen += 1;
            if seen == 600 {
                break;
            }
            nes.tick();
        }
    }
    println!("\ntotal palette-RAM change events: {events}");
}

/// Dump the nametable at several late frames so the screen can be
/// rendered and read. Frame 600 — where the structural golden samples —
/// is mid-test: the readme's expected output ends "the test is in
/// progress", and "Passed" prints later.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_nametable_at_late_frames() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let marks = [600u64, 900, 1200, 1500, 1800, 2100, 2400];
    let mut seen = 0u64;
    let mut next = 0usize;
    loop {
        nes.tick();
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            seen += 1;
            if next < marks.len() && seen == marks[next] {
                println!("=== FRAME {seen}");
                let nt = nes.effective_nametable();
                for row in 0..30 {
                    let hex: String = nt[row * 32..row * 32 + 32]
                        .iter()
                        .map(|b| format!("{b:02X}"))
                        .collect();
                    println!("NT {row:02} {hex}");
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
                println!("PAL {pal}");
                next += 1;
                if next == marks.len() {
                    return;
                }
            }
            nes.tick();
        }
    }
}

/// Sample the palette once per frame at Mesen's `endFrame` position and
/// report every phase change, so the ROM's phase boundaries can be
/// compared against the reference rather than guessed at.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_palette_phase_boundaries() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let resolve = |raw: &[u8; 32]| -> String {
        (0..32)
            .map(|i| {
                let src = if matches!(i, 0x10 | 0x14 | 0x18 | 0x1C) {
                    i - 0x10
                } else {
                    i
                };
                format!("{:02X}", raw[src])
            })
            .collect()
    };

    let mut frame = 0u64;
    let mut last = String::new();
    while frame < 2600 {
        nes.tick();
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            let now = resolve(nes.ppu.palette_ram());
            if now != last {
                // ⚠ Quote CPU cycles alongside the frame. Frame counters
                // are convention-dependent across emulators — a Mesen
                // script's own counter starts at script load,
                // `ppu.frameCount` at power-on, and the gap between them
                // depends on where in the frame you sample. CPU cycles
                // have no such ambiguity.
                println!("frame {frame:>5} cpu={:>10}: {now}", nes.cpu_cycle_count());
                last = now;
            }
            nes.tick();
        }
    }
}

/// Log which nametable rows change on which frame, mirroring
/// `tools/mesen-nes-cross-check/nametable-phases.lua`, so a timing
/// offset against the reference can be localised to the sub-test that
/// caused it.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_nametable_change_frames() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut prev: Vec<Option<u64>> = vec![None; 30];
    let mut frame = 0u64;
    while frame < 700 {
        nes.tick();
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            let nt = nes.effective_nametable();
            let mut changed = Vec::new();
            for row in 0..30 {
                let h = nt[row * 32..row * 32 + 32]
                    .iter()
                    .fold(0u64, |a, &b| (a * 31 + b as u64) % 16_777_216);
                if prev[row] != Some(h) {
                    if prev[row].is_some() {
                        changed.push(row.to_string());
                    }
                    prev[row] = Some(h);
                }
            }
            if !changed.is_empty() {
                println!("NTCHG {frame:>5} rows={}", changed.join(","));
            }
            nes.tick();
        }
    }
}

/// Read the blargg `$6000` console. Mesen2 reports the full report text
/// here for this ROM, ending "Passed" — so the protocol is live, and
/// `CnRom` carries work RAM at `$6000-$7FFF` precisely for it. This
/// checks what our run leaves there.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_6000_console() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    for mark in [60u64, 300, 600, 1200, 1500, 1800, 2400] {
        while nes.frame_count() < mark {
            nes.run_frame();
        }
        let sig = [nes.peek(0x6001), nes.peek(0x6002), nes.peek(0x6003)];
        let text: String = (0x6004u16..0x6404)
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
            "frame {mark:>5}: $6000=${:02X} sig={:02X?} text={text:?}",
            nes.peek(0x6000),
            sig
        );
    }
}

/// CPU cycles between successive crossings of a fixed PPU position.
///
/// Mesen runs one of this ROM's sub-test loops on a repeating
/// 12,10,10-frame period where ours is a flat 12. A period-3 signature
/// suggested the CPU/PPU alignment might not be rotating, so this
/// measured it.
///
/// ⚠ It does not support that: ours alternates 29781/29780 CPU cycles
/// per frame, which is exactly right for a 89 342/89 341-dot pair with
/// the odd-frame dot skip active. The alignment DOES vary. The cause of
/// the 12,10,10 cadence is therefore still unknown — recorded so the
/// next attempt does not re-run this measurement expecting an answer.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_cpu_cycles_per_frame() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);
    for _ in 0..120 {
        nes.run_frame();
    }
    while !(nes.ppu.scanline() == 0 && nes.ppu.dot() == 0) {
        nes.tick();
    }
    let mut prev = nes.cpu_cycle_count();
    let mut prev_clock = nes.master_clock();
    let mut deltas = Vec::new();
    let mut dots = Vec::new();
    for _ in 0..12 {
        nes.tick();
        while !(nes.ppu.scanline() == 0 && nes.ppu.dot() == 0) {
            nes.tick();
        }
        let now = nes.cpu_cycle_count();
        deltas.push(now - prev);
        dots.push((nes.master_clock() - prev_clock) / 4);
        prev = now;
        prev_clock = nes.master_clock();
    }
    println!("dots per frame:       {dots:?}");
    println!("CPU cycles per frame: {deltas:?}");
    println!(
        "(89342 dots = 29780.67 CPU cycles; a constant count means the alignment never rotates)"
    );
}

/// What is the 31-iteration sub-test loop doing with its frames?
///
/// Our iteration takes a flat 12 frames; Mesen2's repeats 12,10,10.
/// Same iteration count, so the loop body is the same work — the
/// question is what it waits on. This samples PC across two whole
/// iterations and reports the hot addresses, so the wait loop can be
/// named rather than guessed at.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_subtest_loop_pcs() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    // Row 15 is the row the loop updates. Track its hash so iteration
    // boundaries are exact rather than assumed from the frame numbers.
    let row_hash = |nes: &Nes| -> u64 {
        let nt = nes.effective_nametable();
        nt[15 * 32..16 * 32]
            .iter()
            .fold(0u64, |a, &b| (a * 31 + b as u64) % 16_777_216)
    };

    // Run into the steady part of the sequence.
    let mut frame = 0u64;
    while frame < 200 {
        nes.run_frame();
        frame += 1;
    }

    let mut prev = row_hash(&nes);
    let mut boundaries: Vec<u64> = Vec::new();
    let mut pcs: BTreeMap<u16, u64> = BTreeMap::new();
    let mut sample_at = nes.master_clock();
    // Two iterations at ~12 frames each, with headroom.
    while boundaries.len() < 3 && frame < 260 {
        nes.tick();
        if nes.master_clock() >= sample_at {
            sample_at += 12; // ~once per CPU cycle
            if boundaries.len() == 1 {
                *pcs.entry(nes.cpu.regs.pc).or_insert(0) += 1;
            }
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            let h = row_hash(&nes);
            if h != prev {
                boundaries.push(frame);
                prev = h;
            }
            nes.tick();
        }
    }

    println!("row-15 changes at frames {boundaries:?}");
    let total: u64 = pcs.values().sum();
    let mut hot: Vec<(u16, u64)> = pcs.into_iter().collect();
    hot.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    println!("PC distribution over one iteration ({total} samples):");
    for (pc, n) in hot.iter().take(16) {
        println!(
            "  ${pc:04X}  {n:>7}  {:>5.1}%",
            100.0 * *n as f64 / total as f64
        );
    }
    println!("distinct PCs: {}", hot.len());
}

/// CPU cycles between consecutive entries to the VBlank-wait routine,
/// with the PPU position of each entry — the exact counterpart of
/// `tools/mesen-nes-cross-check/vbl-wait-trace.lua`.
///
/// ⚠ Entries only. `$EBDA` looks like the exit marker and is not usable
/// as one here: it is the not-taken address of the `BPL $EBD5` at
/// `$EBD8`, so PC passes through it on EVERY poll of the loop, 21 dots
/// apart. Mesen2's exec callback fires on opcode fetch and so does mark
/// the real exit — the two are not comparable, and treating them as
/// comparable produced a bogus "waited 0 frames" reading.
///
/// This is the measurement that localises the 38-frame divergence. The
/// call sequence and positions agree with Mesen2 for the first half of
/// each iteration and then drift a few dots late, which is enough to
/// push one wait past a threshold and cost a whole extra frame.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_vbl_wait_cpu_cycles() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut frame = 0u64;
    while frame < 200 {
        nes.run_frame();
        frame += 1;
    }

    let mut entries: Vec<(u64, u16, u16, u64)> = Vec::new();
    let mut prev_pc = nes.cpu.regs.pc;
    let start = frame;
    while frame < start + 40 {
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if pc != prev_pc {
            if pc == 0xEBD2 {
                entries.push((
                    frame,
                    nes.ppu.scanline(),
                    nes.ppu.dot(),
                    nes.cpu_cycle_count(),
                ));
            }
            prev_pc = pc;
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            nes.tick();
        }
    }

    println!("EMU198X — CPU cycles between consecutive wait entries");
    for i in 1..entries.len() {
        let (pf, psl, pd, pc) = entries[i - 1];
        let (f, sl, d, c) = entries[i];
        println!(
            "  slot {}->{}  f{pf}->f{f}  sl{psl:>3}d{pd:>3} -> sl{sl:>3}d{d:>3}   {:>7} CPU cycles",
            (i - 1) % 10,
            i % 10,
            c - pc
        );
        if i % 10 == 0 {
            println!();
        }
    }
}

/// Which cycle-stealing events happen inside one iteration, and what do
/// they cost?
///
/// Mesen2's per-slot CPU cycle counts jitter by exactly ±7 — one poll of
/// the `BIT $2002 / BPL` wait loop — between iterations, while ours are
/// bit-identical every time. Something perturbs Mesen's phase and
/// nothing perturbs ours. This sub-test is the one that combines
/// "sprite 0 hit flag, $4014 DMA and the RAM mirroring", so OAM DMA,
/// whose length is 513 or 514 cycles depending on CPU parity, is the
/// obvious candidate: a parity that never varies would lock the phase.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_dma_costs_in_the_loop() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut frame = 0u64;
    while frame < 200 {
        nes.run_frame();
        frame += 1;
    }

    // Measure the DMA stall directly: the CPU stops advancing PC for the
    // duration, so a long gap between PC changes IS the DMA. An earlier
    // attempt timed "until the bus address changes" and reported 1 cycle
    // for every DMA, which is the write cycle, not the transfer.
    let mut stalls: Vec<(u64, u16, u64)> = Vec::new();
    let mut last_pc = nes.cpu.regs.pc;
    let mut last_change = nes.cpu_cycle_count();
    let mut last_write_4014: Option<u16> = None;
    let start = frame;
    while frame < start + 26 {
        let addr = nes.cpu.addr;
        let rw = nes.cpu.rw;
        nes.tick();
        if !rw && addr == 0x4014 {
            last_write_4014 = Some(nes.cpu.regs.pc);
        }
        let pc = nes.cpu.regs.pc;
        if pc != last_pc {
            let gap = nes.cpu_cycle_count() - last_change;
            if gap > 100 {
                stalls.push((frame, last_pc, gap));
            }
            last_pc = pc;
            last_change = nes.cpu_cycle_count();
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            nes.tick();
        }
    }

    println!("CPU stalls over 26 frames (a stall of ~513/514 is an OAM DMA):");
    let mut counts: BTreeMap<u64, usize> = BTreeMap::new();
    for (f, pc, gap) in &stalls {
        println!("  f{f}  after PC=${pc:04X}  {gap} CPU cycles");
        *counts.entry(*gap).or_insert(0) += 1;
    }
    println!("distinct stall lengths: {counts:?}");
    let _ = last_write_4014;
}

/// Cost of the OAM DMA at `$E50F`, measured over the same span Mesen2's
/// `oam-dma-cost.lua` uses.
///
/// ```text
/// $E50D  LDA $97
/// $E50F  STA $4014     ; starts the OAM DMA
/// $E512  JSR $E2C9     ; -> $E2C9
/// ```
///
/// ⚠⚠ **This measurement produced a wrong claim; keep the retraction
/// with it.** Ours reports 524/525 against Mesen2's 523/524 over what
/// looks like the same span, which reads as an OAM DMA one cycle too
/// long. It is not: `sprdma_dmc_probe::probe_oam_dma_stall_from_trace`
/// counts the transfer directly off the DMA bus trace — 257 reads, a
/// 512-cycle span, one final write cycle — and gets **513**, which is
/// correct.
///
/// The span is the unreliable part. Both ends are instruction
/// boundaries, but Mesen2's exec callback fires on opcode fetch while
/// this side triggers when the PC register takes the value, and for a
/// `JSR` those are not the same cycle. A cross-emulator span is only
/// trustworthy at 1-cycle resolution if both sides sample the same
/// event — the slot-to-slot counts in `probe_vbl_wait_cpu_cycles` do
/// (they agree exactly, 24666/30699/28122/...), and this does not.
///
/// What the numbers DO show is a distribution difference: ours is
/// skewed 20:4 across the two parities where Mesen2's is 12:14. That is
/// a consequence of the locked phase, not its cause.
///
/// ⚠ Span ends at the JSR TARGET, not at `$E512`: the DMA runs on the
/// cycle after the write, so a window closing at `$E512` measures the
/// STA alone and reports a flat 4 cycles on both emulators.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_oam_dma_cost() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut costs: BTreeMap<u64, usize> = BTreeMap::new();
    let mut at_write: Option<u64> = None;
    let mut prev_pc = nes.cpu.regs.pc;
    let mut frame = 0u64;
    while frame < 260 {
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if pc != prev_pc {
            if pc == 0xE50F {
                at_write = Some(nes.cpu_cycle_count());
            }
            if pc == 0xE2C9
                && let Some(start) = at_write.take()
            {
                *costs.entry(nes.cpu_cycle_count() - start).or_insert(0) += 1;
            }
            prev_pc = pc;
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            nes.tick();
        }
    }
    println!("EMU198X OAM DMA cost ($E50F -> $E2C9): {costs:?}");
    println!("Mesen2 over the same window: {{523: 12, 524: 14}}");
    println!(
        "⚠ Do NOT read a defect out of the 1-cycle offset — see this test's \
         doc comment. The transfer itself is 513, measured from the bus trace."
    );
}

/// Every `$2002` poll that lands near the VBlank set dot, during the two
/// waits that decide the divergence.
///
/// `probe_wait_poll_grid` shows the wait entered at scanline 241 dot 70
/// takes 8506 polls — two frames — while the one entered at dot 64 takes
/// 4252, one frame. Two frames means a VBlank went by unseen, and the
/// only way to miss a 6820-dot window with a 21-dot poll grid is
/// suppression: a read landing on the dot the flag sets returns 0 and
/// consumes it.
///
/// `ricoh_ppu_2c02` suppresses on exactly `(241, 1)`. This reports every
/// poll landing at scanline 241 dots 0-3 and what it read, so the miss
/// can be attributed rather than assumed.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_polls_near_vbl_set() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut frame = 0u64;
    while frame < 200 {
        nes.run_frame();
        frame += 1;
    }

    let mut prev_pc = nes.cpu.regs.pc;
    let start = frame;
    println!("polls landing at scanline 241 dots 0-4, and $2002 reads at those dots:");
    while frame < start + 14 {
        let pre_sl = nes.ppu.scanline();
        let pre_dot = nes.ppu.dot();
        let pre_status = nes.ppu.status();
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if pc != prev_pc {
            if (pc == 0xEBD5 || pc == 0xEBD2) && pre_sl == 241 && pre_dot <= 4 {
                println!(
                    "  f{frame} PC=${pc:04X} poll at sl{pre_sl} dot{pre_dot}  status before=${pre_status:02X} after=${:02X}",
                    nes.ppu.status()
                );
            }
            prev_pc = pc;
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            nes.tick();
        }
    }
    println!("(no lines above = the miss is NOT suppression)");
}

/// Wait durations on our side, with a SOUND exit marker.
///
/// ⚠ `$EBDA` cannot mark the exit (it is the `BPL`'s not-taken address,
/// hit on every poll) and neither can "the next entry", which lumps the
/// wait together with the work that follows it. The exit is the first
/// poll at `$EBD5` that actually observes bit 7 set — that is the read
/// whose `BPL` falls through.
///
/// This is the counterpart of the Mesen2 duration table, which uses exec
/// callbacks and is sound on that side.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_wait_durations() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut frame = 0u64;
    while frame < 200 {
        nes.run_frame();
        frame += 1;
    }

    const FRAME_DOTS: f64 = 89342.0;
    let mut prev_pc = nes.cpu.regs.pc;
    let mut entry: Option<(u64, u16, u16, u64)> = None;
    let start = frame;
    println!("wait durations (exit = first poll that sees bit 7):");
    while frame < start + 26 {
        let pre_sl = nes.ppu.scanline();
        let pre_dot = nes.ppu.dot();
        let pre_status = nes.ppu.status();
        let pre_clock = nes.master_clock();
        nes.tick();
        let pc = nes.cpu.regs.pc;
        if pc != prev_pc {
            if pc == 0xEBD2 {
                entry = Some((frame, pre_sl, pre_dot, pre_clock));
            }
            if pc == 0xEBD5
                && pre_status & 0x80 != 0
                && let Some((ef, esl, edot, eclock)) = entry.take()
            {
                let dots = (pre_clock - eclock) / 4;
                println!(
                    "  f{ef} enter sl{esl:>3}d{edot:>3}  ->  exit f{frame} sl{pre_sl:>3}d{pre_dot:>3}   \
                     {dots:>7} dots = {:.2} frames",
                    dots as f64 / FRAME_DOTS
                );
            }
            prev_pc = pc;
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            nes.tick();
        }
    }
}

/// Every `$2002` read and every NMI entry across the missed VBlank.
///
/// The wait entered at scanline 241 dot 69 lasts exactly 2.00 frames:
/// it clears the flag, then frame 205's VBlank passes unseen and it
/// catches frame 206's. Suppression is ruled out — no poll lands within
/// four dots of the set dot. The remaining consumer is the ROM's NMI
/// handler, which lives in RAM at `$1D18` (the vector points below
/// `$2000`) and reads `$2002` itself. Whether the polling loop or the
/// NMI handler sees the flag first is a race decided within a few dots
/// of scanline 241 dot 1.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_nmi_vs_poll_race() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut frame = 0u64;
    while frame < 203 {
        nes.run_frame();
        frame += 1;
    }

    let mut prev_pc = nes.cpu.regs.pc;
    while frame < 207 {
        let pre_sl = nes.ppu.scanline();
        let pre_dot = nes.ppu.dot();
        let pre_status = nes.ppu.status();
        let addr = nes.cpu.addr;
        let rw = nes.cpu.rw;
        let pc_before = nes.cpu.regs.pc;
        nes.tick();
        // A $2002 read: report who did it and what it took.
        if rw && addr == 0x2002 && (pre_sl == 241 || pre_sl == 240) && pre_dot < 40 {
            println!(
                "  f{frame} $2002 READ by PC=${pc_before:04X} at sl{pre_sl} dot{pre_dot}  \
                 status was ${pre_status:02X}"
            );
        }
        let pc = nes.cpu.regs.pc;
        if pc != prev_pc {
            if pc == 0x1D18 {
                println!(
                    "  f{frame} NMI ENTRY at sl{:>3} dot{:>3}  status=${:02X}",
                    nes.ppu.scanline(),
                    nes.ppu.dot(),
                    nes.ppu.status()
                );
            }
            prev_pc = pc;
        }
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            nes.tick();
        }
    }
}

/// Position of the FIRST `$2002` read in each frame — the counterpart of
/// `tools/mesen-nes-cross-check/first-2002-read.lua`.
///
/// Intended to bisect where the two emulators' phase first diverges. It
/// does not work — see the warning below.
///
/// ⚠ Position labels are NOT directly comparable. Mesen processes cycle
/// N then exposes `_cycle == N`; we expose the dot we are ABOUT to
/// process. **Our dot D is the same physical moment as Mesen's cycle
/// D-1.** Reading the raw labels as equal produced a wrong "our VBL
/// suppression window is off by one" conclusion — in fact both suppress
/// at the same moment, and Mesen loses frames to it too.
///
/// ⚠⚠ **UNSOUND — do not use this as a bisect.** No frame shift aligns
/// the two sides: the best of thirteen candidate shifts leaves 20
/// identical positions out of 84. That is not an offset, it is two
/// different sets of reads, so the detection here does not match
/// Mesen2's read callback. The CPU holds an address across its whole
/// cycle and the de-duplication is the likely culprit.
///
/// The sound bisect needs no frame numbering at all: match palette
/// transitions by their VALUE and compare the CPU cycle at each. See
/// `probe_palette_phase_boundaries`, which prints both. That method
/// found the answer — the gap sits flat and steps by exactly one frame
/// at discrete points, so the divergence is suppression hits differing,
/// amplified from a ~115-cycle startup offset.
///
/// Kept only as the record of an instrument that failed and how it was
/// caught.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic; requires local nes-test-roms"]
fn probe_first_2002_read_per_frame() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let bytes =
        std::fs::read(root.join("ppu_read_buffer/test_ppu_read_buffer.nes")).expect("read rom");
    let parsed = parse_ines(&bytes).expect("parse iNES");
    let mut nes = Nes::new(parsed.mapper);

    let mut frame = 0u64;
    let mut seen_this_frame = false;
    let mut last_addr_was_2002 = false;
    while frame <= 140 {
        let pre_sl = nes.ppu.scanline();
        let pre_dot = nes.ppu.dot();
        let is_read = nes.cpu.rw && nes.cpu.addr == 0x2002;
        nes.tick();
        if is_read && !last_addr_was_2002 && !seen_this_frame && frame >= 40 {
            seen_this_frame = true;
            println!(
                "FIRST2002 {frame:>5} sl={pre_sl:>3} cyc={:>3}",
                pre_dot as i32 - 1
            );
        }
        last_addr_was_2002 = is_read;
        if nes.ppu.scanline() == 240 && nes.ppu.dot() == 0 {
            frame += 1;
            seen_this_frame = false;
            nes.tick();
        }
    }
}
