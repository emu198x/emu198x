use std::collections::HashMap;

use commodore_agnus_ocs::{
    Agnus, AgnusBlitterCompletionDiagnosticPhase, BlitterBus, BlitterBusDiagnosticAuthority,
    BlitterDmaOp, SlotOwner,
    bits::{DMACON_BLTEN, DMACON_BLTPRI, DMACON_DMAEN},
};

#[derive(Default)]
struct TestBus {
    words: HashMap<u32, u16>,
}

impl BlitterBus for TestBus {
    fn read_word(&mut self, addr: u32) -> u16 {
        self.words.get(&(addr & !1)).copied().unwrap_or_default()
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        self.words.insert(addr & !1, value);
    }
}

#[test]
fn bus_snapshot_switches_from_live_plan_to_recorded_cck_authority() {
    let mut agnus = Agnus::new();
    agnus.hpos = 0;
    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
    agnus.bltcon0 = 0x01FF;
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();

    let live = agnus.bus_diagnostic_snapshot();

    assert_eq!(live.plan.slot_owner, SlotOwner::Cpu);
    assert!(live.plan.blitter_chip_bus_granted);
    assert!(!live.plan.cpu_chip_bus_granted);
    assert_eq!(
        live.blitter_authority,
        BlitterBusDiagnosticAuthority::CurrentPlanFallback
    );
    assert!(live.blitter_holds_bus);
    assert_eq!(agnus.bus_diagnostic_snapshot(), live);

    agnus.record_blitter_cck_bus_state(false, false);
    let recorded_free = agnus.bus_diagnostic_snapshot();

    assert_eq!(
        recorded_free.blitter_authority,
        BlitterBusDiagnosticAuthority::RecordedCckState
    );
    assert!(recorded_free.blitter_cck_bus_state_recorded);
    assert!(!recorded_free.blitter_bus_used_this_cck);
    assert!(!recorded_free.blitter_nasty_owned_this_cck);
    assert!(!recorded_free.blitter_holds_bus);

    agnus.record_blitter_cck_bus_state(true, false);
    let recorded_owned = agnus.bus_diagnostic_snapshot();

    assert!(recorded_owned.blitter_nasty_owned_this_cck);
    assert!(recorded_owned.blitter_holds_bus);
}

#[test]
fn ddf_snapshot_exposes_raw_effective_and_latched_comparator_state() {
    let mut agnus = Agnus::new();
    agnus.agnus_id = 0x2000;
    agnus.ddfstrt = 0x0039;
    agnus.ddfstop = 0x0041;
    agnus.hpos = 0x0037;
    agnus.tick_cck();
    agnus.hpos = 0x003F;
    agnus.tick_cck();

    let snapshot = agnus.ddf_diagnostic_snapshot();

    assert_eq!(snapshot.ddfstrt, 0x0039);
    assert_eq!(snapshot.ddfstop, 0x0041);
    assert_eq!(snapshot.agnus_id, 0x2000);
    assert_eq!(snapshot.comparator_mask, 0x00FE);
    assert_eq!(snapshot.effective_ddfstrt, 0x0038);
    assert_eq!(snapshot.effective_ddfstop, 0x0040);
    assert_eq!(snapshot.start_match, Some(0x0038));
    assert_eq!(snapshot.stop_match, Some(0x0040));
    assert_eq!(snapshot.fetch_end, Some(0x0047));
    assert!(!snapshot.ocs_run_aborted);
    assert!(snapshot.ocs_hard_start_open);
    assert_eq!(agnus.ddf_diagnostic_snapshot(), snapshot);
}

#[test]
fn area_blitter_snapshot_exposes_register_scheduler_word_and_fill_state() {
    let mut agnus = Agnus::new();
    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
    agnus.bltcon0 = (4 << 12) | 0x0FCA;
    agnus.bltcon1 = (5 << 12) | 0x000E;
    agnus.bltsize = (2 << 6) | 3;
    agnus.bltsizv_ecs = 0x1234;
    agnus.bltsizh_ecs = 0x0567;
    agnus.blt_apt = 0x1000;
    agnus.blt_bpt = 0x2000;
    agnus.blt_cpt = 0x3000;
    agnus.blt_dpt = 0x4000;
    agnus.blt_amod = 2;
    agnus.blt_bmod = 4;
    agnus.blt_cmod = 6;
    agnus.blt_dmod = 8;
    agnus.blt_adat = 0xAAAA;
    agnus.blt_bdat = 0xBBBB;
    agnus.blt_cdat = 0xCCCC;
    agnus.blt_afwm = 0xFF00;
    agnus.blt_alwm = 0x00FF;
    agnus.start_blit();

    let snapshot = agnus.blitter_diagnostic_snapshot();
    let word = snapshot.word.expect("area blit must install a word state");
    let area = snapshot
        .area
        .as_ref()
        .expect("area runtime must be present");

    assert_eq!(snapshot.registers.bltcon0, (4 << 12) | 0x0FCA);
    assert_eq!(snapshot.registers.bltcon1, (5 << 12) | 0x000E);
    assert_eq!(snapshot.registers.bltsizv_ecs, 0x1234);
    assert_eq!(snapshot.registers.bltsizh_ecs, 0x0567);
    assert_eq!(snapshot.registers.blt_apt, 0x1000);
    assert_eq!(snapshot.registers.blt_dmod, 8);
    assert_eq!(snapshot.registers.blt_adat, 0xAAAA);
    assert_eq!(snapshot.registers.blt_afwm, 0xFF00);
    assert_eq!(snapshot.registers.blt_alwm, 0x00FF);

    assert!(snapshot.execution.dma_enabled);
    assert_eq!(snapshot.execution.agnus_id, 0x0000);
    assert!(snapshot.execution.priority_enabled);
    assert!(snapshot.execution.busy);
    assert!(snapshot.execution.busy_visible);
    assert!(snapshot.execution.busy_copper);
    assert!(snapshot.execution.nasty_active);
    assert_eq!(snapshot.execution.startup_ccks_remaining, 2);
    assert_eq!(snapshot.execution.height, 2);
    assert_eq!(snapshot.execution.width_words, 3);
    assert_eq!(snapshot.execution.ccks_remaining, 24);
    assert_eq!(
        snapshot.execution.next_dma_request,
        Some(BlitterDmaOp::ReadA)
    );
    assert!(snapshot.execution.next_progress_uses_bus);
    assert!(snapshot.execution.incremental_runtime_present);

    assert!(word.need_a);
    assert!(word.need_b);
    assert!(word.need_c);
    assert!(word.need_d);
    assert!(!word.reads_done);
    assert!(!word.internal_only);

    assert_eq!(area.rows_remaining, 2);
    assert_eq!(area.width_words, 3);
    assert_eq!(area.words_remaining_in_row, 3);
    assert!(area.use_a && area.use_b && area.use_c && area.use_d);
    assert_eq!(area.a_shift, 4);
    assert_eq!(area.b_shift, 5);
    assert!(area.descending);
    assert_eq!(area.pointer_step, -2);
    assert_eq!(area.modulo_direction, -1);
    assert!(area.fill_enabled);
    assert!(area.inclusive_fill_enabled);
    assert!(!area.exclusive_fill_enabled);
    assert_eq!(area.fill_carry_initial, 1);
    assert_eq!(area.fill_carry, 1);
    assert_eq!(area.apt, 0x1000);
    assert_eq!(area.dpt, 0x4000);
    assert_eq!(area.a_raw, 0xAAAA);
    assert_eq!(area.b_raw, 0xBBBB);
    assert_eq!(area.c_value, 0xCCCC);
    assert!(snapshot.line.is_none());
    assert_eq!(agnus.blitter_diagnostic_snapshot(), snapshot);
    assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::ReadA));
}

#[test]
fn line_blitter_snapshot_exposes_error_texture_direction_and_onedot_state() {
    let mut agnus = Agnus::new();
    agnus.bltcon0 = (3 << 12) | 0x03CA;
    agnus.bltcon1 = (7 << 12) | 0x000F;
    agnus.bltsize = (4 << 6) | 2;
    agnus.blt_apt = 0x0000_FFFE;
    agnus.blt_bmod = 4;
    agnus.blt_amod = -6;
    agnus.blt_cmod = 8;
    agnus.blt_cpt = 0x2000;
    agnus.blt_dpt = 0x3000;
    agnus.blt_bdat = 0xA55A;
    agnus.start_blit();

    let snapshot = agnus.blitter_diagnostic_snapshot();
    let word = snapshot.word.expect("line blit must install a word state");
    let line = snapshot
        .line
        .as_ref()
        .expect("line runtime must be present");

    assert_eq!(snapshot.execution.height, 4);
    assert_eq!(snapshot.execution.ccks_remaining, 8);
    assert_eq!(
        snapshot.execution.next_dma_request,
        Some(BlitterDmaOp::ReadC)
    );
    assert!(!word.need_a);
    assert!(!word.need_b);
    assert!(word.need_c);
    assert!(word.need_d);

    assert_eq!(line.steps_remaining, 4);
    assert_eq!(line.error, -2);
    assert_eq!(line.error_add, 4);
    assert_eq!(line.error_sub, -6);
    assert_eq!(line.cpt, 0x2000);
    assert_eq!(line.dpt, 0x3000);
    assert_eq!(line.pixel_bit, 3);
    assert_eq!(line.row_mod, 8);
    assert_eq!(line.texture, 0xA55A);
    assert_eq!(line.texture_bit, 7);
    assert_eq!(line.lf, 0xCA);
    assert!(line.sing);
    assert!(!line.one_dot_drawn);
    assert!(line.major_is_y);
    assert!(line.x_negative);
    assert!(!line.y_negative);
    assert!(!line.have_c_word);
    assert!(snapshot.area.is_none());
}

#[test]
fn blitter_snapshot_exposes_buffered_final_destination_write() {
    let mut agnus = Agnus::new();
    let mut bus = TestBus::default();
    agnus.bltcon0 = 0x01FF;
    agnus.blt_dpt = 0x2000;
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();

    let _ = agnus.tick_blitter_cck(true, &mut bus);
    let _ = agnus.tick_blitter_cck(true, &mut bus);
    let finish = agnus.tick_blitter_cck(true, &mut bus);
    let final_result = agnus.blitter_diagnostic_snapshot();

    assert!(finish.interrupt);
    assert!(!finish.bus_used);
    assert_eq!(
        final_result.execution.completion_phase,
        AgnusBlitterCompletionDiagnosticPhase::FinalResult
    );
    assert_eq!(final_result.execution.completion_ccks_remaining, 2);
    assert!(final_result.execution.final_d_pending);
    assert!(final_result.execution.finish_emitted);
    assert!(!final_result.execution.next_progress_uses_bus);

    let result_stage = agnus.tick_blitter_cck(true, &mut bus);
    let final_write = agnus.blitter_diagnostic_snapshot();

    assert!(!result_stage.interrupt);
    assert!(!result_stage.bus_used);
    assert_eq!(
        final_write.execution.completion_phase,
        AgnusBlitterCompletionDiagnosticPhase::FinalWrite {
            address: 0x2000,
            value: 0xFFFF,
        }
    );
    assert_eq!(final_write.execution.completion_ccks_remaining, 1);
    assert!(final_write.execution.next_progress_uses_bus);
    assert!(!bus.words.contains_key(&0x2000));

    let write_stage = agnus.tick_blitter_cck(true, &mut bus);
    let complete = agnus.blitter_diagnostic_snapshot();

    assert!(write_stage.bus_used);
    assert_eq!(bus.words.get(&0x2000), Some(&0xFFFF));
    assert!(!complete.execution.busy);
    assert_eq!(
        complete.execution.completion_phase,
        AgnusBlitterCompletionDiagnosticPhase::None
    );
    assert!(!complete.execution.final_d_pending);
}
