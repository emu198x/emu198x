//! Pipeline-true CIA tests. Cycle counts follow the 6526's internal delays
//! (VICE ciatimer/ciacore semantics, validated end-to-end by the Lorenz
//! full-machine ledger in runtime-commodore-c64):
//!
//! - a START write begins decrementing on the **third** tick after it lands
//!   (COUNT2 → COUNT3 pipeline);
//! - a timer with latch L underflows L + 2 ticks after the START write (the
//!   counter goes 1 → 0 → reload within the underflow cycle, so 0 is never
//!   visible in φ2 mode);
//! - the ICR flag appears in the underflow cycle, but IR (bit 7) and the
//!   /IRQ line follow one cycle later on the old 6526.

use super::*;

/// Latch 5, START: underflow on tick 5 + 3.
#[test]
fn timer_a_start_pipeline_delays_first_decrement() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 5);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x01);

    // Two ticks of pipeline fill: counter untouched.
    cia.tick();
    assert_eq!(cia.timer_a(), 5);
    cia.tick();
    assert_eq!(cia.timer_a(), 5);
    // Third tick: first decrement.
    cia.tick();
    assert_eq!(cia.timer_a(), 4);
}

#[test]
fn timer_a_underflow_fires_icr_at_latch_plus_two() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 10);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x01);
    for _ in 0..11 {
        cia.tick();
    }
    assert_eq!(cia.icr_status() & 0x01, 0, "no underflow before latch+2");
    cia.tick(); // tick 12 = 10 + 2
    assert_eq!(cia.icr_status() & 0x01, 0x01);
    // Counter reloaded from the latch on the underflow cycle.
    assert_eq!(cia.timer_a(), 10);
}

#[test]
fn irq_line_rises_one_cycle_after_the_flag() {
    let mut cia = Cia6526::new();
    cia.write(0x0D, 0x81); // enable TA interrupt
    cia.write(0x04, 4);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x01);
    for _ in 0..6 {
        cia.tick();
    }
    // Underflow cycle (4 + 2): flag set, IR and the line not yet.
    assert_eq!(cia.icr_status() & 0x01, 0x01);
    assert_eq!(cia.icr_status() & 0x80, 0);
    assert!(!cia.irq);
    cia.tick();
    // One cycle later: IR + line.
    assert_eq!(cia.icr_status() & 0x80, 0x80);
    assert!(cia.irq);
}

#[test]
fn timer_a_oneshot_stops_after_underflow() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 5);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x09);
    for _ in 0..8 {
        cia.tick();
    }
    assert!(cia.icr_status() & 0x01 != 0);
    // Live START bit reads back clear.
    assert_eq!(cia.read(0x0E) & 0x01, 0);
    let counter = cia.timer_a();
    cia.tick();
    cia.tick();
    cia.tick();
    assert_eq!(cia.timer_a(), counter, "one-shot timer stays stopped");
}

#[test]
fn force_load_strobe_reloads_after_one_cycle() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 50);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x01);
    for _ in 0..10 {
        cia.tick();
    }
    let before = cia.timer_a();
    assert!(before < 50);
    // Strobe force load (keep START).
    cia.write(0x0E, 0x11);
    cia.tick();
    assert_eq!(cia.timer_a(), before - 1, "load lands one cycle later");
    cia.tick();
    assert_eq!(cia.timer_a(), 50, "latch copied on the LOAD cycle");
}

#[test]
fn icr_read_clears_flags_and_drops_line_but_ir_decays_via_ack() {
    let mut cia = Cia6526::new();
    cia.write(0x0D, 0x81);
    cia.write(0x04, 4);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x01);
    for _ in 0..8 {
        cia.tick();
    }
    assert!(cia.irq);

    let value = cia.read(0x0D);
    assert_eq!(value, 0x81);
    assert!(!cia.irq, "line drops at once");
    // Source flags clear immediately; IR is cleared by the delayed ACK.
    assert_eq!(cia.icr_status() & 0x1F, 0);
    cia.tick();
    cia.tick();
    assert_eq!(cia.icr_status(), 0, "IR gone after the ACK stage lands");
    assert!(!cia.irq);
}

#[test]
fn icr_mask_set_and_clear_bits() {
    let mut cia = Cia6526::new();
    cia.write(0x0D, 0x83);
    assert_eq!(cia.icr_mask(), 0x03);
    cia.write(0x0D, 0x01);
    assert_eq!(cia.icr_mask(), 0x02);
}

/// Enabling the mask for an already-pending flag raises the interrupt two
/// cycles later (old 6526).
#[test]
fn late_mask_enable_raises_delayed_interrupt() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 4);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x01);
    for _ in 0..8 {
        cia.tick();
    }
    assert_eq!(cia.icr_status() & 0x01, 0x01);
    assert!(!cia.irq, "masked flag raises nothing");

    cia.write(0x0D, 0x81);
    assert!(!cia.irq);
    cia.tick();
    assert!(!cia.irq, "old 6526 delays the raise");
    cia.tick();
    assert!(cia.irq);
}

/// The timer B bug: reading the ICR one cycle before a Timer B underflow
/// eats the TB flag — the next read reports nothing and no interrupt fires.
#[test]
fn timer_b_bug_eats_flag_on_racing_icr_read() {
    let mut cia = Cia6526::new();
    cia.write(0x0D, 0x82); // enable TB
    cia.write(0x06, 4);
    cia.write(0x07, 0);
    cia.write(0x0F, 0x01);
    // Underflow lands on tick 6 (4 + 2). Read the ICR after tick 5.
    for _ in 0..5 {
        cia.tick();
    }
    assert_eq!(cia.read(0x0D) & 0x02, 0, "nothing pending yet");
    cia.tick(); // TB underflow — flag set but marked as eaten
    let value = cia.read(0x0D);
    assert_eq!(value & 0x02, 0, "timer B bug: the racing read ate the flag");
    cia.tick();
    assert!(!cia.irq, "no interrupt from the eaten flag");
}

/// Without the racing read, the same underflow reports and raises normally.
#[test]
fn timer_b_flag_survives_without_racing_read() {
    let mut cia = Cia6526::new();
    cia.write(0x0D, 0x82);
    cia.write(0x06, 4);
    cia.write(0x07, 0);
    cia.write(0x0F, 0x01);
    for _ in 0..6 {
        cia.tick();
    }
    assert_eq!(cia.icr_status() & 0x02, 0x02);
    cia.tick();
    assert!(cia.irq);
}

#[test]
fn timer_b_cascade_counts_timer_a_underflows() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 3);
    cia.write(0x05, 0);
    cia.write(0x06, 2);
    cia.write(0x07, 0);
    cia.write(0x0F, 0x41);
    cia.write(0x0E, 0x01);
    // TA underflows first at tick 6 (3 + 3), then every 4 ticks.
    // Each underflow steps TB two ticks later through the STEP pipeline.
    for _ in 0..8 {
        cia.tick();
    }
    assert_eq!(cia.timer_b(), 1, "first TA underflow stepped TB");
    for _ in 0..4 {
        cia.tick();
    }
    assert_eq!(cia.timer_b(), 0);
    for _ in 0..4 {
        cia.tick();
    }
    assert!(cia.icr_status() & 0x02 != 0, "third TA underflow fires TB");
}

#[test]
fn timer_b_phi2_mode_ignores_timer_a() {
    let mut cia = Cia6526::new();
    cia.write(0x06, 5);
    cia.write(0x07, 0);
    cia.write(0x0F, 0x01);
    for _ in 0..5 {
        cia.tick();
    }
    assert_eq!(cia.timer_b(), 2);
}

/// PB6 pulse mode: high for exactly the underflow cycle.
#[test]
fn pb6_pulse_output_marks_the_underflow_cycle() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 4);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x03); // START + PBON, pulse mode
    for _ in 0..5 {
        cia.tick();
    }
    assert_eq!(cia.read(0x01) & 0x40, 0);
    cia.tick(); // underflow cycle (4 + 2)
    assert_eq!(cia.read(0x01) & 0x40, 0x40);
    cia.tick();
    assert_eq!(cia.read(0x01) & 0x40, 0);
}

/// PB6 toggle mode: preset high on START, flips on each underflow.
#[test]
fn pb6_toggle_output_flips_on_underflow() {
    let mut cia = Cia6526::new();
    cia.write(0x04, 4);
    cia.write(0x05, 0);
    cia.write(0x0E, 0x07); // START + PBON + toggle
    assert_eq!(cia.read(0x01) & 0x40, 0x40, "toggle preset high on START");
    for _ in 0..7 {
        cia.tick();
    }
    assert_eq!(cia.read(0x01) & 0x40, 0, "first underflow flips low");
    for _ in 0..5 {
        cia.tick();
    }
    assert_eq!(cia.read(0x01) & 0x40, 0x40, "second underflow flips high");
}

#[test]
fn port_a_read_combines_output_and_external() {
    let mut cia = Cia6526::new();
    cia.write(0x02, 0xF0);
    cia.write(0x00, 0xAB);
    cia.pa_in = 0x55;
    assert_eq!(cia.read(0x00), 0xA5);
}

#[test]
fn port_a_pin_reflects_ddr_masking() {
    let mut cia = Cia6526::new();
    cia.write(0x02, 0xFF);
    cia.write(0x00, 0x42);
    assert_eq!(cia.pa, 0x42);
}

#[test]
fn port_b_reads_external_through_input_bits() {
    let mut cia = Cia6526::new();
    cia.write(0x02, 0xFF);
    cia.write(0x03, 0x00);
    cia.write(0x00, 0xFD);
    cia.pb_in = !0x02;
    assert_eq!(cia.read(0x01) & 0x02, 0x00);
}

// The TOD pin is fed the mains frequency (50 Hz PAL / 60 Hz NTSC); the
// CIA divides that input by 5 or 6 (CRA bit 7) down to a 10 Hz tenths
// counter. These two tests pin the /5 and /6 stages — the regression
// they guard is the tenths advancing at the raw mains rate (5-6x fast).
#[test]
fn tod_tenths_tick_at_10hz_in_50hz_mode() {
    // PAL mains tick every 19_705 phi2 cycles; 50 Hz input / 5 = 10 Hz,
    // so a tenth is 5 mains ticks = 98_525 cycles.
    let mut cia = Cia6526::new_with_tod_dividers(19_705, 16_421);
    cia.write(0x0E, 0x80); // CRA bit 7 = 1 → 50 Hz TOD input
    cia.write(0x08, 0x00); // set tenths = 0 and start the clock
    // One mains-tick period must NOT advance the tenths (the /5 was the
    // missing stage that made it tick here).
    for _ in 0..19_705 {
        cia.tick();
    }
    assert_eq!(cia.tod[0], 0, "tenths advanced at the 50 Hz mains rate");
    // Four more mains ticks complete the 10 Hz period → one tenth.
    for _ in 0..(19_705 * 4) {
        cia.tick();
    }
    assert_eq!(cia.tod[0], 1);
}

#[test]
fn tod_tenths_tick_at_10hz_in_60hz_mode() {
    // NTSC mains tick every 17_045 phi2 cycles; 60 Hz input / 6 = 10 Hz,
    // so a tenth is 6 mains ticks.
    let mut cia = Cia6526::new_with_tod_dividers(19_705, 17_045);
    cia.write(0x0E, 0x00); // CRA bit 7 = 0 → 60 Hz TOD input
    cia.write(0x08, 0x00);
    for _ in 0..(17_045 * 5) {
        cia.tick();
    }
    assert_eq!(cia.tod[0], 0, "tenths advanced before the 6th mains tick");
    for _ in 0..17_045 {
        cia.tick();
    }
    assert_eq!(cia.tod[0], 1);
}

/// The Lorenz `irq` preamble, distilled: one-shot timer started with the
/// force-load strobe (CRA = $19), ICR read at a precise cycle. One cycle
/// before underflow reads $00; in the underflow cycle $01 (flag up, IR not
/// yet); one cycle after $81.
#[test]
fn lorenz_irq_preamble_icr_read_race() {
    // With CRA = $19 (START + ONESHOT + FLOAD) written at cycle W, the
    // force-load lands at W+2 (clearing that cycle's COUNT3), counting
    // resumes at W+4, so a latch of L underflows at W + L + 3.
    let read_at_10 = |latch: u8| -> u8 {
        let mut cia = Cia6526::new();
        cia.write(0x0D, 0x81);
        cia.write(0x04, latch);
        cia.write(0x05, 0);
        cia.write(0x0E, 0x19);
        for _ in 0..10 {
            cia.tick();
        }
        cia.read(0x0D)
    };

    assert_eq!(read_at_10(8), 0x00, "one cycle before underflow");
    assert_eq!(read_at_10(7), 0x01, "underflow cycle: flag up, IR delayed");
    assert_eq!(read_at_10(6), 0x81, "one cycle after underflow: flag + IR");
}

#[test]
fn flag_falling_edge_sets_icr_bit_four() {
    let mut cia = Cia6526::new();
    cia.flag = false;
    cia.tick();
    assert_eq!(cia.icr_status() & 0x10, 0x10);
}

/// The Novaload turbo-tape pulse loop, cycle-exact (Monty on the Run,
/// loader at $03E5): TA latch $81F4 free-running; per FLAG pulse the loader
/// reads TA-hi (~8 cycles after the poll read that saw the flag) and strobes
/// CRA=$11 (START+FLOAD, ~12 cycles after). The decoded bit is TA-hi >= $80,
/// i.e. whether the inter-pulse gap stayed under ~$1F4 cycles.
#[test]
fn novaload_pulse_measurement_loop() {
    let mut cia = Cia6526::new();
    cia.write(0x0E, 0x81); // KERNAL jiffy state: TA running
    for _ in 0..100 {
        cia.tick();
    }
    cia.write(0x04, 0xF4);
    cia.write(0x05, 0x81); // latch = $81F4 (running: no immediate copy)

    // Deliver one pulse: FLAG falling edge, then the loader's read+strobe.
    let pulse = |cia: &mut Cia6526, gap: u32| -> u8 {
        // Gap passes with FLAG high.
        cia.flag = true;
        for _ in 0..gap {
            cia.tick();
        }
        // Falling edge: the poll's BIT $DC0D sees bit 4 within a few cycles;
        // model the loop's post-detect offsets from the edge cycle.
        cia.flag = false;
        cia.tick(); // edge cycle: ICR bit 4 sets
        let mut seen = cia.read(0x0D); // the BIT that catches it
        let mut spin = 0;
        while seen & 0x10 == 0 {
            cia.tick();
            seen = cia.read(0x0D);
            spin += 1;
            assert!(spin < 8, "FLAG should be visible promptly");
        }
        // BIT(sees flag) ... BEQ not taken (2) + LDA $DC05 read on its 4th.
        for _ in 0..6 {
            cia.tick();
        }
        let hi = cia.read(0x05);
        // STX $DC0E lands 4 cycles after the LDA's read.
        for _ in 0..4 {
            cia.tick();
        }
        cia.write(0x0E, 0x11);
        hi
    };

    // Prime: first strobe arms the $81F4 reload.
    let _ = pulse(&mut cia, 100);

    let short1 = pulse(&mut cia, 300);
    let long1 = pulse(&mut cia, 600);
    let short2 = pulse(&mut cia, 300);
    let long2 = pulse(&mut cia, 600);
    println!("short={short1:02X},{short2:02X} long={long1:02X},{long2:02X}");
    assert!(
        short1 >= 0x80,
        "short gap must read TA-hi >= $80, got {short1:02X}"
    );
    assert!(
        short2 >= 0x80,
        "short gap must read TA-hi >= $80, got {short2:02X}"
    );
    assert!(
        long1 < 0x80,
        "long gap must read TA-hi < $80, got {long1:02X}"
    );
    assert!(
        long2 < 0x80,
        "long gap must read TA-hi < $80, got {long2:02X}"
    );
}
