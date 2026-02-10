//! Temporary trace test for debugging Kickstart 1.3 boot.

use std::collections::VecDeque;
use std::fs;

use emu_amiga::{Amiga, AmigaConfig};
use emu_core::{Bus, Tickable};

#[test]
#[ignore]
fn trace_early_boot() {
    let rom_path =
        "/tmp/Kickstart v1.3 r34.005 (1987-12)(Commodore)(A500-A1000-A2000-CDTV)[!].rom";
    let rom = fs::read(rom_path).expect("need Kickstart 1.3 ROM at /tmp/");
    let config = AmigaConfig { kickstart: rom };
    let mut amiga = Amiga::new(&config).expect("valid config");

    println!("Initial state:");
    println!(
        "  PC  = ${:08X}  SSP = ${:08X}  SR = ${:04X}",
        amiga.cpu().regs.pc,
        amiga.cpu().regs.ssp,
        amiga.cpu().regs.sr
    );

    let mut last_pc = amiga.cpu().regs.pc;
    let mut instr_count = 0u32;

    for tick_num in 0..50000 {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc != last_pc {
            instr_count += 1;
            let r = &amiga.cpu().regs;
            let sr = r.sr;
            let d0 = r.d[0];
            let d1 = r.d[1];
            let a0 = r.a[0];
            let a1 = r.a[1];
            let a7 = r.a(7);
            println!(
                "[t{:5}] #{:3}: PC=${:08X} SR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A1=${:08X} A7=${:08X}",
                tick_num, instr_count, pc, sr, d0, d1, a0, a1, a7
            );
            last_pc = pc;

            if instr_count >= 50 || pc == 0x0022_0005 {
                break;
            }
        }
    }
}

/// Verify bus reads at Kickstart ROM addresses match the ROM file contents.
#[test]
#[ignore]
fn verify_bus_rom_reads() {
    let rom_path =
        "/tmp/Kickstart v1.3 r34.005 (1987-12)(Commodore)(A500-A1000-A2000-CDTV)[!].rom";
    let rom = fs::read(rom_path).expect("need Kickstart 1.3 ROM at /tmp/");
    let config = AmigaConfig {
        kickstart: rom.clone(),
    };
    let mut amiga = Amiga::new(&config).expect("valid config");

    // Check reads at $FC00D2 through $FC00E6 match ROM bytes
    println!("Verifying ROM reads via bus:");
    for addr in (0x00FC_00D2u32..0x00FC_00E8).step_by(2) {
        let hi = amiga.bus_mut().read(addr).data;
        let lo = amiga.bus_mut().read(addr + 1).data;
        let word = u16::from(hi) << 8 | u16::from(lo);

        let rom_offset = ((addr - 0xF8_0000) as usize) % rom.len();
        let rom_hi = rom[rom_offset];
        let rom_lo = rom[rom_offset + 1];
        let rom_word = u16::from(rom_hi) << 8 | u16::from(rom_lo);

        let ok = if word == rom_word { "OK" } else { "MISMATCH!" };
        println!(
            "  ${addr:08X}: bus=${word:04X} rom=${rom_word:04X} {ok}"
        );
        assert_eq!(word, rom_word, "Bus read mismatch at ${addr:08X}");
    }
    println!("All bus reads match ROM.");
}

/// Test CPU directly with ROM bus (no Amiga tick loop) to isolate CPU vs tick loop bugs
#[test]
#[ignore]
fn trace_cpu_direct() {
    let rom_path =
        "/tmp/Kickstart v1.3 r34.005 (1987-12)(Commodore)(A500-A1000-A2000-CDTV)[!].rom";
    let rom = fs::read(rom_path).expect("need Kickstart 1.3 ROM at /tmp/");
    let config = AmigaConfig { kickstart: rom };
    let mut amiga = Amiga::new(&config).expect("valid config");

    println!(
        "Initial: PC=${:08X} SSP=${:08X}",
        amiga.cpu().regs.pc,
        amiga.cpu().regs.ssp
    );

    let mut last_pc = amiga.cpu().regs.pc;
    let mut instr_count = 0u32;

    // Tick CPU directly, bypassing Amiga tick loop
    for tick in 0..200 {
        amiga.tick_cpu_only();
        let new_pc = amiga.cpu().regs.pc;
        if new_pc != last_pc {
            instr_count += 1;
            let d0 = amiga.cpu().regs.d[0];
            let a7 = amiga.cpu().regs.a(7);
            println!(
                "[tick {:3}] #{:2}: PC=${:08X} D0=${:08X} A7=${:08X}",
                tick, instr_count, new_pc, d0, a7
            );
            last_pc = new_pc;
            if instr_count >= 20 {
                break;
            }
        }
    }
}

/// Test MOVE.L #imm, D0 + SUBQ.L + BGT.S loop with the actual ROM
#[test]
#[ignore]
fn test_kickstart_loop() {
    let rom_path =
        "/tmp/Kickstart v1.3 r34.005 (1987-12)(Commodore)(A500-A1000-A2000-CDTV)[!].rom";
    let rom = fs::read(rom_path).expect("need Kickstart 1.3 ROM at /tmp/");
    let config = AmigaConfig { kickstart: rom };
    let mut amiga = Amiga::new(&config).expect("valid config");

    // Run until we're past the BGT loop (PC > $FC00E2) or timeout
    // The loop is: SUBQ.L #1,D0 at $FC00DE / BGT.S at $FC00E0
    // After the loop, PC should be at $FC00E2 (LEA instruction)
    let mut tick_count = 0u64;
    loop {
        amiga.tick();
        tick_count += 1;
        let pc = amiga.cpu().regs.pc;

        if tick_count % 1_000_000 == 0 {
            let d0 = amiga.cpu().regs.d[0];
            println!("[{tick_count:8}] PC=${pc:08X} D0=${d0:08X}");
        }

        // After the loop, PC goes past $FC00E0
        if pc == 0x00FC_00E2 {
            println!("Loop exited at tick {tick_count}");
            let d0 = amiga.cpu().regs.d[0];
            println!("  D0 = ${d0:08X} (expect $00000000)");
            assert_eq!(d0, 0, "D0 should be 0 after the loop");
            break;
        }

        if tick_count > 500_000_000 {
            let d0 = amiga.cpu().regs.d[0];
            println!("Timeout! PC=${pc:08X} D0=${d0:08X}");
            panic!("Loop didn't exit in time");
        }
    }
}

/// Trace until the first Line F opcode ($FFFC) appears, then dump recent history.
#[test]
#[ignore]
fn trace_until_line_f() {
    let rom_path =
        "/tmp/Kickstart v1.3 r34.005 (1987-12)(Commodore)(A500-A1000-A2000-CDTV)[!].rom";
    let rom = fs::read(rom_path).expect("need Kickstart 1.3 ROM at /tmp/");
    let config = AmigaConfig { kickstart: rom };
    let mut amiga = Amiga::new(&config).expect("valid config");

    let mut last_pc = amiga.cpu().regs.pc;
    let mut instr_count = 0u32;
    let mut history: VecDeque<String> = VecDeque::with_capacity(40);

    for tick_num in 0..50_000_000u64 {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc != last_pc {
            instr_count += 1;
            let op_addr = pc.wrapping_sub(2);
            let hi = amiga.bus().memory.read(op_addr);
            let lo = amiga.bus().memory.read(op_addr.wrapping_add(1));
            let op = u16::from(hi) << 8 | u16::from(lo);

            let r = &amiga.cpu().regs;
            let line = format!(
                "[t{tick_num:7}] #{instr_count:6} PC=${pc:08X} OP@${op_addr:08X}=${op:04X} SR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A1=${:08X} A7=${:08X}",
                r.sr, r.d[0], r.d[1], r.a[0], r.a[1], r.a(7)
            );
            history.push_back(line);
            if history.len() > 40 {
                history.pop_front();
            }

            if op == 0xFFFC {
                println!(
                    "Hit Line F opcode at PC=${pc:08X} (opcode addr ${op_addr:08X}) after {instr_count} instructions:"
                );
                for entry in history {
                    println!("{entry}");
                }
                break;
            }

            last_pc = pc;
        }
    }
}

/// Trace instructions around the JSR (A0) site near $FEE250 to inspect A0 setup.
#[test]
#[ignore]
fn trace_jsr_a0_setup() {
    let rom_path =
        "/tmp/Kickstart v1.3 r34.005 (1987-12)(Commodore)(A500-A1000-A2000-CDTV)[!].rom";
    let rom = fs::read(rom_path).expect("need Kickstart 1.3 ROM at /tmp/");
    let config = AmigaConfig { kickstart: rom };
    let mut amiga = Amiga::new(&config).expect("valid config");

    let mut last_pc = amiga.cpu().regs.pc;
    let mut instr_count = 0u32;
    let mut last_a0 = amiga.cpu().regs.a[0];
    let mut history: VecDeque<String> = VecDeque::with_capacity(50);

    for tick_num in 0..100_000_000u64 {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;

        if pc != last_pc {
            instr_count += 1;
            let op_addr = pc.wrapping_sub(2);
            let hi = amiga.bus().memory.read(op_addr);
            let lo = amiga.bus().memory.read(op_addr.wrapping_add(1));
            let op = u16::from(hi) << 8 | u16::from(lo);

            let r = &amiga.cpu().regs;
            let line = format!(
                "[t{tick_num:8}] #{instr_count:6} PC=${pc:08X} OP@${op_addr:08X}=${op:04X} SR=${:04X} D0=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A7=${:08X}",
                r.sr, r.d[0], r.a[0], r.a[1], r.a[2], r.a(7)
            );
            history.push_back(line);
            if history.len() > 50 {
                history.pop_front();
            }

            if r.a[0] != last_a0 {
                if r.a[0] == 0x4ED5_303C || r.a[0] == 0xFFFF_FFFF {
                    println!("A0 changed to suspicious value ${:08X} at PC=${pc:08X}", r.a[0]);
                    for entry in history {
                        println!("{entry}");
                    }
                    break;
                }
                last_a0 = r.a[0];
            }

            last_pc = pc;
        }
    }
}
