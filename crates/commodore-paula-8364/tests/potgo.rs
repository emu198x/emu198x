//! Phase 1 characterization tests — POTGO + POTxDAT + POTGOR.
//!
//! Per HRM §6 (Controller I/O Hardware). The four pot pins serve two
//! uses: proportional input (paddle / light pen) when read after
//! starting a charge cycle via POTGO.START, and button input (mouse
//! middle / right, or 2nd/3rd joystick button) when configured as
//! inputs and read straight from POTGOR.
//!
//! Like serial, this never lived in the Paula archive — the old
//! machine archive held POTGO state inline. The tests describe the
//! contract for the fresh implementation.

use commodore_paula_8364::{Paula8364, bits::*};

#[test]
fn potgo_pure_store_for_out_and_dat_bits_strobe_bit_does_not_latch() {
    let mut p = Paula8364::new();
    p.write_potgo(POTGO_OUTRY | POTGO_DATRY | POTGO_START);
    assert_eq!(
        p.potgo() & POTGO_START,
        0,
        "bit 0 START is a strobe — does not read back"
    );
    assert_ne!(p.potgo() & POTGO_OUTRY, 0);
    assert_ne!(p.potgo() & POTGO_DATRY, 0);
}

#[test]
fn potgo_start_clears_both_pot_counters() {
    let mut p = Paula8364::new();
    p.set_pot_data(0, 0x100);
    p.set_pot_data(1, 0x200);
    assert_eq!(p.pot0dat(), 0x100);
    assert_eq!(p.pot1dat(), 0x200);
    p.write_potgo(POTGO_START);
    assert_eq!(p.pot0dat(), 0);
    assert_eq!(p.pot1dat(), 0);
}

#[test]
fn potgor_reads_pot_pin_levels_on_input_pins() {
    // All pins configured as inputs by default (OUT bits clear).
    let p = Paula8364::new();
    let v = p.peek_potgor();
    assert_eq!(
        v & POTGOR_DAT_ALL,
        POTGOR_DAT_ALL,
        "idle: all four pot pins float high (buttons released)"
    );
}

#[test]
fn potgor_input_pin_can_be_pulled_low_by_peripheral() {
    // Port 0 middle button pressed → POTGOR bit 10 = 0.
    let mut p = Paula8364::new();
    p.set_pot_pin_level(POTGOR_BTN_PORT0_MIDDLE, false);
    let v = p.peek_potgor();
    assert_eq!(
        v & POTGOR_BTN_PORT0_MIDDLE,
        0,
        "pulled-low input reads back as 0"
    );
    // Other pins unaffected.
    assert_ne!(v & POTGOR_BTN_PORT0_RIGHT, 0);
    assert_ne!(v & POTGOR_BTN_PORT1_MIDDLE, 0);
    assert_ne!(v & POTGOR_BTN_PORT1_RIGHT, 0);
}

#[test]
fn potgor_output_pin_reads_back_the_latched_dat_bit() {
    // Configure port 0 middle pin as output, drive it low.
    let mut p = Paula8364::new();
    p.write_potgo(POTGO_OUTRX); // OUT_RX = 1, DATRX = 0
    // Even if the peripheral drives the pin high, the output bit
    // wins because the chip is actively driving the line.
    p.set_pot_pin_level(POTGOR_BTN_PORT0_MIDDLE, true);
    let v = p.peek_potgor();
    assert_eq!(
        v & POTGOR_BTN_PORT0_MIDDLE,
        0,
        "output-configured pin reports the driven DAT value"
    );

    p.write_potgo(POTGO_OUTRX | POTGO_DATRX); // drive high
    let v = p.peek_potgor();
    assert_ne!(v & POTGOR_BTN_PORT0_MIDDLE, 0);
}

#[test]
fn set_pot_data_saturates_to_10_bits_per_hrm() {
    let mut p = Paula8364::new();
    p.set_pot_data(0, 0xFFFF);
    assert_eq!(p.pot0dat(), 0x03FF, "HRM: POTxDAT is a 10-bit count");
}

#[test]
fn reset_clears_pot_state_and_restores_pin_default_highs() {
    let mut p = Paula8364::new();
    p.write_potgo(POTGO_OUTRY | POTGO_DATRY);
    p.set_pot_pin_level(POTGOR_BTN_PORT0_MIDDLE, false);
    p.set_pot_data(0, 0x100);
    p.reset();
    assert_eq!(p.potgo(), 0);
    assert_eq!(p.pot0dat(), 0);
    assert_eq!(
        p.peek_potgor() & POTGOR_DAT_ALL,
        POTGOR_DAT_ALL,
        "reset: every input pin floats high again"
    );
}
