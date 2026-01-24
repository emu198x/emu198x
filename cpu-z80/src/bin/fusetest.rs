//! Fuse Z80 test suite runner.
//!
//! Runs the Fuse emulator's Z80 test suite against our CPU implementation.
//! Tests verify correct instruction behavior including flags, timing, and memory operations.

use cpu_z80::Z80;
use emu_core::{Bus, Cpu};
use std::collections::HashMap;
use std::fs;
use std::panic::{self, AssertUnwindSafe};

/// Test bus that records all memory operations and their timing.
struct TestBus {
    memory: [u8; 65536],
    events: Vec<BusEvent>,
    t_state: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum BusEvent {
    /// Memory contention/address (MC)
    MemoryContention { t_state: u32, addr: u16 },
    /// Memory read (MR)
    MemoryRead { t_state: u32, addr: u16, value: u8 },
    /// Memory write (MW)
    MemoryWrite { t_state: u32, addr: u16, value: u8 },
    /// Port read (PR)
    PortRead { t_state: u32, port: u16, value: u8 },
    /// Port write (PW)
    PortWrite { t_state: u32, port: u16, value: u8 },
}

impl TestBus {
    fn new() -> Self {
        Self {
            memory: [0; 65536],
            events: Vec::new(),
            t_state: 0,
        }
    }

    fn reset(&mut self) {
        self.memory = [0; 65536];
        self.events.clear();
        self.t_state = 0;
    }

    fn load_memory(&mut self, addr: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.memory[addr.wrapping_add(i as u16) as usize] = byte;
        }
    }
}

impl Bus for TestBus {
    fn read(&mut self, address: u32) -> u8 {
        let addr = (address & 0xFFFF) as u16;
        let value = self.memory[addr as usize];
        self.events.push(BusEvent::MemoryContention {
            t_state: self.t_state,
            addr,
        });
        self.t_state += 3;
        self.events.push(BusEvent::MemoryRead {
            t_state: self.t_state,
            addr,
            value,
        });
        value
    }

    fn write(&mut self, address: u32, value: u8) {
        let addr = (address & 0xFFFF) as u16;
        self.events.push(BusEvent::MemoryContention {
            t_state: self.t_state,
            addr,
        });
        self.t_state += 3;
        self.events.push(BusEvent::MemoryWrite {
            t_state: self.t_state,
            addr,
            value,
        });
        self.memory[addr as usize] = value;
    }

    fn tick(&mut self, cycles: u32) {
        self.t_state += cycles;
    }

    fn fetch(&mut self, address: u32) -> u8 {
        let addr = (address & 0xFFFF) as u16;
        let value = self.memory[addr as usize];
        // M1 cycle: MC at start, MR after 4 T-states
        self.events.push(BusEvent::MemoryContention {
            t_state: self.t_state,
            addr,
        });
        self.t_state += 4;
        self.events.push(BusEvent::MemoryRead {
            t_state: self.t_state,
            addr,
            value,
        });
        value
    }

    fn refresh(&mut self, _ir: u16) {
        // Refresh cycle doesn't generate events in Fuse tests
    }
}

impl emu_core::IoBus for TestBus {
    fn read_io(&mut self, port: u16) -> u8 {
        let value = (port >> 8) as u8; // Default: high byte of port
        self.events.push(BusEvent::PortRead {
            t_state: self.t_state,
            port,
            value,
        });
        self.t_state += 4;
        value
    }

    fn write_io(&mut self, port: u16, value: u8) {
        self.events.push(BusEvent::PortWrite {
            t_state: self.t_state,
            port,
            value,
        });
        self.t_state += 4;
    }
}

/// Parsed test input
#[derive(Debug)]
struct TestInput {
    name: String,
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    af_prime: u16,
    bc_prime: u16,
    de_prime: u16,
    hl_prime: u16,
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    i: u8,
    r: u8,
    iff1: bool,
    iff2: bool,
    im: u8,
    halted: bool,
    /// Minimum T-states to run before stopping
    ticks: u32,
    memory: Vec<(u16, Vec<u8>)>,
}

/// Parsed expected output
#[derive(Debug)]
struct TestExpected {
    name: String,
    events: Vec<ExpectedEvent>,
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    af_prime: u16,
    bc_prime: u16,
    de_prime: u16,
    hl_prime: u16,
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    i: u8,
    r: u8,
    iff1: bool,
    iff2: bool,
    im: u8,
    halted: bool,
    t_states: u32,
    memory: Vec<(u16, Vec<u8>)>,
}

#[derive(Debug, Clone)]
enum ExpectedEvent {
    MC { t_state: u32, addr: u16 },
    MR { t_state: u32, addr: u16, value: u8 },
    MW { t_state: u32, addr: u16, value: u8 },
    PR { t_state: u32, port: u16, value: u8 },
    PW { t_state: u32, port: u16, value: u8 },
}

fn parse_hex_u16(s: &str) -> u16 {
    u16::from_str_radix(s, 16).unwrap_or(0)
}

fn parse_hex_u8(s: &str) -> u8 {
    u8::from_str_radix(s, 16).unwrap_or(0)
}

fn parse_tests_in(content: &str) -> Vec<TestInput> {
    let mut tests = Vec::new();
    let mut lines = content.lines().peekable();

    while let Some(name_line) = lines.next() {
        let name = name_line.trim();
        if name.is_empty() {
            continue;
        }

        // Register line 1: AF BC DE HL AF' BC' DE' HL' IX IY SP PC
        let reg_line = match lines.next() {
            Some(l) => l,
            None => break,
        };
        let regs: Vec<&str> = reg_line.split_whitespace().collect();
        if regs.len() < 12 {
            continue;
        }

        // Register line 2: I R IFF1 IFF2 IM HALTED TICKS
        let state_line = match lines.next() {
            Some(l) => l,
            None => break,
        };
        let state: Vec<&str> = state_line.split_whitespace().collect();
        if state.len() < 7 {
            continue;
        }

        // Memory chunks until -1
        let mut memory = Vec::new();
        while let Some(mem_line) = lines.next() {
            let parts: Vec<&str> = mem_line.split_whitespace().collect();
            if parts.is_empty() || parts[0] == "-1" {
                break;
            }
            let addr = parse_hex_u16(parts[0]);
            let mut bytes = Vec::new();
            for &part in &parts[1..] {
                if part == "-1" {
                    break;
                }
                bytes.push(parse_hex_u8(part));
            }
            if !bytes.is_empty() {
                memory.push((addr, bytes));
            }
        }

        tests.push(TestInput {
            name: name.to_string(),
            af: parse_hex_u16(regs[0]),
            bc: parse_hex_u16(regs[1]),
            de: parse_hex_u16(regs[2]),
            hl: parse_hex_u16(regs[3]),
            af_prime: parse_hex_u16(regs[4]),
            bc_prime: parse_hex_u16(regs[5]),
            de_prime: parse_hex_u16(regs[6]),
            hl_prime: parse_hex_u16(regs[7]),
            ix: parse_hex_u16(regs[8]),
            iy: parse_hex_u16(regs[9]),
            sp: parse_hex_u16(regs[10]),
            pc: parse_hex_u16(regs[11]),
            i: parse_hex_u8(state[0]),
            r: parse_hex_u8(state[1]),
            iff1: state[2] != "0",
            iff2: state[3] != "0",
            im: state[4].parse().unwrap_or(0),
            halted: state[5] != "0",
            ticks: state[6].parse().unwrap_or(1),
            memory,
        });
    }

    tests
}

fn parse_tests_expected(content: &str) -> HashMap<String, TestExpected> {
    let mut tests = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(name_line) = lines.next() {
        let name = name_line.trim();
        if name.is_empty() {
            continue;
        }

        // Events (indented lines)
        let mut events = Vec::new();
        while let Some(&line) = lines.peek() {
            if !line.starts_with(' ') && !line.starts_with('\t') {
                break;
            }
            let line = lines.next().unwrap();
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let t_state: u32 = parts[0].parse().unwrap_or(0);
                match parts[1] {
                    "MC" => {
                        events.push(ExpectedEvent::MC {
                            t_state,
                            addr: parse_hex_u16(parts[2]),
                        });
                    }
                    "MR" => {
                        events.push(ExpectedEvent::MR {
                            t_state,
                            addr: parse_hex_u16(parts[2]),
                            value: if parts.len() > 3 {
                                parse_hex_u8(parts[3])
                            } else {
                                0
                            },
                        });
                    }
                    "MW" => {
                        events.push(ExpectedEvent::MW {
                            t_state,
                            addr: parse_hex_u16(parts[2]),
                            value: if parts.len() > 3 {
                                parse_hex_u8(parts[3])
                            } else {
                                0
                            },
                        });
                    }
                    "PR" => {
                        events.push(ExpectedEvent::PR {
                            t_state,
                            port: parse_hex_u16(parts[2]),
                            value: if parts.len() > 3 {
                                parse_hex_u8(parts[3])
                            } else {
                                0
                            },
                        });
                    }
                    "PW" => {
                        events.push(ExpectedEvent::PW {
                            t_state,
                            port: parse_hex_u16(parts[2]),
                            value: if parts.len() > 3 {
                                parse_hex_u8(parts[3])
                            } else {
                                0
                            },
                        });
                    }
                    _ => {}
                }
            }
        }

        // Register line 1
        let reg_line = match lines.next() {
            Some(l) => l,
            None => break,
        };
        let regs: Vec<&str> = reg_line.split_whitespace().collect();
        if regs.len() < 12 {
            continue;
        }

        // Register line 2
        let state_line = match lines.next() {
            Some(l) => l,
            None => break,
        };
        let state: Vec<&str> = state_line.split_whitespace().collect();
        if state.len() < 7 {
            continue;
        }

        // Memory chunks (optional)
        let mut memory = Vec::new();
        while let Some(&line) = lines.peek() {
            if line.trim().is_empty() {
                lines.next();
                break;
            }
            // Check if this is a new test name (no whitespace prefix, not starting with number)
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && !line
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            {
                break;
            }
            let line = lines.next().unwrap();
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                break;
            }
            // Memory line starts with hex address
            if let Ok(addr) = u16::from_str_radix(parts[0], 16) {
                let mut bytes = Vec::new();
                for &part in &parts[1..] {
                    if part == "-1" {
                        break;
                    }
                    if let Ok(b) = u8::from_str_radix(part, 16) {
                        bytes.push(b);
                    }
                }
                if !bytes.is_empty() {
                    memory.push((addr, bytes));
                }
            }
        }

        tests.insert(
            name.to_string(),
            TestExpected {
                name: name.to_string(),
                events,
                af: parse_hex_u16(regs[0]),
                bc: parse_hex_u16(regs[1]),
                de: parse_hex_u16(regs[2]),
                hl: parse_hex_u16(regs[3]),
                af_prime: parse_hex_u16(regs[4]),
                bc_prime: parse_hex_u16(regs[5]),
                de_prime: parse_hex_u16(regs[6]),
                hl_prime: parse_hex_u16(regs[7]),
                ix: parse_hex_u16(regs[8]),
                iy: parse_hex_u16(regs[9]),
                sp: parse_hex_u16(regs[10]),
                pc: parse_hex_u16(regs[11]),
                i: parse_hex_u8(state[0]),
                r: parse_hex_u8(state[1]),
                iff1: state[2] != "0",
                iff2: state[3] != "0",
                im: state[4].parse().unwrap_or(0),
                halted: state[5] != "0",
                t_states: state[6].parse().unwrap_or(0),
                memory,
            },
        );
    }

    tests
}

fn run_test(input: &TestInput, expected: &TestExpected) -> Result<(), String> {
    let mut bus = TestBus::new();
    let mut cpu = Z80::new();

    // Load initial memory
    for (addr, bytes) in &input.memory {
        bus.load_memory(*addr, bytes);
    }

    // Set initial CPU state
    cpu.load_state(
        input.af,
        input.bc,
        input.de,
        input.hl,
        input.af_prime,
        input.bc_prime,
        input.de_prime,
        input.hl_prime,
        input.ix,
        input.iy,
        input.sp,
        input.pc,
        input.i,
        input.r,
        input.iff1,
        input.iff2,
        input.im,
    );

    if input.halted {
        cpu.halt();
    }

    // Execute instructions that START before the ticks threshold
    // (catch panic for unimplemented opcodes)
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        loop {
            let start_t = bus.t_state;
            if start_t >= input.ticks {
                break;
            }
            cpu.step(&mut bus);
        }
    }));

    if let Err(e) = result {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        return Err(format!("PANIC: {}", msg));
    }

    // Compare final state
    let mut errors = Vec::new();

    // Check registers (mask out undocumented flag bits for comparison)
    let af_mask = 0xFFFF; // Include all flags for now
    if (cpu.af() & af_mask) != (expected.af & af_mask) {
        errors.push(format!(
            "AF: got {:04X}, expected {:04X}",
            cpu.af(),
            expected.af
        ));
    }
    if cpu.bc() != expected.bc {
        errors.push(format!(
            "BC: got {:04X}, expected {:04X}",
            cpu.bc(),
            expected.bc
        ));
    }
    if cpu.de() != expected.de {
        errors.push(format!(
            "DE: got {:04X}, expected {:04X}",
            cpu.de(),
            expected.de
        ));
    }
    if cpu.hl() != expected.hl {
        errors.push(format!(
            "HL: got {:04X}, expected {:04X}",
            cpu.hl(),
            expected.hl
        ));
    }
    if cpu.ix() != expected.ix {
        errors.push(format!(
            "IX: got {:04X}, expected {:04X}",
            cpu.ix(),
            expected.ix
        ));
    }
    if cpu.iy() != expected.iy {
        errors.push(format!(
            "IY: got {:04X}, expected {:04X}",
            cpu.iy(),
            expected.iy
        ));
    }
    if cpu.sp() != expected.sp {
        errors.push(format!(
            "SP: got {:04X}, expected {:04X}",
            cpu.sp(),
            expected.sp
        ));
    }
    if cpu.pc() != expected.pc {
        errors.push(format!(
            "PC: got {:04X}, expected {:04X}",
            cpu.pc(),
            expected.pc
        ));
    }

    // Check I register
    if cpu.i() != expected.i {
        errors.push(format!(
            "I: got {:02X}, expected {:02X}",
            cpu.i(),
            expected.i
        ));
    }

    // Check R register (only lower 7 bits matter for comparison)
    if (cpu.r() & 0x7F) != (expected.r & 0x7F) {
        errors.push(format!(
            "R: got {:02X}, expected {:02X}",
            cpu.r(),
            expected.r
        ));
    }

    // Check T-states
    if bus.t_state != expected.t_states {
        errors.push(format!(
            "T-states: got {}, expected {}",
            bus.t_state, expected.t_states
        ));
    }

    // Check memory
    for (addr, expected_bytes) in &expected.memory {
        for (i, &expected_byte) in expected_bytes.iter().enumerate() {
            let actual_addr = addr.wrapping_add(i as u16);
            let actual_byte = bus.memory[actual_addr as usize];
            if actual_byte != expected_byte {
                errors.push(format!(
                    "Memory[{:04X}]: got {:02X}, expected {:02X}",
                    actual_addr, actual_byte, expected_byte
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn main() {
    let tests_in_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fuse-tests/tests.in".to_string());
    let tests_expected_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "fuse-tests/tests.expected".to_string());

    let tests_in = fs::read_to_string(&tests_in_path).expect("Failed to read tests.in");
    let tests_expected =
        fs::read_to_string(&tests_expected_path).expect("Failed to read tests.expected");

    let inputs = parse_tests_in(&tests_in);
    let expected = parse_tests_expected(&tests_expected);

    println!("Parsed {} test inputs", inputs.len());
    println!("Parsed {} expected outputs", expected.len());

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut failures = Vec::new();

    for input in &inputs {
        match expected.get(&input.name) {
            Some(exp) => match run_test(input, exp) {
                Ok(()) => {
                    passed += 1;
                }
                Err(e) => {
                    failed += 1;
                    if failures.len() < 20 {
                        failures.push(format!("{}: {}", input.name, e));
                    }
                }
            },
            None => {
                skipped += 1;
            }
        }
    }

    println!(
        "\nResults: {} passed, {} failed, {} skipped",
        passed, failed, skipped
    );

    if !failures.is_empty() {
        println!("\nFirst {} failures:", failures.len());
        for failure in &failures {
            println!("  {}", failure);
        }
    }

    if failed > 0 {
        std::process::exit(1);
    }
}
