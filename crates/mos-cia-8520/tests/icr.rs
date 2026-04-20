//! Phase 1 characterization tests — Interrupt Control Register.
//!
//! Per HRM Appendix F pp. 346-347.
//!
//! ICR register at $D has two roles: data register (flags) and mask
//! register. Read returns the data register PLUS bit 7 (IR) set if
//! `(flags & mask) != 0`; the read CLEARS the data register. Write
//! programs the mask: bit 7 = SET-OR-CLEAR flag (1 = SET bits, 0 =
//! CLEAR bits); low 5 bits select which bits to touch. Untouched
//! bits persist.
//!
//! The five interrupt sources, in bit order:
//!   bit 0 TA     — Timer A underflow
//!   bit 1 TB     — Timer B underflow
//!   bit 2 ALARM  — TOD matches alarm
//!   bit 3 SP     — Serial byte complete
//!   bit 4 FLAG   — /FLAG pin falling edge

use mos_cia_8520::Cia8520;

const CRA: u8 = 0x0E;
const CRB: u8 = 0x0F;
const TALO: u8 = 0x04;
const TAHI: u8 = 0x05;
const TBLO: u8 = 0x06;
const TBHI: u8 = 0x07;
const ICR: u8 = 0x0D;

const ICR_TA: u8 = 0x01;
const ICR_TB: u8 = 0x02;
const ICR_ALARM: u8 = 0x04;
const ICR_SP: u8 = 0x08;
const ICR_FLAG: u8 = 0x10;
const IR_BIT: u8 = 0x80;

// ────────────────────────────────────────────────────────────────
// Mask programming: SET/CLEAR semantics
// ────────────────────────────────────────────────────────────────

#[test]
fn icr_write_bit7_set_adds_to_mask() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x81); // SET, TA
    assert_eq!(cia.icr_mask(), 0x01);
    cia.write(ICR, 0x82); // SET, TB — must not clear TA
    assert_eq!(cia.icr_mask(), 0x03);
}

#[test]
fn icr_write_bit7_clear_removes_from_mask() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x9F); // SET, all 5
    assert_eq!(cia.icr_mask(), 0x1F);
    cia.write(ICR, 0x04); // CLEAR, ALARM only
    assert_eq!(cia.icr_mask(), 0x1B, "only the named bit cleared");
}

#[test]
fn icr_write_lower_bits_of_zero_is_a_nop() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x9F);
    cia.write(ICR, 0x80); // SET with no bits specified — no-op
    assert_eq!(cia.icr_mask(), 0x1F);
    cia.write(ICR, 0x00); // CLEAR with no bits — no-op
    assert_eq!(cia.icr_mask(), 0x1F);
}

#[test]
fn icr_write_ignores_bits_5_and_6() {
    let mut cia = Cia8520::new("T");
    // HRM: bits 5-6 unused. Only low 5 of the write affect the mask.
    cia.write(ICR, 0xFF); // SET, with bits 5,6,7 all set
    assert_eq!(cia.icr_mask(), 0x1F, "only low 5 programmed");
}

// ────────────────────────────────────────────────────────────────
// Read side: returns flags | IR, clears flags
// ────────────────────────────────────────────────────────────────

#[test]
fn icr_read_returns_zero_when_no_flags() {
    let mut cia = Cia8520::new("T");
    assert_eq!(cia.read(ICR), 0);
}

#[test]
fn icr_read_returns_flags_with_ir_bit_when_any_masked_flag_active() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x81); // unmask TA
    cia.write(TALO, 0x01);
    cia.write(TAHI, 0x00);
    cia.write(CRA, 0x19); // LOAD | START | ONESHOT
    // Tick to underflow
    for _ in 0..3 {
        cia.tick();
    }
    assert_ne!(cia.icr_status() & ICR_TA, 0);
    assert!(cia.irq_active());
    let ret = cia.read(ICR);
    assert_eq!(ret & ICR_TA, ICR_TA);
    assert_eq!(ret & IR_BIT, IR_BIT, "IR bit 7 set when masked flag active");
}

#[test]
fn icr_read_returns_flags_without_ir_when_flag_is_masked_off() {
    let mut cia = Cia8520::new("T");
    // Don't set mask bit. Flag latches but IR (bit 7) should NOT.
    cia.receive_serial_byte(0x42); // flags ICR_SP
    assert_ne!(cia.icr_status() & ICR_SP, 0);
    assert!(!cia.irq_active());
    let ret = cia.read(ICR);
    assert_eq!(ret & ICR_SP, ICR_SP, "flag bit present");
    assert_eq!(ret & IR_BIT, 0, "IR bit 7 clear when masked off");
}

#[test]
fn icr_read_clears_all_flag_bits() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x9F); // unmask all 5
    cia.receive_serial_byte(0x01); // SP flag
    cia.flag_falling_edge(); // FLAG bit 4
    let ret = cia.read(ICR);
    assert_eq!(ret & (ICR_SP | ICR_FLAG), ICR_SP | ICR_FLAG);
    assert_eq!(cia.icr_status(), 0, "all flags cleared after ICR read");
    assert!(!cia.irq_active());
}

// ────────────────────────────────────────────────────────────────
// Each of the 5 sources latches its own bit
// ────────────────────────────────────────────────────────────────

#[test]
fn ta_underflow_latches_bit_0() {
    let mut cia = Cia8520::new("T");
    cia.write(TALO, 1);
    cia.write(TAHI, 0);
    cia.write(CRA, 0x19); // LOAD | START | ONESHOT
    for _ in 0..3 {
        cia.tick();
    }
    assert_eq!(cia.icr_status() & ICR_TA, ICR_TA);
    assert_eq!(cia.icr_status() & 0x1E, 0, "no other flags");
}

#[test]
fn tb_underflow_latches_bit_1() {
    let mut cia = Cia8520::new("T");
    cia.write(TBLO, 1);
    cia.write(TBHI, 0);
    cia.write(CRB, 0x19);
    for _ in 0..3 {
        cia.tick();
    }
    assert_eq!(cia.icr_status() & ICR_TB, ICR_TB);
    assert_eq!(cia.icr_status() & (0x1F & !ICR_TB), 0, "no other flags");
}

#[test]
fn alarm_latches_bit_2() {
    let mut cia = Cia8520::new("T");
    cia.set_tod_counter(0);
    cia.write(CRB, 0x80); // alarm select
    cia.write(0x08, 1); // alarm = 1
    cia.tod_pulse(); // counter → 1, alarm match
    assert_eq!(cia.icr_status() & ICR_ALARM, ICR_ALARM);
}

#[test]
fn sp_latches_bit_3() {
    let mut cia = Cia8520::new("T");
    cia.receive_serial_byte(0);
    assert_eq!(cia.icr_status() & ICR_SP, ICR_SP);
}

#[test]
fn flag_pin_falling_edge_latches_bit_4() {
    let mut cia = Cia8520::new("T");
    cia.flag_falling_edge();
    assert_eq!(cia.icr_status() & ICR_FLAG, ICR_FLAG);
}

// ────────────────────────────────────────────────────────────────
// /IRQ output is level-sensitive on (flags & mask)
// ────────────────────────────────────────────────────────────────

#[test]
fn irq_level_depends_only_on_masked_flags() {
    let mut cia = Cia8520::new("T");
    cia.receive_serial_byte(0); // SP flag
    assert!(!cia.irq_active(), "no mask → no IRQ");
    cia.write(ICR, 0x88); // unmask SP
    assert!(cia.irq_active(), "mask now matches flag → IRQ");
    cia.write(ICR, 0x08); // CLEAR SP mask
    assert!(!cia.irq_active(), "mask removed → no IRQ");
}

#[test]
fn irq_reasserts_if_flag_latches_while_already_unmasked() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x90); // unmask FLAG
    cia.flag_falling_edge();
    assert!(cia.irq_active());
    let _ = cia.read(ICR); // clears flag + drops IRQ
    assert!(!cia.irq_active());
    cia.flag_falling_edge();
    assert!(cia.irq_active(), "new edge re-latches and reasserts");
}

// ────────────────────────────────────────────────────────────────
// Multiple sources simultaneously
// ────────────────────────────────────────────────────────────────

#[test]
fn multiple_flags_latched_together_all_visible_on_read() {
    let mut cia = Cia8520::new("T");
    cia.write(ICR, 0x9F); // unmask all
    cia.receive_serial_byte(0);
    cia.flag_falling_edge();

    // Also trigger TA underflow
    cia.write(TALO, 1);
    cia.write(TAHI, 0);
    cia.write(CRA, 0x19);
    for _ in 0..3 {
        cia.tick();
    }

    let ret = cia.read(ICR);
    assert_eq!(ret & ICR_SP, ICR_SP);
    assert_eq!(ret & ICR_FLAG, ICR_FLAG);
    assert_eq!(ret & ICR_TA, ICR_TA);
    assert_eq!(ret & IR_BIT, IR_BIT);
}

// ────────────────────────────────────────────────────────────────
// Unmask-after-latch: IRQ asserts immediately
// ────────────────────────────────────────────────────────────────

#[test]
fn unmasking_after_flag_already_latched_asserts_irq() {
    let mut cia = Cia8520::new("T");
    cia.receive_serial_byte(0); // SP flag without mask → no IRQ
    assert!(!cia.irq_active());
    cia.write(ICR, 0x88); // SET SP mask now
    assert!(
        cia.irq_active(),
        "unmasking an already-latched flag must immediately raise /IRQ"
    );
}
