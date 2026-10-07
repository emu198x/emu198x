//! Combined RGA effects are checked against compiled registered source, including
//! suppressed data strobes, retained refresh addresses and pointer side effects.
use commodore_agnus_ocs::{
    Agnus, DisplayDmaChannel, DisplayDmaReservation, DmaStrobe, DmaTransfer, DmaTransferTarget,
    SlotOwner,
};
use commodore_denise_ocs::DeniseOcs;
use common_commodore_amiga::{
    denise::{BitplaneDmaInput, Denise, DeniseOutputSignals},
    memory::Memory,
};

fn edge(chip: &mut Agnus) {
    chip.tick_cck();
    if let Some(request) = chip.begin_dma_cck() {
        chip.sample_bitplane_dma_address(request);
    }
}
fn replay(chip: &Agnus) -> Agnus {
    chip.dma_pipeline()
        .validate()
        .expect("valid combined state");
    let bytes = postcard::to_allocvec(chip).expect("save RGA");
    let restored = postcard::from_bytes(&bytes).expect("restore RGA");
    assert_eq!(
        bytes,
        postcard::to_allocvec(&restored).expect("save restored RGA")
    );
    restored
}

#[test]
fn combined_bitplane_refresh_service_matches_all_compiled_reference_rows() {
    let mut rows = 0;
    for row in
        include_str!("../../../test-data/commodore/amiga/rga-conflicts/reference-service.csv")
            .lines()
            .skip(1)
    {
        let r: Vec<u32> = row
            .split(',')
            .map(|s| s.parse().expect("reference integer"))
            .collect();
        assert_eq!(r.len(), 10);
        let mut chip = Agnus::new();
        chip.agnus_id = match r[0] {
            0 => 0,
            1 => 0x2000,
            _ => 0x2300,
        };
        chip.max_bitplanes = if r[0] == 2 { 8 } else { 6 };
        chip.fmode = match r[1] {
            0 => 0,
            1 => 1,
            _ => 3,
        };
        edge(&mut chip);
        assert!(chip.admit_refresh_dma(DmaTransfer {
            target: DmaTransferTarget::Refresh,
            address: chip.refresh_dma_pointer()
        }));
        edge(&mut chip);
        chip.claim_dma_service()
            .expect("preceding ordinary refresh");
        chip.service_refresh_dma();
        edge(&mut chip);
        let selected = DisplayDmaReservation {
            channel: DisplayDmaChannel::Bitplane(r[2] as u8),
            width_words: chip.bpl_fetch_width(),
            fmode: chip.fmode,
            add_modulo: true,
        };
        chip.bpl_pt[r[2] as usize] = 0x30000;
        chip.bpl1mod = -6;
        chip.bpl2mod = -6;
        assert!(chip.reserve_display_dma(selected));
        chip = replay(&chip);
        edge(&mut chip);
        let fixed = match r[3] {
            0x38 => DmaTransferTarget::Strobe(DmaStrobe::Equalisation),
            0x3a => DmaTransferTarget::Strobe(DmaStrobe::VerticalBlank),
            0x3c => DmaTransferTarget::Strobe(DmaStrobe::Horizontal),
            _ => DmaTransferTarget::Refresh,
        };
        chip.lol = r[3] == 0x3e;
        assert!(chip.admit_refresh_dma(DmaTransfer {
            target: fixed,
            address: chip.refresh_dma_pointer()
        }));
        chip = replay(&chip);
        edge(&mut chip);
        assert_eq!(chip.cck_bus_plan().slot_owner, SlotOwner::Refresh);
        let transfer = chip.claim_dma_service().expect("combined service");
        assert_eq!(
            transfer.target,
            DmaTransferTarget::DisplayRefresh {
                reservation: selected,
                fixed_register: r[3] as u16,
            }
        );
        assert_eq!(transfer.address, r[5], "captured refresh address");
        assert_eq!(selected.rga_register() & r[3] as u16, r[4] as u16);
        chip.service_combined_refresh_dma(transfer);
        let mut denise = Denise::<DeniseOcs>::new();
        let data_selected = (0x110..0x120).contains(&(selected.rga_register() & r[3] as u16));
        assert_eq!(data_selected, r[8] != 0);
        if data_selected {
            let memory = Memory::new(vec![0; 256 * 1024]);
            denise.tick_with_dma_output_signals(
                0,
                Some(BitplaneDmaInput::Serviced(transfer)),
                DeniseOutputSignals::unblanked(false),
                &mut chip,
                &memory,
                227,
            );
            let payload = denise
                .board_pipeline_diagnostic_snapshot()
                .pending_bitplane_dma
                .expect("reference-selected data strobe");
            assert_eq!(u32::from(payload.width_words), 1 << r[1]);
            assert_eq!(r[8], 1);
            assert_eq!(r[9], 1);
        } else {
            chip.service_displaced_display_dma(transfer);
            assert!(
                denise
                    .board_pipeline_diagnostic_snapshot()
                    .pending_bitplane_dma
                    .is_none()
            );
            assert_eq!(r[9], 0);
        }
        assert_eq!(chip.refresh_dma_pointer(), r[6]);
        assert_eq!(
            chip.bpl_pt[r[2] as usize], r[7],
            "reference combined pointer effect"
        );
        assert_eq!(chip.claim_dma_service(), None);
        chip = replay(&chip);
        assert_eq!(
            chip.cck_bus_plan().slot_owner,
            SlotOwner::Refresh,
            "second CPU phase retains owner"
        );
        edge(&mut chip);
        assert_eq!(chip.cck_bus_plan().slot_owner, SlotOwner::Cpu);
        rows += 1;
    }
    assert_eq!(rows, 200);
}

#[test]
fn combined_sprite_refresh_service_matches_all_compiled_reference_rows() {
    let mut rows = 0;
    for row in include_str!("../../../test-data/commodore/amiga/rga-conflicts/reference-sprite.csv")
        .lines()
        .skip(1)
    {
        let r: Vec<u32> = row
            .split(',')
            .map(|s| s.parse().expect("reference integer"))
            .collect();
        assert_eq!(r.len(), 12);
        let mut chip = Agnus::new();
        chip.agnus_id = match r[0] {
            0 => 0,
            1 => 0x2000,
            _ => 0x2300,
        };
        chip.max_bitplanes = if r[0] == 2 { 8 } else { 6 };
        chip.fmode = match r[1] {
            0 => 0,
            1 => 1 << 2,
            _ => 3 << 2,
        };
        edge(&mut chip);
        assert!(chip.admit_refresh_dma(DmaTransfer {
            target: DmaTransferTarget::Refresh,
            address: chip.refresh_dma_pointer()
        }));
        edge(&mut chip);
        chip.claim_dma_service()
            .expect("preceding ordinary refresh");
        chip.service_refresh_dma();
        edge(&mut chip);
        let selected = DisplayDmaReservation {
            channel: DisplayDmaChannel::Sprite {
                channel: r[2] as u8,
                control: r[3] != 0,
                second_word: r[4] != 0,
            },
            width_words: chip.spr_fetch_width(),
            fmode: chip.fmode,
            add_modulo: false,
        };
        chip.spr_pt[r[2] as usize] = 0x30000;
        assert!(chip.reserve_display_dma(selected));
        chip = replay(&chip);
        edge(&mut chip);
        let fixed = match r[5] {
            0x38 => DmaTransferTarget::Strobe(DmaStrobe::Equalisation),
            0x3a => DmaTransferTarget::Strobe(DmaStrobe::VerticalBlank),
            0x3c => DmaTransferTarget::Strobe(DmaStrobe::Horizontal),
            _ => DmaTransferTarget::Refresh,
        };
        chip.lol = r[5] == 0x3e;
        assert!(chip.admit_refresh_dma(DmaTransfer {
            target: fixed,
            address: chip.refresh_dma_pointer()
        }));
        chip = replay(&chip);
        edge(&mut chip);
        let transfer = chip.claim_dma_service().expect("combined sprite service");
        assert_eq!(transfer.address, r[7]);
        let register = selected.rga_register() & r[5] as u16;
        assert_eq!(register, r[6] as u16);
        chip.service_combined_refresh_dma(transfer);
        let mut reads = 0;
        if (0x140..0x180).contains(&register) {
            let width = chip.spr_fetch_width();
            let (control, data) = chip.service_retained_sprite_dma(transfer, width, |_| {
                reads += 1;
                0xABCD
            });
            assert_eq!(control, r[3] != 0);
            assert_eq!(data as u16, 0xABCD, "actual sprite payload");
            assert_eq!(reads, if control { 1 } else { u32::from(width) });
            assert_eq!(
                r[10], 1,
                "reference invokes one width-selected fetch helper"
            );
            assert_eq!(r[11], 1);
        } else {
            chip.service_displaced_display_dma(transfer);
            assert_eq!(r[10], 0);
            assert_eq!(r[11], 0);
            assert_eq!(reads, 0);
        }
        let mask = match r[0] {
            0 => 0x7ffff,
            1 => 0xfffff,
            _ => 0x1fffff,
        };
        assert_eq!(chip.refresh_dma_pointer(), r[8] & mask);
        assert_eq!(
            chip.spr_pt[r[2] as usize], r[9],
            "reference sprite PT side effect"
        );
        chip = replay(&chip);
        assert_eq!(chip.cck_bus_plan().slot_owner, SlotOwner::Refresh);
        assert_eq!(chip.claim_dma_service(), None);
        rows += 1;
    }
    assert_eq!(rows, 800);
}
