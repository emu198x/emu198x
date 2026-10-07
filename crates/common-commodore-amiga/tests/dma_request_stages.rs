//! Registered FS-UAE uses two CCKs for display reservation -> service,
//! with pointer sampling in between, and one for other admitted transfers.
use commodore_agnus_ocs::{
    Agnus, BlitterDmaOp, DisplayDmaChannel, DisplayDmaReservation, DmaAddressStage, DmaStrobe,
    DmaTransfer, DmaTransferTarget,
};

fn next_cck(agnus: &mut Agnus) -> Option<DisplayDmaReservation> {
    agnus.tick_cck();
    agnus.begin_dma_cck()
}

fn round_trip(agnus: &Agnus) -> Agnus {
    assert!(agnus.dma_pipeline().validate().is_ok());
    let bytes = postcard::to_allocvec(agnus).expect("encode retained DMA stages");
    let restored = postcard::from_bytes(&bytes).expect("decode retained DMA stages");
    assert_eq!(
        bytes,
        postcard::to_allocvec(&restored).expect("encode restored stages")
    );
    restored
}

#[test]
fn connected_bplcon0_copies_after_two_edges_without_replacing_selected_display() {
    let mut chip = Agnus::new();
    chip.agnus_id = 0x2300;
    chip.max_bitplanes = 8;
    chip.bplcon0 = 0x1000;
    next_cck(&mut chip);
    let selected = DisplayDmaReservation {
        channel: DisplayDmaChannel::Bitplane(0),
        width_words: 1,
        fmode: 0,
        add_modulo: false,
    };
    assert!(chip.reserve_display_dma(selected));
    chip.write_bplcon0(0x2000);
    assert_eq!(chip.num_bitplanes(), 1);
    chip = round_trip(&chip);
    assert_eq!(next_cck(&mut chip), Some(selected));
    chip.address_display_dma(0x2000, 0);
    assert_eq!(chip.num_bitplanes(), 1, "first register-copy edge");
    chip = round_trip(&chip);
    next_cck(&mut chip);
    assert_eq!(chip.num_bitplanes(), 2, "reference BPLCON0_delayed edge");
    let transfer = chip
        .claim_dma_service()
        .expect("previously selected display");
    assert_eq!(transfer.address, 0x2000);
    assert!(
        matches!(transfer.target, DmaTransferTarget::Display { reservation, .. } if reservation == selected)
    );
}

#[test]
fn connected_fmode_changes_reservations_immediately_but_keeps_selected_identity() {
    let mut chip = Agnus::new();
    chip.agnus_id = 0x2300;
    chip.max_bitplanes = 8;
    chip.bplcon0 = 0x8000 | 0x4000;
    chip.dmacon = 0x0300;
    chip.ddfstrt = 0x30;
    chip.ddfstop = 0xd0;
    // Establish a running sequence using only ordinary CCK boundaries.
    while chip.hpos < 100 {
        let addressed = next_cck(&mut chip);
        if addressed.is_some() {
            chip.address_display_dma(0x2000, 0);
        }
        chip.claim_dma_service();
        chip.generate_display_dma_request(true, false, 227);
    }
    chip.write_fmode(3);
    chip = round_trip(&chip);
    let selected = next_cck(&mut chip).expect("old cadence selected at h100");
    assert_eq!(selected.width_words, 1);
    chip.address_display_dma(0x2000, 0);
    chip.claim_dma_service();
    chip.generate_display_dma_request(true, false, 227);
    let new = chip
        .dma_pipeline()
        .reservation()
        .expect("new cadence request");
    assert_eq!(new.width_words, 4, "FMODE setup_fmodes is immediate");
    chip = round_trip(&chip);
    assert_eq!(next_cck(&mut chip), Some(new));
    chip.address_display_dma(0x3000, 0);
    assert!(
        matches!(chip.claim_dma_service().expect("old identity retires").target,
        DmaTransferTarget::Display { reservation, .. } if reservation == selected)
    );
}

#[test]
fn display_samples_the_middle_pointer_and_retains_it_through_service() {
    for channel in [
        DisplayDmaChannel::Bitplane(0),
        DisplayDmaChannel::Bitplane(7),
        DisplayDmaChannel::Sprite {
            channel: 0,
            second_word: false,
            control: false,
        },
        DisplayDmaChannel::Sprite {
            channel: 7,
            second_word: true,
            control: true,
        },
    ] {
        for width_words in [1, 2, 4] {
            let mut original = Agnus::new();
            original.bpl_pt[0] = 0x1000;
            assert_eq!(next_cck(&mut original), None);
            let request = DisplayDmaReservation {
                channel,
                width_words,
                fmode: 0x000f,
                add_modulo: false,
            };
            assert!(original.reserve_display_dma(request));
            assert_eq!(original.dma_pipeline().service(), None);
            let mut restored = round_trip(&original);
            // A pointer rewrite after reservation must be sampled by addressing.
            for chip in [&mut original, &mut restored] {
                chip.bpl_pt[0] = 0x2000;
                assert_eq!(next_cck(chip), Some(request));
                assert_eq!(chip.dma_pipeline().service(), None);
                chip.address_display_dma(chip.bpl_pt[0], -4);
                chip.bpl_pt[0] = 0x3000;
            }
            assert_eq!(original.dma_pipeline(), restored.dma_pipeline());
            restored = round_trip(&original);
            for chip in [&mut original, &mut restored] {
                assert_eq!(next_cck(chip), None);
                assert_eq!(
                    chip.dma_pipeline().service(),
                    Some(DmaTransfer {
                        target: DmaTransferTarget::Display {
                            reservation: request,
                            pointer_modulo: -4,
                        },
                        address: 0x2000,
                    })
                );
                // Current service remains inspectable through both CPU phases.
                assert_eq!(round_trip(chip).dma_pipeline(), chip.dma_pipeline());
                assert!(chip.claim_dma_service().is_some());
                assert_eq!(chip.claim_dma_service(), None);
                let mut served_restore = round_trip(chip);
                assert_eq!(served_restore.claim_dma_service(), None);
                assert_eq!(next_cck(chip), None);
                assert_eq!(chip.dma_pipeline().service(), None);
            }
        }
    }
}

#[test]
fn every_other_client_enters_addressing_and_services_once_on_the_next_cck() {
    let targets = [
        DmaTransferTarget::Copper {
            instruction_word: 1,
        },
        DmaTransferTarget::Copper {
            instruction_word: 2,
        },
        DmaTransferTarget::Blitter {
            operation: BlitterDmaOp::ReadA,
            write_value: None,
        },
        DmaTransferTarget::Blitter {
            operation: BlitterDmaOp::WriteD,
            write_value: Some(0xa55a),
        },
        DmaTransferTarget::Disk {
            slot: 0,
            write: false,
        },
        DmaTransferTarget::Disk {
            slot: 2,
            write: true,
        },
        DmaTransferTarget::Audio {
            channel: 0,
            reload: false,
        },
        DmaTransferTarget::Audio {
            channel: 3,
            reload: true,
        },
        DmaTransferTarget::Refresh,
        DmaTransferTarget::Strobe(DmaStrobe::Horizontal),
        DmaTransferTarget::Strobe(DmaStrobe::VerticalBlank),
        DmaTransferTarget::Strobe(DmaStrobe::Equalisation),
    ];
    for target in targets {
        let mut agnus = Agnus::new();
        next_cck(&mut agnus);
        let request = DmaTransfer {
            target,
            address: 0x4000,
        };
        assert!(agnus.admit_dma_transfer(request));
        assert_eq!(agnus.dma_pipeline().service(), None);
        agnus = round_trip(&agnus);
        next_cck(&mut agnus);
        assert_eq!(agnus.dma_pipeline().service(), Some(request));
        assert_eq!(agnus.claim_dma_service(), Some(request));
        assert_eq!(agnus.claim_dma_service(), None);
        agnus = round_trip(&agnus);
        assert_eq!(agnus.claim_dma_service(), None);
        next_cck(&mut agnus);
        assert_eq!(agnus.dma_pipeline().service(), None);
    }
}

#[test]
fn a_display_reservation_blocks_replacement_before_and_after_addressing() {
    let mut agnus = Agnus::new();
    next_cck(&mut agnus);
    let request = DisplayDmaReservation {
        channel: DisplayDmaChannel::Bitplane(0),
        width_words: 1,
        fmode: 0,
        add_modulo: false,
    };
    assert!(agnus.reserve_display_dma(request));
    assert!(!agnus.reserve_display_dma(DisplayDmaReservation {
        channel: DisplayDmaChannel::Bitplane(1),
        ..request
    }));
    next_cck(&mut agnus);
    let intruder = DmaTransfer {
        target: DmaTransferTarget::Refresh,
        address: 0x1000,
    };
    assert!(!agnus.admit_dma_transfer(intruder));
    assert_eq!(
        agnus.dma_pipeline().address(),
        Some(DmaAddressStage::Display(request))
    );
    agnus.address_display_dma(0x2000, 2);
    assert!(!agnus.admit_dma_transfer(intruder));
    next_cck(&mut agnus);
    assert_eq!(
        agnus
            .dma_pipeline()
            .service()
            .expect("display service")
            .address,
        0x2000
    );
}

#[test]
#[should_panic(expected = "DMA stages advanced twice in one CCK")]
fn stage_service_cannot_be_repeated_in_the_second_master_phase() {
    let mut agnus = Agnus::new();
    next_cck(&mut agnus);
    agnus = round_trip(&agnus);
    agnus.begin_dma_cck();
}

#[test]
#[should_panic(expected = "display DMA reached service without an addressed descriptor")]
fn a_missed_addressing_stage_cannot_silently_drop_the_reserved_transfer() {
    let mut agnus = Agnus::new();
    next_cck(&mut agnus);
    assert!(agnus.reserve_display_dma(DisplayDmaReservation {
        channel: DisplayDmaChannel::Bitplane(0),
        width_words: 1,
        fmode: 0,
        add_modulo: false,
    }));
    next_cck(&mut agnus);
    next_cck(&mut agnus);
}

#[test]
fn malformed_saved_dma_targets_are_rejected_before_array_access_or_service() {
    let malformed = [
        DmaTransferTarget::Copper {
            instruction_word: 0,
        },
        DmaTransferTarget::Copper {
            instruction_word: 3,
        },
        DmaTransferTarget::Audio {
            channel: 4,
            reload: false,
        },
        DmaTransferTarget::Disk {
            slot: 3,
            write: false,
        },
        DmaTransferTarget::Blitter {
            operation: BlitterDmaOp::ReadA,
            write_value: Some(1),
        },
        DmaTransferTarget::Blitter {
            operation: BlitterDmaOp::WriteD,
            write_value: None,
        },
        DmaTransferTarget::Blitter {
            operation: BlitterDmaOp::Internal,
            write_value: None,
        },
    ];
    for target in malformed {
        let mut agnus = Agnus::new();
        next_cck(&mut agnus);
        assert!(agnus.admit_dma_transfer(DmaTransfer {
            target,
            address: 0x1000
        }));
        assert!(agnus.dma_pipeline().validate().is_err());
        next_cck(&mut agnus);
        assert!(agnus.dma_pipeline().validate().is_err());
    }
    for (channel, width_words) in [
        (DisplayDmaChannel::Bitplane(8), 1),
        (
            DisplayDmaChannel::Sprite {
                channel: 8,
                second_word: false,
                control: false,
            },
            1,
        ),
        (DisplayDmaChannel::Bitplane(0), 0),
        (DisplayDmaChannel::Bitplane(0), 3),
    ] {
        let mut agnus = Agnus::new();
        next_cck(&mut agnus);
        assert!(agnus.reserve_display_dma(DisplayDmaReservation {
            channel,
            width_words,
            fmode: 0,
            add_modulo: false,
        }));
        assert!(agnus.dma_pipeline().validate().is_err());
    }
}

#[test]
#[should_panic(expected = "DMA service missed before the next CCK")]
fn an_unserviced_admitted_transfer_cannot_silently_disappear() {
    let mut agnus = Agnus::new();
    next_cck(&mut agnus);
    assert!(agnus.admit_dma_transfer(DmaTransfer {
        target: DmaTransferTarget::Copper {
            instruction_word: 1
        },
        address: 0x1000,
    }));
    next_cck(&mut agnus);
    assert!(agnus.dma_pipeline().service().is_some());
    next_cck(&mut agnus);
}

#[test]
fn bitplane_address_sampling_matches_all_registered_ptmod_rows() {
    let reference =
        include_str!("../../../test-data/commodore/amiga/display-dma-address/registered-ptmod.csv");
    let mut rows = 0;
    let mut services = 0;
    for line in reference.lines() {
        let row: Vec<i32> = line
            .split(',')
            .map(|value| value.parse().expect("reference number"))
            .collect();
        assert_eq!(row.len(), 7);
        rows += 1;
        for width_words in [1, 2, 4] {
            let plane = row[0] as u8;
            let request = DisplayDmaReservation {
                channel: DisplayDmaChannel::Bitplane(plane),
                width_words,
                fmode: 0,
                add_modulo: row[1] != 0,
            };
            let mut agnus = Agnus::new();
            agnus.hpos = 60;
            agnus.vpos = row[2] as u16;
            next_cck(&mut agnus);
            agnus.bpl_pt[usize::from(plane)] = 0x1000;
            assert!(agnus.reserve_display_dma(request));
            agnus = round_trip(&agnus);
            assert_eq!(next_cck(&mut agnus), Some(request));
            // PT and MOD are sampled now, not from their reservation values.
            agnus.bpl_pt[usize::from(plane)] = row[5] as u32;
            agnus.bpl1mod = -4;
            agnus.bpl2mod = 6;
            agnus.diwstrt = (row[3] as u16) << 8;
            agnus.fmode = if row[4] != 0 { 0x4000 } else { 0 };
            agnus.sample_bitplane_dma_address(request);
            agnus = round_trip(&agnus);
            agnus.bpl_pt[usize::from(plane)] = 0x9000;
            agnus.bpl1mod = 100;
            agnus.bpl2mod = 200;
            agnus.fmode ^= 0x4000;
            agnus.dmacon = 0;
            next_cck(&mut agnus);
            assert_eq!(
                agnus.claim_dma_service(),
                Some(DmaTransfer {
                    target: DmaTransferTarget::Display {
                        reservation: request,
                        pointer_modulo: row[6],
                    },
                    address: row[5] as u32,
                })
            );
            assert_eq!(agnus.claim_dma_service(), None);
            agnus = round_trip(&agnus);
            assert_eq!(agnus.claim_dma_service(), None);
            services += 1;
        }
    }
    assert_eq!(
        rows, 128,
        "reference coverage must be positive and complete"
    );
    assert_eq!(services, 384);
}

#[test]
fn bitplane_service_matches_registered_live_fmode_width_and_lane_rows() {
    use commodore_denise_ocs::DeniseOcs;
    use common_commodore_amiga::denise::{BitplaneDmaInput, Denise, DeniseOutputSignals};
    use common_commodore_amiga::memory::Memory;
    let reference = include_str!(
        "../../../test-data/commodore/amiga/display-dma-address/registered-service.csv"
    );
    let width_for_mode = |mode| match mode {
        0 => 1,
        3 => 4,
        _ => 2,
    };
    let mut rows = 0;
    let mut seen = 0;
    let mut page_mode_disagreements = 0;
    for line in reference.lines() {
        let row: Vec<i32> = line
            .split(',')
            .map(|value| value.parse().expect("service reference number"))
            .collect();
        assert_eq!(row.len(), 10);
        seen += 1;
        // The registered FS-UAE producer routes mode 2 through fetch64.
        // Lisa's spec and vendored WinUAE use two 16-bit CAS transfers.
        // Keep these source rows as explicit disagreements, not goldens.
        if row[1] == 2 {
            assert_eq!(row[5], 4, "known reference branch must remain explicit");
            page_mode_disagreements += 1;
            continue;
        }
        let mut agnus = Agnus::new();
        agnus.hpos = 60;
        agnus.bpl_pt[0] = 0x9000;
        agnus.bpl1mod = 200;
        agnus.fmode = row[1] as u16;
        let mut memory = Memory::new(vec![0; 256 * 1024]);
        for (index, word) in [0x1111, 0x2222, 0x3333, 0x4444].into_iter().enumerate() {
            memory.write_word(0x2000 + index as u32 * 2, word);
        }
        let transfer = DmaTransfer {
            address: row[2] as u32,
            target: DmaTransferTarget::Display {
                reservation: DisplayDmaReservation {
                    channel: DisplayDmaChannel::Bitplane(0),
                    width_words: width_for_mode(row[0]),
                    fmode: row[0] as u16,
                    add_modulo: true,
                },
                pointer_modulo: row[3],
            },
        };
        let mut denise = Denise::<DeniseOcs>::new();
        denise.tick_with_dma_output_signals(
            0,
            Some(BitplaneDmaInput::Serviced(transfer)),
            DeniseOutputSignals::unblanked(false),
            &mut agnus,
            &memory,
            227,
        );
        let payload = denise
            .board_pipeline_diagnostic_snapshot()
            .pending_bitplane_dma
            .expect("reference service must produce actual retained data");
        assert_eq!(
            payload.width_words, row[5] as u8,
            "row {rows}: actual service width"
        );
        let expected = [row[6] as u16, row[7] as u16, row[8] as u16, row[9] as u16];
        assert_eq!(payload.words, expected, "row {rows}: actual retained lanes");
        assert_eq!(
            agnus.bpl_pt[0], row[4] as u32,
            "row {rows}: captured PT/MOD plus actual bytes"
        );
        rows += 1;
    }
    assert_eq!(seen, 128);
    assert_eq!(page_mode_disagreements, 32);
    assert_eq!(
        rows, 96,
        "all supported reference mode changes, lanes and signed modulos must be compared"
    );
}
