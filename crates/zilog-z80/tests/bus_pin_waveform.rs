//! What a chip on the bus actually sees, half-cycle by half-cycle.
//!
//! The Spectrum's ULA decides contention from `addr`, `/MREQ` and `/IORQ`
//! as they stand at each half-cycle, and the machine ticks the ULA
//! *before* the CPU — so the ULA sees the pins left by the previous CPU
//! tick. Every attempt so far to reason that offset out has been wrong,
//! including several of mine, so this asserts it instead of printing it.
//!
//! ## The convention, stated once
//!
//! Each row is one half-cycle. The `phase` column names the phase handler
//! that ran during it, so `M1(T1Fall)` is half-cycle `T1b` of an `M1`
//! cycle. The pins listed are the ones that handler left asserted — which
//! is what the next chip on the bus sees for the whole of that half-cycle,
//! because the Z80 drives them until its next tick. A strobe first listed
//! on `T1Fall` is therefore low from the falling edge of `T1`, which is how
//! Zilog draws it.
//!
//! A strobe that runs to *the end of* its last T-state has no handler of
//! its own to release it: the releasing clock edge is the next M-cycle's
//! `T1Rise`. So "still asserted on the last row of the cycle" is the
//! correct rendering of "released at the end of `T3`", not a leak.
//!
//! ## The reference
//!
//! Zilog UM0080 (*Z80 CPU User Manual*), the timing diagrams for the
//! opcode-fetch, memory read/write and input/output cycles.
//!
//! | cycle | strobe | Zilog | asserted during |
//! |---|---|---|---|
//! | `M1` opcode | `/MREQ`, `/RD` | `T1`↓ → `T3`↑ | `T1b`–`T2b` |
//! | `M1` refresh | `/RFSH` | `T3`↑ → next `T1`↑ | `T3a`–`T4b` |
//! | `M1` refresh | `/MREQ` | `T3`↓ → `T4`↓ | `T3b`–`T4a` |
//! | memory read | `/MREQ`, `/RD` | `T1`↓ → end of `T3` | `T1b`–`T3b` |
//! | memory write | `/MREQ` | `T1`↓ → end of `T3` | `T1b`–`T3b` |
//! | memory write | `/WR` | `T2`↓ → end of `T3` | `T2b`–`T3b` |
//! | I/O | `/IORQ` + `/RD`\|`/WR` | `T2`↓ → end of `T3` | `T2b`–`T3b`, over `T2`/`TW`/`T3` |
//!
//! The I/O cycle's automatic wait state means its four T-states are `T1`,
//! `T2`, `TW`, `T3`; this state machine names them `T1`–`T4`, so a row
//! reading `IoRead(T3Fall)` is Zilog's `TWb` and `IoRead(T4Fall)` is `T3b`.
//!
//! ## Departures
//!
//! None outstanding. Every strobe above matches the reference, each
//! address is presented on its own cycle's `T1`↑, and an internal cycle
//! drives nothing at all.
//!
//! Where the table and this engine disagree with SpecIde, the only other
//! signal-level Z80 in the tree, it is in two places and Zilog governs both:
//! SpecIde runs the `M1` `/MREQ` as one continuous pulse from `T1b` to
//! `T4a` where Zilog has two, separated by `T3a`; and it drops `/IORQ` on
//! `T2`↑ where Zilog draws `T2`↓. The first is invisible on a Spectrum,
//! whose refresh address is uncontended; the second is half a T-state of
//! I/O contention and is not.
//!
//! ```sh
//! cargo test -p zilog-z80 --test bus_pin_waveform
//! ```

use zilog_z80::Z80;

/// Tick the CPU one half-cycle at a time and record what the next chip on
/// the bus would see: the phase that ran, and the pins as they stand
/// *after* each tick.
fn waveform(program: &[u8], setup: fn(&mut Z80), halfcycles: usize) -> String {
    let mut cpu = Z80::new();

    setup(&mut cpu);

    let mut out = String::new();
    for i in 0..halfcycles {
        // Feed the opcode stream from a fixed window; the machine would do
        // this from memory, and for a pin trace only the pins matter.
        let index = (cpu.addr as usize).wrapping_sub(0x4000);
        cpu.data_in = program
            .get(index % program.len().max(1))
            .copied()
            .unwrap_or(0);

        // `Internal`'s Debug carries the countdown in a struct literal;
        // flatten it to `Internal(4)` so the column stays readable.
        let phase = format!("{:?}", cpu.phase)
            .replace("InternalPhase { remaining: ", "")
            .replace(" }", "");
        cpu.tick();

        let mut pins = Vec::new();
        for (name, level) in [
            ("M1", cpu.m1),
            ("MREQ", cpu.mreq),
            ("IORQ", cpu.iorq),
            ("RD", cpu.rd),
            ("WR", cpu.wr),
            ("RFSH", cpu.rfsh),
        ] {
            if level {
                pins.push(name);
            }
        }

        let row = format!("{i:<3} {phase:<18} {:04X}  {}", cpu.addr, pins.join(" "));
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out
}

/// Back-to-back `M1` cycles from a `NOP` stream.
fn m1_waveform(halfcycles: usize) -> String {
    waveform(&[0x00], |c| c.regs.pc = 0x4000, halfcycles)
}

/// `LD A,(HL)` — `M1` then a memory read from $5000.
fn memory_read_waveform() -> String {
    waveform(
        &[0x7E],
        |c| {
            c.regs.pc = 0x4000;
            c.regs.hl = 0x5000;
        },
        14,
    )
}

/// `LD (HL),A` — `M1` then a memory write to $5000.
fn memory_write_waveform() -> String {
    waveform(
        &[0x77],
        |c| {
            c.regs.pc = 0x4000;
            c.regs.hl = 0x5000;
        },
        14,
    )
}

/// `INC BC` — `M1` then two internal T-states with no bus activity.
///
/// The case that catches a leak. A strobe released "at the end of its last
/// T-state" has no handler of its own to release it — the next M-cycle's
/// `T1Rise` does that. An internal cycle has no `T1Rise`, so a strobe left
/// asserted into one stays asserted, and on a Spectrum the ULA would go on
/// reading it as a live access.
fn internal_cycle_waveform() -> String {
    waveform(&[0x03], |c| c.regs.pc = 0x4000, 14)
}

/// `JR NZ,e` with Z set — `M1` then the not-taken displacement cycle.
///
/// The Z80 still fetches the displacement byte it is about to ignore, and
/// FUSE scores it as a plain contended read: `contend_read( PC, 3 )` in
/// `opcodes_base.c`, the same as any operand fetch. `MStep::ContendPc`
/// models it as a cycle of its own.
fn contend_cycle_waveform() -> String {
    waveform(
        &[0x20, 0x05],
        |c| {
            c.regs.pc = 0x4000;
            c.regs.set_f(0x40); // Z set, so the branch is not taken
        },
        14,
    )
}

/// `LDI` — `M1`, `M1`, a read from `HL`, a write to `DE`, then two
/// internal T-states.
///
/// The internal cycles are what this case is for. Nothing drives the
/// address bus during them, so it holds the last address driven — `DE`.
/// FUSE scores exactly that: `contend_write_no_mreq( DE, 1 )` twice in
/// `z80_ed.c`, against `contend_read_no_mreq( IR, 1 )` for the internal
/// cycles of `INC BC`, which follow `M1` and so see the refresh address.
/// One rule, two addresses.
fn internal_after_write_waveform() -> String {
    waveform(
        &[0xED, 0xA0],
        |c| {
            c.regs.pc = 0x4000;
            c.regs.hl = 0x5000;
            c.regs.de = 0x6000;
        },
        32,
    )
}

/// `IN A,(C)` — `ED` prefix, opcode, then the I/O read cycle.
fn io_read_waveform() -> String {
    waveform(
        &[0xED, 0x78],
        |c| {
            c.regs.pc = 0x4000;
            c.regs.bc = 0xC0FE;
        },
        24,
    )
}

/// `OUT (C),A` — `ED` prefix, opcode, then the I/O write cycle.
fn io_write_waveform() -> String {
    waveform(
        &[0xED, 0x79],
        |c| {
            c.regs.pc = 0x4000;
            c.regs.bc = 0xC0FE;
        },
        24,
    )
}

#[test]
fn m1_cycle_bus_pins() {
    assert_eq!(
        m1_waveform(16),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   M1(T1Rise)         4001  M1\n",
            "9   M1(T1Fall)         4001  M1 MREQ RD\n",
            "10  M1(T2Rise)         4001  M1 MREQ RD\n",
            "11  M1(T2Fall)         4001  M1 MREQ RD\n",
            "12  M1(T3Rise)         0001  RFSH\n",
            "13  M1(T3Fall)         0001  MREQ RFSH\n",
            "14  M1(T4Rise)         0001  MREQ RFSH\n",
            "15  M1(T4Fall)         0001  RFSH\n",
        )
    );
}

#[test]
fn memory_read_bus_pins() {
    assert_eq!(
        memory_read_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   MemRead(T1Rise)    5000\n",
            "9   MemRead(T1Fall)    5000  MREQ RD\n",
            "10  MemRead(T2Rise)    5000  MREQ RD\n",
            "11  MemRead(T2Fall)    5000  MREQ RD\n",
            "12  MemRead(T3Rise)    5000  MREQ RD\n",
            "13  MemRead(T3Fall)    5000  MREQ RD\n",
        )
    );
}

#[test]
fn memory_write_bus_pins() {
    assert_eq!(
        memory_write_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   MemWrite(T1Rise)   5000\n",
            "9   MemWrite(T1Fall)   5000  MREQ\n",
            "10  MemWrite(T2Rise)   5000  MREQ\n",
            "11  MemWrite(T2Fall)   5000  MREQ WR\n",
            "12  MemWrite(T3Rise)   5000  MREQ WR\n",
            "13  MemWrite(T3Fall)   5000  MREQ WR\n",
        )
    );
}

#[test]
fn io_read_bus_pins() {
    assert_eq!(
        io_read_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   M1(T1Rise)         4001  M1\n",
            "9   M1(T1Fall)         4001  M1 MREQ RD\n",
            "10  M1(T2Rise)         4001  M1 MREQ RD\n",
            "11  M1(T2Fall)         4001  M1 MREQ RD\n",
            "12  M1(T3Rise)         0001  RFSH\n",
            "13  M1(T3Fall)         0001  MREQ RFSH\n",
            "14  M1(T4Rise)         0001  MREQ RFSH\n",
            "15  M1(T4Fall)         0001  RFSH\n",
            "16  IoRead(T1Rise)     C0FE\n",
            "17  IoRead(T1Fall)     C0FE\n",
            "18  IoRead(T2Rise)     C0FE\n",
            "19  IoRead(T2Fall)     C0FE  IORQ RD\n",
            "20  IoRead(T3Rise)     C0FE  IORQ RD\n",
            "21  IoRead(T3Fall)     C0FE  IORQ RD\n",
            "22  IoRead(T4Rise)     C0FE  IORQ RD\n",
            "23  IoRead(T4Fall)     C0FE  IORQ RD\n",
        )
    );
}

#[test]
fn io_write_bus_pins() {
    assert_eq!(
        io_write_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   M1(T1Rise)         4001  M1\n",
            "9   M1(T1Fall)         4001  M1 MREQ RD\n",
            "10  M1(T2Rise)         4001  M1 MREQ RD\n",
            "11  M1(T2Fall)         4001  M1 MREQ RD\n",
            "12  M1(T3Rise)         0001  RFSH\n",
            "13  M1(T3Fall)         0001  MREQ RFSH\n",
            "14  M1(T4Rise)         0001  MREQ RFSH\n",
            "15  M1(T4Fall)         0001  RFSH\n",
            "16  IoWrite(T1Rise)    C0FE\n",
            "17  IoWrite(T1Fall)    C0FE\n",
            "18  IoWrite(T2Rise)    C0FE\n",
            "19  IoWrite(T2Fall)    C0FE  IORQ WR\n",
            "20  IoWrite(T3Rise)    C0FE  IORQ WR\n",
            "21  IoWrite(T3Fall)    C0FE  IORQ WR\n",
            "22  IoWrite(T4Rise)    C0FE  IORQ WR\n",
            "23  IoWrite(T4Fall)    C0FE  IORQ WR\n",
        )
    );
}

#[test]
fn internal_cycle_bus_pins() {
    assert_eq!(
        internal_cycle_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   Internal(4)        0000\n",
            "9   Internal(3)        0000\n",
            "10  Internal(2)        0000\n",
            "11  Internal(1)        0000\n",
            "12  M1(T1Rise)         4001  M1\n",
            "13  M1(T1Fall)         4001  M1 MREQ RD\n",
        )
    );
}

#[test]
fn contend_cycle_bus_pins() {
    assert_eq!(
        contend_cycle_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   Contend(T1Rise)    4001\n",
            "9   Contend(T1Fall)    4001  MREQ RD\n",
            "10  Contend(T2Rise)    4001  MREQ RD\n",
            "11  Contend(T2Fall)    4001  MREQ RD\n",
            "12  Contend(T3Rise)    4001  MREQ RD\n",
            "13  Contend(T3Fall)    4001  MREQ RD\n",
        )
    );
}

#[test]
fn internal_after_write_bus_pins() {
    assert_eq!(
        internal_after_write_waveform(),
        concat!(
            "0   M1(T1Rise)         4000  M1\n",
            "1   M1(T1Fall)         4000  M1 MREQ RD\n",
            "2   M1(T2Rise)         4000  M1 MREQ RD\n",
            "3   M1(T2Fall)         4000  M1 MREQ RD\n",
            "4   M1(T3Rise)         0000  RFSH\n",
            "5   M1(T3Fall)         0000  MREQ RFSH\n",
            "6   M1(T4Rise)         0000  MREQ RFSH\n",
            "7   M1(T4Fall)         0000  RFSH\n",
            "8   M1(T1Rise)         4001  M1\n",
            "9   M1(T1Fall)         4001  M1 MREQ RD\n",
            "10  M1(T2Rise)         4001  M1 MREQ RD\n",
            "11  M1(T2Fall)         4001  M1 MREQ RD\n",
            "12  M1(T3Rise)         0001  RFSH\n",
            "13  M1(T3Fall)         0001  MREQ RFSH\n",
            "14  M1(T4Rise)         0001  MREQ RFSH\n",
            "15  M1(T4Fall)         0001  RFSH\n",
            "16  MemRead(T1Rise)    5000\n",
            "17  MemRead(T1Fall)    5000  MREQ RD\n",
            "18  MemRead(T2Rise)    5000  MREQ RD\n",
            "19  MemRead(T2Fall)    5000  MREQ RD\n",
            "20  MemRead(T3Rise)    5000  MREQ RD\n",
            "21  MemRead(T3Fall)    5000  MREQ RD\n",
            "22  MemWrite(T1Rise)   6000\n",
            "23  MemWrite(T1Fall)   6000  MREQ\n",
            "24  MemWrite(T2Rise)   6000  MREQ\n",
            "25  MemWrite(T2Fall)   6000  MREQ WR\n",
            "26  MemWrite(T3Rise)   6000  MREQ WR\n",
            "27  MemWrite(T3Fall)   6000  MREQ WR\n",
            "28  Internal(4)        6000\n",
            "29  Internal(3)        6000\n",
            "30  Internal(2)        6000\n",
            "31  Internal(1)        6000\n",
        )
    );
}
