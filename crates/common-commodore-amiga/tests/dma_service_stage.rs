//! The admitted service cell is historical state, not a live-register decode.
use commodore_agnus_ocs::{Agnus, SlotOwner, bits};

#[test]
fn every_dma_owner_survives_register_changes_and_serialization() {
    let mut seen = [false; 7];
    for hpos in 0..227 {
        let mut agnus = Agnus::new();
        agnus.vpos = 0x20;
        agnus.write_diwstrt(0x2010);
        agnus.write_diwstop(0xa0c1);
        agnus.write_ddfstrt(0x38);
        agnus.write_ddfstop(0xd0);
        agnus.bplcon0 = 0x6000;
        agnus.dmacon = bits::DMACON_DMAEN | 0x01bf;
        for channel in 0..8 {
            agnus.poke_sprite_pos(channel, 0x2000);
            agnus.poke_sprite_ctl(channel, 0x2100);
        }
        while agnus.hpos < hpos {
            agnus.tick_cck();
        }
        let plan = agnus.cck_bus_plan();
        seen[match plan.slot_owner {
            SlotOwner::Cpu => 0,
            SlotOwner::Refresh => 1,
            SlotOwner::Disk => 2,
            SlotOwner::Audio(_) => 3,
            SlotOwner::Sprite(_) => 4,
            SlotOwner::Bitplane(_) => 5,
            SlotOwner::Copper => 6,
        }] = true;
        agnus.record_dma_service_plan(plan);
        agnus.dmacon = 0;
        agnus.bplcon0 = 0;
        let encoded = postcard::to_allocvec(&agnus).expect("encode admitted DMA cell");
        let mut restored: Agnus = postcard::from_bytes(&encoded).expect("decode admitted DMA cell");
        assert_eq!(restored.dma_service_plan(), Some(plan));
        assert_eq!(restored.bus_diagnostic_snapshot().service_plan, Some(plan));
        restored.tick_cck();
        assert_eq!(
            restored.dma_service_plan(),
            None,
            "next CCK needs a new admission"
        );
    }
    assert!(
        seen.into_iter().all(|owner| owner),
        "every owner must be exercised"
    );
}

#[test]
#[should_panic(expected = "DMA cell admitted twice")]
fn a_cell_cannot_be_admitted_again_after_its_registers_change() {
    let mut agnus = Agnus::new();
    agnus.record_dma_service_plan(agnus.cck_bus_plan());
    agnus.dmacon = bits::DMACON_DMAEN | bits::DMACON_COPEN;
    agnus.record_dma_service_plan(agnus.cck_bus_plan());
}
