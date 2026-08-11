//! What a chip on the bus actually sees, half-cycle by half-cycle.
//!
//! The Spectrum's ULA decides contention from `addr`, `/MREQ` and `/IORQ`
//! as they stand at each half-cycle, and the machine ticks the ULA
//! *before* the CPU — so the ULA sees the pins left by the previous CPU
//! tick. Every attempt so far to reason that offset out has been wrong,
//! including several of mine, so this prints it instead.
//!
//! The reference waveforms are Zilog's, and they are what
//! `ferranti-ula-6c001e/tests/hdl_gate_reference.rs` models. In half-cycles
//! from the start of the M-cycle, with `T1a` = 0:
//!
//! | cycle | `/MREQ` low | `/IORQ` low |
//! |---|---|---|
//! | `M1` opcode | `T1b`–`T2b` | — |
//! | `M1` refresh | `T3b`–`T4a` | — |
//! | memory read | `T1b`–`T3b` | — |
//! | memory write | `T1b`–`T3b` | — |
//! | I/O read | — | `T2b`–`T3b` (Zilog `T2`–`T3`, incl. `TW`) |
//!
//! ```sh
//! cargo test -p zilog-z80 --test bus_pin_waveform -- --nocapture
//! ```

use zilog_z80::Z80;

/// Tick the CPU one half-cycle at a time and record what the next chip on
/// the bus would see: the pins as they stand *after* each tick.
fn waveform(
    program: &[u8],
    setup: fn(&mut Z80),
    halfcycles: usize,
) -> Vec<(u16, bool, bool, bool)> {
    let mut cpu = Z80::new();

    setup(&mut cpu);

    let mut out = Vec::new();
    for _ in 0..halfcycles {
        // Feed the opcode stream from a fixed window; the machine would do
        // this from memory, and for a pin trace only the pins matter.
        let index = (cpu.addr as usize).wrapping_sub(0x4000);
        cpu.data_in = program
            .get(index % program.len().max(1))
            .copied()
            .unwrap_or(0);
        cpu.tick();
        out.push((cpu.addr, cpu.mreq, cpu.iorq, cpu.rfsh));
    }
    out
}

fn show(name: &str, w: &[(u16, bool, bool, bool)]) {
    println!("\n{name}");
    print!("  half-cycle ");
    for i in 0..w.len() {
        print!("{:>3}", i);
    }
    println!();
    print!("  addr $     ");
    for (a, _, _, _) in w {
        print!("{:>3}", (a >> 12) & 0xF);
    }
    println!("   (top nibble)");
    print!("  /MREQ low  ");
    for (_, m, _, _) in w {
        print!("{:>3}", if *m { "L" } else { "." });
    }
    println!();
    print!("  /IORQ low  ");
    for (_, _, i, _) in w {
        print!("{:>3}", if *i { "L" } else { "." });
    }
    println!();
    print!("  /RFSH low  ");
    for (_, _, _, r) in w {
        print!("{:>3}", if *r { "L" } else { "." });
    }
    println!();
}

#[test]
#[ignore = "diagnostic pin trace"]
fn print_bus_pin_waveforms() {
    // NOP stream: back-to-back M1 cycles.
    show(
        "NOP, NOP  (M1 x2)",
        &waveform(&[0x00], |c| c.regs.pc = 0x4000, 16),
    );

    // LD A,(HL): M1 then a memory read from $5000.
    show(
        "LD A,(HL)  (M1 + memory read)",
        &waveform(
            &[0x7E],
            |c| {
                c.regs.pc = 0x4000;
                c.regs.hl = 0x5000;
            },
            14,
        ),
    );

    // LD (HL),A: M1 then a memory write.
    show(
        "LD (HL),A  (M1 + memory write)",
        &waveform(
            &[0x77],
            |c| {
                c.regs.pc = 0x4000;
                c.regs.hl = 0x5000;
            },
            14,
        ),
    );

    // IN A,(C): ED prefix, opcode, then the I/O cycle.
    show(
        "IN A,(C)  (M1 x2 + I/O read)",
        &waveform(
            &[0xED, 0x78],
            |c| {
                c.regs.pc = 0x4000;
                c.regs.bc = 0xC0FE;
            },
            24,
        ),
    );
}
