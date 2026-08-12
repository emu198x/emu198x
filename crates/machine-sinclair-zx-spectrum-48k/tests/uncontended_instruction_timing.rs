//! What each instruction costs with no ULA in the way.
//!
//! Both timing surveys fail on the I/O instruction family in
//! **Uncontended** mode as well as Contended — 48K tests 32, 33 and 35,
//! 128K tests 32 and 33. An uncontended failure has no contention in it.
//! It is the instruction's own T-state structure, and it is wrong on two
//! machines in the same way because both run the same `zilog-z80`.
//!
//! This measures that directly: each instruction executed from
//! **uncontended** RAM, cost taken from the retired-instruction counter,
//! compared against the T-state counts in Zilog UM0080. No ULA, no FUSE,
//! no contention model — if a case here is red, no amount of work on the
//! contention gate will fix the survey case that matches it.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k \
//!     --test uncontended_instruction_timing -- --ignored --nocapture
//! ```
//!
//! ## What it found: nothing, and that is the result
//!
//! **Every instruction costs exactly what Zilog says**, at five frame
//! positions each. So the surveys' uncontended I/O failures are *not*
//! wrong T-state totals, and the plan that led here — "block and port I/O
//! instruction timing, do this first" — is disconfirmed at its premise.
//!
//! What remains is that "Uncontended" in those survey cases names where
//! the *code* runs, not where the *port* is. `B` is both the loop counter
//! and the port address's high byte for block I/O, and `A` supplies it for
//! `IN A,(n)` — so a suite sweeping those registers walks its port through
//! the contended page whatever page its code sits in. That points the
//! uncontended failures back at the I/O contention gate, which already has
//! 21,510 mismatches and three disconfirmed terms.
//!
//! This file is kept because a green instrument that can fail is worth
//! more than the hypothesis it killed: it holds the T-state totals for
//! sixteen instructions against Zilog, and it is the thing that will say
//! so next time a survey case is blamed on instruction timing.
//!
//! ## Three harness faults it caught first, all mine
//!
//! Worth recording, because each produced a confident wrong answer:
//!
//! - `BC = 0x40FF` put the **port** in the contended page, so the "no ULA
//!   involved" harness was measuring contention. `INI` read 24 T-states
//!   against Zilog's 16 and looked like a real defect.
//! - Writing the program before advancing the machine let the ROM's boot
//!   RAM clear wipe it, so every case read 4 — the cost of a field of
//!   `NOP`.
//! - Redirecting `PC` mid-M-cycle let the in-flight instruction finish and
//!   set `PC` itself, so execution carried on in the ROM.
//!
//! Hence the self-checks: measure at several frame positions and require
//! one answer, step to an instruction boundary before redirecting `PC`,
//! and assert afterwards that `PC` is still in the stream and the stream
//! is still in memory.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";

/// Uncontended RAM. Nothing here should ever meet the ULA's gate.
const CODE_BASE: u16 = 0x8000;
/// Somewhere uncontended for block instructions to read and write.
const DATA_ADDR: u16 = 0x9000;

/// How many different frame positions each case is measured at.
///
/// The first draft of this harness measured once, at whatever frame
/// position the machine happened to reach, and reported `INI` at 24
/// T-states against Zilog's 16. That was contention: `B` is both the loop
/// counter and the port address's high byte, and `BC = 0x40FF` puts the
/// port in the **contended page**, so the memory gate fired on the I/O
/// M-cycle exactly as `contention_arming` documents for `$40FF`.
///
/// Two changes came out of it. The port is now `$C0FF` — uncontended, and
/// still leaving `B` non-zero so the repeat forms repeat. And every case
/// is measured at several frame positions and required to give the *same*
/// answer, because a cost that varies with the raster is contention
/// leaking in and this file's whole claim is that there is none.
const FRAME_OFFSETS: [u32; 5] = [0, 7_000, 14_500, 30_000, 60_000];

/// One instruction, its bytes, and what Zilog says it costs.
struct Case {
    name: &'static str,
    bytes: &'static [u8],
    /// T-states, per Zilog UM0080's instruction tables.
    zilog: u32,
    setup: fn(&mut Spectrum48k),
}

fn cases() -> Vec<Case> {
    vec![
        // Anchors. If either of these is wrong the harness is wrong, not
        // the CPU.
        Case {
            name: "NOP",
            bytes: &[0x00],
            zilog: 4,
            setup: |_| {},
        },
        Case {
            name: "LD A,(HL)",
            bytes: &[0x7E],
            zilog: 7,
            setup: |m| m.z80_mut().regs.hl = DATA_ADDR,
        },
        // Survey test 35: plain port I/O.
        // `IN A,(n)` and `OUT (n),A` take the port's *high* byte from
        // `A`, so a stream of them walks its own port around the address
        // space as `A` changes — and lands in the contended page whenever
        // `A` happens to reach `$40..$7F`. Pinning `A` high keeps the port
        // at `$C0FF`, uncontended, which is what makes this a measurement
        // of the instruction rather than of the raster.
        Case {
            name: "IN A,(n)",
            bytes: &[0xDB, 0xFF],
            zilog: 11,
            setup: |m| m.z80_mut().regs.af = 0xC000,
        },
        Case {
            name: "OUT (n),A",
            bytes: &[0xD3, 0xFF],
            zilog: 11,
            setup: |m| m.z80_mut().regs.af = 0xC000,
        },
        Case {
            name: "IN r,(C)",
            bytes: &[0xED, 0x78],
            zilog: 12,
            setup: |m| m.z80_mut().regs.bc = 0x00FF,
        },
        Case {
            name: "OUT (C),r",
            bytes: &[0xED, 0x79],
            zilog: 12,
            setup: |m| m.z80_mut().regs.bc = 0x00FF,
        },
        // Survey test 32: block input. The repeat forms cost 21 while
        // `B` is non-zero and 16 on the final iteration; `B` is set high
        // enough here that every measured pass is a repeating one.
        Case {
            name: "INI",
            bytes: &[0xED, 0xA2],
            zilog: 16,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        Case {
            name: "IND",
            bytes: &[0xED, 0xAA],
            zilog: 16,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        Case {
            name: "INIR (B != 0)",
            bytes: &[0xED, 0xB2],
            zilog: 21,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        Case {
            name: "INDR (B != 0)",
            bytes: &[0xED, 0xBA],
            zilog: 21,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        // Survey test 33: block output.
        Case {
            name: "OUTI",
            bytes: &[0xED, 0xA3],
            zilog: 16,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        Case {
            name: "OUTD",
            bytes: &[0xED, 0xAB],
            zilog: 16,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        Case {
            name: "OTIR (B != 0)",
            bytes: &[0xED, 0xB3],
            zilog: 21,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        Case {
            name: "OTDR (B != 0)",
            bytes: &[0xED, 0xBB],
            zilog: 21,
            setup: |m| {
                m.z80_mut().regs.bc = 0xC0FF;
                m.z80_mut().regs.hl = DATA_ADDR;
            },
        },
        // Block *transfer* for contrast: same ED-prefixed shape, and the
        // surveys do not complain about it. If these are right and the
        // I/O ones are not, the defect is specific to the I/O M-cycle
        // rather than to ED-prefixed repeats in general.
        Case {
            name: "LDI",
            bytes: &[0xED, 0xA0],
            zilog: 16,
            setup: |m| {
                m.z80_mut().regs.bc = 0x4000;
                m.z80_mut().regs.hl = DATA_ADDR;
                m.z80_mut().regs.de = DATA_ADDR + 0x100;
            },
        },
        Case {
            name: "LDIR (BC != 0)",
            bytes: &[0xED, 0xB0],
            zilog: 21,
            setup: |m| {
                m.z80_mut().regs.bc = 0x4000;
                m.z80_mut().regs.hl = DATA_ADDR;
                m.z80_mut().regs.de = DATA_ADDR + 0x100;
            },
        },
    ]
}

fn rom_bytes() -> Option<Vec<u8>> {
    let path = std::env::var(ROM_PATH_ENV).ok()?;
    std::fs::read(path).ok()
}

/// Advance until one more instruction retires; returns its cost.
fn step_one_instruction(machine: &mut Spectrum48k) -> u32 {
    let target = machine.z80().instructions_retired() + 1;
    let mut cost = 0u32;
    while machine.z80().instructions_retired() < target {
        machine.advance_tstates(1);
        cost += 1;
        assert!(cost <= 512, "instruction should retire within 512 T-states");
    }
    cost
}

/// Measure one case at one frame position, discarding two retirements.
fn measure_at(case: &Case, rom: &[u8], skew: u32) -> Result<u32, String> {
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(rom).expect("48K ROM should load");
    machine.reset();

    // Advance *first*, then write. The 48K ROM clears RAM during boot, so
    // a program written before this loop is zeroed underneath the test —
    // which reads as every instruction costing 4 T-states, because that is
    // what a field of `NOP` costs.
    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }
    machine.advance_tstates(skew);

    let mut addr = CODE_BASE;
    let mut index = 0usize;
    while addr < CODE_BASE + 0x2000 {
        machine
            .memory_mut()
            .write(addr, case.bytes[index % case.bytes.len()]);
        index += 1;
        addr += 1;
    }
    // Reach an instruction boundary before redirecting `PC`. Writing it
    // mid-M-cycle leaves the in-flight instruction to finish and set `PC`
    // itself, so execution carries on in the ROM and the harness measures
    // ROM instructions — which is why every case read 4 T-states at one
    // frame offset and only there.
    step_one_instruction(&mut machine);

    machine.z80_mut().regs.pc = CODE_BASE;
    machine.z80_mut().regs.iff1 = false;
    machine.z80_mut().regs.iff2 = false;
    (case.setup)(&mut machine);

    // Aiming `PC` at the stream lands mid-M-cycle, so the first
    // retirements are the tail of whatever the ROM was already doing. A
    // fixed discard is not enough — measured, a two-instruction discard
    // still returned 4 T-states for `LDIR` at some frame positions.
    //
    // Instead: run a run of them and require the tail to agree with
    // itself. A settled uniform instruction stream gives the same answer
    // every time, and anything that does not has not settled.
    const DISCARD: usize = 6;
    const SETTLED: usize = 8;
    let mut costs = Vec::new();
    for _ in 0..DISCARD + SETTLED {
        (case.setup)(&mut machine);
        costs.push(step_one_instruction(&mut machine));
    }
    // The harness must be able to say it ran what it claims. A cost is
    // meaningless if `PC` wandered out of the stream or the bytes were
    // never there.
    let pc = machine.z80().regs.pc;
    if !(CODE_BASE..CODE_BASE + 0x2000).contains(&pc) {
        return Err(format!("execution left the stream, pc = {pc:#06x}"));
    }
    let first_byte = machine.memory().read(CODE_BASE);
    if first_byte != case.bytes[0] {
        return Err(format!(
            "the stream is not in memory: {CODE_BASE:#06x} holds {first_byte:#04x}, \
             expected {:#04x}",
            case.bytes[0]
        ));
    }

    let tail = &costs[DISCARD..];
    if tail.iter().any(|c| *c != tail[0]) {
        return Err(format!("did not settle: {costs:?}"));
    }
    Ok(tail[0])
}

/// Measure across the frame, and fail if the answer moves.
///
/// A varying cost means the ULA is involved, which for uncontended code on
/// an uncontended port it must not be.
fn measure(case: &Case, rom: &[u8]) -> Result<u32, String> {
    let readings: Vec<u32> = FRAME_OFFSETS
        .iter()
        .map(|&skew| measure_at(case, rom, skew))
        .collect::<Result<Vec<u32>, String>>()?;
    let first = readings[0];
    if readings.iter().any(|&r| r != first) {
        return Err(format!(
            "cost varies with frame position ({readings:?}) — the ULA is \
             contending something this case says is uncontended"
        ));
    }
    Ok(first)
}

/// Every instruction must cost what Zilog says, with no ULA involved.
#[test]
#[ignore = "needs EMU198X_SPECTRUM_48K_ROM"]
fn uncontended_costs_match_zilog() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    println!(
        "\n{:<16} {:>8} {:>8} {:>8}",
        "instruction", "measured", "zilog", "delta"
    );
    println!("{}", "-".repeat(46));

    let mut wrong = Vec::new();
    let mut unstable = Vec::new();
    for case in cases() {
        match measure(&case, &rom) {
            Ok(measured) => {
                let delta = measured as i64 - case.zilog as i64;
                println!(
                    "{:<16} {measured:>8} {:>8} {:>+8}",
                    case.name, case.zilog, delta
                );
                if delta != 0 {
                    wrong.push((case.name, measured, case.zilog));
                }
            }
            Err(why) => {
                println!("{:<16} {:>8} {:>8}  {why}", case.name, "varies", case.zilog);
                unstable.push((case.name, why));
            }
        }
    }

    assert!(
        unstable.is_empty(),
        "\n{} case(s) cost different amounts at different frame positions, \
         so this harness is measuring contention rather than instruction \
         timing:\n{}",
        unstable.len(),
        unstable
            .iter()
            .map(|(n, w)| format!("  {n}: {w}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    assert!(
        wrong.is_empty(),
        "\n{} instruction(s) do not cost what Zilog UM0080 says, with no \
         contention involved:\n{}\n\nEach of these is a survey case that no \
         amount of work on the ULA can fix.",
        wrong.len(),
        wrong
            .iter()
            .map(|(n, got, want)| format!("  {n:<16} measured {got}, Zilog {want}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
