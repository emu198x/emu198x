//! Independent compiled reference schedules, including late IRQ changes.
#[path = "../examples/manual_boundary_probe.rs"]
mod boundary;
#[path = "../examples/manual_interrupt_probe.rs"]
mod interrupt;

#[test]
fn winuae_manual_boundaries() {
    boundary::main();
}

#[test]
fn vamiga_manual_start_and_holding() {
    interrupt::main();
}

#[test]
fn manual_attachment_irq_edges_and_target_registers_match_winuae() {
    use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState};
    let reference: Vec<Vec<u32>> =
        include_str!("../../../test-data/commodore/amiga/paula-audio/manual-probe/attachments.csv")
            .lines()
            .map(|line| {
                line.split(',')
                    .map(|v| v.parse().expect("reference integer"))
                    .collect()
            })
            .collect();
    assert_eq!(reference.len(), 576);
    let mut rows = 0;
    for source in 0..4u8 {
        for attach in [0, 1, 16, 17] {
            for ack in [false, true] {
                let mut p = Paula8364::new();
                for channel in 0..4 {
                    p.write_audio(channel, AudioField::Per, 8);
                    p.write_audio(channel, AudioField::Vol, 64);
                }
                let irq = 0x80 << source;
                p.write_adkcon(0x8000 | (attach << source));
                p.write_audio(source, AudioField::Dat, 0x0011);
                for time in 0..=17 {
                    if time != 0 {
                        p.begin_audio_cck();
                        if ack && (time == 1 || time == 9) {
                            p.write_intreq(irq);
                        }
                        if time == 4 {
                            p.write_audio(source, AudioField::Dat, 0x0022);
                        }
                        if time == 12 {
                            p.write_audio(source, AudioField::Dat, 0x0033);
                        }
                        p.finish_audio_cck(0, None, |_| panic!("manual attachment requested DMA"));
                    }
                    let channels = p.audio_diagnostic_snapshot().channels;
                    let ch = channels[usize::from(source)];
                    let state = match ch.state {
                        PaulaAudioDmaState::Idle => 0,
                        PaulaAudioDmaState::Playing => {
                            if ch.next_byte_is_high {
                                3
                            } else {
                                2
                            }
                        }
                        _ => panic!("manual attachment entered DMA wait"),
                    };
                    let (period, volume) = if source < 3 {
                        let target = channels[usize::from(source + 1)];
                        (u32::from(target.period), u32::from(target.volume))
                    } else {
                        (0, 0)
                    };
                    let row = vec![
                        u32::from(source),
                        u32::from(attach),
                        u32::from(ack),
                        time,
                        state,
                        u32::from(p.intreq() & irq != 0),
                        period,
                        volume,
                    ];
                    assert_eq!(row, reference[rows], "attachment observation {rows}");
                    rows += 1;
                }
            }
        }
    }
    assert_eq!(rows, reference.len());
}
