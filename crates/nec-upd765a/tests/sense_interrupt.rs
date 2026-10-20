//! Public-register result framing, independent of the private result buffer.
//! No-pending SIS already selects Invalid Command; that result has ST0 only.

use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::Upd765a;

fn assert_empty_interrupt_result(fdc: &mut Upd765a, busy: u8) {
    fdc.write_data(0x08);
    assert_eq!(fdc.read_status(), 0xd0 | busy);
    assert_eq!(fdc.read_data(), 0x80);
    assert_eq!(fdc.read_status(), 0x80 | busy, "ST0 ends the result");
    // A different command must be accepted immediately, not discarded while
    // the controller waits for a fictitious PCN read.
    fdc.write_data(0x04);
    fdc.write_data(0);
    assert_eq!(fdc.read_status(), 0xd0 | busy);
    assert_eq!(fdc.read_data() & 3, 0);
    assert_eq!(fdc.read_status(), 0x80 | busy);
}

#[test]
fn no_pending_interrupt_returns_one_byte_then_accepts_a_command() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    assert_empty_interrupt_result(&mut fdc, 0);
    assert!(!fdc.interrupt);
}

#[test]
fn querying_before_seek_completion_keeps_the_seek_active() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    for byte in [0x0f, 0, 7] {
        fdc.write_data(byte);
    }
    assert_eq!(fdc.read_status(), 0x81);
    assert_empty_interrupt_result(&mut fdc, 1);
    assert!(!fdc.interrupt);
    for hc in 0..1024 {
        fdc.tick(hc);
    }
    assert!(fdc.interrupt);
    fdc.write_data(0x08);
    assert_eq!(fdc.read_data(), 0x68); // no disk: existing seek readiness model
    assert_eq!(fdc.read_status(), 0xd0, "completed seek still has PCN");
    assert_eq!(fdc.read_data(), 0);
    assert_eq!(fdc.read_status(), 0x80);
    assert!(!fdc.interrupt);
    assert_empty_interrupt_result(&mut fdc, 0);
}

#[test]
fn completed_seeks_keep_two_bytes_and_drain_in_drive_order() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    for byte in [0x0f, 3, 9, 0x0f, 0, 7] {
        fdc.write_data(byte);
    }
    for hc in 0..1024 {
        fdc.tick(hc);
    }
    for drive in [0, 3] {
        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x68 | drive);
        assert_eq!(fdc.read_status(), 0xd0, "pending seek includes PCN");
        assert_eq!(fdc.read_data(), 0);
        assert_eq!(fdc.read_status(), 0x80);
        assert!(
            !fdc.interrupt,
            "SIS acknowledges IRQ independently of pending status"
        );
    }
    assert_empty_interrupt_result(&mut fdc, 0);
}
