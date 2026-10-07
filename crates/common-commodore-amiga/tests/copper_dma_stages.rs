use commodore_agnus_ocs::DmaTransferTarget;
use common_commodore_amiga::copper::Copper;

fn restore(chip: Copper) -> Copper {
    postcard::from_bytes(&postcard::to_allocvec(&chip).expect("save Copper input stage"))
        .expect("restore Copper input stage")
}

#[test]
fn captured_fetch_address_and_serviced_ir1_survive_ram_and_pointer_changes() {
    let mut chip = Copper::new();
    chip.pc = 0x2000;
    let first = chip
        .request_dma_cck(60, 65, true, false)
        .expect("IR1 request");
    assert_eq!(first.address, 0x2000);
    assert_eq!(
        first.target,
        DmaTransferTarget::Copper {
            instruction_word: 1
        }
    );
    assert_eq!(chip.pc, 0x2000, "admission must not retire the instruction");
    assert!(!chip.bus_used_this_cck);
    chip = restore(chip);
    // A pointer change after admission must not redirect this read.
    chip.pc = 0x3000;
    assert_eq!(chip.service_dma_fetch(1, first.address, 0x180), None);
    assert_eq!(chip.pc, 0x2002);
    assert!(chip.bus_used_this_cck);
    chip.bus_used_this_cck = false;
    let second = chip
        .request_dma_cck(60, 67, true, false)
        .expect("IR2 request");
    assert_eq!(second.address, 0x2002);
    chip = restore(chip);
    assert_eq!(
        chip.service_dma_fetch(2, second.address, 0xf00),
        Some((0x180, 0xf00))
    );
    assert_eq!(chip.pc, 0x2004);
    assert!(chip.pending_dma_fetch.is_none());
}

#[test]
fn parked_wait_only_releases_on_a_free_copper_polarity_and_compares_once_after_finish() {
    let mut chip = Copper::new();
    chip.pc = 0x2000;
    chip.waiting = true;
    chip.wait_target = 0x3c40;
    chip.wait_mask = 0xfffe;
    chip.wait_bfd = false;
    chip.wait_blitter_blocked = true;
    assert!(chip.request_dma_cck(60, 64, true, false).is_none());
    assert!(
        chip.waiting,
        "the even CCK is not a Copper comparison clock"
    );
    assert!(chip.request_dma_cck(60, 65, false, false).is_none());
    assert!(chip.waiting, "an occupied input cell cannot complete WAIT1");
    chip = restore(chip);
    assert!(chip.request_dma_cck(60, 67, true, false).is_none());
    assert!(!chip.waiting);
    assert!(
        !chip.pending_wait_delay,
        "returning WAIT1 already performed the comparison"
    );
    chip = restore(chip);
    assert!(chip.request_dma_cck(60, 68, true, false).is_none());
    assert!(chip.request_dma_cck(60, 69, true, false).is_some());
}

#[test]
fn jump_after_admission_keeps_the_owned_read_but_discards_its_decode() {
    let mut chip = Copper::new();
    chip.pc = 0x2000;
    let first = chip
        .request_dma_cck(60, 65, true, false)
        .expect("IR1 request");
    chip.cop1lc = 0x3000;
    chip.jump1();
    chip = restore(chip);
    assert_eq!(chip.service_dma_fetch(1, first.address, 0x180), None);
    assert!(
        chip.bus_used_this_cck,
        "the admitted physical cell still retired"
    );
    assert_eq!(chip.pc, 0x3000);
    assert_eq!(chip.cck_phase, 0);
}
