// Trace Z80 execution to find exactly what triggers the error during boot
use emu_core::Tickable;
use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumModel};

const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

fn main() {
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: ROM_48K.to_vec(),
    };
    let mut spectrum = Spectrum::new(&config);

    // Run to frame 81 (well before error which happens around frame 82-83)
    for _ in 0..81 {
        spectrum.run_frame();
    }

    println!("After 81 frames: PC={:04X} SP={:04X} IM={} IFF1={}",
        spectrum.cpu().regs.pc, spectrum.cpu().regs.sp,
        spectrum.cpu().regs.im, spectrum.cpu().regs.iff1);
    println!("ERR_NR=0x{:02X}", spectrum.bus().memory.peek(0x5C3A));

    // Now tick the master clock one tick at a time, watching for:
    // 1. PC reaching $0053 (error handler entry)
    // 2. PC reaching $1C8A or $21CE (RST 8 with error code $0B)
    // 3. ERR_NR changing
    // Also log PC when it enters interesting ranges
    let mut prev_err = spectrum.bus().memory.peek(0x5C3A);
    let mut prev_pc = spectrum.cpu().regs.pc;
    let mut tick_count = 0u64;
    let mut last_pcs: Vec<u16> = Vec::new();
    let mut found = false;

    // Log first few PCs to see where we are
    let mut log_count = 0;
    let max_ticks = 20_000_000u64; // Safety limit

    loop {
        spectrum.tick();
        tick_count += 1;

        let pc = spectrum.cpu().regs.pc;
        let err = spectrum.bus().memory.peek(0x5C3A);

        // Track PC changes (instruction boundaries)
        if pc != prev_pc {
            // Keep a rolling buffer of last 50 PCs
            last_pcs.push(prev_pc);
            if last_pcs.len() > 50 {
                last_pcs.remove(0);
            }

            // Log if PC enters the error handler
            if pc == 0x0053 {
                println!("\n*** PC reached $0053 (error handler) at tick {} ***", tick_count);
                println!("Registers: A={:02X} F={:02X} B={:02X} C={:02X} D={:02X} E={:02X} H={:02X} L={:02X}",
                    spectrum.cpu().regs.a, spectrum.cpu().regs.f,
                    spectrum.cpu().regs.b, spectrum.cpu().regs.c,
                    spectrum.cpu().regs.d, spectrum.cpu().regs.e,
                    spectrum.cpu().regs.h, spectrum.cpu().regs.l);
                println!("SP={:04X} PC={:04X}", spectrum.cpu().regs.sp, spectrum.cpu().regs.pc);

                // Dump stack
                let sp = spectrum.cpu().regs.sp;
                println!("Stack:");
                for i in (0..16u16).step_by(2) {
                    let lo = spectrum.bus().memory.peek(sp.wrapping_add(i));
                    let hi = spectrum.bus().memory.peek(sp.wrapping_add(i + 1));
                    let addr = u16::from(lo) | (u16::from(hi) << 8);
                    println!("  SP+{}: ${:04X} ({:02X} {:02X})", i, addr, lo, hi);
                }

                // Show last 50 PCs
                println!("\nLast 50 instruction PCs:");
                for (i, &p) in last_pcs.iter().enumerate() {
                    print!("{:04X} ", p);
                    if (i + 1) % 10 == 0 { println!(); }
                }
                println!();
                found = true;
                break;
            }

            // Log if PC reaches RST 8 locations with error $0B
            if pc == 0x1C8A || pc == 0x21CE {
                println!("*** PC at ${:04X} (RST 8 + $0B site) at tick {} ***", pc, tick_count);
            }

            // Log if PC reaches RST 8 handler
            if pc == 0x0008 {
                println!("*** RST 8 called at tick {} (from PC={:04X}) ***", tick_count, prev_pc);
                let sp = spectrum.cpu().regs.sp;
                let lo = spectrum.bus().memory.peek(sp);
                let hi = spectrum.bus().memory.peek(sp.wrapping_add(1));
                let ret = u16::from(lo) | (u16::from(hi) << 8);
                println!("    Return address: ${:04X}", ret);
            }

            // Log first 200 instruction boundaries after IM is set
            if spectrum.cpu().regs.im == 1 && log_count < 200 {
                if log_count == 0 {
                    println!("\n--- IM=1 detected, logging instructions ---");
                }
                println!("[{:4}] PC={:04X} A={:02X} F={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:04X}",
                    log_count, pc,
                    spectrum.cpu().regs.a, spectrum.cpu().regs.f,
                    spectrum.cpu().regs.b, spectrum.cpu().regs.c,
                    spectrum.cpu().regs.d, spectrum.cpu().regs.e,
                    spectrum.cpu().regs.h, spectrum.cpu().regs.l,
                    spectrum.cpu().regs.sp);
                log_count += 1;
            }

            prev_pc = pc;
        }

        // Check ERR_NR change
        if err != prev_err {
            println!("\n*** ERR_NR changed from ${:02X} to ${:02X} at tick {} ***",
                prev_err, err, tick_count);
            println!("PC={:04X}", pc);
            if !found {
                // Show last 50 PCs
                println!("Last 50 instruction PCs:");
                for (i, &p) in last_pcs.iter().enumerate() {
                    print!("{:04X} ", p);
                    if (i + 1) % 10 == 0 { println!(); }
                }
                println!();
            }
            found = true;
            break;
        }
        prev_err = err;

        if tick_count >= max_ticks {
            println!("Timeout after {} ticks", tick_count);
            break;
        }
    }

    if !found {
        println!("No error detected within {} ticks", tick_count);
    }
}
