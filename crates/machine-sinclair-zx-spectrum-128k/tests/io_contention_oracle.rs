//! 128K port timing against FUSE 1.7.0's ula_contend_port_early/late.
//! Adapted from the 48K arrival-resolved oracle. No ROM fixture is needed:
//! a synthetic ED 78/79 stream runs with interrupts disabled. Both the
//! fixed contended page and the paged bank's odd/even classification matter.
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::TIMING_128K;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;

const FRAME: u32 = 70_908;
// FUSE first-data timestamp 14364 minus physical C8/T4; not an IRQ origin.
const PATTERN_ORIGIN: u32 = 14_360;

fn delay(t: u32) -> u32 {
    let t = t % FRAME;
    if t < 14_361 {
        return 0;
    }
    let d = t - 14_361;
    if d / 228 >= 192 || d % 228 >= 128 {
        return 0;
    }
    [6, 5, 4, 3, 2, 1, 0, 0][(d % 228 % 8) as usize]
}

fn port_contended(port: u16, bank: u8) -> bool {
    port >> 14 == 1 || (port >> 14 == 3 && bank & 1 != 0)
}

fn reference_cost(start: u32, port: u16, bank: u8) -> u32 {
    let mut t = start;
    // Two contended M1 fetches, then FUSE's early and late port branches.
    for _ in 0..2 {
        t += delay(t) + 4;
    }
    if port_contended(port, bank) {
        t += delay(t);
    }
    t += 1;
    if port & 1 == 0 {
        t += delay(t) + 3;
    } else if port_contended(port, bank) {
        for _ in 0..3 {
            t += delay(t) + 1;
        }
    } else {
        t += 3;
    }
    t - start
}

fn retire(machine: &mut Spectrum128K) -> u32 {
    let end = machine.z80.instructions_retired() + 1;
    for cost in 1..=512 {
        machine.advance_tstates(1);
        if machine.z80.instructions_retired() == end {
            return cost;
        }
    }
    panic!("instruction did not retire");
}

fn score(span: u32, skews: &[u32]) {
    let mut wrong = 0;
    let mut total = 0;
    for bank in [0, 1, 2, 3] {
        for port in [0x00fe, 0x00ff, 0x40fe, 0x40ff, 0xc0fe, 0xc0ff] {
            for opcode in [0x78, 0x79] {
                let mut class_wrong = 0;
                for &skew in skews {
                    let mut m = Spectrum128K::new();
                    // Lock paging so OUT cannot alter the test's bank mapping.
                    m.memory.write_7ffd(0x20 | bank);
                    for address in (0x4000..0x8000).step_by(2) {
                        m.memory.write(address, 0xed);
                        m.memory.write(address + 1, opcode);
                    }
                    m.advance_tstates(skew);
                    m.z80.regs.pc = 0x4000;
                    m.z80.regs.bc = port;
                    // Flush any in-flight reset-ROM fetch after redirecting PC.
                    retire(&mut m);
                    retire(&mut m);
                    let mut elapsed = 0;
                    while elapsed < span {
                        assert!((0x4000..0x8000).contains(&m.z80.regs.pc));
                        let arrival = m.frame_position().tstate(&TIMING_128K);
                        let actual = retire(&mut m);
                        let expected = reference_cost(arrival + PATTERN_ORIGIN, port, bank);
                        if actual != expected {
                            if wrong < 8 {
                                eprintln!(
                                    "bank={bank} port={port:04x} op={opcode:02x} arrival={arrival} actual={actual} expected={expected}"
                                );
                            }
                            wrong += 1;
                            class_wrong += 1;
                        }
                        elapsed += actual;
                        total += 1;
                    }
                }
                if class_wrong != 0 {
                    eprintln!(
                        "bank={bank} port={port:04x} op={opcode:02x}: {class_wrong} mismatches"
                    );
                }
            }
        }
    }
    eprintln!("128K I/O: {wrong}/{total} mismatches");
    assert_eq!(
        wrong, 0,
        "port timing must match independently classified FUSE costs"
    );
}

#[test]
fn port_costs_cover_both_bank_parities_and_all_four_classes() {
    score(456, &[0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
#[ignore = "SLOW: full-frame arrival sweep; run with --release --ignored --nocapture"]
fn port_costs_match_across_the_frame() {
    score(FRAME, &[0, 1, 2, 3, 4, 5, 6, 7]);
}
