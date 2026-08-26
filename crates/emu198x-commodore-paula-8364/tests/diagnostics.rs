use emu198x_commodore_paula_8364::{
    AudioField, Paula8364, PaulaAudioDmaState, PaulaChannel, bits::*,
};

#[test]
fn interrupt_diagnostic_snapshot_exposes_registers_active_sources_and_ipl() {
    let mut paula = Paula8364::new();
    paula.write_intena(INT_SETCLR | INT_INTEN | INT_VERTB | INT_EXTER);
    paula.write_intreq(INT_SETCLR | INT_SOFT | INT_VERTB | INT_EXTER);
    paula.write_adkcon(INT_SETCLR | ADKCON_WORDSYNC | ADKCON_FAST);

    let snapshot = paula.interrupt_diagnostic_snapshot();

    assert_eq!(snapshot.intena, INT_INTEN | INT_VERTB | INT_EXTER);
    assert_eq!(snapshot.intreq, INT_SOFT | INT_VERTB | INT_EXTER);
    assert_eq!(snapshot.adkcon, ADKCON_WORDSYNC | ADKCON_FAST);
    assert_eq!(snapshot.active_sources, INT_VERTB | INT_EXTER);
    assert_eq!(snapshot.ipl, 6);
    assert_eq!(
        paula.interrupt_diagnostic_snapshot(),
        snapshot,
        "capturing interrupt diagnostics must not change Paula state"
    );
}

#[test]
fn audio_diagnostic_snapshot_exposes_complete_playback_pipeline_and_controls() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(INT_SETCLR | ADKCON_USE_PER[0] | ADKCON_USE_VOL[0]);
    paula.write_audio(0, AudioField::LcHi, 0x0012);
    paula.write_audio(0, AudioField::LcLo, 0x3457);
    paula.write_audio(0, AudioField::Len, 2);
    paula.write_audio(0, AudioField::Per, 200);
    paula.write_audio(0, AudioField::Vol, 32);
    paula.write_audio(0, AudioField::Dat, 0x1234);

    let mut controls = paula.audio_controls();
    controls.set_master_gain(0.5);
    controls.set_channel_enabled(PaulaChannel::Channel1, false);
    controls.set_channel_gain(PaulaChannel::Channel1, 0.25);
    paula.set_audio_controls(controls);

    let dmacon = DMA_MASTER | DMA_AUD0;
    let sample = |address: u32| if address & 1 == 0 { 0xAB } else { 0xCD };

    paula.tick_audio_cck(dmacon, None, sample);
    let waiting_for_word_1 = paula.audio_diagnostic_snapshot();
    assert_eq!(
        waiting_for_word_1.channels[0].state,
        PaulaAudioDmaState::WaitWord1
    );
    assert_eq!(waiting_for_word_1.channels[0].dma_requests_pending, 1);

    paula.tick_audio_cck(dmacon, Some(0), sample);
    let waiting_for_word_2 = paula.audio_diagnostic_snapshot();
    assert_eq!(
        waiting_for_word_2.channels[0].state,
        PaulaAudioDmaState::WaitWord2
    );
    assert_eq!(waiting_for_word_2.channels[0].dma_pointer, 0x0012_3456);
    assert_eq!(waiting_for_word_2.channels[0].words_remaining, 2);

    paula.tick_audio_cck(dmacon, Some(0), sample);
    let snapshot = paula.audio_diagnostic_snapshot();
    let channel = snapshot.channels[0];

    assert_eq!(channel.location, 0x0012_3456);
    assert_eq!(channel.dma_pointer, 0x0012_3458);
    assert_eq!(channel.length_words, 2);
    assert_eq!(channel.programmed_length_words, 2);
    assert_eq!(channel.words_remaining, 1);
    assert_eq!(channel.period, 200);
    assert_eq!(channel.effective_period, 200);
    assert_eq!(channel.volume, 32);
    assert_eq!(channel.data, 0x1234);
    assert_eq!(channel.current_word, Some(0xABCD));
    assert_eq!(channel.next_word, None);
    assert!(!channel.next_byte_is_high);
    assert_eq!(channel.period_counter, 199);
    assert_eq!(channel.output_sample, 0xAB_u8 as i8);
    assert_eq!(channel.state, PaulaAudioDmaState::Playing);
    assert!(channel.dma_active);
    assert!(channel.dma_enabled_previous);
    assert_eq!(channel.dma_requests_pending, 1);
    assert!(channel.period_modulation_enabled);
    assert!(channel.volume_modulation_enabled);
    assert!(channel.host_control.enabled());
    assert_eq!(channel.host_control.gain(), 1.0);

    assert_eq!(snapshot.controls.master_gain(), 0.5);
    assert!(!snapshot.controls.channel(PaulaChannel::Channel1).enabled());
    assert_eq!(
        snapshot.controls.channel(PaulaChannel::Channel1).gain(),
        0.25
    );
    assert_eq!(
        snapshot.channels[1].programmed_length_words, 65_536,
        "an AUDxLEN value of zero represents 65,536 words"
    );
    assert_eq!(
        paula.audio_diagnostic_snapshot(),
        snapshot,
        "capturing audio diagnostics must not advance playback"
    );
}

#[test]
fn serial_and_pot_diagnostic_snapshots_preserve_read_latches() {
    let mut paula = Paula8364::new();
    paula.write_serdat(0x0141);
    paula.write_serper(SERPER_LONG | 0x01FB);
    paula.receive_serial(0x11);
    paula.receive_serial(0x22);

    let serial = paula.serial_diagnostic_snapshot();

    assert_eq!(serial.serdat, 0x0141);
    assert_eq!(serial.serper, SERPER_LONG | 0x01FB);
    assert_eq!(serial.receive_data, 0x22);
    assert!(serial.receive_full);
    assert!(serial.receive_overrun);
    assert_eq!(
        serial.serdatr & (SERDATR_OVRUN | SERDATR_RBF | SERDATR_DATA_MASK),
        SERDATR_OVRUN | SERDATR_RBF | 0x22
    );
    assert_eq!(
        paula.serial_diagnostic_snapshot(),
        serial,
        "capturing UART diagnostics must not clear its receive latches"
    );
    assert_ne!(paula.read_serdatr() & SERDATR_OVRUN, 0);

    paula.write_potgo(POTGO_OUTRX | POTGO_DATRX | POTGO_OUTLX);
    paula.set_pot_pin_level(POTGO_DATRX, false);
    paula.set_pot_data(0, 0x0123);
    paula.set_pot_data(1, 0x02AB);

    let pot = paula.pot_diagnostic_snapshot();

    assert_eq!(pot.potgo, POTGO_OUTRX | POTGO_DATRX | POTGO_OUTLX);
    assert_eq!(pot.raw_pin_levels, POTGOR_DAT_ALL & !POTGO_DATRX);
    assert_eq!(
        pot.potgor,
        POTGO_OUTRX | POTGO_OUTLX | POTGO_DATRY | POTGO_DATLY
    );
    assert_eq!(pot.pot0dat, 0x0123);
    assert_eq!(pot.pot1dat, 0x02AB);
    assert_eq!(
        paula.pot_diagnostic_snapshot(),
        pot,
        "capturing pot diagnostics must not change pin or counter state"
    );
}

#[test]
fn log_diagnostic_snapshot_copies_bounded_logs_and_summarises_disk_logs() {
    let mut paula = Paula8364::new();
    for value in 0..18 {
        paula.write_intena(INT_SETCLR | value);
    }
    paula.write_intreq(INT_SETCLR | INT_VERTB);
    paula.write_intreq(INT_VERTB);
    paula.note_disk_write_dma_word(0x1111);
    paula.note_disk_write_dma_word(0x2222);
    paula.note_disk_write_pio_word(0x3333);

    let snapshot = paula.log_diagnostic_snapshot();

    assert_eq!(snapshot.intena_write_count, 16);
    assert_eq!(snapshot.intena_writes.len(), 16);
    assert_eq!(snapshot.intena_writes[0], INT_SETCLR | 2);
    assert_eq!(snapshot.last_intena_write, Some(INT_SETCLR | 17));
    assert_eq!(snapshot.intreq_writes, [INT_SETCLR | INT_VERTB, INT_VERTB]);
    assert_eq!(snapshot.intreq_write_count, 2);
    assert_eq!(snapshot.last_intreq_write, Some(INT_VERTB));
    assert_eq!(snapshot.disk_write_dma_count, 2);
    assert_eq!(snapshot.last_disk_write_dma_word, Some(0x2222));
    assert_eq!(snapshot.disk_write_pio_count, 1);
    assert_eq!(snapshot.last_disk_write_pio_word, Some(0x3333));

    assert_eq!(paula.debug_intena_writes().len(), 16);
    assert_eq!(paula.debug_intreq_writes().len(), 2);
    assert_eq!(paula.debug_disk_write_dma_log(), &[0x1111, 0x2222]);
    assert_eq!(paula.debug_disk_write_pio_log(), &[0x3333]);
    assert_eq!(
        paula.log_diagnostic_snapshot(),
        snapshot,
        "capturing log diagnostics must not consume retained entries"
    );
}
