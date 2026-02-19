//! Real Kickstart 1.3 boot test for emu-amiga-rock.

use emu_amiga_rock::Amiga;
use cpu_m68k_rock::cpu::State;
use std::fs;

#[test]
#[ignore] // Requires real KS1.3 ROM at roms/kick13.rom
fn test_boot_kick13() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    println!(
        "Reset: SSP=${:08X} PC=${:08X} SR=${:04X}",
        amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    let total_ticks: u64 = 1_500_000_000; // ~53 seconds PAL
    let report_interval: u64 = 28_375_160; // ~1 second

    let mut last_report = 0u64;
    let mut last_pc = 0u32;
    let mut stuck_count = 0u32;

    // Track PC ranges to understand boot progress
    let mut pc_ranges: [u64; 16] = [0; 16];
    let mut range_ticks: u64 = 0;

    for i in 0..total_ticks {
        amiga.tick();

        // Sample PC every 256 ticks for range tracking
        if i & 0xFF == 0 {
            let pc = amiga.cpu.regs.pc;
            let range = if pc >= 0x00FC0000 && pc < 0x00FD0000 {
                ((pc - 0x00FC0000) >> 12) as usize // 4KB buckets within ROM
            } else if pc < 0x00080000 {
                15 // Chip RAM
            } else {
                14 // Other
            };
            if range < 16 {
                pc_ranges[range] += 1;
            }
            range_ticks += 1;
        }

        // Detect halt
        if matches!(amiga.cpu.state, State::Halted) {
            println!(
                "CPU HALTED at tick {} ({}ms), PC=${:08X} IR=${:04X} SR=${:04X}",
                i,
                i / (PAL_CRYSTAL_HZ / 1000),
                amiga.cpu.regs.pc,
                amiga.cpu.ir,
                amiga.cpu.regs.sr,
            );
            println!("D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1], amiga.cpu.regs.d[2], amiga.cpu.regs.d[3],
                amiga.cpu.regs.d[4], amiga.cpu.regs.d[5], amiga.cpu.regs.d[6], amiga.cpu.regs.d[7]);
            println!("A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.a(0), amiga.cpu.regs.a(1), amiga.cpu.regs.a(2), amiga.cpu.regs.a(3),
                amiga.cpu.regs.a(4), amiga.cpu.regs.a(5), amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            break;
        }

        // Periodic status report
        if i - last_report >= report_interval {
            let pc = amiga.cpu.regs.pc;
            let seconds = i / PAL_CRYSTAL_HZ;
            let sr = amiga.cpu.regs.sr;
            let ipl = (sr >> 8) & 7;
            println!(
                "[{:2}s] PC=${:08X} SR=${:04X}(IPL{}) D0=${:08X} D1=${:08X} A7=${:08X} DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                seconds, pc, sr, ipl, amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                amiga.cpu.regs.a(7),
                amiga.agnus.dmacon,
                amiga.paula.intena, amiga.paula.intreq,
            );

            // Detect stuck PC (same 4KB page)
            let pc_page = pc >> 12;
            let last_page = last_pc >> 12;
            if pc_page == last_page {
                stuck_count += 1;
                if stuck_count >= 30 {
                    println!("PC stuck in page ${:05X}xxx for 30+ seconds — stopping", pc_page);
                    break;
                }
            } else {
                stuck_count = 0;
            }
            last_pc = pc;
            last_report = i;
        }
    }

    println!("\nFinal state after {} ticks:", amiga.master_clock);
    println!(
        "  PC=${:08X} SR=${:04X} SSP=${:08X}",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr, amiga.cpu.regs.ssp
    );
    println!("  Palette[0]=${:04X} overlay={}", amiga.denise.palette[0], amiga.memory.overlay);

    // Print PC range heat map
    println!("\nPC range heat map (% of sampled ticks):");
    for i in 0..14 {
        if pc_ranges[i] > 0 {
            let pct = (pc_ranges[i] as f64 / range_ticks as f64) * 100.0;
            println!("  $FC{:X}000-$FC{:X}FFF: {:6.2}%", i, i, pct);
        }
    }
    if pc_ranges[14] > 0 {
        let pct = (pc_ranges[14] as f64 / range_ticks as f64) * 100.0;
        println!("  Other:            {:6.2}%", pct);
    }
    if pc_ranges[15] > 0 {
        let pct = (pc_ranges[15] as f64 / range_ticks as f64) * 100.0;
        println!("  Chip RAM:         {:6.2}%", pct);
    }
}

/// Short trace test: log key addresses hit during the first ~5 seconds.
/// Helps debug the SAD handler flow without running for minutes.
#[test]
#[ignore]
fn test_boot_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    println!(
        "Reset: SSP=${:08X} PC=${:08X} SR=${:04X}",
        amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    // Key addresses to watch
    const SERIAL_DIAG: u32 = 0xFC3090;
    const SERDATR_READ: u32 = 0xFC30DA;
    const SAD_ENTRY: u32 = 0xFC237E;
    const SER_RECV_BYTE: u32 = 0xFC223E;
    const SER_RECV_WAIT: u32 = 0xFC225E;
    const WARM_RESTART: u32 = 0xFC05F0;
    const CPU_DETECT: u32 = 0xFC0546;
    const PRIV_VIOLATION: u32 = 0xFC30B2;
    const USER_MODE_SET: u32 = 0xFC2398;
    const MOVEQ_M1: u32 = 0xFC30C0;       // MOVEQ #-1, D0 (before inner loops)
    const DBEQ_D1: u32 = 0xFC30F0;        // DBEQ D1 (outer loop counter)
    const BMI_WARMRST: u32 = 0xFC30F4;    // BMI $FC05F0 (after diagnostic)
    const HELP_CHECK: u32 = 0xFC3100;     // BRA target after ROM entry setup
    const COLD_WARM: u32 = 0xFC014C;      // MOVE.L $4.W, D0 (ExecBase check)
    const REINIT: u32 = 0xFC01CE;         // LEA $400.W, A6 (cold boot init)
    const ROM_ENTRY: u32 = 0xFC00D2;      // Initial ROM entry point
    const DEFAULT_EXC: u32 = 0xFC05B4;    // Default exception handler
    const RESET_INSTR: u32 = 0xFC05FA;    // RESET instruction in warm restart

    let total_ticks: u64 = 420_000_000; // ~15 seconds (covers 2+ boot cycles)
    let mut last_key_pc: u32 = 0;
    let mut prev_pc: u32 = 0;
    let mut serdatr_count: u32 = 0;

    for i in 0..total_ticks {
        amiga.tick();

        // Only check on CPU ticks (every 4 crystal ticks)
        if i % 4 != 0 { continue; }

        let pc = amiga.cpu.regs.pc;

        // Log when PC enters a key address for the first time in a sequence.
        // NOTE: 68000 prefetch pipeline means regs.pc points 2-4 bytes ahead
        // of the executing instruction. For critical addresses, verify IR too.
        let ir = amiga.cpu.ir;
        let key = match pc {
            SERIAL_DIAG => Some("SERIAL_DIAG"),
            SERDATR_READ => {
                serdatr_count += 1;
                if serdatr_count <= 10 { Some("SERDATR_READ") } else { None }
            }
            SAD_ENTRY => Some("SAD_ENTRY"),
            SER_RECV_BYTE => Some("SER_RECV_BYTE"),
            SER_RECV_WAIT => Some("SER_RECV_WAIT"),
            WARM_RESTART => Some("WARM_RESTART"),
            CPU_DETECT => Some("CPU_DETECT"),
            PRIV_VIOLATION => Some("PRIV_VIOLATION"),
            USER_MODE_SET => Some("USER_MODE_SET"),
            MOVEQ_M1 => Some("MOVEQ_M1"),
            DBEQ_D1 => Some("DBEQ_D1"),
            BMI_WARMRST => Some("BMI_WARMRST"),
            HELP_CHECK => Some("HELP_CHECK"),
            COLD_WARM => Some("COLD_WARM"),
            REINIT => Some("REINIT"),
            ROM_ENTRY => Some("ROM_ENTRY"),
            // DEFAULT_EXC: verify IR=$303C (MOVE.W #imm,D0) to avoid
            // prefetch pipeline false positives.
            DEFAULT_EXC if ir == 0x303C => Some("DEFAULT_EXC"),
            RESET_INSTR => Some("RESET_INSTR"),
            _ => None,
        };

        if let Some(name) = key {
            if pc != last_key_pc {
                let ms = i / (28_375_160 / 1000);
                // Read ExecBase pointer from chip RAM $0004
                let eb0 = amiga.memory.chip_ram[4] as u32;
                let eb1 = amiga.memory.chip_ram[5] as u32;
                let eb2 = amiga.memory.chip_ram[6] as u32;
                let eb3 = amiga.memory.chip_ram[7] as u32;
                let exec_base = (eb0 << 24) | (eb1 << 16) | (eb2 << 8) | eb3;
                // Also read ChkBase at ExecBase+$26
                let chkbase = if exec_base > 0 && (exec_base as usize + 0x29) < amiga.memory.chip_ram.len() {
                    let off = exec_base as usize + 0x26;
                    (amiga.memory.chip_ram[off] as u32) << 24
                        | (amiga.memory.chip_ram[off+1] as u32) << 16
                        | (amiga.memory.chip_ram[off+2] as u32) << 8
                        | amiga.memory.chip_ram[off+3] as u32
                } else {
                    0
                };
                println!(
                    "[{:4}ms] {} PC=${:08X} SR=${:04X} D0=${:08X} D1=${:08X} A7=${:08X} *$4={:08X} ChkBase={:08X}",
                    ms, name, pc, amiga.cpu.regs.sr,
                    amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                    amiga.cpu.regs.a(7), exec_base, chkbase,
                );
                // Dump exception frame when default handler is entered
                if pc == DEFAULT_EXC {
                    let sp = amiga.cpu.regs.a(7) as usize;
                    if sp >= 6 && sp + 10 <= amiga.memory.chip_ram.len() {
                        // Group 0 (bus/addr error): [fn_code:16][access_addr:32][ir:16][sr:16][pc:32]
                        let fn_code = (amiga.memory.chip_ram[sp] as u16) << 8 | amiga.memory.chip_ram[sp+1] as u16;
                        let acc_hi = (amiga.memory.chip_ram[sp+2] as u32) << 24 | (amiga.memory.chip_ram[sp+3] as u32) << 16
                            | (amiga.memory.chip_ram[sp+4] as u32) << 8 | amiga.memory.chip_ram[sp+5] as u32;
                        let ir = (amiga.memory.chip_ram[sp+6] as u16) << 8 | amiga.memory.chip_ram[sp+7] as u16;
                        let sr_stk = (amiga.memory.chip_ram[sp+8] as u16) << 8 | amiga.memory.chip_ram[sp+9] as u16;
                        let pc_hi = (amiga.memory.chip_ram[sp+10] as u32) << 24 | (amiga.memory.chip_ram[sp+11] as u32) << 16
                            | (amiga.memory.chip_ram[sp+12] as u32) << 8 | amiga.memory.chip_ram[sp+13] as u32;
                        println!("  Exception frame (group 0): fn_code=${:04X} access_addr=${:08X} IR=${:04X} SR=${:04X} PC=${:08X}",
                            fn_code, acc_hi, ir, sr_stk, pc_hi);
                        // Also try group 1/2 frame: [sr:16][pc:32]
                        let sr_g1 = fn_code;
                        let pc_g1 = acc_hi;
                        println!("  Exception frame (group 1/2): SR=${:04X} PC=${:08X}", sr_g1, pc_g1);
                    }
                }
                last_key_pc = pc;
            }
        } else if pc != prev_pc && last_key_pc != 0 {
            // Reset key tracking when we leave a key address
            last_key_pc = 0;
        }

        prev_pc = pc;

        if matches!(amiga.cpu.state, State::Halted) {
            println!(
                "CPU HALTED at tick {} PC=${:08X} IR=${:04X} SR=${:04X}",
                i, amiga.cpu.regs.pc, amiga.cpu.ir, amiga.cpu.regs.sr,
            );
            break;
        }
    }

    println!("\nFinal state after {} ticks:", amiga.master_clock);
    println!(
        "  PC=${:08X} SR=${:04X} SSP=${:08X} USP=${:08X}",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr,
        amiga.cpu.regs.ssp, amiga.cpu.regs.usp
    );
    println!("  D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
        amiga.cpu.regs.d[0], amiga.cpu.regs.d[1], amiga.cpu.regs.d[2], amiga.cpu.regs.d[3],
        amiga.cpu.regs.d[4], amiga.cpu.regs.d[5], amiga.cpu.regs.d[6], amiga.cpu.regs.d[7]);
    println!("  A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
        amiga.cpu.regs.a(0), amiga.cpu.regs.a(1), amiga.cpu.regs.a(2), amiga.cpu.regs.a(3),
        amiga.cpu.regs.a(4), amiga.cpu.regs.a(5), amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
}

const PAL_CRYSTAL_HZ: u64 = 28_375_160;

/// Trace the last N PCs before WARM_RESTART to find the exact triggering code path.
#[test]
#[ignore]
fn test_warm_restart_path() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    // Ring buffer of last 200 unique PCs
    const BUF_SIZE: usize = 200;
    let mut pc_ring: [(u32, u16, u16); BUF_SIZE] = [(0, 0, 0); BUF_SIZE]; // (PC, IR, SR)
    let mut ring_idx: usize = 0;
    let mut prev_pc: u32 = 0;

    // Also track 4KB page transitions
    let mut last_page: u32 = 0xFFFFFFFF;

    // Run for ~4 seconds (past the 2936ms warm restart)
    let total_ticks: u64 = 120_000_000;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let pc = amiga.cpu.regs.pc;

        // Track page transitions
        let page = pc >> 12;
        if page != last_page {
            let ms = i / (PAL_CRYSTAL_HZ / 1000);
            if ms > 660 && ms < 3000 {
                println!("[{:4}ms] Page ${:05X}xxx  PC=${:08X} IR=${:04X} SR=${:04X} D0=${:08X} D1=${:08X} A7=${:08X}",
                    ms, page, pc, amiga.cpu.ir, amiga.cpu.regs.sr,
                    amiga.cpu.regs.d[0], amiga.cpu.regs.d[1], amiga.cpu.regs.a(7));
            }
            last_page = page;
        }

        // Capture state at Guru alert code points (BEFORE prev_pc update)
        if pc >= 0xFC3040 && pc <= 0xFC3070 && pc != prev_pc {
            let ms = i / (PAL_CRYSTAL_HZ / 1000);
            println!("[{:4}ms] ALERT PC=${:08X} IR=${:04X} D7=${:08X} D0=${:08X} A5=${:08X} A6=${:08X} SR=${:04X}",
                ms, pc, amiga.cpu.ir, amiga.cpu.regs.d[7], amiga.cpu.regs.d[0],
                amiga.cpu.regs.a(5), amiga.cpu.regs.a(6), amiga.cpu.regs.sr);
        }

        // Record unique PCs in ring buffer
        if pc != prev_pc {
            pc_ring[ring_idx] = (pc, amiga.cpu.ir, amiga.cpu.regs.sr);
            ring_idx = (ring_idx + 1) % BUF_SIZE;
        }
        prev_pc = pc;

        // Stop at WARM_RESTART
        if pc == 0xFC05F0 {
            let ms = i / (PAL_CRYSTAL_HZ / 1000);
            println!("\n=== WARM_RESTART at {}ms ===", ms);
            println!("  D0=${:08X} D1=${:08X} D2=${:08X} D3=${:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                amiga.cpu.regs.d[2], amiga.cpu.regs.d[3]);
            println!("  A0=${:08X} A6=${:08X} A7=${:08X} SR=${:04X}",
                amiga.cpu.regs.a(0), amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7), amiga.cpu.regs.sr);

            // Dump ring buffer (last 200 unique PCs)
            println!("\nLast {} unique PCs before WARM_RESTART:", BUF_SIZE);
            for j in 0..BUF_SIZE {
                let idx = (ring_idx + j) % BUF_SIZE;
                let (rpc, rir, rsr) = pc_ring[idx];
                if rpc != 0 {
                    println!("  PC=${:08X} IR=${:04X} SR=${:04X}", rpc, rir, rsr);
                }
            }
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("CPU HALTED at tick {}", i);
            break;
        }
    }
}

/// Trace what sets D7 and triggers Alert(3).
/// Captures: D7 transitions, InitResident calls, and Guru entry.
#[test]
#[ignore]
fn test_alert_trigger_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    let mut prev_pc: u32 = 0;
    let mut prev_d7: u32 = 0;
    let mut prev_d0: u32 = 0;

    // Track InitResident calls and results
    let mut in_resident_init = false;
    let mut resident_init_count = 0u32;

    // Find ExecBase for computing LVOs
    let mut exec_base: u32 = 0;

    // Run for ~2.5 seconds (past the 1715ms alert)
    let total_ticks: u64 = 80_000_000;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let pc = amiga.cpu.regs.pc;
        let d7 = amiga.cpu.regs.d[7];
        let d0 = amiga.cpu.regs.d[0];
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Track ExecBase from $4.L
        if ms > 200 && exec_base == 0 {
            let eb = (amiga.memory.chip_ram[4] as u32) << 24
                | (amiga.memory.chip_ram[5] as u32) << 16
                | (amiga.memory.chip_ram[6] as u32) << 8
                | amiga.memory.chip_ram[7] as u32;
            if eb > 0 && eb < 0x80000 {
                exec_base = eb;
                println!("[{:4}ms] ExecBase = ${:08X}", ms, exec_base);
            }
        }

        // Log when D7 changes (the alert code register) — no pc!=prev_pc
        // requirement so we catch changes during bus waits
        if d7 != prev_d7 && ms > 600 {
            println!("[{:4}ms] D7 changed: ${:08X} -> ${:08X}  PC=${:08X} IR=${:04X} SR=${:04X} A5=${:08X} A6=${:08X} A7=${:08X}",
                ms, prev_d7, d7, pc, amiga.cpu.ir, amiga.cpu.regs.sr,
                amiga.cpu.regs.a(5), amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            // Dump stack at the moment D7 changes to find caller chain
            if d7 == 3 {
                let sp = amiga.cpu.regs.a(7) as usize;
                println!("  Stack at D7=3:");
                for s in 0..16 {
                    let addr = sp + s * 2;
                    if addr + 1 < amiga.memory.chip_ram.len() {
                        let w = (amiga.memory.chip_ram[addr] as u16) << 8
                            | amiga.memory.chip_ram[addr + 1] as u16;
                        print!(" {:04X}", w);
                    }
                }
                println!();
            }
        }

        // Log entry to ResidentInit ($FC0B2C) and InitResident calls
        if pc == 0xFC0B2C && pc != prev_pc {
            println!("[{:4}ms] ResidentInit entry: D0=${:08X}(flags) D1=${:08X}(pri) A2=${:08X}",
                ms, d0, amiga.cpu.regs.d[1], amiga.cpu.regs.a(2));
            in_resident_init = true;
        }

        // Track InitResident (LVO -102 = -$66). It's called at $FC0B58.
        if pc == 0xFC0B58 && pc != prev_pc {
            resident_init_count += 1;
            // A1 points to the RomTag
            let a1 = amiga.cpu.regs.a(1);
            // Read rt_Name at offset $E in the RomTag (it's a pointer)
            let name_ptr = if a1 >= 0xFC0000 {
                let off = (a1 - 0xFC0000) as usize;
                if off + 0x12 < amiga.memory.kickstart.len() {
                    (amiga.memory.kickstart[off + 0xE] as u32) << 24
                    | (amiga.memory.kickstart[off + 0xF] as u32) << 16
                    | (amiga.memory.kickstart[off + 0x10] as u32) << 8
                    | amiga.memory.kickstart[off + 0x11] as u32
                } else { 0 }
            } else { 0 };
            // Read the name string
            let name = if name_ptr >= 0xFC0000 {
                let off = (name_ptr - 0xFC0000) as usize;
                let mut s = String::new();
                for j in 0..40 {
                    if off + j >= amiga.memory.kickstart.len() { break; }
                    let ch = amiga.memory.kickstart[off + j];
                    if ch == 0 { break; }
                    s.push(ch as char);
                }
                s
            } else { format!("@${:08X}", name_ptr) };
            println!("[{:4}ms] InitResident #{}: A1=${:08X} name=\"{}\" D7=${:08X}",
                ms, resident_init_count, a1, name, d7);
        }

        // Track return from InitResident — watch for D0 result
        // InitResident is JSR (A6,-102) at $B58, returns to $B5C
        if pc == 0xFC0B5C && pc != prev_pc {
            println!("[{:4}ms] InitResident returned: D0=${:08X} D7=${:08X}", ms, d0, d7);
        }

        // After ResidentInit loop ends ($B5E)
        if pc == 0xFC0B5E && pc != prev_pc {
            println!("[{:4}ms] ResidentInit loop done, {} modules initialized", ms, resident_init_count);
            in_resident_init = false;
        }

        // Log all unique PCs in the 1710-1716ms window to find the code path
        if ms >= 1710 && ms <= 1716 && pc != prev_pc && pc >= 0xFC0000 {
            // Only log ROM PCs to avoid flooding with jump-table entries
            let rom_offset = pc - 0xFC0000;
            // Skip if we're in the alert handler area (logged separately)
            if rom_offset < 0x3020 || rom_offset > 0x3200 {
                println!("[{:4}ms] ROM PC=${:08X} IR=${:04X} D0=${:08X} D7=${:08X} A6=${:08X} A7=${:08X}",
                    ms, pc, amiga.cpu.ir, d0, d7,
                    amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            }
        }

        // Capture entry to alert handler area
        if pc >= 0xFC3020 && pc <= 0xFC3070 && pc != prev_pc {
            println!("[{:4}ms] ALERT HANDLER PC=${:08X} IR=${:04X} D0=${:08X} D7=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                ms, pc, amiga.cpu.ir, d0, d7,
                amiga.cpu.regs.a(5), amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
        }

        // Capture Guru handler entry ($FC3128)
        if pc == 0xFC3128 && pc != prev_pc {
            println!("[{:4}ms] GURU HANDLER entry: D7=${:08X} A6=${:08X}", ms, d7, amiga.cpu.regs.a(6));
        }

        // Track SSP changes. SSP = A7 when in supervisor mode (SR bit 13 set),
        // or stored in cpu.regs.ssp when in user mode.
        let sr = amiga.cpu.regs.sr;
        let ssp = if sr & 0x2000 != 0 {
            amiga.cpu.regs.a(7) // Supervisor mode: A7 is SSP
        } else {
            amiga.cpu.regs.ssp  // User mode: SSP stored separately
        };
        if ssp >= 0x80000 && pc != prev_pc && ms > 200 {
            println!("[{:4}ms] SSP HIGH! ssp=${:08X} PC=${:08X} IR=${:04X} SR=${:04X} A7=${:08X}",
                ms, ssp, pc, amiga.cpu.ir, sr, amiga.cpu.regs.a(7));
        }

        // Stop at WARM_RESTART
        if pc == 0xFC05F0 {
            println!("\n[{:4}ms] === WARM_RESTART ===", ms);
            println!("  D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                amiga.cpu.regs.d[2], amiga.cpu.regs.d[3],
                amiga.cpu.regs.d[4], amiga.cpu.regs.d[5],
                amiga.cpu.regs.d[6], amiga.cpu.regs.d[7]);
            println!("  A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.a(0), amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2), amiga.cpu.regs.a(3),
                amiga.cpu.regs.a(4), amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("CPU HALTED at tick {}", i);
            break;
        }

        prev_d7 = d7;
        prev_d0 = d0;
        prev_pc = pc;
    }
}

/// Focused trace: log every instruction PC in the REINIT→DEFAULT_EXC range.
#[test]
#[ignore]
fn test_reinit_path() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    // Run until we hit REINIT ($FC01CE) — about 221ms = ~6.3M CPU ticks
    let mut reached_reinit = false;
    let mut tracing = false;
    let mut prev_pc: u32 = 0;
    let mut log_count: u32 = 0;

    for i in 0u64..300_000_000 {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let pc = amiga.cpu.regs.pc;

        if !reached_reinit && pc == 0xFC01CE {
            reached_reinit = true;
            tracing = true;
            println!("=== REINIT reached at tick {} ===", i);
            println!("  A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                amiga.cpu.regs.a(0), amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2), amiga.cpu.regs.a(3));
            println!("  A4=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                amiga.cpu.regs.a(4), amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            println!("  overlay={}", amiga.memory.overlay);
        }

        // Once A0 is near the end of the test, log ALL PCs
        if tracing && amiga.cpu.regs.a(0) >= 0x7E000 && pc >= 0xFC0000 && pc != prev_pc && log_count < 2000 {
            println!(
                "  [DETAIL] PC=${:06X} IR=${:04X} D0={:08X} A0={:08X} A2={:08X} chip[0]={:08X}",
                pc, amiga.cpu.ir,
                amiga.cpu.regs.d[0], amiga.cpu.regs.a(0), amiga.cpu.regs.a(2),
                {
                    let r = &amiga.memory.chip_ram;
                    (u32::from(r[0]) << 24) | (u32::from(r[1]) << 16) | (u32::from(r[2]) << 8) | u32::from(r[3])
                },
            );
        }

        if tracing && pc != prev_pc {
            // Log every unique PC, but limit total output
            if log_count < 500 {
                // Only log PCs outside tight loops (not in delay loops)
                let in_loop = matches!(pc,
                    0xFC0594..=0xFC05B2 // chip RAM test inner loop
                    | 0xFC061A..=0xFC068E // expansion test
                    | 0xFC05D2..=0xFC05EE // LED flash delay
                    | 0xFC05F6..=0xFC05F8 // warm restart delay
                );
                if !in_loop || (pc == 0xFC0592 || pc == 0xFC05B0 || pc == 0xFC05B2
                    || pc == 0xFC05B4 || pc == 0xFC061A || pc == 0xFC068E) {
                    // Only log last 10 iterations plus non-loop entries
                    if in_loop && amiga.cpu.regs.a(0) < 0x7A000 {
                        // Skip early loop iterations
                    } else {
                        println!(
                            "  PC=${:06X} IR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} A5=${:08X}",
                            pc, amiga.cpu.ir,
                            amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                            amiga.cpu.regs.a(0), amiga.cpu.regs.a(1),
                            amiga.cpu.regs.a(2), amiga.cpu.regs.a(3), amiga.cpu.regs.a(5),
                        );
                    }
                    log_count += 1;
                }
            }

            // Stop at DEFAULT_EXC (verify with IR=$303C to avoid prefetch false positives)
            // or the green screen error
            if (pc == 0xFC05B4 && amiga.cpu.ir == 0x303C)
                || pc == 0xFC05B8 || pc == 0xFC0238 {
                println!("=== Hit ${:06X} at tick {} ===", pc, i);
                println!("  A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                    amiga.cpu.regs.a(0), amiga.cpu.regs.a(1),
                    amiga.cpu.regs.a(2), amiga.cpu.regs.a(3));
                println!("  A4=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                    amiga.cpu.regs.a(4), amiga.cpu.regs.a(5),
                    amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
                println!("  D0=${:08X} D1=${:08X} D2=${:08X} SR=${:04X}",
                    amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                    amiga.cpu.regs.d[2], amiga.cpu.regs.sr);
                println!("  overlay={}", amiga.memory.overlay);
                // Print chip_ram[0..8]
                println!("  chip_ram[0..8]: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                    amiga.memory.chip_ram[0], amiga.memory.chip_ram[1],
                    amiga.memory.chip_ram[2], amiga.memory.chip_ram[3],
                    amiga.memory.chip_ram[4], amiga.memory.chip_ram[5],
                    amiga.memory.chip_ram[6], amiga.memory.chip_ram[7]);
                break;
            }
        }

        prev_pc = pc;

        if matches!(amiga.cpu.state, State::Halted) {
            println!("CPU HALTED at tick {}", i);
            break;
        }
    }
}

/// Trace overlay state, vector table writes, and CPU_DETECT timing.
/// Answers: does exec turn off overlay BEFORE CPU_DETECT runs?
#[test]
#[ignore]
fn test_vector_table_init() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    // Show what ROM has at the exception vector offsets
    println!("ROM exception vector area (what CPU reads when overlay is ON):");
    for v in 0..16u32 {
        let off = (v * 4) as usize;
        let val = (rom[off] as u32) << 24
            | (rom[off + 1] as u32) << 16
            | (rom[off + 2] as u32) << 8
            | rom[off + 3] as u32;
        println!("  Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
    }

    let mut amiga = Amiga::new(rom);

    let mut prev_overlay = amiga.memory.overlay;
    let mut prev_vec4: [u8; 4] = [0; 4]; // chip_ram[$10..$14]
    let mut prev_vec3: [u8; 4] = [0; 4]; // chip_ram[$0C..$10]
    let mut prev_pc: u32 = 0;
    let mut cpu_detect_seen = false;

    // Ring buffer to catch what leads PC to $10
    const RING_SIZE: usize = 64;
    let mut pc_ring: [(u32, u16, u16); RING_SIZE] = [(0, 0, 0); RING_SIZE];
    let mut ring_idx: usize = 0;
    let mut caught_pc10 = false;

    // Run for ~3 seconds — enough for cold boot init
    let total_ticks: u64 = 100_000_000;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let pc = amiga.cpu.regs.pc;
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Track overlay changes
        if amiga.memory.overlay != prev_overlay {
            println!("[{:4}ms] OVERLAY: {} -> {}  PC=${:08X} IR=${:04X}",
                ms, prev_overlay, amiga.memory.overlay, pc, amiga.cpu.ir);
            // Dump vector table from chip RAM at this moment
            println!("  Chip RAM vectors at overlay change:");
            for v in 0..16u32 {
                let off = (v * 4) as usize;
                let val = (amiga.memory.chip_ram[off] as u32) << 24
                    | (amiga.memory.chip_ram[off + 1] as u32) << 16
                    | (amiga.memory.chip_ram[off + 2] as u32) << 8
                    | amiga.memory.chip_ram[off + 3] as u32;
                if val != 0 {
                    println!("    Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
                }
            }
            prev_overlay = amiga.memory.overlay;
        }

        // Track writes to vector 4 (illegal instruction, $10-$13)
        let vec4 = [
            amiga.memory.chip_ram[0x10],
            amiga.memory.chip_ram[0x11],
            amiga.memory.chip_ram[0x12],
            amiga.memory.chip_ram[0x13],
        ];
        if vec4 != prev_vec4 {
            let old_val = (prev_vec4[0] as u32) << 24
                | (prev_vec4[1] as u32) << 16
                | (prev_vec4[2] as u32) << 8
                | prev_vec4[3] as u32;
            let new_val = (vec4[0] as u32) << 24
                | (vec4[1] as u32) << 16
                | (vec4[2] as u32) << 8
                | vec4[3] as u32;
            println!("[{:4}ms] Vector 4 ($10): ${:08X} -> ${:08X}  PC=${:08X} IR=${:04X} overlay={}",
                ms, old_val, new_val, pc, amiga.cpu.ir, amiga.memory.overlay);
            prev_vec4 = vec4;
        }

        // Track writes to vector 3 (address error, $0C-$0F)
        let vec3 = [
            amiga.memory.chip_ram[0x0C],
            amiga.memory.chip_ram[0x0D],
            amiga.memory.chip_ram[0x0E],
            amiga.memory.chip_ram[0x0F],
        ];
        if vec3 != prev_vec3 {
            let old_val = (prev_vec3[0] as u32) << 24
                | (prev_vec3[1] as u32) << 16
                | (prev_vec3[2] as u32) << 8
                | prev_vec3[3] as u32;
            let new_val = (vec3[0] as u32) << 24
                | (vec3[1] as u32) << 16
                | (vec3[2] as u32) << 8
                | vec3[3] as u32;
            println!("[{:4}ms] Vector 3 ($0C): ${:08X} -> ${:08X}  PC=${:08X}",
                ms, old_val, new_val, pc);
            prev_vec3 = vec3;
        }

        // Ring buffer: record unique PCs
        if pc != prev_pc {
            pc_ring[ring_idx] = (pc, amiga.cpu.ir, amiga.cpu.regs.sr);
            ring_idx = (ring_idx + 1) % RING_SIZE;
        }

        // Catch execution from the vector table area ($00-$FF)
        if pc < 0x100 && pc != prev_pc && !caught_pc10 {
            caught_pc10 = true;
            println!("[{:4}ms] CPU executing from vector table! PC=${:08X} IR=${:04X} SR=${:04X}",
                ms, pc, amiga.cpu.ir, amiga.cpu.regs.sr);
            println!("  D0=${:08X} D1=${:08X} A6=${:08X} A7=${:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            println!("  Last {} unique PCs:", RING_SIZE);
            for j in 0..RING_SIZE {
                let idx = (ring_idx + j) % RING_SIZE;
                let (rpc, rir, rsr) = pc_ring[idx];
                if rpc != 0 {
                    println!("    PC=${:08X} IR=${:04X} SR=${:04X}", rpc, rir, rsr);
                }
            }
        }

        // Detect CPU_DETECT entry
        if pc == 0xFC0546 && pc != prev_pc && !cpu_detect_seen {
            cpu_detect_seen = true;
            println!("[{:4}ms] CPU_DETECT entry: PC=${:08X} overlay={}",
                ms, pc, amiga.memory.overlay);
            // Dump vectors 3 and 4 as seen through the bus
            let v3_bus = (amiga.memory.read_byte(0x0C) as u32) << 24
                | (amiga.memory.read_byte(0x0D) as u32) << 16
                | (amiga.memory.read_byte(0x0E) as u32) << 8
                | amiga.memory.read_byte(0x0F) as u32;
            let v4_bus = (amiga.memory.read_byte(0x10) as u32) << 24
                | (amiga.memory.read_byte(0x11) as u32) << 16
                | (amiga.memory.read_byte(0x12) as u32) << 8
                | amiga.memory.read_byte(0x13) as u32;
            println!("  Vector 3 via bus (with overlay): ${:08X}", v3_bus);
            println!("  Vector 4 via bus (with overlay): ${:08X}", v4_bus);
            let v3_ram = (amiga.memory.chip_ram[0x0C] as u32) << 24
                | (amiga.memory.chip_ram[0x0D] as u32) << 16
                | (amiga.memory.chip_ram[0x0E] as u32) << 8
                | amiga.memory.chip_ram[0x0F] as u32;
            let v4_ram = (amiga.memory.chip_ram[0x10] as u32) << 24
                | (amiga.memory.chip_ram[0x11] as u32) << 16
                | (amiga.memory.chip_ram[0x12] as u32) << 8
                | amiga.memory.chip_ram[0x13] as u32;
            println!("  Vector 3 in chip RAM (hidden by overlay): ${:08X}", v3_ram);
            println!("  Vector 4 in chip RAM (hidden by overlay): ${:08X}", v4_ram);
        }

        // Stop once we're past CPU_DETECT and init is done
        if cpu_detect_seen && pc == 0xFC05B4 && amiga.cpu.ir == 0x303C {
            println!("[{:4}ms] DEFAULT_EXC handler hit — stopping", ms);
            break;
        }

        // Also stop at WARM_RESTART
        if pc == 0xFC05F0 {
            println!("[{:4}ms] WARM_RESTART — stopping", ms);
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("[{:4}ms] CPU HALTED PC=${:08X} IR=${:04X}", ms, pc, amiga.cpu.ir);
            break;
        }

        prev_pc = pc;
    }

    // Final dump of vector table
    println!("\nFinal chip RAM vector table:");
    for v in 0..16u32 {
        let off = (v * 4) as usize;
        let val = (amiga.memory.chip_ram[off] as u32) << 24
            | (amiga.memory.chip_ram[off + 1] as u32) << 16
            | (amiga.memory.chip_ram[off + 2] as u32) << 8
            | amiga.memory.chip_ram[off + 3] as u32;
        if val != 0 {
            println!("  Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
        }
    }
    println!("  overlay={}", amiga.memory.overlay);
}

/// Diagnose why boot stalls in idle loop after init.
/// Monitors InitResident calls, VERTB handler targets, and task lists.
#[test]
#[ignore]
fn test_idle_loop_diagnosis() {
    let ks_path = "../../roms/kick13.rom";
    let rom_data = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom_data.clone());

    let total_ticks: u64 = 150_000_000; // ~5.3 seconds
    let mut prev_pc: u32 = 0;
    let mut first_stop = false;
    let mut stop_count = 0u32;
    let mut unique_pcs: std::collections::HashSet<u32> = std::collections::HashSet::new();

    // Track InitResident calls
    let mut init_resident_count = 0u32;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let pc = amiga.cpu.regs.pc;
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Track InitCode entry at $FC0B2C (called with D0=flags mask, D1=min version)
        if pc == 0xFC0B2C && pc != prev_pc {
            println!("[{:4}ms] InitCode entry: D0=${:02X}(flags mask) D1=${:02X}(min ver) A6=${:08X}",
                ms, amiga.cpu.regs.d[0] & 0xFF, amiga.cpu.regs.d[1] & 0xFF, amiga.cpu.regs.a(6));
        }

        // Use instr_start_pc for accurate instruction-level tracing
        let ipc = amiga.cpu.instr_start_pc;

        // Detailed trace around keyboard.device init (~1759ms)
        // Log every unique instruction in $FC0B30-$FC0BC0 and the init function
        if ms >= 1755 && ms <= 1770 && ipc != prev_pc {
            if (ipc >= 0xFC0B30 && ipc <= 0xFC0BC0) || ipc >= 0xFE4F40 && ipc <= 0xFE4F60 {
                let ir = amiga.cpu.ir;
                let sr = amiga.cpu.regs.sr;
                let d0 = amiga.cpu.regs.d[0];
                let a1 = amiga.cpu.regs.a(1);
                let a2 = amiga.cpu.regs.a(2);
                let sp = amiga.cpu.regs.a(7);
                println!("  [{:4}ms] ipc=${:08X} IR=${:04X} SR=${:04X} D0=${:08X} A1=${:08X} A2=${:08X} SP=${:08X}",
                    ms, ipc, ir, sr, d0, a1, a2, sp);
            }
        }

        // Track InitResident calls at $FC0B58
        if pc == 0xFC0B58 && pc != prev_pc {
            init_resident_count += 1;
            let a1 = amiga.cpu.regs.a(1);
            // Read rt_Name pointer at offset $E in the RomTag
            let name = read_rom_string(&rom_data, a1, 0x0E);
            // Read rt_Pri at offset $0D and rt_Flags at offset $0A
            let (pri, flags) = if a1 >= 0xFC0000 {
                let off = (a1 - 0xFC0000) as usize;
                if off + 0x0D < rom_data.len() {
                    (rom_data[off + 0x0D] as i8, rom_data[off + 0x0A])
                } else { (0, 0) }
            } else { (0, 0) };
            println!("[{:4}ms] InitResident #{}: A1=${:08X} pri={} flags=${:02X} \"{}\"",
                ms, init_resident_count, a1, pri, flags, name);
        }

        // After last known InitResident (intuition at ~2430ms) and before STOP,
        // trace page transitions to see what code runs in the gap
        if ms >= 2400 && ms <= 3300 && !first_stop {
            let page = pc >> 12;
            let prev_page = prev_pc >> 12;
            if page != prev_page && pc != prev_pc {
                println!("[{:4}ms] GAP PC=${:08X} IR=${:04X} SR=${:04X} D0=${:08X} A6=${:08X} A7=${:08X}",
                    ms, pc, amiga.cpu.ir, amiga.cpu.regs.sr,
                    amiga.cpu.regs.d[0], amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            }
        }

        // Detect first STOP (entering idle loop)
        if pc == 0xFC0F94 && !first_stop {
            first_stop = true;
            println!("\n[{:4}ms] === First STOP (idle loop entered) ===", ms);
            println!("  SR=${:04X} SSP=${:08X} DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                amiga.cpu.regs.sr, amiga.cpu.regs.ssp,
                amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq);

            // Dump ExecBase
            let exec_base = read_chip_long(&amiga.memory.chip_ram, 4);
            println!("  ExecBase = ${:08X}", exec_base);
            if exec_base > 0 && exec_base < 0x80000 {
                let eb = exec_base as usize;
                // TaskReady list: ExecBase + $196 (head pointer)
                let task_ready = read_chip_long(&amiga.memory.chip_ram, eb + 0x196);
                let task_wait = read_chip_long(&amiga.memory.chip_ram, eb + 0x1A4);
                let task_ready_list_addr = (exec_base + 0x196) as usize;
                let task_wait_list_addr = (exec_base + 0x1A4) as usize;
                println!("  TaskReady head=${:08X} (list@${:08X}, tail@${:08X})",
                    task_ready, task_ready_list_addr, task_ready_list_addr + 4);
                println!("  TaskWait  head=${:08X} (list@${:08X}, tail@${:08X})",
                    task_wait, task_wait_list_addr, task_wait_list_addr + 4);

                // Check if TaskReady is empty (head == &lh_Tail)
                let ready_empty = task_ready == (exec_base + 0x196 + 4);
                let wait_empty = task_wait == (exec_base + 0x1A4 + 4);
                println!("  TaskReady empty={}, TaskWait empty={}", ready_empty, wait_empty);

                // Walk task lists if non-empty
                if !ready_empty {
                    println!("  TaskReady nodes:");
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
                }
                if !wait_empty {
                    println!("  TaskWait nodes:");
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
                }

                // ThisTask: ExecBase + $114
                let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
                println!("  ThisTask = ${:08X}", this_task);

                // Dump the 32 bytes around TaskReady for inspection
                println!("  TaskReady raw (ExecBase+$190..+$1B0):");
                for off in (0x190..0x1B0).step_by(4) {
                    let val = read_chip_long(&amiga.memory.chip_ram, eb + off);
                    print!("    +${:03X}: ${:08X}", off, val);
                    if off == 0x196 { print!(" <- TR.lh_Head"); }
                    if off == 0x19A { print!(" <- TR.lh_Tail"); }
                    if off == 0x19E { print!(" <- TR.lh_TailPred"); }
                    if off == 0x1A4 { print!(" <- TW.lh_Head"); }
                    if off == 0x1A8 { print!(" <- TW.lh_Tail"); }
                    if off == 0x1AC { print!(" <- TW.lh_TailPred"); }
                    println!();
                }

                // Interrupt vectors: ExecBase + $54 (IntVects array)
                // Each IntVect is 12 bytes: iv_Data(4), iv_Code(4), iv_Node(4)
                println!("  IntVects:");
                for level in 0..7u32 {
                    let base = eb + 0x54 + (level as usize) * 12;
                    let iv_data = read_chip_long(&amiga.memory.chip_ram, base);
                    let iv_code = read_chip_long(&amiga.memory.chip_ram, base + 4);
                    let iv_node = read_chip_long(&amiga.memory.chip_ram, base + 8);
                    if iv_code != 0 {
                        println!("    Level {}: code=${:08X} data=${:08X} node=${:08X}",
                            level, iv_code, iv_data, iv_node);
                    }
                }

                // ColdCapture, CoolCapture, WarmCapture
                let cold = read_chip_long(&amiga.memory.chip_ram, eb + 0x2A);
                let cool = read_chip_long(&amiga.memory.chip_ram, eb + 0x2E);
                let warm = read_chip_long(&amiga.memory.chip_ram, eb + 0x32);
                println!("  ColdCapture=${:08X} CoolCapture=${:08X} WarmCapture=${:08X}",
                    cold, cool, warm);
            }

            // Dump exception vectors from chip RAM
            println!("  Exception vectors:");
            for v in [24u32, 25, 26, 27, 28, 29, 30] {
                let addr = (v * 4) as usize;
                let val = read_chip_long(&amiga.memory.chip_ram, addr);
                println!("    Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
            }
        }

        // Count STOP entries
        if pc == 0xFC0F94 && pc != prev_pc {
            stop_count += 1;
        }

        // After first STOP, collect unique PCs for a while
        if first_stop && ms < 5000 {
            if pc != prev_pc {
                unique_pcs.insert(pc);
            }
        }

        // Track VERTB handler: when JSR (A5) at $FC0E94 executes
        if pc == 0xFC0E94 && pc != prev_pc && stop_count <= 3 {
            let a5 = amiga.cpu.regs.a(5);
            let a1 = amiga.cpu.regs.a(1);
            let a6 = amiga.cpu.regs.a(6);
            println!("[{:4}ms] VERTB JSR (A5): A5=${:08X} A1=${:08X} A6=${:08X}",
                ms, a5, a1, a6);
        }

        // Track server chain walker calls: JSR (A5) at various offsets in $FC1338-$FC135E
        // The server walker loads A5 from the server node and calls it
        if (pc >= 0xFC1340 && pc <= 0xFC1360) && pc != prev_pc && stop_count <= 3 {
            let a5 = amiga.cpu.regs.a(5);
            let a1 = amiga.cpu.regs.a(1);
            println!("  ServerChain PC=${:08X}: A5=${:08X} A1=${:08X}", pc, a5, a1);
        }

        prev_pc = pc;
    }

    println!("\n=== Summary after {:.1}s ===", total_ticks as f64 / PAL_CRYSTAL_HZ as f64);
    println!("  InitResident calls: {}", init_resident_count);
    println!("  STOP entries: {}", stop_count);
    println!("  Unique PCs after idle: {}", unique_pcs.len());

    // Print unique PCs in sorted order (in ROM range)
    let mut sorted_pcs: Vec<u32> = unique_pcs.into_iter().filter(|&p| p >= 0xFC0000).collect();
    sorted_pcs.sort();
    println!("  Unique ROM PCs:");
    for pc in &sorted_pcs {
        // Read the opcode from ROM
        let off = (*pc - 0xFC0000) as usize;
        let opcode = if off + 1 < rom_data.len() {
            (u16::from(rom_data[off]) << 8) | u16::from(rom_data[off + 1])
        } else { 0 };
        println!("    ${:08X}: ${:04X}", pc, opcode);
    }
}

fn walk_task_list(ram: &[u8], rom: &[u8], head: usize) {
    let mut node = head;
    for _ in 0..10 {
        if node == 0 || node + 20 >= ram.len() { break; }
        let succ = read_chip_long(ram, node);
        if succ == 0 { break; } // reached tail
        // Task name: Task.tc_Node.ln_Name at offset $0A (within Node)
        let name_ptr = read_chip_long(ram, node + 0x0A);
        let name = read_string(ram, rom, name_ptr);
        let state = ram.get(node + 0x0F).copied().unwrap_or(0);
        let pri = ram.get(node + 0x09).copied().unwrap_or(0) as i8;
        println!("    ${:06X}: succ=${:08X} state={} pri={} name=\"{}\"",
            node, succ, state, pri, name);
        node = succ as usize;
    }
}

fn read_string(ram: &[u8], rom: &[u8], addr: u32) -> String {
    let buf = if addr >= 0xFC0000 && addr < 0xFE0000 {
        let off = (addr - 0xFC0000) as usize;
        &rom[off..]
    } else if (addr as usize) < ram.len() {
        &ram[addr as usize..]
    } else {
        return format!("@${:08X}", addr);
    };
    let mut s = String::new();
    for &b in buf.iter().take(40) {
        if b == 0 { break; }
        s.push(b as char);
    }
    s
}

fn read_chip_long(ram: &[u8], addr: usize) -> u32 {
    if addr + 3 < ram.len() {
        (u32::from(ram[addr]) << 24) | (u32::from(ram[addr + 1]) << 16)
        | (u32::from(ram[addr + 2]) << 8) | u32::from(ram[addr + 3])
    } else { 0 }
}

/// Trace A2 at each InitCode loop iteration to find the triple-init cause.
/// Possibilities: (a) ResModules array has duplicate entries, (b) A2 rewinds
/// after InitResident returns, (c) loop re-entered.
#[test]
#[ignore]
fn test_triple_init_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom_data = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom_data.clone());

    let total_ticks: u64 = 200_000_000; // ~7 seconds
    let mut prev_ipc: u32 = 0;
    let mut initcode_entered = false;
    let mut loop_iter = 0u32;
    let mut resmodules_dumped = false;
    let mut last_progress_ms = 0u32;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let ipc = amiga.cpu.instr_start_pc;
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Detect InitCode entry at $FC0B2C
        if ipc == 0xFC0B2C && ipc != prev_ipc {
            initcode_entered = true;
            println!("[{:4}ms] InitCode entry: D0=${:02X} D1=${:02X} A6=${:08X}",
                ms, amiga.cpu.regs.d[0] & 0xFF, amiga.cpu.regs.d[1] & 0xFF,
                amiga.cpu.regs.a(6));
        }

        // At $FC0B34 (MOVEA.L $12C(A6),A2): dump ResModules array
        if ipc == 0xFC0B34 && !resmodules_dumped {
            resmodules_dumped = true;
            // A6 = ExecBase at this point
            let eb = amiga.cpu.regs.a(6) as usize;
            let resmod_ptr = read_chip_long(&amiga.memory.chip_ram, eb + 0x12C) as usize;
            println!("[{:4}ms] ResModules array at ${:08X}:", ms, resmod_ptr);
            for j in 0..40 {
                let entry = read_chip_long(&amiga.memory.chip_ram, resmod_ptr + j * 4);
                if entry == 0 {
                    println!("  [{:2}] ${:08X} (NULL - end)", j, entry);
                    break;
                }
                // Read name and flags from RomTag
                let (name, flags, pri) = if entry >= 0xFC0000 {
                    let off = (entry - 0xFC0000) as usize;
                    let f = if off + 0x0A < rom_data.len() { rom_data[off + 0x0A] } else { 0 };
                    let p = if off + 0x0D < rom_data.len() { rom_data[off + 0x0D] as i8 } else { 0 };
                    (read_rom_string(&rom_data, entry, 0x0E), f, p)
                } else {
                    (format!("@${:08X}", entry), 0, 0)
                };
                let cold = if flags & 0x01 != 0 { "COLD" } else { "    " };
                let auto = if flags & 0x80 != 0 { "AUTO" } else { "    " };
                println!("  [{:2}] ${:08X} flags=${:02X}({} {}) pri={:3} \"{}\"",
                    j, entry, flags, cold, auto, pri, name);
            }
        }

        // Trace each loop iteration: $FC0B38 = MOVE.L (A2)+,D0
        if ipc == 0xFC0B38 && ipc != prev_ipc && initcode_entered {
            loop_iter += 1;
            let a2 = amiga.cpu.regs.a(2);
            let d0 = amiga.cpu.regs.d[0];
            let sp = amiga.cpu.regs.a(7);
            let sr = amiga.cpu.regs.sr;
            println!("[{:4}ms] Loop #{:2}: A2=${:08X} D0=${:08X} SP=${:08X} SR=${:04X}",
                ms, loop_iter, a2, d0, sp, sr);
        }

        // Trace InitResident call at $FC0B58
        if ipc == 0xFC0B58 && ipc != prev_ipc && initcode_entered {
            let a1 = amiga.cpu.regs.a(1);
            let a2 = amiga.cpu.regs.a(2);
            let name = if a1 >= 0xFC0000 {
                read_rom_string(&rom_data, a1, 0x0E)
            } else {
                format!("@${:08X}", a1)
            };
            println!("[{:4}ms]   -> InitResident: A1=${:08X} A2=${:08X} \"{}\"",
                ms, a1, a2, name);
        }

        // Trace InitResident return (BRA.S back to loop at $FC0B5C)
        if ipc == 0xFC0B5C && ipc != prev_ipc && initcode_entered {
            let a2 = amiga.cpu.regs.a(2);
            let d0 = amiga.cpu.regs.d[0];
            println!("[{:4}ms]   <- Return: D0=${:08X} A2=${:08X}", ms, d0, a2);
        }

        // Detect end of InitCode loop
        if ipc == 0xFC0B62 && ipc != prev_ipc && initcode_entered {
            println!("[{:4}ms] InitCode loop done after {} iterations", ms, loop_iter);
            initcode_entered = false;
        }

        // After intuition entry, trace page transitions and key events
        if ms >= 2430 && ms <= 3300 && ipc != prev_ipc {
            // Intuition init wrapper entry/exit
            if ipc == 0xFD3DB6 {
                println!("[{:4}ms] >> Intuition init entry: D0=${:08X} A6=${:08X}",
                    ms, amiga.cpu.regs.d[0], amiga.cpu.regs.a(6));
            }
            if ipc == 0xFD3DC0 {
                println!("[{:4}ms] << Intuition init JSR returned: A6=${:08X}",
                    ms, amiga.cpu.regs.a(6));
            }
            if ipc == 0xFD68B4 {
                println!("[{:4}ms] >> IntuitionInit main entry at $FD68B4", ms);
            }

            // Track page transitions (256-byte pages)
            let page = ipc >> 8;
            let prev_page = prev_ipc >> 8;
            if page != prev_page && ms >= 2490 && ms <= 2700 {
                let sr = amiga.cpu.regs.sr;
                let sp = amiga.cpu.regs.a(7);
                let mode = if sr & 0x2000 != 0 { "SUP" } else { "USR" };
                println!("[{:4}ms] PAGE ${:06X} -> ${:06X} ({}) SP=${:08X} D0=${:08X} A6=${:08X}",
                    ms, prev_ipc, ipc, mode, sp, amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(6));
            }

            // Detect when execution leaves intuition code ($FD3xxx-$FDFxxx)
            // and enters exec idle area ($FC0Fxx)
            if ipc >= 0xFC0F70 && ipc <= 0xFC0FA0 && prev_ipc >= 0xFD0000 {
                println!("[{:4}ms] !! Intuition -> Idle loop: ipc=${:08X} prev=${:08X} SR=${:04X} SP=${:08X}",
                    ms, ipc, prev_ipc, amiga.cpu.regs.sr, amiga.cpu.regs.a(7));
            }
        }

        // Periodic progress
        let ms_u32 = ms as u32;
        if ms_u32 >= 2400 && ms_u32 <= 3400 && ms_u32 >= last_progress_ms + 200 {
            last_progress_ms = ms_u32;
            println!("[{:4}ms] Progress: ipc=${:08X} IR=${:04X} SR=${:04X} SP=${:08X}",
                ms, ipc, amiga.cpu.ir, amiga.cpu.regs.sr, amiga.cpu.regs.a(7));
        } else if ms_u32 >= last_progress_ms + 1000 {
            last_progress_ms = ms_u32;
            println!("[{:4}ms] Progress: ipc=${:08X} IR=${:04X} SR=${:04X}",
                ms, ipc, amiga.cpu.ir, amiga.cpu.regs.sr);
        }

        // Stop at first STOP instruction
        if matches!(amiga.cpu.state, State::Stopped) {
            println!("[{:4}ms] === STOP reached ===", ms);
            // Dump task lists
            let exec_base = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            if exec_base > 0 && exec_base + 0x1B0 < amiga.memory.chip_ram.len() {
                let task_ready = read_chip_long(&amiga.memory.chip_ram, exec_base + 0x196);
                let task_wait = read_chip_long(&amiga.memory.chip_ram, exec_base + 0x1A4);
                let ready_empty = task_ready == (exec_base as u32 + 0x196 + 4);
                println!("  TaskReady empty={} head=${:08X}", ready_empty, task_ready);
                if !ready_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
                }
                let wait_empty = task_wait == (exec_base as u32 + 0x1A4 + 4);
                println!("  TaskWait empty={} head=${:08X}", wait_empty, task_wait);
                if !wait_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
                }
            }
            println!("  DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq);
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("[{:4}ms] CPU HALTED at PC=${:08X}", ms, amiga.cpu.regs.pc);
            break;
        }

        prev_ipc = ipc;
    }
}

/// Trace intuition.library's init function in detail.
///
/// Intuition's init at $FD68B4 never returns to the InitCode loop.
/// This test traces: page transitions, TRAP instructions, task switches,
/// and the exact transition from intuition code to the exec idle loop.
#[test]
#[ignore]
fn test_intuition_init_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom_data = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom_data.clone());

    let total_ticks: u64 = 120_000_000; // ~4.2 seconds
    let mut prev_ipc: u32 = 0;
    let mut prev_this_task: u32 = 0;
    let mut intuition_entered = false;
    let mut tracing = false;
    let mut last_progress_ms = 0u32;
    let mut prev_page: u32 = 0xFFFFFFFF;
    let mut page_transition_count = 0u32;

    // Ring buffer of last 100 unique IPCs before any notable event
    const RING_SIZE: usize = 100;
    let mut ipc_ring: [(u32, u16, u16); RING_SIZE] = [(0, 0, 0); RING_SIZE]; // (ipc, ir, sr)
    let mut ring_idx: usize = 0;

    // Track when we enter intuition init
    let mut intuition_init_ms: u32 = 0;

    // Track the SSP at intuition init entry to detect stack corruption
    let mut init_entry_ssp: u32 = 0;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 { continue; }

        let ipc = amiga.cpu.instr_start_pc;
        let ir = amiga.cpu.ir;
        let sr = amiga.cpu.regs.sr;
        let ms = (i / (PAL_CRYSTAL_HZ / 1000)) as u32;

        // Always record unique IPCs in ring buffer
        if ipc != prev_ipc {
            ipc_ring[ring_idx] = (ipc, ir, sr);
            ring_idx = (ring_idx + 1) % RING_SIZE;
        }

        // Detect intuition main init entry at $FD68B4
        if ipc == 0xFD68B4 && !intuition_entered {
            intuition_entered = true;
            tracing = true;
            intuition_init_ms = ms;
            init_entry_ssp = if sr & 0x2000 != 0 {
                amiga.cpu.regs.a(7)
            } else {
                amiga.cpu.regs.ssp
            };
            println!("=== Intuition main init entry at {}ms ===", ms);
            println!("  D0=${:08X} D1=${:08X} D2=${:08X} D3=${:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1],
                amiga.cpu.regs.d[2], amiga.cpu.regs.d[3]);
            println!("  A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                amiga.cpu.regs.a(0), amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2), amiga.cpu.regs.a(3));
            println!("  A4=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                amiga.cpu.regs.a(4), amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6), amiga.cpu.regs.a(7));
            println!("  SR=${:04X} SSP=${:08X} USP=${:08X}",
                sr, amiga.cpu.regs.ssp, amiga.cpu.regs.usp);

            // Dump ExecBase ThisTask
            let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
            prev_this_task = this_task;
            println!("  ExecBase=${:08X} ThisTask=${:08X}", eb, this_task);
        }

        if !tracing {
            prev_ipc = ipc;
            continue;
        }

        // Stop after 1 second past intuition entry
        if ms > intuition_init_ms + 1000 {
            println!("\n=== Timeout: {}ms past intuition entry ===", ms - intuition_init_ms);
            break;
        }

        if ipc != prev_ipc {
            // --- TRAP instructions (0x4E40-0x4E4F) ---
            // On Amiga, TRAP #0 = Supervisor(), TRAP #15 = KPutStr (debug)
            if ir >= 0x4E40 && ir <= 0x4E4F {
                let trap_num = ir & 0xF;
                println!("[{:4}ms] TRAP #{} at ipc=${:08X} SR=${:04X} SP=${:08X} D0=${:08X}",
                    ms, trap_num, ipc, sr, amiga.cpu.regs.a(7), amiga.cpu.regs.d[0]);
            }

            // --- Unimplemented opcode detection ---
            // The decode.rs logs unimpl opcodes via eprintln, but let's also
            // detect it here by watching for the illegal instruction vector ($10)
            // being fetched immediately after a new opcode.

            // --- Page transitions (4KB pages) ---
            let page = ipc >> 12;
            if page != prev_page {
                page_transition_count += 1;
                let mode = if sr & 0x2000 != 0 { "SUP" } else { "USR" };
                let current_ssp = if sr & 0x2000 != 0 {
                    amiga.cpu.regs.a(7)
                } else {
                    amiga.cpu.regs.ssp
                };

                // Identify the region
                let region = if ipc >= 0xFD0000 && ipc < 0xFE0000 {
                    "INTUI"
                } else if ipc >= 0xFC0000 && ipc < 0xFC2000 {
                    "EXEC-low"
                } else if ipc >= 0xFC0E00 && ipc < 0xFC1000 {
                    "EXEC-int"
                } else if ipc >= 0xFC1000 && ipc < 0xFC2000 {
                    "EXEC-hi"
                } else if ipc >= 0xFC2000 && ipc < 0xFD0000 {
                    "ROM-other"
                } else if ipc < 0x80000 {
                    "CHIPRAM"
                } else {
                    "???"
                };

                // Only log transitions involving intuition code or notable areas
                // (suppress exec interrupt handler chatter when not near intuition)
                let prev_region_intui = prev_ipc >= 0xFD0000 && prev_ipc < 0xFE0000;
                let curr_region_intui = ipc >= 0xFD0000 && ipc < 0xFE0000;
                let notable = prev_region_intui || curr_region_intui
                    || (ipc >= 0xFC0E00 && ipc < 0xFC1000) // exec interrupt handler
                    || ipc < 0x80000  // chip RAM execution
                    || page_transition_count <= 50; // first 50 transitions always

                if notable {
                    println!("[{:4}ms] PAGE {:08X}->{:08X} {} ({}) SSP=${:08X} D0=${:08X} A6=${:08X}",
                        ms, prev_ipc, ipc, mode, region, current_ssp,
                        amiga.cpu.regs.d[0], amiga.cpu.regs.a(6));
                }

                prev_page = page;
            }

            // --- ThisTask changes (task switch detection) ---
            let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            if eb > 0 && eb + 0x118 < amiga.memory.chip_ram.len() {
                let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
                if this_task != prev_this_task {
                    let task_name = read_string(&amiga.memory.chip_ram, &rom_data,
                        read_chip_long(&amiga.memory.chip_ram, this_task as usize + 0x0A));
                    let old_name = if prev_this_task > 0 && (prev_this_task as usize + 0x0A) < amiga.memory.chip_ram.len() {
                        read_string(&amiga.memory.chip_ram, &rom_data,
                            read_chip_long(&amiga.memory.chip_ram, prev_this_task as usize + 0x0A))
                    } else {
                        format!("${:08X}", prev_this_task)
                    };
                    println!("[{:4}ms] TASK SWITCH: \"{}\"(${:08X}) -> \"{}\"(${:08X}) at ipc=${:08X}",
                        ms, old_name, prev_this_task, task_name, this_task, ipc);
                    prev_this_task = this_task;
                }
            }

            // --- Detect first entry to exec idle loop ($FC0F70-$FC0FA0) ---
            if ipc >= 0xFC0F70 && ipc <= 0xFC0FA0 {
                println!("[{:4}ms] IDLE LOOP entry at ipc=${:08X} IR=${:04X} SR=${:04X}",
                    ms, ipc, ir, sr);

                // Dump the last 20 unique IPCs before idle
                println!("  Last 20 IPCs before idle:");
                for j in (RING_SIZE - 20)..RING_SIZE {
                    let idx = (ring_idx + j) % RING_SIZE;
                    let (rpc, rir, rsr) = ipc_ring[idx];
                    if rpc != 0 {
                        let mode = if rsr & 0x2000 != 0 { "S" } else { "U" };
                        println!("    ipc=${:08X} IR=${:04X} SR=${:04X} {}", rpc, rir, rsr, mode);
                    }
                }

                // Dump task lists
                let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
                let task_ready = read_chip_long(&amiga.memory.chip_ram, eb + 0x196);
                let task_wait = read_chip_long(&amiga.memory.chip_ram, eb + 0x1A4);
                let ready_empty = task_ready == (eb as u32 + 0x196 + 4);
                let wait_empty = task_wait == (eb as u32 + 0x1A4 + 4);
                println!("  TaskReady empty={} head=${:08X}", ready_empty, task_ready);
                if !ready_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
                }
                println!("  TaskWait empty={} head=${:08X}", wait_empty, task_wait);
                if !wait_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
                }
                println!("  DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                    amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq);
                break;
            }

            // --- Detect Intuition init wrapper return at $FD3DC0 ---
            if ipc == 0xFD3DC0 {
                println!("[{:4}ms] << Intuition init JSR returned: D0=${:08X} A6=${:08X}",
                    ms, amiga.cpu.regs.d[0], amiga.cpu.regs.a(6));
            }
            if ipc == 0xFD3DC6 {
                println!("[{:4}ms] << Intuition init RTS: D0=${:08X}", ms, amiga.cpu.regs.d[0]);
            }
        }

        // Periodic progress with more detail
        if ms >= last_progress_ms + 100 {
            last_progress_ms = ms;
            let mode = if sr & 0x2000 != 0 { "SUP" } else { "USR" };
            let current_ssp = if sr & 0x2000 != 0 {
                amiga.cpu.regs.a(7)
            } else {
                amiga.cpu.regs.ssp
            };
            let region = if ipc >= 0xFD0000 && ipc < 0xFE0000 {
                "INTUI"
            } else if ipc >= 0xFC0000 && ipc < 0xFD0000 {
                "EXEC/ROM"
            } else if ipc < 0x80000 {
                "CHIPRAM"
            } else {
                "OTHER"
            };
            println!("[{:4}ms] ipc=${:08X} ({}) {} IR=${:04X} SSP=${:08X} D0=${:08X} A6=${:08X}",
                ms, ipc, region, mode, ir, current_ssp,
                amiga.cpu.regs.d[0], amiga.cpu.regs.a(6));
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("[{:4}ms] CPU HALTED at PC=${:08X}", ms, amiga.cpu.regs.pc);
            break;
        }

        // Detect STOP state (idle loop reached)
        if matches!(amiga.cpu.state, State::Stopped) {
            println!("[{:4}ms] STOP instruction at ipc=${:08X}", ms, ipc);
            let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
            let task_name = read_string(&amiga.memory.chip_ram, &rom_data,
                read_chip_long(&amiga.memory.chip_ram, this_task as usize + 0x0A));
            println!("  ThisTask=\"{}\" (${:08X})", task_name, this_task);

            // Dump last 20 IPCs
            println!("  Last 20 IPCs before STOP:");
            for j in (RING_SIZE - 20)..RING_SIZE {
                let idx = (ring_idx + j) % RING_SIZE;
                let (rpc, rir, rsr) = ipc_ring[idx];
                if rpc != 0 {
                    let mode = if rsr & 0x2000 != 0 { "S" } else { "U" };
                    println!("    ipc=${:08X} IR=${:04X} SR=${:04X} {}", rpc, rir, rsr, mode);
                }
            }

            // Dump task lists
            let task_ready = read_chip_long(&amiga.memory.chip_ram, eb + 0x196);
            let task_wait = read_chip_long(&amiga.memory.chip_ram, eb + 0x1A4);
            let ready_empty = task_ready == (eb as u32 + 0x196 + 4);
            let wait_empty = task_wait == (eb as u32 + 0x1A4 + 4);
            println!("  TaskReady empty={} head=${:08X}", ready_empty, task_ready);
            if !ready_empty {
                walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
            }
            println!("  TaskWait empty={} head=${:08X}", wait_empty, task_wait);
            if !wait_empty {
                walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
            }
            println!("  DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq);
            break;
        }

        prev_ipc = ipc;
    }
}

fn read_rom_string(rom: &[u8], rom_tag_addr: u32, name_offset: usize) -> String {
    if rom_tag_addr < 0xFC0000 { return format!("@${:08X}", rom_tag_addr); }
    let off = (rom_tag_addr - 0xFC0000) as usize;
    if off + name_offset + 3 >= rom.len() { return "??".into(); }
    let name_ptr = (u32::from(rom[off + name_offset]) << 24)
        | (u32::from(rom[off + name_offset + 1]) << 16)
        | (u32::from(rom[off + name_offset + 2]) << 8)
        | u32::from(rom[off + name_offset + 3]);
    if name_ptr < 0xFC0000 { return format!("@${:08X}", name_ptr); }
    let noff = (name_ptr - 0xFC0000) as usize;
    let mut s = String::new();
    for j in 0..40 {
        if noff + j >= rom.len() { break; }
        let ch = rom[noff + j];
        if ch == 0 { break; }
        s.push(ch as char);
    }
    s
}
