use commodore_agnus_ocs::{Agnus, BlitterBus, DmaTransferTarget, bits};

#[derive(Default)]
struct Bus {
    writes: Vec<(u32, u16)>,
}
impl BlitterBus for Bus {
    fn read_word(&mut self, _: u32) -> u16 {
        panic!("D-only blit read memory");
    }
    fn write_word(&mut self, address: u32, value: u16) {
        self.writes.push((address, value));
    }
}

#[test]
fn accepted_d_words_retire_once_from_retained_addresses_and_replay() {
    for width in [1, 2, 4, 8, 16, 32, 64, 128] {
        let mut chip = Agnus::new();
        chip.dmacon = bits::DMACON_DMAEN | bits::DMACON_BLTEN;
        chip.bltcon0 = 0x01ff;
        chip.blt_dpt = 0x40000;
        let size = if width == 128 {
            0x80
        } else {
            0x40 | (width & 63)
        };
        chip.write_blitter_register(0x58, size);
        let mut bus = Bus::default();
        let mut admitted = Vec::new();
        let mut interrupts = 0;
        for cck in 0..1024 {
            chip.tick_cck();
            assert_eq!(chip.begin_dma_cck(), None);
            if let Some(transfer) = chip.claim_dma_service() {
                let before = bus.writes.len();
                let outcome = chip.service_blitter_dma(transfer, &mut bus);
                interrupts += usize::from(outcome.interrupt);
                assert_eq!(outcome.bus_used, bus.writes.len() != before);
                if let DmaTransferTarget::Blitter {
                    write_value: Some(value),
                    ..
                }
                | DmaTransferTarget::BlitterFinalWrite { value } = transfer.target
                {
                    assert_eq!(&bus.writes[before..], &[(transfer.address, value)]);
                }
                assert_eq!(chip.claim_dma_service(), None);
            }
            let before = bus.writes.len();
            let outcome = chip.admit_blitter_dma_cck(true);
            interrupts += usize::from(outcome.interrupt);
            assert!(!outcome.bus_used, "admission claimed physical memory");
            assert_eq!(
                bus.writes.len(),
                before,
                "admission touched memory at {cck}"
            );
            if let Some(commodore_agnus_ocs::DmaAddressStage::Transfer(transfer)) =
                chip.dma_pipeline().address()
                && matches!(
                    transfer.target,
                    DmaTransferTarget::Blitter {
                        write_value: Some(_),
                        ..
                    } | DmaTransferTarget::BlitterFinalWrite { .. }
                )
            {
                admitted.push(transfer.address);
            }
            let data = postcard::to_allocvec(&chip).expect("save all pending blitter stages");
            let restored: Agnus = postcard::from_bytes(&data).expect("restore blitter stages");
            assert_eq!(
                data,
                postcard::to_allocvec(&restored).expect("save restored stages")
            );
            chip = restored;
            if !chip.blitter_busy {
                break;
            }
        }
        let expected: Vec<_> = (0..width)
            .map(|word| (0x40000 + 2 * u32::from(word), 0xffff))
            .collect();
        assert_eq!(bus.writes, expected, "width {width}");
        assert_eq!(
            admitted,
            expected
                .iter()
                .map(|(address, _)| *address)
                .collect::<Vec<_>>()
        );
        assert!(!chip.blitter_busy);
        assert_eq!(interrupts, 1);
    }
}

#[test]
fn all_reference_channel_programs_retire_one_cck_after_admission() {
    use commodore_agnus_ocs::BlitterDmaOp;
    use std::collections::BTreeSet;
    #[derive(Default)]
    struct TraceBus {
        events: Vec<(u32, char, u32)>,
        cck: u32,
    }
    impl BlitterBus for TraceBus {
        fn read_word(&mut self, address: u32) -> u16 {
            self.events.push((self.cck, 'R', address));
            0xffff
        }
        fn write_word(&mut self, address: u32, _: u16) {
            self.events.push((self.cck, 'W', address));
        }
    }
    let mut cases = BTreeSet::new();
    for row in include_str!("../../../test-data/commodore/amiga/area-dma/reference-programs.tsv")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let values: Vec<u16> = row
            .split_whitespace()
            .map(|value| value.parse().expect("compiled reference integer"))
            .collect();
        let (mode, fill) = (values[0], values[1] != 0);
        assert!(cases.insert((mode, fill)));
        let flags = &values[2..];
        let mut chip = Agnus::new();
        chip.bltcon0 = (mode << 8) | 0xff;
        chip.bltcon1 = if fill { 0x000a } else { 0 };
        chip.blt_apt = 0x1000;
        chip.blt_bpt = 0x2000;
        chip.blt_cpt = 0x3000;
        chip.blt_dpt = 0x4000;
        chip.blt_afwm = 0xffff;
        chip.blt_alwm = 0xffff;
        chip.dmacon = bits::DMACON_DMAEN | bits::DMACON_BLTEN;
        chip.write_blitter_register(0x58, 0x44);
        let mut bus = TraceBus::default();
        let mut expected = Vec::new();
        let mut pointers = [0x1000, 0x2000, 0x3000, 0x4000];
        for word in 0..4 {
            for (phase, flag) in flags.iter().enumerate() {
                let admitted_at = (2 + word * flags.len() + phase) as u32;
                let channel = if flag & 8 != 0 {
                    Some(0)
                } else if flag & 16 != 0 {
                    Some(1)
                } else if flag & 32 != 0 {
                    Some(2)
                } else if flag & 4 != 0 {
                    Some(3)
                } else {
                    None
                };
                if let Some(channel) = channel.filter(|&channel| channel != 3 || word != 0) {
                    expected.push((
                        admitted_at + 1,
                        if channel == 3 { 'W' } else { 'R' },
                        pointers[channel],
                    ));
                    pointers[channel] = if fill {
                        pointers[channel] - 2
                    } else {
                        pointers[channel] + 2
                    };
                }
            }
        }
        if mode & 1 != 0 {
            expected.push(((2 + 4 * flags.len() + 2) as u32, 'W', pointers[3]));
        }
        for cck in 0..256 {
            bus.cck = cck;
            chip.tick_cck();
            assert_eq!(chip.begin_dma_cck(), None);
            if let Some(transfer) = chip.claim_dma_service() {
                let before = bus.events.len();
                let outcome = chip.service_blitter_dma(transfer, &mut bus);
                assert_eq!(outcome.bus_used, bus.events.len() != before);
                assert_eq!(chip.claim_dma_service(), None);
            }
            if cck >= 2 && cck < 2 + 4 * flags.len() as u32 {
                let flag = flags[(cck as usize - 2) % flags.len()];
                let operation = if flag & 8 != 0 {
                    BlitterDmaOp::ReadA
                } else if flag & 16 != 0 {
                    BlitterDmaOp::ReadB
                } else if flag & 32 != 0 {
                    BlitterDmaOp::ReadC
                } else if flag & 4 != 0 {
                    BlitterDmaOp::WriteD
                } else {
                    BlitterDmaOp::Internal
                };
                assert_eq!(
                    chip.next_blitter_dma_request(),
                    Some(operation),
                    "{mode}/{fill} at {cck}"
                );
            }
            let before = bus.events.len();
            let _ = chip.admit_blitter_dma_cck(true);
            assert_eq!(before, bus.events.len());
            let state = postcard::to_allocvec(&chip).expect("save queued channels");
            chip = postcard::from_bytes(&state).expect("restore queued channels");
            if !chip.blitter_busy {
                break;
            }
        }
        assert!(!chip.blitter_busy, "{mode}/{fill} failed to drain");
        assert_eq!(bus.events, expected, "reference program {mode}/{fill}");
    }
    assert_eq!(cases.len(), 32);
}
