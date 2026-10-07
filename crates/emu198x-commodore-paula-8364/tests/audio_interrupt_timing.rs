//! Executable reference observations of DMA loop interrupts. Scheduling and memory grants are controlled by the adapter;
//! these tests do not assert full-machine bus or CPU IPL timing.

use emu198x_commodore_paula_8364::{AudioField, Paula8364, bits::*};

fn expected(kind: &str) -> Vec<Vec<i32>> {
    include_str!("../../../test-data/commodore/amiga/paula-audio/interrupt-probe/reference.csv")
        .lines()
        .filter(|line| line.starts_with(kind))
        .map(|line| {
            line.split(',')
                .skip(1)
                .map(|v| v.parse().expect("reference integer"))
                .collect()
        })
        .collect()
}

#[test]
fn dma_loop_interrupts_wait_for_the_selected_byte_transition_and_irq_delay() {
    let reference = expected("dma,");
    assert_eq!(reference.len(), 4896);
    let mut rows = 0;
    let mut differences = [0; 4];
    for (mode, difference_count) in differences.iter_mut().enumerate() {
        for arrival in [1, 7, 15, 17, 23, 31] {
            for (source, dma_bit) in DMA_AUD.into_iter().enumerate() {
                let mut p = Paula8364::new();
                p.write_audio(source as u8, AudioField::LcLo, 0x1000);
                p.write_audio(source as u8, AudioField::Len, 1);
                p.write_audio(source as u8, AudioField::Per, 16);
                let attach = if mode & 1 != 0 {
                    ADKCON_USE_PER[source]
                } else {
                    0
                } | if mode & 2 != 0 {
                    ADKCON_USE_VOL[source]
                } else {
                    0
                };
                p.write_adkcon(INT_SETCLR | attach);
                let dma = DMA_MASTER | dma_bit;
                let irq = INT_AUD0 << source;
                p.tick_audio_cck(dma, None, |_| 0);
                // Origin: real startup word accepted after the dummy. The
                // startup IRQ is cleared to isolate the following wrap.
                let (address, reload) = p.audio_dma_request(source as u8).expect("dummy request");
                p.service_audio_dma_word(source as u8, address, reload, 0);
                // Give even a delayed startup IRQ time to become visible.
                p.tick_audio_cck(dma, None, |_| 0);
                p.write_intreq(irq);
                let (address, reload) = p
                    .audio_dma_request(source as u8)
                    .expect("real startup request");
                p.service_audio_dma_word(source as u8, address, reload, 0x1122);
                let mut delivered = false;
                for time in 0..=50 {
                    if time != 0 {
                        p.begin_audio_cck();
                        if !delivered
                            && time >= arrival
                            && let Some((address, reload)) = p.audio_dma_request(source as u8)
                        {
                            assert!(reload, "one-word buffer must wrap");
                            p.service_audio_dma_word(source as u8, address, reload, 0x3344);
                            delivered = true;
                        }
                        p.finish_audio_cck(dma, None, |_| 0);
                    }
                    let row = vec![
                        source as i32,
                        mode as i32,
                        arrival,
                        time,
                        i32::from(p.intreq() & irq != 0),
                        i32::from(delivered),
                    ];
                    if row != reference[rows] {
                        *difference_count += 1;
                        eprintln!("dma native={row:?} reference={:?}", reference[rows]);
                    }
                    rows += 1;
                }
            }
        }
    }
    assert_eq!(rows, reference.len());
    eprintln!("DMA mismatches by attachment mode: {differences:?}");
    assert_eq!(differences, [0; 4]);
}

#[test]
fn dma_stop_discards_a_held_loop_but_preserves_an_issued_irq() {
    for (source, dma_bit) in DMA_AUD.into_iter().enumerate() {
        let mut p = Paula8364::new();
        p.write_audio(source as u8, AudioField::Len, 1);
        p.write_audio(source as u8, AudioField::Per, 8);
        let dma = DMA_MASTER | dma_bit;
        let irq = INT_AUD0 << source;
        p.tick_audio_cck(dma, Some(source as u8), |_| 0);
        assert!(p.audio_diagnostic_snapshot().channels[source].interrupt_request_pending);
        // Disable before the delivery boundary. The issued IRQ still arrives.
        p.tick_audio_cck(0, None, |_| 0);
        assert_ne!(p.intreq() & irq, 0);
        p.write_intreq(irq);
        // Restart, discard the dummy, then supply the real and wrapped words.
        p.tick_audio_cck(dma, Some(source as u8), |_| 0);
        p.tick_audio_cck(dma, Some(source as u8), |_| 0);
        p.write_intreq(irq);
        p.tick_audio_cck(dma, Some(source as u8), |_| 0);
        assert!(p.audio_diagnostic_snapshot().channels[source].loop_interrupt_pending);
        p.tick_audio_cck(0, None, |_| 0);
        assert!(!p.audio_diagnostic_snapshot().channels[source].loop_interrupt_pending);
        for _ in 0..32 {
            p.tick_audio_cck(0, None, |_| 0);
        }
        assert_eq!(
            p.intreq() & irq,
            0,
            "unissued loop IRQ is discarded on stop"
        );
    }
}
