//! Agnus blitter startup timing and the A1000-visible BBUSY exception.
//!
//! Every supported revision consumes two accepted scheduler CCKs before its
//! first A/B/C/D/internal operation. Later original Agnus and enhanced chips
//! expose BBUSY immediately. The 8361/8367 installed in the A1000 keeps BBUSY
//! clear until the first accepted startup CCK.

use commodore_agnus_ocs::{
    Agnus, AgnusRegion, BlitterBus, BlitterDmaOp, BlitterProgress, SlotOwner, bits,
};

const BBUSY: u16 = 0x4000;

fn program_one_word_a_to_d(agnus: &mut Agnus) {
    agnus.bltcon0 = 0x0900 | 0x00F0; // USEA | USED | D=A
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();
}

#[test]
fn every_revision_uses_two_startup_ccks_but_only_a1000_hides_initial_busy() {
    let mut enhanced_with_a1000_identity = Agnus::new_a1000_with_region(AgnusRegion::Pal);
    enhanced_with_a1000_identity.agnus_id = 0x2000;

    let cases = [
        (
            "A1000 original Agnus",
            Agnus::new_a1000_with_region(AgnusRegion::Pal),
            false,
        ),
        (
            "later original Agnus",
            Agnus::new_with_region(AgnusRegion::Pal),
            true,
        ),
        (
            "enhanced identity with nested A1000 state",
            enhanced_with_a1000_identity,
            true,
        ),
    ];

    for (name, mut agnus, initially_visible) in cases {
        agnus.blitter_dzero = false;
        program_one_word_a_to_d(&mut agnus);
        let operations = agnus.blitter_ccks_remaining;

        assert!(agnus.blitter_busy, "{name}: internal activity is immediate");
        assert_eq!(
            agnus.blitter_busy_visible(),
            initially_visible,
            "{name}: unexpected just-started BBUSY visibility",
        );
        assert_eq!(agnus.blitter_startup_ccks_remaining(), 2);
        assert!(
            !agnus.blitter_dzero,
            "{name}: the preceding BZERO result survives until a startup CCK",
        );

        assert_eq!(
            agnus.tick_blitter_scheduler_op(true),
            BlitterProgress::Startup,
        );
        assert_eq!(agnus.blitter_startup_ccks_remaining(), 1);
        assert!(agnus.blitter_busy_visible());
        assert!(
            agnus.blitter_dzero,
            "{name}: first accepted startup CCK reloads BZERO",
        );
        assert_eq!(agnus.blitter_ccks_remaining, operations);
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::ReadA),);

        assert_eq!(
            agnus.tick_blitter_scheduler_op(true),
            BlitterProgress::Startup,
        );
        assert_eq!(agnus.blitter_startup_ccks_remaining(), 0);
        assert_eq!(agnus.blitter_ccks_remaining, operations);

        assert_eq!(
            agnus.tick_blitter_scheduler_op(true),
            BlitterProgress::Operation(BlitterDmaOp::ReadA),
        );
        assert_eq!(agnus.blitter_ccks_remaining, operations - 1);
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::WriteD),);
    }
}

#[test]
fn dma_disable_and_contended_slots_do_not_advance_a1000_startup() {
    let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
    agnus.blitter_dzero = false;
    program_one_word_a_to_d(&mut agnus);
    let operations = agnus.blitter_ccks_remaining;

    agnus.hpos = 0x00;
    let disabled = agnus.cck_bus_plan();
    assert_eq!(disabled.slot_owner, SlotOwner::Cpu);
    assert!(!disabled.blitter_dma_progress_granted);
    assert_eq!(
        agnus.tick_blitter_scheduler_op(disabled.blitter_dma_progress_granted),
        BlitterProgress::NoProgress,
    );

    agnus.dmacon = bits::DMACON_DMAEN | bits::DMACON_BLTEN | bits::DMACON_BLTPRI;
    assert!(
        agnus.blitter_nasty_active(),
        "internal activity participates in arbitration before BBUSY is visible",
    );
    agnus.hpos = 0x01; // unconditional refresh slot
    let contended = agnus.cck_bus_plan();
    assert_eq!(contended.slot_owner, SlotOwner::Refresh);
    assert!(!contended.blitter_dma_progress_granted);
    assert_eq!(
        agnus.tick_blitter_scheduler_op(contended.blitter_dma_progress_granted),
        BlitterProgress::NoProgress,
    );

    assert_eq!(agnus.blitter_startup_ccks_remaining(), 2);
    assert_eq!(agnus.blitter_ccks_remaining, operations);
    assert!(!agnus.blitter_busy_visible());
    assert!(!agnus.blitter_dzero);

    agnus.hpos = 0x00;
    let admitted = agnus.cck_bus_plan();
    assert!(admitted.blitter_dma_progress_granted);
    assert_eq!(
        agnus.tick_blitter_scheduler_op(admitted.blitter_dma_progress_granted),
        BlitterProgress::Startup,
    );
    assert!(agnus.blitter_busy_visible());
    assert!(agnus.blitter_dzero);
    assert_eq!(agnus.blitter_ccks_remaining, operations);
}

#[derive(Default)]
struct NullBus {
    reads: u32,
    writes: u32,
}

impl BlitterBus for NullBus {
    fn read_word(&mut self, _addr: u32) -> u16 {
        self.reads += 1;
        0
    }

    fn write_word(&mut self, _addr: u32, _val: u16) {
        self.writes += 1;
    }
}

#[test]
fn one_operation_blit_cannot_complete_during_startup() {
    let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
    agnus.blitter_dzero = false;
    agnus.bltcon0 = 0; // no channels: one internal operation per word
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();
    let mut bus = NullBus::default();

    assert!(!agnus.tick_blitter_dma(&mut bus));
    assert!(agnus.blitter_busy);
    assert!(agnus.blitter_busy_visible());
    assert_eq!(agnus.blitter_startup_ccks_remaining(), 1);
    assert_eq!(agnus.blitter_ccks_remaining, 1);
    assert_eq!((bus.reads, bus.writes), (0, 0));

    assert!(!agnus.tick_blitter_dma(&mut bus));
    assert!(agnus.blitter_busy);
    assert_eq!(agnus.blitter_startup_ccks_remaining(), 0);
    assert_eq!(agnus.blitter_ccks_remaining, 1);
    assert_eq!((bus.reads, bus.writes), (0, 0));

    assert!(agnus.tick_blitter_dma(&mut bus));
    assert!(!agnus.blitter_busy);
    assert!(
        agnus.blitter_busy_visible(),
        "DMACONR retains busy through the finish-source CCK",
    );
    assert!(agnus.blitter_busy_copper());
    assert_eq!(agnus.blitter_ccks_remaining, 0);
    assert_eq!((bus.reads, bus.writes), (0, 0));

    assert!(
        !agnus.run_blit_to_completion(&mut bus),
        "an idle synchronous drain has no new finish source",
    );
    assert!(agnus.blitter_busy_visible());
    assert!(
        agnus.blitter_busy_copper(),
        "an idle synchronous drain must preserve observer holds",
    );

    agnus.tick_cck();
    assert!(!agnus.blitter_busy_visible());
    assert!(agnus.blitter_busy_copper());
    agnus.tick_cck();
    assert!(!agnus.blitter_busy_copper());
}

#[test]
fn new_blit_rearms_startup_and_preserves_bzero_until_first_acceptance() {
    let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
    program_one_word_a_to_d(&mut agnus);
    assert_eq!(
        agnus.tick_blitter_scheduler_op(true),
        BlitterProgress::Startup,
    );
    assert!(agnus.blitter_busy_visible());

    agnus.blitter_dzero = false; // previous/current non-zero result
    program_one_word_a_to_d(&mut agnus);

    assert_eq!(agnus.blitter_startup_ccks_remaining(), 2);
    assert!(!agnus.blitter_busy_visible());
    assert_eq!(agnus.dmaconr() & BBUSY, 0);
    assert!(!agnus.blitter_dzero);

    assert_eq!(
        agnus.tick_blitter_scheduler_op(true),
        BlitterProgress::Startup,
    );
    assert!(agnus.blitter_busy_visible());
    assert!(agnus.blitter_dzero);
}

#[test]
fn compatibility_dma_wrapper_reports_only_full_pipeline_drain() {
    let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
    agnus.bltcon0 = 0x0100; // D only: one operation
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();
    let mut bus = NullBus::default();

    assert!(!agnus.tick_blitter_dma(&mut bus));
    assert!(!agnus.tick_blitter_dma(&mut bus));
    assert!(
        !agnus.tick_blitter_dma(&mut bus),
        "pre-AGA source finish precedes pipeline drain",
    );
    assert!(!agnus.tick_blitter_dma(&mut bus));
    assert!(agnus.tick_blitter_dma(&mut bus));

    assert!(!agnus.blitter_busy);
    assert_eq!(bus.writes, 1);
    assert_eq!(agnus.blitter_startup_ccks_remaining(), 0);
}

#[test]
fn synchronous_drain_consumes_startup_without_extra_bus_operations() {
    let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
    agnus.blitter_dzero = false;
    program_one_word_a_to_d(&mut agnus);
    let mut bus = NullBus::default();

    agnus.run_blit_to_completion(&mut bus);

    assert_eq!((bus.reads, bus.writes), (1, 1));
    assert!(agnus.blitter_dzero);
    assert!(!agnus.blitter_busy);
    assert!(!agnus.blitter_busy_visible());
    assert_eq!(agnus.blitter_startup_ccks_remaining(), 0);
}
