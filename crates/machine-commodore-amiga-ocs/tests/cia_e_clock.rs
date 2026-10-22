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

/// Where a CIA access falls against the CIA's own count matters to code
/// that reads a timer. vAmiga (`Agnus::syncWithEClock`, `CIA::wakeUp`) and
/// WinUAE (`cia_wait_pre`/`cia_wait_post`, `CIA_update_check`) agree: the
/// read sees the count it coincides with, and the access ends six clocks
/// after that count.
///
/// The ROM starts CIA-A timer A free-running from $FFFF, then reads its low
/// byte in a loop. Each read must end six clocks after the timer last
/// counted and return the value the timer then holds.
#[test]
fn cia_reads_end_six_clocks_after_the_cia_counts() {
    use motorola_68000::cpu::State;

    let mut rom = vec![0u8; 256 * 1024];
    put_l(&mut rom, 0x0000, 0x0000_8000);
    put_l(&mut rom, 0x0004, ROM_BASE + LOOP_START as u32);
    let mut at = LOOP_START;
    // MOVE.B #imm,(xxx).l: TALO = $FF, TAHI = $FF, CRA = START.
    for (value, addr) in [
        (0x00FF, 0x00BF_E401),
        (0x00FF, 0x00BF_E501),
        (0x0001, 0x00BF_EE01),
    ] {
        put_w(&mut rom, at, 0x13FC);
        put_w(&mut rom, at + 2, value);
        put_l(&mut rom, at + 4, addr);
        at += 8;
    }
    let loop_at = at;
    for _ in 0..8 {
        // MOVE.B $00BFE401,D0
        put_w(&mut rom, at, 0x1039);
        put_l(&mut rom, at + 2, 0x00BF_E401);
        at += 6;
    }
    put_w(&mut rom, at, 0x6000);
    put_w(&mut rom, at + 2, (loop_at as i32 - (at as i32 + 2)) as u16);

    let mut amiga = AmigaOcs::new(rom);
    let mut timer = amiga.cia_a().peek(4);
    let mut last_count = 0u64;
    let mut reading = false;
    let mut pending: Option<u8> = None;
    let mut checked = 0;
    for tick in 0..20_000u64 {
        amiga.tick();
        let now = amiga.cia_a().peek(4);
        if now != timer {
            timer = now;
            last_count = tick;
        }
        let in_read = matches!(
            amiga.cpu().state,
            State::BusCycle { addr, .. } if addr & 0x00FF_FFFF == 0x00BF_E401
        );
        if in_read && !reading {
            // The previous read has reached D0 by the time the next begins.
            if let Some(value) = pending.take() {
                assert_eq!(amiga.cpu().regs.d[0] & 0xFF, u32::from(value));
                checked += 1;
            }
        }
        if reading && !in_read && tick > 1_000 {
            assert_eq!(
                tick - last_count,
                6,
                "a CIA read should end six clocks after the count"
            );
            pending = Some(timer);
        }
        reading = in_read;
    }
    assert!(checked > 100, "too few timer reads checked: {checked}");
}

/// CIA space as the Hardware Reference Manual (appendix F) decodes it: the
/// whole $A00000-$BFFFFF region, CIA-A on data bits 7-0 when A12 is low,
/// CIA-B on bits 15-8 when A13 is low, both when both are low.
#[test]
fn cia_space_follows_the_hardware_reference_manual() {
    let mut rom = vec![0u8; 256 * 1024];
    put_l(&mut rom, 0x0000, 0x0000_8000);
    put_l(&mut rom, 0x0004, ROM_BASE + LOOP_START as u32);
    let mut at = LOOP_START;
    let mut emit = |rom: &mut Vec<u8>, words: &[u16]| {
        for &word in words {
            put_w(rom, at, word);
            at += 2;
        }
    };
    // MOVE.B #$55,$00A0E301: CIA-A DDRB through a mirror address.
    emit(&mut rom, &[0x13FC, 0x0055, 0x00A0, 0xE301]);
    // MOVE.B #$AA,$00B4D300: CIA-B DDRB through another.
    emit(&mut rom, &[0x13FC, 0x00AA, 0x00B4, 0xD300]);
    // MOVE.W $00BFD300,D1: CIA-B on the high byte; CIA-A unselected.
    emit(&mut rom, &[0x3239, 0x00BF, 0xD300]);
    // MOVE.B $00BFC301,D2: both selected; the odd byte is CIA-A's.
    emit(&mut rom, &[0x1439, 0x00BF, 0xC301]);
    // MOVE.B #$0F,$00BFC201: both selected, so both DDRAs take it.
    emit(&mut rom, &[0x13FC, 0x000F, 0x00BF, 0xC201]);
    // BRA.S *
    emit(&mut rom, &[0x60FE]);

    let mut amiga = AmigaOcs::new(rom);
    for _ in 0..2_000 {
        amiga.tick();
    }
    assert_eq!(amiga.cia_a().peek(3), 0x55, "CIA-A DDRB via $A0E301");
    assert_eq!(amiga.cia_b().peek(3), 0xAA, "CIA-B DDRB via $B4D300");
    assert_eq!(
        amiga.cpu().regs.d[1] >> 8 & 0xFF,
        0xAA,
        "word read: CIA-B high byte"
    );
    assert_eq!(
        amiga.cpu().regs.d[2] & 0xFF,
        0x55,
        "both selected: odd byte is CIA-A"
    );
    assert_eq!(
        amiga.cia_a().peek(2),
        0x0F,
        "both selected: CIA-A DDRA written"
    );
    assert_eq!(
        amiga.cia_b().peek(2),
        0x0F,
        "both selected: CIA-B DDRA written"
    );
}
