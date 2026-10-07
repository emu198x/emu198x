//! Alice's BPLCON0 DMA copy retains the two-CCK register and two-CCK arbitration stages.
use commodore_agnus_ocs::Agnus;

#[test]
fn alice_dma_uses_the_previous_plane_count_through_the_pending_arbitration_slots() {
    let mut chip = Agnus::new();
    chip.agnus_id = 0x2300;
    chip.max_bitplanes = 8;
    chip.bplcon0 = 0x1000;
    chip.write_bplcon0(0x2000);
    assert_eq!(chip.bplcon0, 0x2000);
    assert_eq!(chip.num_bitplanes(), 1, "write slot retains old DMA copy");
    chip.tick_cck();
    assert_eq!(chip.num_bitplanes(), 1, "first following CCK");
    chip.tick_cck();
    assert_eq!(chip.num_bitplanes(), 1, "second following CCK");
    chip.tick_cck();
    assert_eq!(chip.num_bitplanes(), 1, "third following CCK");
    chip.tick_cck();
    assert_eq!(chip.num_bitplanes(), 2, "fourth following CCK");
}

#[test]
fn changing_width_keeps_the_next_selected_slot_but_uses_the_new_transfer_width() {
    let mut chip = Agnus::new();
    chip.agnus_id = 0x2300;
    chip.max_bitplanes = 8;
    chip.bplcon0 = 0x1040;
    chip.dmacon = 0x0300;
    chip.ddfstrt = 0x38;
    chip.ddfstop = 0xD0;
    chip.diwstrt = 0x2C81;
    chip.diwstop = 0xF4C1;
    chip.vpos = 132;
    chip.fmode = 1;
    while chip.hpos < 130 {
        chip.tick_cck();
    }
    chip.write_fmode(3);
    chip.tick_cck();
    assert_eq!(chip.cck_bus_plan().bitplane_dma_fetch_plane, Some(0));
    assert_eq!(chip.bpl_fetch_width(), 4, "transfer uses the new width");
    chip.tick_cck();
    chip.tick_cck();
    assert_eq!(chip.cck_bus_plan().bitplane_dma_fetch_plane, None);
    chip.tick_cck();
    chip.tick_cck();
    assert_eq!(chip.cck_bus_plan().bitplane_dma_fetch_plane, Some(0));
}
