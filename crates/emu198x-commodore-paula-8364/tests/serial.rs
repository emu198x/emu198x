//! Phase 1 characterization tests — Paula serial UART.
//!
//! Per HRM §6 (Serial Port Hardware). Paula owns a byte-level UART
//! addressed at \$018 (SERDATR read), \$030 (SERDAT write), and \$032
//! (SERPER write). Two interrupt sources:
//!   INT_TBE  (bit 0)  — transmit buffer empty
//!   INT_RBF  (bit 11) — receive buffer full
//!
//! SERDATR bit layout:
//!   bit 15  OVRUN — set when a new byte arrives before the CPU reads
//!                    the previous one; cleared on SERDATR read.
//!   bit 14  RBF   — receive buffer full; mirrors INTREQ.RBF; clears
//!                    on SERDATR read.
//!   bit 13  TBE   — transmit buffer empty (always set in our model).
//!   bit 12  TSRE  — transmit shift register empty (always set).
//!   bits 7-0      — received data.
//!
//! Task #128 is first-implementation, not a port (serial never lived
//! in the Paula archive). These tests describe the expected
//! contract; together with the machine-level tests in
//! `paula_phase2_machine.rs` they pin it down.

use emu198x_commodore_paula_8364::{IntSource, Paula8364, bits::*};

#[test]
fn serdatr_default_has_tbe_and_tsre_set() {
    // HRM: transmitter is idle at reset — both TBE and TSRE read back
    // as set so a driver's "wait until TBE" loop passes immediately.
    let p = Paula8364::new();
    assert_ne!(p.peek_serdatr() & SERDATR_TBE, 0);
    assert_ne!(p.peek_serdatr() & SERDATR_TSRE, 0);
    assert_eq!(
        p.peek_serdatr() & SERDATR_RBF,
        0,
        "RBF clear — nothing received"
    );
    assert_eq!(p.peek_serdatr() & SERDATR_OVRUN, 0);
}

#[test]
fn serdat_write_raises_int_tbe() {
    let mut p = Paula8364::new();
    p.write_serdat(0x0100 | 0x41); // stop-bit + 'A'
    assert_ne!(
        p.intreq() & IntSource::Tbe.mask(),
        0,
        "SERDAT write must raise INT_TBE so driver's next-byte loop progresses"
    );
    assert_eq!(p.serdat(), 0x0141, "SERDAT stores the written value");
}

#[test]
fn serper_write_is_a_pure_store() {
    let mut p = Paula8364::new();
    p.write_serper(0x8000 | 0x01FB); // LONG + divisor for 31250 baud (MIDI)
    assert_eq!(p.serper(), 0x81FB);
    assert_ne!(p.serper() & SERPER_LONG, 0);
}

#[test]
fn receive_serial_latches_byte_raises_rbf_and_is_visible_in_serdatr() {
    let mut p = Paula8364::new();
    p.receive_serial(0x5A);
    assert_ne!(p.intreq() & IntSource::Rbf.mask(), 0);
    let v = p.peek_serdatr();
    assert_ne!(v & SERDATR_RBF, 0);
    assert_eq!(v & SERDATR_DATA_MASK, 0x5A);
}

#[test]
fn serdatr_read_clears_rbf_and_overrun() {
    let mut p = Paula8364::new();
    p.receive_serial(0x42);
    assert_ne!(p.peek_serdatr() & SERDATR_RBF, 0);
    let v = p.read_serdatr();
    assert_ne!(v & SERDATR_RBF, 0, "read returns the pre-clear state");
    assert_eq!(p.peek_serdatr() & SERDATR_RBF, 0, "RBF clears on read");
    assert_eq!(p.peek_serdatr() & SERDATR_OVRUN, 0);
}

#[test]
fn receive_while_rbf_pending_sets_overrun_but_keeps_new_byte() {
    let mut p = Paula8364::new();
    p.receive_serial(0x01);
    p.receive_serial(0x02); // CPU never read byte 1

    assert_ne!(p.peek_serdatr() & SERDATR_OVRUN, 0);
    assert_ne!(p.peek_serdatr() & SERDATR_RBF, 0);
    // HRM: the new byte overwrites; OVRUN flags that data was lost.
    assert_eq!(p.peek_serdatr() & SERDATR_DATA_MASK, 0x02);

    // Draining clears both.
    let _ = p.read_serdatr();
    assert_eq!(p.peek_serdatr() & (SERDATR_RBF | SERDATR_OVRUN), 0);
}

#[test]
fn tbe_and_rbf_route_to_correct_ipl_levels() {
    // TBE → IPL 1 (level 1); RBF → IPL 5 (level 5). Use INTENA to
    // confirm both paths reach compute_ipl.
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_INTEN | IntSource::Tbe.mask());
    p.write_serdat(0);
    assert_eq!(p.compute_ipl(), 1);

    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_INTEN | IntSource::Rbf.mask());
    p.receive_serial(0);
    assert_eq!(p.compute_ipl(), 5);
}

#[test]
fn reset_clears_all_serial_state() {
    let mut p = Paula8364::new();
    p.write_serdat(0x0141);
    p.write_serper(0x8100);
    p.receive_serial(0xAB);
    p.receive_serial(0xCD); // set OVRUN

    p.reset();

    assert_eq!(p.serdat(), 0);
    assert_eq!(p.serper(), 0);
    let v = p.peek_serdatr();
    assert_eq!(v & SERDATR_DATA_MASK, 0);
    assert_eq!(v & (SERDATR_RBF | SERDATR_OVRUN), 0);
    assert_eq!(p.intreq(), 0);
}
