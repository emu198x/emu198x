use super::*;

/// An MFP with every channel enabled and unmasked, vectors at $40.
fn armed() -> Mfp68901 {
    let mut mfp = Mfp68901::new();
    for r in [reg::IERA, reg::IERB, reg::IMRA, reg::IMRB] {
        mfp.write(r, 0xFF);
    }
    mfp.write(reg::VR, 0x40);
    mfp
}

#[test]
fn timer_c_at_200_hz_is_the_st_system_tick() {
    // TOS: prescaler 64 (control 5), data 192. 2_457_600 / 64 / 192 = 200 Hz.
    let mut mfp = armed();
    mfp.write(reg::TCDR, 192);
    mfp.write(reg::TCDCR, 0x50);
    let period = 64 * 192;
    for _ in 0..period - 1 {
        mfp.tick();
    }
    assert!(!mfp.irq(), "not before the 192nd count");
    mfp.tick();
    assert!(mfp.irq());
    assert_eq!(mfp.acknowledge(), Some(0x40 | channel::TIMER_C));
    assert!(!mfp.irq(), "acknowledging clears pending in automatic mode");
    for _ in 0..period {
        mfp.tick();
    }
    assert!(mfp.irq(), "it reloads and fires again one period later");
}

#[test]
fn a_data_value_of_zero_counts_256() {
    let mut mfp = armed();
    mfp.write(reg::TADR, 0);
    mfp.write(reg::TACR, 1); // prescale 4
    for _ in 0..4 * 256 - 1 {
        mfp.tick();
    }
    assert!(!mfp.irq());
    mfp.tick();
    assert!(mfp.irq());
}

#[test]
fn reading_the_data_register_reads_the_counter() {
    let mut mfp = armed();
    mfp.write(reg::TBDR, 10);
    mfp.write(reg::TBCR, 1);
    for _ in 0..4 * 3 {
        mfp.tick();
    }
    assert_eq!(mfp.read(reg::TBDR), 7);
}

#[test]
fn writing_data_while_running_waits_for_the_time_out() {
    let mut mfp = armed();
    mfp.write(reg::TDDR, 5);
    mfp.write(reg::TCDCR, 0x01); // timer D, prescale 4
    mfp.tick();
    mfp.write(reg::TDDR, 100);
    assert_eq!(mfp.read(reg::TDDR), 5, "the running counter is untouched");
    for _ in 0..4 * 5 {
        mfp.tick();
    }
    assert_eq!(mfp.read(reg::TDDR), 100, "the time-out loads the new data");
}

#[test]
fn event_count_mode_counts_active_edges_of_tbi() {
    let mut mfp = armed();
    mfp.write(reg::AER, 0x08); // TBI active high
    // GPIP 3 shares AER bit 3 and its line is high, so the write completed an
    // edge on that channel; clear it.
    mfp.write(reg::IPRB, 0);
    mfp.write(reg::TBDR, 2);
    mfp.write(reg::TBCR, 8);
    for _ in 0..1000 {
        mfp.tick();
    }
    assert!(!mfp.irq(), "event mode ignores the clock");
    mfp.set_timer_input(TimerId::B, true);
    mfp.set_timer_input(TimerId::B, false);
    assert!(!mfp.irq());
    mfp.set_timer_input(TimerId::B, true);
    assert_eq!(mfp.acknowledge(), Some(0x40 | channel::TIMER_B));
}

#[test]
fn pulse_width_mode_counts_only_while_the_input_is_active() {
    let mut mfp = armed();
    mfp.write(reg::AER, 0x10); // TAI active high
    mfp.write(reg::TADR, 0);
    mfp.write(reg::TACR, 9); // pulse width, prescale 4
    for _ in 0..40 {
        mfp.tick();
    }
    assert_eq!(mfp.peek(reg::TADR), 0, "inactive: no counting");
    mfp.set_timer_input(TimerId::A, true);
    for _ in 0..40 {
        mfp.tick();
    }
    assert_eq!(mfp.peek(reg::TADR), 246, "ten counts while active");
    mfp.set_timer_input(TimerId::A, false);
    assert_eq!(
        mfp.acknowledge(),
        Some(0x40 | channel::GPIP4),
        "end of pulse"
    );
}

#[test]
fn the_highest_pending_channel_wins() {
    let mut mfp = armed();
    mfp.set_gpip_input(0, false); // AER 0: a falling edge is active
    mfp.set_gpip_input(7, false);
    assert_eq!(mfp.acknowledge(), Some(0x40 | channel::GPIP7));
    assert_eq!(mfp.acknowledge(), Some(0x40 | channel::GPIP0));
    assert_eq!(mfp.acknowledge(), None);
}

#[test]
fn in_service_blocks_lower_and_equal_priorities_until_cleared() {
    let mut mfp = armed();
    mfp.write(reg::VR, 0x40 | 0x08); // software end of interrupt
    mfp.set_gpip_input(4, false); // channel 6
    assert_eq!(mfp.acknowledge(), Some(0x46));
    assert_eq!(mfp.peek(reg::ISRB), 0x40);
    mfp.set_gpip_input(0, false); // channel 0: lower
    assert!(!mfp.irq(), "masked by the channel in service");
    mfp.set_gpip_input(7, false); // channel 15: higher
    assert_eq!(mfp.acknowledge(), Some(0x4F));
    mfp.write(reg::ISRA, 0x7F);
    mfp.write(reg::ISRB, 0xBF);
    assert_eq!(
        mfp.acknowledge(),
        Some(0x40),
        "channel 0 once both are serviced"
    );
}

#[test]
fn clearing_the_s_bit_clears_in_service() {
    let mut mfp = armed();
    mfp.write(reg::VR, 0x48);
    mfp.set_gpip_input(1, false);
    mfp.acknowledge();
    assert_ne!(mfp.peek(reg::ISRB), 0);
    mfp.write(reg::VR, 0x40);
    assert_eq!(mfp.peek(reg::ISRB), 0);
}

#[test]
fn disabling_a_channel_drops_its_pending_bit_and_masking_holds_it() {
    let mut mfp = armed();
    mfp.write(reg::IMRB, 0);
    mfp.set_gpip_input(2, false);
    assert!(!mfp.irq(), "masked");
    assert_eq!(mfp.peek(reg::IPRB), 0x04, "but pending");
    mfp.write(reg::IMRB, 0xFF);
    assert!(mfp.irq());
    mfp.write(reg::IERB, 0xFB);
    assert_eq!(mfp.peek(reg::IPRB), 0, "disabling clears pending");
    assert!(!mfp.irq());
}

#[test]
fn writing_ipr_clears_only_where_zero_is_written() {
    let mut mfp = armed();
    mfp.set_gpip_input(0, false);
    mfp.set_gpip_input(1, false);
    mfp.write(reg::IPRB, !0x01);
    assert_eq!(mfp.peek(reg::IPRB), 0x02);
    mfp.write(reg::IPRB, 0xFF);
    assert_eq!(mfp.peek(reg::IPRB), 0x02);
}

#[test]
fn a_disabled_channel_never_goes_pending() {
    let mut mfp = Mfp68901::new();
    mfp.write(reg::IMRB, 0xFF);
    mfp.set_gpip_input(3, false);
    assert_eq!(mfp.peek(reg::IPRB), 0);
}

#[test]
fn the_active_edge_register_picks_rising_or_falling() {
    let mut mfp = armed();
    mfp.set_gpip_input(0, false); // AER 0: falling is active
    assert_eq!(mfp.acknowledge(), Some(0x40));
    mfp.write(reg::AER, 0x01); // line 0 rising, with the line low: no edge
    assert!(!mfp.irq());
    mfp.set_gpip_input(0, false);
    assert!(!mfp.irq(), "no change, no edge");
    mfp.set_gpip_input(0, true);
    assert_eq!(mfp.acknowledge(), Some(0x40));
    // With the line high, switching AER to falling-active does not fire; back
    // to rising with the line high completes an edge.
    mfp.write(reg::AER, 0x00);
    assert!(!mfp.irq());
    mfp.write(reg::AER, 0x01);
    assert!(mfp.irq());
}

#[test]
fn gpip_reads_outputs_from_the_latch_and_inputs_from_the_pins() {
    let mut mfp = Mfp68901::new();
    mfp.set_gpip_inputs(0b1010_0000);
    mfp.write(reg::DDR, 0x0F);
    mfp.write(reg::GPIP, 0xF5);
    assert_eq!(mfp.read(reg::GPIP), 0b1010_0101);
}

#[test]
fn the_usart_sends_and_receives_on_timer_d() {
    let mut mfp = armed();
    // 8N1, divide by 16, timer D prescale 4 data 1: a USART clock edge every
    // 8 MFP clocks, so a 10-bit frame takes 16 * 10 edges.
    mfp.write(reg::UCR, 0x88);
    mfp.write(reg::TDDR, 1);
    mfp.write(reg::TCDCR, 0x01);
    mfp.write(reg::TSR, TSR_TE);
    mfp.write(reg::RSR, RSR_RE);
    mfp.write(reg::UDR, 0x5A);
    assert_eq!(mfp.peek(reg::TSR) & TSR_BE, 0);
    mfp.receive(0xA5);
    for _ in 0..8 * 16 * 10 + 8 * 16 {
        mfp.tick();
    }
    assert_eq!(mfp.take_transmitted(), vec![0x5A]);
    assert_ne!(mfp.peek(reg::TSR) & TSR_BE, 0, "buffer empty again");
    assert_ne!(mfp.peek(reg::RSR) & RSR_BF, 0);
    assert_eq!(mfp.read(reg::UDR), 0xA5);
    assert_eq!(mfp.peek(reg::RSR) & RSR_BF, 0, "reading empties the buffer");
}

#[test]
fn a_second_byte_before_the_first_is_read_overruns() {
    let mut mfp = armed();
    mfp.write(reg::RSR, RSR_RE);
    mfp.receive(1);
    mfp.receive(2);
    for _ in 0..30 {
        mfp.usart_clock();
    }
    assert_eq!(mfp.peek(reg::RSR) & RSR_OE, RSR_OE);
    assert_eq!(mfp.read(reg::UDR), 1);
    mfp.read(reg::RSR);
    assert_eq!(mfp.peek(reg::RSR) & RSR_OE, 0, "reading RSR clears overrun");
}

#[test]
fn seven_bit_words_mask_the_byte() {
    let mut mfp = armed();
    mfp.write(reg::UCR, 0x28); // 7 data bits, 1 stop, divide by 1
    mfp.write(reg::TSR, TSR_TE);
    mfp.write(reg::UDR, 0xFF);
    for _ in 0..20 {
        mfp.usart_clock();
    }
    assert_eq!(mfp.take_transmitted(), vec![0x7F]);
}

#[test]
fn reset_keeps_timer_data_and_stops_everything() {
    let mut mfp = armed();
    mfp.write(reg::TADR, 77);
    mfp.write(reg::TACR, 1);
    mfp.reset();
    assert_eq!(mfp.peek(reg::TACR), 0);
    assert_eq!(mfp.peek(reg::TADR), 77);
    assert_eq!(mfp.peek(reg::IERA), 0);
    assert!(!mfp.irq());
}

#[test]
fn pulse_width_timer_b_ends_on_gpip3_and_a_stopped_timer_c_ignores_input() {
    let mut mfp = armed();
    mfp.write(reg::TBCR, 9);
    mfp.set_timer_input(TimerId::B, false);
    mfp.set_timer_input(TimerId::B, true);
    mfp.set_timer_input(TimerId::B, false);
    // AER bit 3 is 0: low is active, so the rise ended a pulse.
    assert_eq!(mfp.acknowledge(), Some(0x40 | channel::GPIP3));
    mfp.set_timer_input(TimerId::C, true);
    assert!(!mfp.timer_output(TimerId::C));
}

#[test]
fn state_round_trips_through_serde() {
    let mut mfp = armed();
    mfp.write(reg::TCDR, 9);
    mfp.write(reg::TCDCR, 0x10);
    for _ in 0..50 {
        mfp.tick();
    }
    let json = serde_json::to_string(&mfp).expect("serde round trip");
    let back: Mfp68901 = serde_json::from_str(&json).expect("serde round trip");
    assert_eq!(back.peek(reg::TCDR), mfp.peek(reg::TCDR));
}
