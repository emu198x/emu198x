//! KS 1.2 diagnostic: trace exec init after RAM test.

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use std::collections::VecDeque;
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

    let total_ticks: u64 = 28_375_160 * 10;
    let trace_start: u64 = (28_375_160.0 * 1.77) as u64;
    let mut tracing = false;
    let mut pc_ring: VecDeque<(u64, u32)> = VecDeque::with_capacity(300);
    let mut last_ring_pc: u32 = 0;
    let mut alert_count: u32 = 0;

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

        if !tracing && i >= trace_start {
            tracing = true;
        }
        if !tracing {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        if pc != last_ring_pc {
            last_ring_pc = pc;
            if pc_ring.len() >= 250 {
                pc_ring.pop_front();
            }
            pc_ring.push_back((i, pc));
        }

        // Trigger on COLOR00 changing to yellow ($CC0) — definitive
        // alert indicator, happens AFTER the alert handler writes it.
        if amiga.denise.palette[0] == 0x0CC0 {
            alert_count += 1;
            if alert_count <= 2 {
                let elapsed_s = i as f64 / 28_375_160.0;
                println!(
                    "\n=== Alert #{} at {:.3}s (COLOR00=$CC0) ===",
                    alert_count, elapsed_s,
                );
                println!(
                    "A3=${:08X} D0=${:08X} SP=${:08X} A5=${:08X}",
                    amiga.cpu.regs.a(3),
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(7),
                    amiga.cpu.regs.a(5),
                );
                // Print filtered ring buffer (exclude RAM test loop $FC059E-$FC05AE
                // AND alert blink loop $FC05CE-$FC05FA)
                let filtered: Vec<_> = pc_ring
                    .iter()
                    .filter(|&&(_, p)| {
                        !((p >= 0xFC059E && p <= 0xFC05AE)
                            || (p >= 0xFC05CE && p <= 0xFC05FA))
                    })
                    .copied()
                    .collect();
                let start = filtered.len().saturating_sub(50);
                println!("  Last {} non-loop PCs:", filtered.len() - start);
                for &(tick, addr) in &filtered[start..] {
                    let label = match addr {
                        0xFC0222 => " ← EXEC INIT",
                        0xFC021A => " ← post-RAM cmpa",
                        0xFC0220 => " ← bcs (RAM too small?)",
                        0xFC0238 => " ← RAM too small!",
                        0xFC05B0 => " ← movea a0,a3",
                        0xFC05B2 => " ← jmp (a5)",
                        0xFC05B4 => " ← alert move.w",
                        0xFC05B8 => " ← alert lea DFF000",
                        0xFC05BC => " ← alert cont",
                        _ => "",
                    };
                    println!("    tick={:>10} PC=${:08X}{}", tick, addr, label);
                }
                pc_ring.clear();
                last_ring_pc = 0;
            }
            if alert_count >= 2 {
                break;
            }
        }
    }
}
