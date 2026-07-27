//! Interrupt-vector inheritance regressions for `Cpu68040`.
//!
//! `Cpu68040` currently inherits the family compatibility-bus model from
//! `Cpu68000`. This test protects the inherited architectural result; it
//! does not claim pin-accurate MC68040 interrupt-acknowledge signalling.

#[path = "../../motorola-68000/test-support/device_vectored_interrupt.rs"]
mod device_vectored_interrupt;

use motorola_68040::Cpu68040;

#[test]
fn device_vector_is_fetched_through_vbr_and_stacked() {
    let mut cpu = Cpu68040::new();
    device_vectored_interrupt::assert_device_vector_is_fetched_and_stacked(&mut cpu);
}
