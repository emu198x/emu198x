//! Reference-backed live attachment transitions, including unchanged controls.
#[path = "support/live_attachment.rs"]
mod probe;

#[test]
fn live_attachment_matches_all_reference_observations() {
    probe::check(include_str!(
        "../../../test-data/commodore/amiga/paula-audio/live-attachment-probe/winuae.csv"
    ))
    .expect("complete reference matrix");
}

use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState};

#[test]
fn warm_buffer_survives_attached_restart_and_cancelled_startup() {
    // WinUAE zerostate retains dat2; loaddat with volume attachment leaves it
    // untouched. vAmiga disableDMA and pbufld1 preserve the same buffer.
    // Compare the retained bytes with those actually played before stopping.
    let mut cases = 0;
    for source in 0..4u8 {
        for warm_word in [0x5566u16, 0x80ff] {
            for attachment in [1u16, 17] {
                // Manual restart, DMA restart, cancellation at either wait.
                for path in 0..4 {
                    let mut p = Paula8364::new();
                    let irq = 0x80 << source;
                    let dma = 0x200 | (1 << source);
                    p.write_audio(source, AudioField::Per, 8);
                    p.write_audio(source, AudioField::Len, 1);
                    p.write_audio(source, AudioField::Dat, warm_word);
                    let high = p.audio_state(source).expect("valid source").sample;
                    for _ in 0..8 {
                        p.tick_audio_cck(0, None, |_| panic!("no memory grant"));
                    }
                    let low = p.audio_state(source).expect("valid source").sample;
                    for _ in 0..8 {
                        p.tick_audio_cck(0, None, |_| panic!("no memory grant"));
                    }
                    assert_eq!(
                        p.audio_diagnostic_snapshot().channels[usize::from(source)].state,
                        PaulaAudioDmaState::Idle
                    );
                    p.begin_audio_cck();
                    p.write_intreq(irq);
                    p.write_adkcon(0x8000 | (attachment << source));
                    if path != 0 {
                        p.sync_audio_dma_control(dma);
                        assert_eq!(
                            p.audio_diagnostic_snapshot().channels[usize::from(source)]
                                .current_word,
                            Some(warm_word)
                        );
                        if path != 2 {
                            let (address, reload) =
                                p.audio_dma_request(source).expect("dummy request");
                            p.service_audio_dma_word(source, address, reload, 0xdead);
                        }
                    }
                    if path >= 2 {
                        p.sync_audio_dma_control(0);
                        assert_eq!(
                            p.audio_diagnostic_snapshot().channels[usize::from(source)]
                                .current_word,
                            Some(warm_word)
                        );
                        p.begin_audio_cck();
                        p.write_intreq(irq);
                    }
                    if path == 1 {
                        let (address, reload) =
                            p.audio_dma_request(source).expect("sample request");
                        p.service_audio_dma_word(source, address, reload, 0x1122);
                    } else {
                        p.write_audio(source, AudioField::Dat, 0x1122);
                    }
                    assert_eq!(p.audio_state(source).expect("valid source").sample, high);
                    let enabled = if path == 1 { dma } else { 0 };
                    for _ in 0..8 {
                        p.begin_audio_cck();
                        p.write_intreq(irq);
                        p.finish_audio_cck(enabled, None, |_| panic!("no memory grant"));
                    }
                    assert_eq!(p.audio_state(source).expect("valid source").sample, low);
                    assert_eq!(
                        p.audio_diagnostic_snapshot().channels[usize::from(source)].current_word,
                        Some(warm_word)
                    );
                    p.write_adkcon(0x11 << source);
                    p.write_audio(source, AudioField::Dat, 0x3344);
                    for _ in 0..8 {
                        p.begin_audio_cck();
                        p.write_intreq(irq);
                        p.finish_audio_cck(enabled, None, |_| panic!("no memory grant"));
                    }
                    assert_eq!(
                        p.audio_diagnostic_snapshot().channels[usize::from(source)].current_word,
                        Some(0x3344)
                    );
                    assert_eq!(p.audio_state(source).expect("valid source").sample, 0x33);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 64);
}
