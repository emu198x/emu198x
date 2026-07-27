//! Interrupt-vector inheritance regressions for `Cpu68020`.

#[path = "../../motorola-68000/test-support/device_vectored_interrupt.rs"]
mod device_vectored_interrupt;

use motorola_68020::Cpu68020;

#[test]
fn device_vector_is_fetched_through_vbr_and_stacked() {
    let mut cpu = Cpu68020::new();
    device_vectored_interrupt::assert_device_vector_is_fetched_and_stacked(&mut cpu);
}
