//! Klaus Dormann's 6502 functional test suite runner.
//!
//! This runs the comprehensive 6502 test suite to verify CPU correctness.
//! The test binary should be placed at `test-roms/6502_functional_test.bin`.
//!
//! The test is considered passing when PC reaches $3469.
//! A trap (PC stuck in a loop) indicates a test failure.
//!
//! Download the test from: https://github.com/Klaus2m5/6502_65C02_functional_tests

use cpu_6502::Mos6502;
use emu_core::{Bus, Cpu};
use std::fs;

struct TestBus {
    memory: [u8; 65536],
}

impl TestBus {
    fn new() -> Self {
        Self { memory: [0; 65536] }
    }

    fn load(&mut self, addr: usize, data: &[u8]) {
        self.memory[addr..addr + data.len()].copy_from_slice(data);
    }
}

impl Bus for TestBus {
    fn read(&mut self, address: u32) -> u8 {
        self.memory[(address & 0xFFFF) as usize]
    }

    fn write(&mut self, address: u32, value: u8) {
        self.memory[(address & 0xFFFF) as usize] = value;
    }

    fn tick(&mut self, _cycles: u32) {}
}

fn main() {
    let test_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "test-roms/6502_functional_test.bin".to_string());

    let test_data = match fs::read(&test_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to load test ROM: {}", e);
            eprintln!();
            eprintln!("To run the 6502 functional test:");
            eprintln!("1. Download from: https://github.com/Klaus2m5/6502_65C02_functional_tests");
            eprintln!("2. Assemble 6502_functional_test.a65 with origin at $0000");
            eprintln!("3. Place the binary at test-roms/6502_functional_test.bin");
            eprintln!();
            eprintln!("Or specify the path: cargo run -p cpu-6502 --bin 6502test -- /path/to/test.bin");
            std::process::exit(1);
        }
    };

    println!("Running 6502 functional test suite...");
    println!("Test binary: {} ({} bytes)", test_path, test_data.len());
    println!();

    let mut cpu = Mos6502::new();
    let mut bus = TestBus::new();

    // Load test at $0000 (the test sets up its own vectors)
    bus.load(0x0000, &test_data);

    // Start execution at $0400 (standard start address for the test)
    cpu.set_pc(0x0400);

    let mut last_pc = cpu.pc();
    let mut stuck_count = 0;
    let mut total_cycles: u64 = 0;
    let mut instruction_count: u64 = 0;

    let start_time = std::time::Instant::now();

    loop {
        let pc_before = cpu.pc();
        let cycles = cpu.step(&mut bus);
        total_cycles += cycles as u64;
        instruction_count += 1;

        // Check for success (PC = $3469)
        if cpu.pc() == 0x3469 {
            let elapsed = start_time.elapsed();
            println!("SUCCESS! All tests passed.");
            println!();
            println!("Statistics:");
            println!("  Instructions executed: {}", instruction_count);
            println!("  Total cycles: {}", total_cycles);
            println!("  Time elapsed: {:?}", elapsed);
            println!(
                "  Effective speed: {:.2} MHz",
                total_cycles as f64 / elapsed.as_secs_f64() / 1_000_000.0
            );
            std::process::exit(0);
        }

        // Check for trap (stuck in a loop)
        if cpu.pc() == last_pc {
            stuck_count += 1;
            if stuck_count >= 3 {
                println!("TRAP detected at PC=${:04X}", cpu.pc());
                println!();
                println!("Test failed! The CPU got stuck in an infinite loop.");
                println!("This indicates a bug in the 6502 implementation.");
                println!();
                println!("Context:");
                println!("  A=${:02X} X=${:02X} Y=${:02X}", cpu.a(), cpu.x(), cpu.y());
                println!("  SP=${:02X} P=${:02X}", cpu.sp(), cpu.status());
                println!("  Instructions executed: {}", instruction_count);

                // Dump nearby memory
                println!();
                println!("Memory around PC:");
                let start = cpu.pc().saturating_sub(8) as usize;
                for i in 0..16 {
                    let addr = start + i;
                    print!("{:02X} ", bus.memory[addr]);
                }
                println!();

                std::process::exit(1);
            }
        } else {
            stuck_count = 0;
        }

        // Handle BRK instructions in the test (they shouldn't occur in normal flow)
        if bus.memory[pc_before as usize] == 0x00 && pc_before >= 0x0400 {
            println!(
                "BRK instruction at ${:04X} - possible test failure",
                pc_before
            );
        }

        last_pc = cpu.pc();

        // Progress indicator every million instructions
        if instruction_count % 1_000_000 == 0 {
            print!("\rExecuted {} million instructions, PC=${:04X}...", instruction_count / 1_000_000, cpu.pc());
            use std::io::Write;
            std::io::stdout().flush().unwrap();
        }
    }
}
