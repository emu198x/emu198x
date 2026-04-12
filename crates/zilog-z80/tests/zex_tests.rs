/// ZEXDOC / ZEXALL test harness.
///
/// These are Frank Cringle's Z80 exerciser programs, originally CP/M .COM files.
/// They test every Z80 instruction by running it with many inputs and comparing
/// a CRC of the resulting flags/registers against known-good values.
///
/// We load them at $0100 (CP/M TPA), trap BDOS calls at $0005 for console
/// output, and trap JP $0000 (warm boot) as exit.
mod support;

use support::find_zex_binary;
use zilog_z80::Z80;

struct ZexRunResult {
    output: String,
    completed: bool,
}

/// Minimal CP/M memory: 64K flat, .COM loaded at $0100.
struct CpmMemory {
    mem: [u8; 65536],
}

impl CpmMemory {
    fn new(com: &[u8]) -> Self {
        let mut mem = [0u8; 65536];
        // Load .COM at $0100
        let end = (0x0100 + com.len()).min(65536);
        mem[0x0100..end].copy_from_slice(&com[..end - 0x0100]);
        // Put RET at $0005 (BDOS entry) — we trap before it executes
        mem[0x0005] = 0xC9; // RET
        // Put HALT at $0000 (warm boot) — we trap this
        mem[0x0000] = 0x76; // HALT
        Self { mem }
    }

    fn read(&self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.mem[addr as usize] = val;
    }
}

/// Run a ZEX .COM file, returning the console output and completion status.
fn run_zex(com_data: &[u8]) -> ZexRunResult {
    let mut mem = CpmMemory::new(com_data);
    let mut z80 = Z80::new();
    let mut output = String::new();
    let mut completed = false;
    let mut bdos_call_active = false;

    // CP/M entry: PC = $0100, SP = $FFFE (below BDOS)
    z80.regs.pc = 0x0100;
    z80.regs.sp = 0xFFFE;

    let mut cycle_count: u64 = 0;
    let max_cycles: u64 = 500_000_000_000; // Safety limit

    loop {
        z80.tick();
        cycle_count += 1;

        // Handle bus: memory read/write
        if z80.mreq && z80.rd {
            z80.data_in = mem.read(z80.addr);
        } else if z80.mreq && z80.wr {
            mem.write(z80.addr, z80.data);
        } else if z80.iorq && z80.m1 {
            // Interrupt ack (shouldn't happen — no interrupts)
            z80.data_in = 0xFF;
        }

        // Check for BDOS call: handle it once when the M1 fetch at $0005 begins.
        let at_bdos_fetch = z80.m1 && z80.addr == 0x0005;
        if at_bdos_fetch && !bdos_call_active {
            bdos_call_active = true;
            let func = z80.regs.bc & 0xFF; // C register = BDOS function
            match func as u8 {
                2 => {
                    // Print character (E register)
                    let ch = (z80.regs.de & 0xFF) as u8 as char;
                    output.push(ch);
                    eprint!("{}", ch);
                }
                9 => {
                    // Print string (DE = address, '$' terminated)
                    let mut addr = z80.regs.de;
                    let mut message = String::new();
                    loop {
                        let ch = mem.read(addr);
                        if ch == b'$' {
                            break;
                        }
                        let ch = ch as char;
                        output.push(ch);
                        message.push(ch);
                        addr = addr.wrapping_add(1);
                    }
                    let message = message.trim().to_string();
                    if !message.is_empty() {
                        eprintln!("{message}");
                        if message.contains("complete") {
                            completed = true;
                        }
                    }
                }
                _ => {}
            }
            // The RET at $0005 will pop back to the caller
        } else if !at_bdos_fetch {
            bdos_call_active = false;
        }

        if completed {
            eprintln!("\nZEX complete after {} cycles", cycle_count);
            break;
        }

        // Check for warm boot (HALT at $0000)
        if z80.halt {
            eprintln!("\nZEX complete after {} cycles", cycle_count);
            break;
        }

        if cycle_count > max_cycles {
            eprintln!("\nZEX timed out after {} cycles", cycle_count);
            break;
        }
    }

    ZexRunResult { output, completed }
}

#[test]
#[ignore = "requires local ZEX corpus and runs for minutes"]
fn run_zexdoc() {
    let path = match find_zex_binary("zexdoc") {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return;
        }
    };
    let com = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    };
    let result = run_zex(&com);
    let output = &result.output;

    // Count passes and failures
    let tests_ok = output.matches("OK").count();
    let tests_fail = output.matches("ERROR").count();
    eprintln!("ZEXDOC: {} OK, {} ERROR", tests_ok, tests_fail);

    assert!(result.completed, "ZEXDOC did not report completion");
    assert_eq!(
        tests_fail, 0,
        "ZEXDOC had {} failures:\n{}",
        tests_fail, output
    );
}

#[test]
#[ignore = "requires local ZEX corpus and runs for minutes"]
fn run_zexall() {
    let path = match find_zex_binary("zexall") {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return;
        }
    };
    let com = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    };
    let result = run_zex(&com);
    let output = &result.output;

    let tests_ok = output.matches("OK").count();
    let tests_fail = output.matches("ERROR").count();
    eprintln!("ZEXALL: {} OK, {} ERROR", tests_ok, tests_fail);

    assert!(result.completed, "ZEXALL did not report completion");
    assert_eq!(
        tests_fail, 0,
        "ZEXALL had {} failures:\n{}",
        tests_fail, output
    );
}
