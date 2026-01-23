//! CP/M harness for running ZEXDOC/ZEXALL Z80 instruction exercisers.
//!
//! Usage:
//!   cargo run -p cpu-z80 --bin zextest --release -- [zexdoc|zexall]
//!
//! The test output is printed in real-time so you can see progress.

use std::env;
use std::io::{self, Write};
use std::time::Instant;

use cpu_z80::Z80;
use emu_core::{Bus, Cpu, IoBus};

/// A minimal CP/M memory/IO implementation for running ZEX tests.
struct CpmBus {
    memory: Box<[u8; 65536]>,
}

impl CpmBus {
    fn new() -> Self {
        let mut memory = Box::new([0u8; 65536]);

        // At 0x0000: JP 0x0000 (warm boot trap - infinite loop we detect)
        memory[0x0000] = 0xC3; // JP
        memory[0x0001] = 0x00;
        memory[0x0002] = 0x00;

        // At 0x0005: JP 0x0005 (BDOS entry - we intercept before this executes)
        memory[0x0005] = 0xC3; // JP
        memory[0x0006] = 0x05;
        memory[0x0007] = 0x00;

        Self { memory }
    }

    fn load_com(&mut self, data: &[u8]) {
        // CP/M COM files load at 0x0100
        let start = 0x0100;
        for (i, &byte) in data.iter().enumerate() {
            if start + i < 65536 {
                self.memory[start + i] = byte;
            }
        }
    }
}

impl Bus for CpmBus {
    fn read(&self, addr: u32) -> u8 {
        self.memory[(addr & 0xFFFF) as usize]
    }

    fn write(&mut self, addr: u32, val: u8) {
        self.memory[(addr & 0xFFFF) as usize] = val;
    }
}

impl IoBus for CpmBus {
    fn read_io(&self, _port: u16) -> u8 {
        0xFF
    }

    fn write_io(&mut self, _port: u16, _val: u8) {
        // Ignore I/O
    }
}

/// Handle CP/M BDOS call. Returns true if program should exit.
fn handle_bdos(cpu: &mut Z80, bus: &CpmBus) -> bool {
    let function = cpu.c();

    match function {
        0 => {
            // System reset - program is done
            return true;
        }
        2 => {
            // Console output - character in E
            let ch = cpu.e();
            print!("{}", ch as char);
            io::stdout().flush().unwrap();
        }
        9 => {
            // Print string - DE points to $-terminated string
            let mut addr = cpu.de();
            loop {
                let ch = bus.read(addr as u32);
                if ch == b'$' {
                    break;
                }
                print!("{}", ch as char);
                addr = addr.wrapping_add(1);
            }
            io::stdout().flush().unwrap();
        }
        _ => {
            // Ignore other BDOS calls
        }
    }

    false
}

fn run_test(test_name: &str, test_bin: &[u8]) {
    let mut cpu = Z80::new();
    let mut bus = CpmBus::new();

    bus.load_com(test_bin);

    // Set initial CPU state for CP/M
    cpu.set_pc(0x0100); // COM files start at 0x0100
    cpu.set_sp(0xFFFE); // Stack near top of memory

    // Push return address 0x0000 so RET will jump to warm boot
    let sp = cpu.sp().wrapping_sub(2);
    cpu.set_sp(sp);
    bus.write(sp as u32, 0x00);
    bus.write(sp.wrapping_add(1) as u32, 0x00);

    let start_time = Instant::now();
    let mut instructions: u64 = 0;
    let mut cycles: u64 = 0;

    let report_interval = 100_000_000u64; // Report every 100M instructions
    let mut next_report = report_interval;

    eprintln!("Running {}...\n", test_name);

    loop {
        // Check if we're at the BDOS entry point (0x0005)
        if cpu.pc() == 0x0005 {
            if handle_bdos(&mut cpu, &bus) {
                break;
            }
            // Return from BDOS call
            cpu.force_ret(&mut bus);
            continue;
        }

        // Check if we've reached warm boot (0x0000) or halted
        if cpu.pc() == 0x0000 || cpu.halted() {
            break;
        }

        // Execute one instruction
        cycles += cpu.step(&mut bus) as u64;
        instructions += 1;

        // Periodic progress report
        if instructions >= next_report {
            let elapsed = start_time.elapsed().as_secs_f64();
            let mips = instructions as f64 / elapsed / 1_000_000.0;
            eprint!(
                "\r[{:.1}s] {:>6.2}B instructions, {:.1} MIPS",
                elapsed,
                instructions as f64 / 1_000_000_000.0,
                mips
            );
            io::stderr().flush().unwrap();
            next_report += report_interval;
        }
    }

    let elapsed = start_time.elapsed();
    eprintln!("\n\nCompleted in {:.2}s", elapsed.as_secs_f64());
    eprintln!(
        "Instructions: {} ({:.2}B)",
        instructions,
        instructions as f64 / 1_000_000_000.0
    );
    eprintln!(
        "Cycles: {} ({:.2}B)",
        cycles,
        cycles as f64 / 1_000_000_000.0
    );
    eprintln!(
        "Speed: {:.1} MIPS",
        instructions as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let test_name = if args.len() > 1 {
        args[1].to_lowercase()
    } else {
        "zexdoc".to_string()
    };

    match test_name.as_str() {
        "zexdoc" => {
            let test_bin = include_bytes!("../../tests/fixtures/zexdoc.com");
            run_test("ZEXDOC", test_bin);
        }
        "zexall" => {
            let test_bin = include_bytes!("../../tests/fixtures/zexall.com");
            run_test("ZEXALL", test_bin);
        }
        _ => {
            eprintln!("Usage: zextest [zexdoc|zexall]");
            eprintln!("  zexdoc - Test documented Z80 behavior (faster)");
            eprintln!("  zexall - Test all behavior including undocumented flags");
            std::process::exit(1);
        }
    }
}
