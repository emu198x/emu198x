//! CIA accesses wait for the E clock.
//!
//! Gary asserts VPA for the CIAs, so the 68000 synchronises each CIA access
//! to the E clock, a tenth of the system clock. WinUAE's cycle-exact model
//! (`cia_wait_pre`/`cia_wait_post`) gives a CIA access 10 to 19 clocks,
//! ending at the same point in the E period every time, where an ordinary
//! cycle takes 4.
//!
//! A synthetic ROM runs `TST.B $BFE001` back to back. Its other accesses
//! (two extension words and the prefetch, all ROM) take 12 clocks, so each
//! instruction must take 22 to 31 clocks and end at one E-clock position.

use machine_commodore_amiga_ocs::AmigaOcs;

const ROM_BASE: u32 = 0x00FC_0000;
const LOOP_START: usize = 0x0100;
const TESTS: usize = 12;

fn put_w(buf: &mut [u8], at: usize, val: u16) {
    buf[at] = (val >> 8) as u8;
    buf[at + 1] = val as u8;
}

fn put_l(buf: &mut [u8], at: usize, val: u32) {
    put_w(buf, at, (val >> 16) as u16);
    put_w(buf, at + 2, val as u16);
}

fn build_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    put_l(&mut rom, 0x0000, 0x0000_8000);
    put_l(&mut rom, 0x0004, ROM_BASE + LOOP_START as u32);
    let mut at = LOOP_START;
    for _ in 0..TESTS {
        // TST.B $00BFE001
        put_w(&mut rom, at, 0x4A39);
        put_l(&mut rom, at + 2, 0x00BF_E001);
        at += 6;
    }
    // BRA.W back to the first TST.
    put_w(&mut rom, at, 0x6000);
    put_w(
        &mut rom,
        at + 2,
        (LOOP_START as i32 - (at as i32 + 2)) as u16,
    );
    rom
}

#[test]
fn cia_reads_wait_for_the_e_clock() {
    let mut amiga = AmigaOcs::new(build_rom());
    for _ in 0..20_000 {
        amiga.tick();
    }
    let boundaries: Vec<_> = amiga.drain_cpu_boundaries().collect();
    let first_test = ROM_BASE + LOOP_START as u32;
    let last_test = first_test + 6 * (TESTS as u32 - 1);

    let mut durations = Vec::new();
    let mut end_phases = Vec::new();
    for pair in boundaries.windows(2) {
        let (start, end) = (&pair[0], &pair[1]);
        if (first_test..=last_test).contains(&start.instr_start_pc) && start.opcode == 0x4A39 {
            durations.push(end.system_tick - start.system_tick);
            end_phases.push(end.system_tick % 10);
        }
    }

    assert!(
        durations.len() > 50,
        "too few CIA reads timed: {}",
        durations.len()
    );
    assert!(
        durations.iter().all(|d| (22..=31).contains(d)),
        "TST.B of a CIA register should take 22 to 31 clocks: {durations:?}"
    );
    assert!(
        end_phases.windows(2).all(|w| w[0] == w[1]),
        "each CIA read should end at the same E-clock position: {end_phases:?}"
    );
}
