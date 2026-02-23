// Trace all CALL/RST/RET instructions during boot to find the corruption
use emu_core::Tickable;
use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumModel};

const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

fn main() {
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: ROM_48K.to_vec(),
    };
    let mut spectrum = Spectrum::new(&config);

    // Run to frame 81
    for _ in 0..81 {
        spectrum.run_frame();
    }

    println!(
        "After 81 frames: PC={:04X} SP={:04X}",
        spectrum.cpu().regs.pc,
        spectrum.cpu().regs.sp
    );

    // Trace execution with a rolling buffer of the last 500 instruction records
    let mut history: Vec<String> = Vec::new();
    let mut prev_pc = spectrum.cpu().regs.pc;
    let mut tick_count = 0u64;
    let max_ticks = 20_000_000u64;

    loop {
        spectrum.tick();
        tick_count += 1;

        let pc = spectrum.cpu().regs.pc;
        let sp = spectrum.cpu().regs.sp;

        if pc != prev_pc {
            // Read the opcode at prev_pc to identify the instruction type
            let opcode = spectrum.bus().memory.peek(prev_pc);

            // Detect calls, RSTs, RETs, and interrupts by looking at opcode
            let is_call = opcode == 0xCD  // CALL nn
                || (opcode & 0xC7) == 0xC4; // CALL cc,nn
            let is_rst = (opcode & 0xC7) == 0xC7; // RST n
            let is_ret = opcode == 0xC9  // RET
                || (opcode & 0xC7) == 0xC0; // RET cc
            let is_push = (opcode & 0xCF) == 0xC5; // PUSH rr
            let is_pop = (opcode & 0xCF) == 0xC1; // POP rr

            // Detect interrupt: PC jumped to $0038 without a call/rst instruction
            let is_interrupt = pc == 0x0038 && !is_call && !is_rst;

            // Log interesting instructions
            if is_call || is_rst || is_ret || is_interrupt || pc == 0x0053 || pc == 0x0008 {
                let desc = if is_interrupt {
                    format!("INT")
                } else if is_call {
                    let lo = spectrum.bus().memory.peek(prev_pc + 1);
                    let hi = spectrum.bus().memory.peek(prev_pc + 2);
                    let target = u16::from(lo) | (u16::from(hi) << 8);
                    format!("CALL ${:04X}", target)
                } else if is_rst {
                    let target = opcode & 0x38;
                    format!("RST ${:02X}", target)
                } else if is_ret {
                    format!("RET → ${:04X}", pc)
                } else if pc == 0x0053 {
                    format!("ERROR HANDLER ENTRY")
                } else {
                    format!("→ ${:04X}", pc)
                };

                let entry = format!(
                    "t={:8} PC={:04X} {} SP={:04X} A={:02X} HL={:02X}{:02X}",
                    tick_count,
                    prev_pc,
                    desc,
                    sp,
                    spectrum.cpu().regs.a,
                    spectrum.cpu().regs.h,
                    spectrum.cpu().regs.l
                );
                history.push(entry);
                if history.len() > 500 {
                    history.remove(0);
                }
            }

            // Detect reaching error handler
            if pc == 0x0053 {
                println!("\n*** ERROR HANDLER REACHED ***");
                println!("Last {} calls/returns:", history.len());
                for entry in &history {
                    println!("  {}", entry);
                }

                // Dump stack
                println!("\nStack at SP={:04X}:", sp);
                for i in (0..20u16).step_by(2) {
                    let lo = spectrum.bus().memory.peek(sp.wrapping_add(i));
                    let hi = spectrum.bus().memory.peek(sp.wrapping_add(i + 1));
                    let addr = u16::from(lo) | (u16::from(hi) << 8);
                    println!("  SP+{:2}: ${:04X}", i, addr);
                }

                // Also dump the byte at $4000 (which will be the "error code")
                println!(
                    "\nByte at $4000 (supposed error code): 0x{:02X}",
                    spectrum.bus().memory.peek(0x4000)
                );

                break;
            }

            prev_pc = pc;
        }

        if tick_count >= max_ticks {
            println!("Timeout");
            break;
        }
    }
}
