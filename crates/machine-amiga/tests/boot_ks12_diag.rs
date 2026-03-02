//! Diagnostic: trace exec init between RAM test exit and alert.

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use std::fs;

#[test]
#[ignore]
fn test_kick12_boot_trace() {
    let rom = match fs::read("../../roms/kick12_33_180_a500_a1000_a2000.rom") {
        Ok(r) => r,
        Err(_) => {
            eprintln!("KS 1.2 ROM not found, skipping");
            return;
        }
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 0,
    });

    let total_ticks: u64 = 28_375_160 * 5;
    let mut alert_count: u32 = 0;
    let mut saw_fc0222 = false;
    let mut exec_init_pcs: Vec<(u64, u32)> = Vec::new();
    let mut last_exec_pc: u32 = 0;

    for i in 0..total_ticks {
        amiga.tick();

        if i >= 2 * 28_375_160 {
            let tod = amiga.cia_a.tod_counter();
            if tod < 0x010000 {
                amiga.cia_a.set_tod_counter(0x010000 | tod);
            }
        }

        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        // Trace A3 at $FC021A — ALL hits, with IR for context
        if pc == 0xFC021A && alert_count < 2 {
            static HIT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = HIT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 5 || (n > 0 && amiga.cpu.regs.a(3) != 0) {
                println!(
                    "  tick={} @FC021A #{}: A3=${:08X} A0=${:08X} IR=${:04X}",
                    i, n + 1, amiga.cpu.regs.a(3), amiga.cpu.regs.a(0), amiga.cpu.ir,
                );
            }
        }

        // Detect exec init success path
        if pc == 0xFC0222 && !saw_fc0222 {
            saw_fc0222 = true;
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.3}s] @FC0222 EXEC INIT START: A3=${:08X} D0=${:08X}",
                elapsed_s,
                amiga.cpu.regs.a(3),
                amiga.cpu.regs.d[0],
            );
        }

        // Log exec init PCs (outside RAM test range)
        if saw_fc0222 && pc != last_exec_pc {
            last_exec_pc = pc;
            exec_init_pcs.push((i, pc));
        }

        if pc == 0xFC05B4 {
            alert_count += 1;
            if alert_count <= 2 {
                let elapsed_s = i as f64 / 28_375_160.0;
                println!(
                    "[{:.3}s] *** ALERT #{} *** A3=${:08X} D0=${:08X} SP=${:08X}",
                    elapsed_s,
                    alert_count,
                    amiga.cpu.regs.a(3),
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(7),
                );
                if saw_fc0222 {
                    // Print the exec init PCs that led here
                    let n = exec_init_pcs.len();
                    let start = n.saturating_sub(30);
                    println!("  {} exec init PCs recorded, last {}:", n, n - start);
                    for &(tick, addr) in &exec_init_pcs[start..] {
                        println!("    tick={} PC=${:08X}", tick, addr);
                    }
                }
                // Reset for next cycle
                saw_fc0222 = false;
                exec_init_pcs.clear();
                last_exec_pc = 0;
            }
            if alert_count >= 3 {
                break;
            }
        }
    }
}
