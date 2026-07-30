//! Read-only diagnostic snapshot coverage.

use mos_cia_8520::{
    Cia8520,
    bits::{CR_LOAD, CR_OUTMODE, CR_PBON, CR_RUNMODE, CR_START, CRB_ALARM_SELECT, ICR_IR, ICR_SP},
};

const PRA: u8 = 0x00;
const PRB: u8 = 0x01;
const DDRA: u8 = 0x02;
const DDRB: u8 = 0x03;
const TA_LO: u8 = 0x04;
const TA_HI: u8 = 0x05;
const TB_LO: u8 = 0x06;
const TB_HI: u8 = 0x07;
const TOD_LO: u8 = 0x08;
const TOD_MID: u8 = 0x09;
const TOD_HI: u8 = 0x0A;
const ICR: u8 = 0x0D;
const CRA: u8 = 0x0E;
const CRB: u8 = 0x0F;

#[test]
fn default_diagnostic_snapshot_reports_every_power_on_field() {
    let cia = Cia8520::new();
    let snapshot = cia.diagnostic_snapshot();

    assert_eq!(snapshot.port_a, 0xFF);
    assert_eq!(snapshot.port_b, 0xFF);
    assert_eq!(snapshot.ddr_a, 0);
    assert_eq!(snapshot.ddr_b, 0);
    assert_eq!(snapshot.external_a, 0xFF);
    assert_eq!(snapshot.external_b, 0xFF);
    assert_eq!(snapshot.timer_a, 0xFFFF);
    assert_eq!(snapshot.timer_a_latch, 0xFFFF);
    assert!(!snapshot.timer_a_running);
    assert!(!snapshot.timer_a_oneshot);
    assert!(!snapshot.timer_a_force_load);
    assert_eq!(snapshot.timer_b, 0xFFFF);
    assert_eq!(snapshot.timer_b_latch, 0xFFFF);
    assert!(!snapshot.timer_b_running);
    assert!(!snapshot.timer_b_oneshot);
    assert!(!snapshot.timer_b_force_load);
    assert_eq!(snapshot.icr_status, 0);
    assert_eq!(snapshot.icr_mask, 0);
    assert_eq!(snapshot.cra, 0);
    assert_eq!(snapshot.crb, 0);
    assert!(!snapshot.pb6_out);
    assert!(!snapshot.pb7_out);
    assert_eq!(snapshot.sdr, 0);
    assert_eq!(snapshot.tod_counter, 0);
    assert_eq!(snapshot.tod_alarm, 0);
    assert_eq!(snapshot.tod_latch, 0);
    assert!(!snapshot.tod_latched);
    assert_eq!(snapshot.timer_a_read_hi_latch, 0);
    assert!(!snapshot.timer_a_read_hi_latched);
    assert_eq!(snapshot.timer_b_read_hi_latch, 0);
    assert!(!snapshot.timer_b_read_hi_latched);
    assert!(!snapshot.tod_halted);
    assert_eq!(snapshot.port_a_output, 0xFF);
    assert_eq!(snapshot.port_b_output, 0xFF);
    assert!(!snapshot.irq_active);
    assert!(!snapshot.tod_write_targets_alarm);
}

#[test]
fn diagnostic_snapshot_preserves_hidden_latches_and_has_no_read_side_effects() {
    let mut cia = Cia8520::new();

    cia.set_external_a(0x0F);
    cia.set_external_b(0xF0);
    cia.write(PRA, 0xA5);
    cia.write(PRB, 0x5A);
    cia.write(DDRA, 0xF0);
    cia.write(DDRB, 0x0F);

    cia.write(TA_LO, 0x34);
    cia.write(TA_HI, 0x12);
    assert_eq!(cia.read(TA_LO), 0x34);
    cia.write(CRA, CR_START | CR_PBON | CR_OUTMODE | CR_RUNMODE | CR_LOAD);

    cia.write(TB_LO, 0xCD);
    cia.write(TB_HI, 0xAB);
    assert_eq!(cia.read(TB_LO), 0xCD);

    cia.write(TOD_HI, 0x11);
    cia.write(TOD_MID, 0x22);
    cia.write(TOD_LO, 0x33);
    assert_eq!(cia.read(TOD_HI), 0x11);

    cia.write(
        CRB,
        CRB_ALARM_SELECT | CR_START | CR_PBON | CR_OUTMODE | CR_RUNMODE | CR_LOAD,
    );
    cia.write(TOD_HI, 0xAA);
    cia.write(TOD_MID, 0xBB);
    cia.write(TOD_LO, 0xCC);
    cia.receive_serial_byte(0xA5);
    cia.write(ICR, ICR_IR | ICR_SP);

    let snapshot = cia.diagnostic_snapshot();
    assert_eq!(snapshot.port_a, 0xA5);
    assert_eq!(snapshot.port_b, 0x5A);
    assert_eq!(snapshot.ddr_a, 0xF0);
    assert_eq!(snapshot.ddr_b, 0x0F);
    assert_eq!(snapshot.external_a, 0x0F);
    assert_eq!(snapshot.external_b, 0xF0);
    assert_eq!(snapshot.timer_a, 0x1234);
    assert_eq!(snapshot.timer_a_latch, 0x1234);
    assert!(snapshot.timer_a_running);
    assert!(snapshot.timer_a_oneshot);
    assert!(snapshot.timer_a_force_load);
    assert_eq!(snapshot.timer_b, 0xABCD);
    assert_eq!(snapshot.timer_b_latch, 0xABCD);
    assert!(snapshot.timer_b_running);
    assert!(snapshot.timer_b_oneshot);
    assert!(snapshot.timer_b_force_load);
    assert_eq!(snapshot.icr_status, ICR_SP);
    assert_eq!(snapshot.icr_mask, ICR_SP);
    assert_eq!(snapshot.cra, CR_START | CR_PBON | CR_OUTMODE | CR_RUNMODE);
    assert_eq!(
        snapshot.crb,
        CRB_ALARM_SELECT | CR_START | CR_PBON | CR_OUTMODE | CR_RUNMODE
    );
    assert!(snapshot.pb6_out);
    assert!(snapshot.pb7_out);
    assert_eq!(snapshot.sdr, 0xA5);
    assert_eq!(snapshot.tod_counter, 0x112233);
    assert_eq!(snapshot.tod_alarm, 0xAABBCC);
    assert_eq!(snapshot.tod_latch, 0x112233);
    assert!(snapshot.tod_latched);
    assert_eq!(snapshot.timer_a_read_hi_latch, 0x12);
    assert!(snapshot.timer_a_read_hi_latched);
    assert_eq!(snapshot.timer_b_read_hi_latch, 0xAB);
    assert!(snapshot.timer_b_read_hi_latched);
    assert!(!snapshot.tod_halted);
    assert_eq!(snapshot.port_a_output, 0xAF);
    assert_eq!(snapshot.port_b_output, 0xFA);
    assert!(snapshot.irq_active);
    assert!(snapshot.tod_write_targets_alarm);

    assert_eq!(cia.diagnostic_snapshot(), snapshot);
    assert_eq!(cia.icr_status(), ICR_SP, "snapshot must not clear ICR");
}
