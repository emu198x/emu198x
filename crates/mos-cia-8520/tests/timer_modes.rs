//! Phase 1 characterization tests — Timer A and Timer B all modes.
//!
//! These tests describe MOS 8520 Timer A / Timer B hardware behaviour
//! as known from the Amiga HRM Appendix F and the 8520 datasheet,
//! cross-referenced against WinUAE `cia.cpp` and vAmiga `CIARegs.cpp`.
//!
//! They run against the -archive crate as ground truth. Every test
//! here must pass; failures would indicate an archive bug to fix
//! BEFORE Phase 2 port begins. When Phase 2 ports the code into the
//! live tree, these same tests are re-used to verify parity.
//!
//! Coverage:
//!   - One-shot vs continuous mode (CRx bit 3 / RUNMODE)
//!   - PHI2 (E-clock) vs CNT count source (CRA bit 5 / CRB bits 6-5)
//!   - Timer B cascade on Timer A underflow (CRB bits 6-5 = 10)
//!   - Timer B CNT-gated cascade (CRB bits 6-5 = 11)
//!   - LOAD strobe (CRx bit 4) — writes to counter, reads back as 0
//!   - 8520-only TxHI-in-one-shot-mode auto-start (Amiga HRM)
//!   - Underflow timing ($0000 visible one cycle before flag set)
//!   - Timer read-latch: reading LSB latches MSB until MSB read
//!   - START auto-clear on one-shot underflow
//!   - TxLO/TxHI writes while running vs stopped
//!   - ICR flag latches (TA bit 0, TB bit 1) on underflow

use mos_cia_8520::Cia8520;

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

const CRA: u8 = 0x0E;
const CRB: u8 = 0x0F;
const TALO: u8 = 0x04;
const TAHI: u8 = 0x05;
const TBLO: u8 = 0x06;
const TBHI: u8 = 0x07;
const ICR: u8 = 0x0D;

/// CRx common bits
const START: u8 = 0x01;
const ONESHOT: u8 = 0x08;
const LOAD: u8 = 0x10;
const INMODE_CNT: u8 = 0x20; // CRA bit 5

/// ICR flag bits
const ICR_TA: u8 = 0x01;
const ICR_TB: u8 = 0x02;

fn program_timer_a(cia: &mut Cia8520, value: u16) {
    cia.write(TALO, value as u8);
    cia.write(TAHI, (value >> 8) as u8);
}

fn program_timer_b(cia: &mut Cia8520, value: u16) {
    cia.write(TBLO, value as u8);
    cia.write(TBHI, (value >> 8) as u8);
}

fn tick_n(cia: &mut Cia8520, n: usize) {
    for _ in 0..n {
        cia.phi2_pulse();
    }
}

// ────────────────────────────────────────────────────────────────
// Timer A — continuous mode, PHI2 source
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_a_continuous_counts_down_on_each_phi2_tick() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 5);
    cia.write(CRA, LOAD); // load then stop
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 5);
    cia.write(CRA, START); // continuous, PHI2, running
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 4);
    tick_n(&mut cia, 3);
    assert_eq!(cia.timer_a(), 1);
}

#[test]
fn timer_a_continuous_underflow_reloads_from_latch() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 2);
    cia.write(CRA, LOAD | START);
    // 2 → 1 → 0 → underflow reloads to 2. Archive models the "$0000
    // visible for one cycle before flag" rule so reload takes 3 ticks.
    tick_n(&mut cia, 3);
    assert_eq!(cia.timer_a(), 2, "continuous mode reloads latch");
    assert_ne!(cia.icr_status() & ICR_TA, 0, "TA flag latched on underflow");
    assert!(
        cia.timer_a_running(),
        "continuous mode keeps the timer running"
    );
}

#[test]
fn timer_a_continuous_latch_update_affects_next_reload() {
    // Writing TxLO / TxHI while running updates the LATCH only —
    // the live counter keeps ticking until it underflows.
    //
    // Timing note: on the tick where LOAD strobe is applied and
    // START is set simultaneously, the counter first reloads from
    // latch THEN decrements — so after the first tick, counter =
    // latch - 1.
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 5);
    cia.write(CRA, LOAD | START);
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 4, "LOAD+decrement on same tick");
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 3);
    // Change latch to 10 mid-flight. The counter keeps counting the
    // old value (3 → 2 → 1 → 0 → reload from NEW latch = 10).
    program_timer_a(&mut cia, 10);
    assert_eq!(
        cia.timer_a(),
        3,
        "counter not affected by latch write while running"
    );
    tick_n(&mut cia, 4);
    assert_eq!(cia.timer_a(), 10, "underflow reloads from updated latch");
}

// ────────────────────────────────────────────────────────────────
// Timer A — one-shot mode
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_a_oneshot_stops_on_underflow_and_clears_start_bit() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 2);
    cia.write(CRA, LOAD | START | ONESHOT);
    tick_n(&mut cia, 3);
    assert_eq!(
        cia.timer_a(),
        2,
        "counter reloaded to latch after one-shot underflow"
    );
    assert!(!cia.timer_a_running(), "one-shot auto-stops");
    assert_eq!(cia.read(CRA) & START, 0, "START bit cleared automatically");
    assert_ne!(cia.icr_status() & ICR_TA, 0, "TA flag still latches");
}

#[test]
fn timer_a_oneshot_txhi_autostart_without_start_bit() {
    // 8520-only Amiga-HRM quirk: in one-shot mode, a write to TxHI
    // transfers latch → counter AND starts the timer, regardless of
    // the START bit. This is what rescues KS 1.3's timer.device
    // UNIT_MICROHZ re-arming from the TB interrupt handler.
    let mut cia = Cia8520::new();
    cia.write(CRA, ONESHOT); // one-shot, stopped
    assert!(!cia.timer_a_running());
    program_timer_a(&mut cia, 0x0003);
    assert!(
        cia.timer_a_running(),
        "TAHI write in one-shot mode auto-starts timer"
    );
    assert_ne!(cia.read(CRA) & START, 0, "START bit reads back as 1");
    assert_eq!(cia.timer_a(), 3);
    tick_n(&mut cia, 4); // 3 → 2 → 1 → 0 → reload to 3, stop
    assert!(
        !cia.timer_a_running(),
        "auto-started one-shot still self-stops"
    );
    assert_ne!(cia.icr_status() & ICR_TA, 0);
}

#[test]
fn timer_a_oneshot_txhi_does_not_autostart_when_running() {
    // Auto-start only applies while stopped. If the timer is already
    // running, TxHI updates only the latch — doesn't force-reload.
    // (First tick after LOAD+START sets counter = latch-1 due to
    // same-tick load-then-count semantics.)
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 100);
    cia.write(CRA, LOAD | START | ONESHOT);
    cia.phi2_pulse(); // running, counter = 99
    assert_eq!(cia.timer_a(), 99);
    program_timer_a(&mut cia, 50); // latch changes, counter unchanged
    assert_eq!(cia.timer_a(), 99, "counter NOT force-reloaded mid-flight");
}

// ────────────────────────────────────────────────────────────────
// LOAD strobe
// ────────────────────────────────────────────────────────────────

#[test]
fn load_strobe_forces_counter_from_latch_and_reads_back_as_zero() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 0x1234);
    assert_eq!(
        cia.timer_a(),
        0x1234,
        "TAHI write loads counter while stopped"
    );
    cia.write(TALO, 0xFF);
    cia.write(TAHI, 0xFF); // latch = $FFFF
    cia.write(CRA, LOAD); // strobe only
    cia.phi2_pulse(); // apply_timer_force_loads()
    assert_eq!(cia.timer_a(), 0xFFFF);
    assert_eq!(cia.read(CRA) & LOAD, 0, "LOAD strobe reads back as 0");
}

#[test]
fn load_strobe_while_running_reloads_without_stopping() {
    // LOAD-strobe while running: on the tick after the strobe, the
    // counter first loads from latch and then decrements (same-tick
    // semantics). So after the strobe-tick, counter = latch - 1.
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 5);
    cia.write(CRA, LOAD | START);
    tick_n(&mut cia, 3); // 4 → 3 → 2 after three ticks
    assert_eq!(cia.timer_a(), 2);
    cia.write(CRA, START | LOAD); // re-strobe LOAD, stay running
    cia.phi2_pulse();
    assert_eq!(
        cia.timer_a(),
        4,
        "LOAD reloads then counts in the same tick"
    );
    assert!(cia.timer_a_running(), "still running");
}
#[test]
// ────────────────────────────────────────────────────────────────
// Timer A — CNT source (CRA bit 5 = 1)
// ────────────────────────────────────────────────────────────────
#[test]
fn timer_a_cnt_mode_ignores_phi2_and_counts_cnt_pulses() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 3);
    cia.write(CRA, LOAD | START | INMODE_CNT);
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 3, "PHI2 tick ignored in CNT mode");
    cia.cnt_pulse();
    assert_eq!(cia.timer_a(), 2);
    cia.cnt_pulse();
    assert_eq!(cia.timer_a(), 1);
    cia.cnt_pulse();
    assert_eq!(cia.timer_a(), 0);
    cia.cnt_pulse();
    assert_eq!(cia.timer_a(), 3, "CNT underflow reloads latch");
    assert_ne!(cia.icr_status() & ICR_TA, 0);
}

// ────────────────────────────────────────────────────────────────
// Timer B — basic, PHI2 source (CRB bits 6:5 = 00)
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_b_continuous_counts_down_on_phi2() {
    let mut cia = Cia8520::new();
    program_timer_b(&mut cia, 5);
    cia.write(CRB, LOAD | START);
    tick_n(&mut cia, 2);
    assert_eq!(cia.timer_b(), 3);
}

#[test]
fn timer_b_oneshot_stops_on_underflow() {
    let mut cia = Cia8520::new();
    program_timer_b(&mut cia, 1);
    cia.write(CRB, LOAD | START | ONESHOT);
    tick_n(&mut cia, 2); // 1 → 0 → reload+stop
    assert!(!cia.timer_b_running());
    assert_ne!(cia.icr_status() & ICR_TB, 0);
}

#[test]
fn timer_b_oneshot_txhi_autostart() {
    let mut cia = Cia8520::new();
    cia.write(CRB, ONESHOT);
    assert!(!cia.timer_b_running());
    program_timer_b(&mut cia, 2);
    assert!(cia.timer_b_running(), "TBHI in one-shot mode auto-starts");
}

// ────────────────────────────────────────────────────────────────
// Timer B — CNT source (CRB bits 6:5 = 01)
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_b_cnt_mode_counts_on_cnt_pulses_only() {
    let mut cia = Cia8520::new();
    program_timer_b(&mut cia, 2);
    // CRB bits 6:5 = 01 → CNT source; bit 0 = START
    cia.write(CRB, LOAD | START | 0x20);
    cia.phi2_pulse();
    assert_eq!(cia.timer_b(), 2, "PHI2 tick ignored");
    cia.cnt_pulse();
    assert_eq!(cia.timer_b(), 1);
    cia.cnt_pulse();
    assert_eq!(cia.timer_b(), 0);
    cia.cnt_pulse();
    assert_eq!(cia.timer_b(), 2);
}

// ────────────────────────────────────────────────────────────────
// Timer B — Timer A underflow cascade (CRB bits 6:5 = 10)
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_b_cascade_mode_counts_only_on_timer_a_underflow() {
    let mut cia = Cia8520::new();

    // Timer A: continuous, PHI2, very fast — underflows every 2
    // ticks after initial load of 1.
    program_timer_a(&mut cia, 1);
    cia.write(CRA, LOAD | START);

    // Timer B: cascade on TA underflow (CRB bits 6:5 = 10).
    program_timer_b(&mut cia, 3);
    cia.write(CRB, LOAD | START | 0x40);

    cia.phi2_pulse(); // TA: 1 → 0, no underflow yet, TB unchanged
    assert_eq!(cia.timer_b(), 3);
    cia.phi2_pulse(); // TA underflow, reload to 1; TB: 3 → 2
    assert_eq!(cia.timer_b(), 2);
    cia.phi2_pulse(); // TA: 1 → 0
    assert_eq!(cia.timer_b(), 2);
    cia.phi2_pulse(); // TA underflow; TB: 2 → 1
    assert_eq!(cia.timer_b(), 1);
}

// ────────────────────────────────────────────────────────────────
// Timer A one-shot underflow: $0000 visible for one tick before flag
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_a_oneshot_zero_visible_for_one_tick_before_flag() {
    // Per 8520 datasheet: $0000 appears on the bus for one cycle
    // before the underflow flag fires and the reload happens. The
    // archive models this by returning 0 on the zero-tick, then
    // raising the flag on the next tick.
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 2);
    cia.write(CRA, LOAD | START | ONESHOT);
    cia.phi2_pulse(); // 2 → 1
    assert_eq!(cia.timer_a(), 1);
    assert_eq!(cia.icr_status() & ICR_TA, 0, "no flag yet");
    cia.phi2_pulse(); // 1 → 0
    assert_eq!(cia.timer_a(), 0, "$0000 visible before flag");
    assert_eq!(cia.icr_status() & ICR_TA, 0, "no flag on zero-tick");
    cia.phi2_pulse(); // underflow fires flag + reload
    assert_ne!(
        cia.icr_status() & ICR_TA,
        0,
        "flag raised on the tick AFTER zero"
    );
    assert_eq!(cia.timer_a(), 2, "reloaded from latch");
}

// ────────────────────────────────────────────────────────────────
// Timer read-latch (LSB read latches MSB until MSB read)
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_a_lsb_read_latches_msb_for_atomic_read() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 0xABCD);
    cia.write(CRA, LOAD); // stopped, loaded
    cia.phi2_pulse(); // apply force-load
    assert_eq!(cia.timer_a(), 0xABCD);

    // Read LSB — hi byte snapshot frozen
    let lo = cia.read(TALO);
    assert_eq!(lo, 0xCD);

    // Change the live counter (via programming a new value after LOAD
    // strobe). Read of MSB should still return the frozen value.
    // Counter stays stopped so program new latch + LOAD strobe.
    program_timer_a(&mut cia, 0x1234);
    cia.write(CRA, LOAD);
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 0x1234);

    let hi = cia.read(TAHI);
    assert_eq!(hi, 0xAB, "read returns latched MSB from earlier LSB read");

    // After MSB read, latch is released; further MSB reads are live.
    let hi2 = cia.read(TAHI);
    assert_eq!(hi2, 0x12, "live MSB after latch release");
}

#[test]
fn timer_b_lsb_read_latches_msb_independently_of_timer_a() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 0x1111);
    cia.write(CRA, LOAD);
    program_timer_b(&mut cia, 0x2222);
    cia.write(CRB, LOAD);
    cia.phi2_pulse(); // apply loads

    cia.read(TALO); // latch MSB-A
    let _ = cia.read(TBLO); // latch MSB-B — independent from A

    // B read uses B's own latch
    let hi_b = cia.read(TBHI);
    assert_eq!(hi_b, 0x22);
    // A's latch survives — independent
    let hi_a = cia.read(TAHI);
    assert_eq!(hi_a, 0x11);
}

// ────────────────────────────────────────────────────────────────
// START bit semantics
// ────────────────────────────────────────────────────────────────

#[test]
fn start_bit_off_stops_ticking() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 5);
    cia.write(CRA, LOAD | START);
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 4);
    cia.write(CRA, 0); // stop
    cia.phi2_pulse();
    assert_eq!(cia.timer_a(), 4, "counter frozen when stopped");
}

#[test]
fn stopped_timer_txhi_loads_counter_without_starting_in_continuous_mode() {
    // Only one-shot mode auto-starts on TxHI write. Continuous mode
    // does NOT auto-start; the value just lands in the counter.
    let mut cia = Cia8520::new();
    cia.write(CRA, 0); // continuous, stopped
    program_timer_a(&mut cia, 0x0042);
    assert!(
        !cia.timer_a_running(),
        "continuous mode: TAHI must NOT auto-start"
    );
    assert_eq!(cia.timer_a(), 0x0042, "counter did get loaded");
}

// ────────────────────────────────────────────────────────────────
// ICR flag-latching timing
// ────────────────────────────────────────────────────────────────

#[test]
fn timer_a_icr_flag_persists_until_icr_read() {
    let mut cia = Cia8520::new();
    program_timer_a(&mut cia, 1);
    cia.write(CRA, LOAD | START | ONESHOT);
    tick_n(&mut cia, 2); // underflow
    assert_ne!(cia.icr_status() & ICR_TA, 0);
    tick_n(&mut cia, 10);
    assert_ne!(cia.icr_status() & ICR_TA, 0, "flag not self-clearing");
    let _ = cia.read(ICR); // read-clear
    assert_eq!(cia.icr_status() & ICR_TA, 0, "cleared after ICR read");
}

#[test]
fn timer_b_icr_flag_sets_on_underflow_independent_of_timer_a() {
    let mut cia = Cia8520::new();
    program_timer_b(&mut cia, 1);
    cia.write(CRB, LOAD | START | ONESHOT);
    tick_n(&mut cia, 2);
    assert_eq!(cia.icr_status() & ICR_TB, ICR_TB, "TB flag only");
    assert_eq!(cia.icr_status() & ICR_TA, 0, "TA unaffected");
}
