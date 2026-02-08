// Diagnostic: tick-level trace to find exact point of ERR_NR write
use emu_core::Tickable;
use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumModel};

const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

fn main() {
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: ROM_48K.to_vec(),
    };
    let mut spectrum = Spectrum::new(&config);

    // Run to frame 82 (just before the error frame)
    for _ in 0..83 {
        spectrum.run_frame();
    }

    println!("After 83 frames: PC={:04X} SP={:04X} ERR_NR={:02X}",
        spectrum.cpu().regs.pc, spectrum.cpu().regs.sp,
        spectrum.bus().memory.peek(0x5C3A));

    // Now tick through frame 83 checking ERR_NR every 100 ticks
    let mut prev_err = spectrum.bus().memory.peek(0x5C3A);
    let mut ticks = 0u64;
    let mut found = false;

    // First pass: coarse (every 100 crystal ticks)
    let mut last_good_tick = 0u64;
    loop {
        for _ in 0..100 {
            spectrum.tick();
            ticks += 1;
        }

        let err = spectrum.bus().memory.peek(0x5C3A);
        if err != prev_err {
            println!("ERR_NR changed from {:02X} to {:02X} between tick {} and {}",
                prev_err, err, last_good_tick, ticks);
            println!("PC={:04X} SP={:04X}", spectrum.cpu().regs.pc, spectrum.cpu().regs.sp);
            found = true;
            break;
        }
        last_good_tick = ticks;

        if spectrum.bus().video.take_frame_complete() {
            println!("Frame complete at tick {} without ERR_NR change", ticks);
            break;
        }

        if ticks > 300_000 {
            println!("Timeout at tick {}", ticks);
            break;
        }
    }

    if !found {
        println!("ERR_NR didn't change. Current: {:02X}", spectrum.bus().memory.peek(0x5C3A));
        return;
    }

    // Re-run from frame 82 with fine granularity
    println!("\n=== Fine-grained trace ===");
    let mut spectrum2 = Spectrum::new(&config);
    for _ in 0..83 {
        spectrum2.run_frame();
    }

    let target_tick = last_good_tick;
    // Advance to last_good_tick
    for _ in 0..target_tick {
        spectrum2.tick();
    }

    println!("At tick {}: PC={:04X} SP={:04X} ERR_NR={:02X}",
        target_tick, spectrum2.cpu().regs.pc, spectrum2.cpu().regs.sp,
        spectrum2.bus().memory.peek(0x5C3A));

    // Now tick one at a time
    for i in 0..200 {
        let pc_before = spectrum2.cpu().regs.pc;
        let sp_before = spectrum2.cpu().regs.sp;
        spectrum2.tick();
        let err = spectrum2.bus().memory.peek(0x5C3A);
        let pc_after = spectrum2.cpu().regs.pc;
        let sp_after = spectrum2.cpu().regs.sp;

        if i < 20 || err != prev_err {
            println!("  tick +{}: PC {:04X}->{:04X} SP {:04X}->{:04X} ERR={:02X}",
                target_tick + i + 1, pc_before, pc_after, sp_before, sp_after, err);
        }

        if err != prev_err {
            println!("\n*** ERR_NR changed at tick {} ***", target_tick + i + 1);
            println!("PC={:04X} SP={:04X}", pc_after, sp_after);
            println!("A={:02X} F={:02X} B={:02X} C={:02X} D={:02X} E={:02X} H={:02X} L={:02X}",
                spectrum2.cpu().regs.a, spectrum2.cpu().regs.f,
                spectrum2.cpu().regs.b, spectrum2.cpu().regs.c,
                spectrum2.cpu().regs.d, spectrum2.cpu().regs.e,
                spectrum2.cpu().regs.h, spectrum2.cpu().regs.l);
            println!("IX={:04X} IY={:04X}", spectrum2.cpu().regs.ix, spectrum2.cpu().regs.iy);

            // Dump stack
            let sp = sp_after;
            print!("Stack: ");
            for j in 0..10u16 {
                print!("{:02X} ", spectrum2.bus().memory.peek(sp.wrapping_add(j)));
            }
            println!();

            // What's at the return addresses on the stack?
            for j in (0..10).step_by(2) {
                let lo = spectrum2.bus().memory.peek(sp.wrapping_add(j));
                let hi = spectrum2.bus().memory.peek(sp.wrapping_add(j + 1));
                let addr = u16::from(lo) | (u16::from(hi) << 8);
                println!("  Stack[{}] = ${:04X}", j, addr);
            }
            break;
        }
        prev_err = err;
    }
}
