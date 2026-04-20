//! Phase 1 characterization tests — TOD counter + alarm.
//!
//! Per HRM Appendix F p.344:
//!   - 24-bit binary counter (8520, not BCD like 6526)
//!   - External pulse pin increments the counter
//!   - "TOD is automatically stopped whenever a write to the
//!     register occurs. The clock will not start again until after
//!     a write to the LSB event register."
//!   - Read-latch: "All TOD registers latch on a read of MSB event
//!     and remain latched until after a read of LSB event."
//!   - CRB bit 7 = 1 routes writes to the alarm; 0 routes to the
//!     counter.
//!   - Alarm equals counter → ICR bit 2 (ALARM) latches.
//!
//! Archive matches this HRM text throughout. Note: vAmiga CIARegs.cpp
//! implements the write-halt only on TODHI ($A), which is 6526-style;
//! 8520 HRM is unambiguous that any TOD write halts.

use mos_cia_8520::Cia8520;

const CRA: u8 = 0x0E;
const CRB: u8 = 0x0F;
const TODLO: u8 = 0x08;
const TODMID: u8 = 0x09;
const TODHI: u8 = 0x0A;
const ICR: u8 = 0x0D;

const ICR_ALARM: u8 = 0x04;

// ────────────────────────────────────────────────────────────────
// Basic counter semantics
// ────────────────────────────────────────────────────────────────

#[test]
fn tod_counts_up_on_each_pulse() {
    let mut cia = Cia8520::new("T");
    assert_eq!(cia.tod_counter(), 0);
    cia.tod_pulse();
    assert_eq!(cia.tod_counter(), 1);
    for _ in 0..10 {
        cia.tod_pulse();
    }
    assert_eq!(cia.tod_counter(), 11);
}

#[test]
fn tod_wraps_at_24_bits() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x00FF_FFFE);
    cia.tod_pulse();
    assert_eq!(cia.tod_counter(), 0x00FF_FFFF);
    cia.tod_pulse();
    assert_eq!(cia.tod_counter(), 0x0000_0000, "wraps at $1000000");
}

#[test]
fn tod_is_binary_not_bcd() {
    // 8520 is binary — $0F + 1 = $10, not $11 (which a BCD 6526
    // would give).
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x000F);
    cia.tod_pulse();
    assert_eq!(cia.tod_counter(), 0x0010);
}

// ────────────────────────────────────────────────────────────────
// Write-halt: any TOD write stops the counter
// ────────────────────────────────────────────────────────────────

#[test]
fn tod_writes_to_todhi_halt_the_counter() {
    let mut cia = Cia8520::new("T");
    cia.write(CRB, 0x00); // target counter
    cia.write(TODHI, 0x12);
    assert!(cia.tod_halted(), "any TOD write halts");
    cia.tod_pulse();
    assert_eq!(
        cia.tod_counter() & 0x00FF_0000,
        0x0012_0000,
        "counter frozen at the written state"
    );
}

#[test]
fn tod_writes_to_todmid_halt_the_counter_8520_specific() {
    // 8520-specific: MID write halts too. 6526 does NOT halt on MID.
    let mut cia = Cia8520::new("T");
    cia.write(CRB, 0x00);
    cia.write(TODMID, 0x34);
    assert!(cia.tod_halted(), "8520: MID write halts (HRM §F)");
}

#[test]
fn tod_write_to_todlo_commits_and_restarts_counter() {
    let mut cia = Cia8520::new("T");
    cia.write(CRB, 0x00);
    cia.write(TODHI, 0x12); // halts
    assert!(cia.tod_halted());
    cia.write(TODMID, 0x34); // still halted
    assert!(cia.tod_halted());
    cia.write(TODLO, 0x56); // commits + restarts
    assert!(!cia.tod_halted(), "LSB write resumes counting");
    assert_eq!(cia.tod_counter(), 0x00123456);
}

#[test]
fn tod_pulses_are_ignored_while_halted() {
    let mut cia = Cia8520::new("T");
    cia.write(CRB, 0x00);
    cia.write(TODHI, 0x01);
    assert!(cia.tod_halted());
    cia.tod_pulse();
    cia.tod_pulse();
    cia.tod_pulse();
    assert_eq!(
        cia.tod_counter() & 0x00FF_0000,
        0x0001_0000,
        "no ticks while halted"
    );
}

// ────────────────────────────────────────────────────────────────
// CRB bit 7 routes TOD writes to alarm
// ────────────────────────────────────────────────────────────────

#[test]
fn tod_writes_target_alarm_when_crb_bit7_set() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x112233);
    cia.write(CRB, 0x80); // alarm-select
    cia.write(TODHI, 0xAA);
    cia.write(TODMID, 0xBB);
    cia.write(TODLO, 0xCC);
    assert_eq!(cia.tod_alarm(), 0x00AA_BBCC);
    assert_eq!(cia.tod_counter(), 0x00112233, "counter untouched");
    assert!(!cia.tod_halted(), "alarm writes never halt counter");
}

#[test]
fn alarm_write_after_counter_halt_does_not_restart_counter() {
    let mut cia = Cia8520::new("T");
    cia.write(CRB, 0x00);
    cia.write(TODHI, 0x12); // halt
    assert!(cia.tod_halted());

    // Switch to alarm-write mode and write all three bytes.
    cia.write(CRB, 0x80);
    cia.write(TODHI, 0xAA);
    cia.write(TODMID, 0xBB);
    cia.write(TODLO, 0xCC);

    assert!(cia.tod_halted(), "alarm writes must not restart a halted counter");
    assert_eq!(cia.tod_alarm(), 0x00AA_BBCC);
}

// ────────────────────────────────────────────────────────────────
// Alarm interrupt: ICR bit 2 latches on equality
// ────────────────────────────────────────────────────────────────

#[test]
fn tod_alarm_equality_latches_icr_alarm_bit() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x00FF);
    // Program alarm = $0100 with alarm-select.
    cia.write(CRB, 0x80);
    cia.write(TODHI, 0x00);
    cia.write(TODMID, 0x01);
    cia.write(TODLO, 0x00);
    assert_eq!(cia.tod_alarm(), 0x0100);
    // Counter ticks 0xFF → 0x100; at that moment alarm matches.
    cia.tod_pulse();
    assert_eq!(cia.tod_counter(), 0x0100);
    assert_ne!(
        cia.icr_status() & ICR_ALARM,
        0,
        "ALARM flag latches on match"
    );
}

#[test]
fn tod_alarm_only_fires_once_per_match_cycle() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0);
    cia.write(CRB, 0x80);
    cia.write(TODLO, 0x02); // alarm = 2
    for _ in 0..2 {
        cia.tod_pulse();
    }
    assert_ne!(cia.icr_status() & ICR_ALARM, 0);

    // Clear ICR; next wrap-around to alarm should re-latch.
    let _ = cia.read(ICR);
    assert_eq!(cia.icr_status() & ICR_ALARM, 0);

    // Wrap all the way back to 2 via 24-bit overflow is expensive —
    // jump via set_tod_counter and verify the match pulse still fires.
    cia.set_tod_counter(1);
    cia.tod_pulse();
    assert_eq!(cia.tod_counter(), 2);
    assert_ne!(
        cia.icr_status() & ICR_ALARM,
        0,
        "alarm re-latches after ICR clear"
    );
}

// ────────────────────────────────────────────────────────────────
// Read-latch: reading MSB freezes, reading LSB releases
// ────────────────────────────────────────────────────────────────

#[test]
fn tod_read_msb_latches_snapshot_until_lsb_read() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x00AA_BBCC);

    // Read MSB — snapshot captured
    let hi = cia.read(TODHI);
    assert_eq!(hi, 0xAA);

    // Advance counter between reads
    for _ in 0..5 {
        cia.tod_pulse();
    }
    assert_eq!(cia.tod_counter(), 0x00AA_BBD1);

    // MID and LO reads return the frozen values, not the live ones
    let mid = cia.read(TODMID);
    assert_eq!(mid, 0xBB, "MID read returns latched snapshot");
    let lo = cia.read(TODLO);
    assert_eq!(lo, 0xCC, "LO read returns latched snapshot (AND releases latch)");
}

#[test]
fn tod_lo_read_after_latch_releases_subsequent_reads_return_live() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x00AA_BBCC);
    let _ = cia.read(TODHI);
    let _ = cia.read(TODLO); // releases latch
    // Advance
    for _ in 0..3 {
        cia.tod_pulse();
    }
    assert_eq!(cia.tod_counter(), 0x00AA_BBCF);
    // Next MID read should see live value
    let mid = cia.read(TODMID);
    assert_eq!(mid, 0xBB);
    let lo = cia.read(TODLO);
    assert_eq!(lo, 0xCF, "live LO after latch released");
}

#[test]
fn tod_mid_or_lo_read_without_prior_msb_read_returns_live() {
    // HRM: if only one register is to be read, it can be read "on
    // the fly" provided any MSB read is followed by an LSB read to
    // disable the latching.
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x001234);
    let lo1 = cia.read(TODLO);
    assert_eq!(lo1, 0x34, "live read when no latch was taken");
    cia.tod_pulse();
    let lo2 = cia.read(TODLO);
    assert_eq!(lo2, 0x35);
}

// ────────────────────────────────────────────────────────────────
// Reset state
// ────────────────────────────────────────────────────────────────

#[test]
fn tod_counter_and_alarm_survive_hardware_reset() {
    // HRM §F: TOD counter and alarm registers are NOT affected by
    // hardware reset. The `reset()` method clears timers, ICR, etc.
    // but preserves TOD state.
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0x00ABCDEF);
    cia.write(CRB, 0x80);
    cia.write(TODLO, 0x42); // alarm LO
    cia.reset();
    assert_eq!(cia.tod_counter(), 0x00ABCDEF, "counter preserved");
    // Alarm preservation is also HRM — check that.
    assert_eq!(cia.tod_alarm() & 0xFF, 0x42);
}
