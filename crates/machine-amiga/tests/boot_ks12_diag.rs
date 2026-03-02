//! Diagnostic trace for KS 1.2 boot — find why it alerts (yellow screen).

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

    let total_ticks: u64 = 28_375_160 * 3;
    let mut last_color00: u16 = 0xFFFF;
    let mut alert_count: u32 = 0;

    // Track all visits to key addresses in the boot flow
    let mut hit_fc021a = 0u32;  // After RAM test
    let mut hit_fc0238 = 0u32;  // "RAM too small" alert path
    let mut hit_fc0240 = 0u32;  // RAM size OK, continue

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
        let color00 = amiga.denise.palette[0];

        if color00 != last_color00 {
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.3}s] COLOR00 ${:03X} -> ${:03X}  PC=${:08X}",
                elapsed_s, last_color00, color00, pc
            );
            last_color00 = color00;
        }

        // Track first hit of key addresses (only first occurrence)
        if pc == 0xFC021A {
            hit_fc021a += 1;
            if hit_fc021a <= 5 || amiga.cpu.regs.a(3) != 0 {
                println!(
                    "  tick={} @FC021A #{}: A3=${:08X} A0=${:08X}",
                    i, hit_fc021a, amiga.cpu.regs.a(3), amiga.cpu.regs.a(0),
                );
            }
        }

        if pc == 0xFC0238 && hit_fc0238 < 2 {
            hit_fc0238 += 1;
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.3}s] @FC0238 (RAM too small!) #{}: A3=${:08X}",
                elapsed_s, hit_fc0238, amiga.cpu.regs.a(3),
            );
        }

        if pc == 0xFC0240 && hit_fc0240 < 2 {
            hit_fc0240 += 1;
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.3}s] @FC0240 (RAM OK, continue) #{}: A3=${:08X} A4=${:08X}",
                elapsed_s, hit_fc0240, amiga.cpu.regs.a(3), amiga.cpu.regs.a(4),
            );
        }

        // Track the warm restart path at $FC014C
        if pc == 0xFC014C {
            let elapsed_s = i as f64 / 28_375_160.0;
            let exec_base = u32::from(amiga.memory.chip_ram[4]) << 24
                | u32::from(amiga.memory.chip_ram[5]) << 16
                | u32::from(amiga.memory.chip_ram[6]) << 8
                | u32::from(amiga.memory.chip_ram[7]);
            println!(
                "[{:.3}s] @FC014C (warm restart check): ExecBase=${:08X} overlay={}",
                elapsed_s, exec_base, amiga.memory.overlay,
            );
        }

        // Track the cold boot path at $FC01CE
        if pc == 0xFC01CE {
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.3}s] @FC01CE (cold boot): A3=${:08X} A4=${:08X}",
                elapsed_s, amiga.cpu.regs.a(3), amiga.cpu.regs.a(4),
            );
        }

        // Track the RAM test itself
        // Track the memory clear routine entry at $FC0602
        if pc == 0xFC0602 {
            let a0 = amiga.cpu.regs.a(0);
            let d0 = amiga.cpu.regs.d[0];
            println!(
                "  tick={} @FC0602 (clear entry): A0=${:08X} D0=${:08X} (clearing ${:X} bytes)",
                i, a0, d0, d0,
            );
        }

        if pc == 0xFC0592 {
            println!(
                "  tick={} @FC0592 (RAM test): A0=${:08X} A1=${:08X}",
                i, amiga.cpu.regs.a(0), amiga.cpu.regs.a(1),
            );
        }

        // Only trace the beq when A0 is near the 512KB boundary or at first page
        if pc == 0xFC05AA {
            let a0 = amiga.cpu.regs.a(0);
            let a2 = amiga.cpu.regs.a(2);
            let sr = amiga.cpu.regs.sr;
            let z = (sr >> 2) & 1;
            // tst.l (a2) result: bne taken if Z=0
            if z == 0 || a0 >= 0x7E000 {
                let val = u32::from(amiga.memory.chip_ram[(a2 & amiga.memory.chip_ram_mask) as usize]) << 24
                    | u32::from(amiga.memory.chip_ram[((a2 + 1) & amiga.memory.chip_ram_mask) as usize]) << 16
                    | u32::from(amiga.memory.chip_ram[((a2 + 2) & amiga.memory.chip_ram_mask) as usize]) << 8
                    | u32::from(amiga.memory.chip_ram[((a2 + 3) & amiga.memory.chip_ram_mask) as usize]);
                println!(
                    "  tick={} @FC05AA: Z={} A0=${:08X} A2=${:08X} chip[(A2)]=${:08X} SR=${:04X}",
                    i, z, a0, a2, val, sr,
                );
            }
        }

        // $FC059E is the loop target — only reached when beq IS taken
        // (page test passed). First and last few are most interesting.
        if pc == 0xFC059E {
            static LOOP_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = LOOP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 3 || (n >= 126 && n <= 130) {
                println!(
                    "  tick={} @FC059E (loop #{}) A0=${:08X}",
                    i, n + 1, amiga.cpu.regs.a(0),
                );
            }
        }

        // $FC0222 is the success path after RAM test
        if pc == 0xFC0222 {
            println!(
                "  tick={} @FC0222 (RAM OK!): A3=${:08X}",
                i, amiga.cpu.regs.a(3),
            );
        }

        if pc == 0xFC05B4 {
            alert_count += 1;
            let elapsed_s = i as f64 / 28_375_160.0;
            // Read the exception stack frame to see what caused this
            let sp = amiga.cpu.regs.a(7);
            let frame_sr = u16::from(amiga.memory.chip_ram[(sp & amiga.memory.chip_ram_mask) as usize]) << 8
                | u16::from(amiga.memory.chip_ram[((sp + 1) & amiga.memory.chip_ram_mask) as usize]);
            let frame_pc = u32::from(amiga.memory.chip_ram[((sp + 2) & amiga.memory.chip_ram_mask) as usize]) << 24
                | u32::from(amiga.memory.chip_ram[((sp + 3) & amiga.memory.chip_ram_mask) as usize]) << 16
                | u32::from(amiga.memory.chip_ram[((sp + 4) & amiga.memory.chip_ram_mask) as usize]) << 8
                | u32::from(amiga.memory.chip_ram[((sp + 5) & amiga.memory.chip_ram_mask) as usize]);
            println!(
                "[{:.3}s] *** ALERT #{} *** A3=${:08X} A5=${:08X} SP=${:08X} frame_SR=${:04X} frame_PC=${:08X}",
                elapsed_s, alert_count, amiga.cpu.regs.a(3), amiga.cpu.regs.a(5),
                sp, frame_sr, frame_pc,
            );
            if alert_count >= 2 {
                break;
            }
        }
    }
}
