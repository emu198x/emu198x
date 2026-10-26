//! Lisa (AGA Denise) driven through this crate's board wrapper and
//! display window. These tests moved here from the Lisa crate, which no
//! longer depends on the family glue.

use common_commodore_amiga::denise::Denise;
use common_commodore_amiga::denise_counter::DeniseStrobe;
use common_commodore_amiga::denise_window::DeniseWindow;
use emu198x_commodore_denise_aga::DeniseAga;
use emu198x_commodore_denise_ocs::DeniseChip;

#[test]
fn lisa_window_matches_reach_output_at_the_counter_equality() {
    let denise = DeniseAga::new();

    let mut window = DeniseWindow::default();
    window.queue_write(0x08E, 129, true, true, true);
    window.queue_write(0x090, 193, true, true, true);
    for _ in 0..3 {
        window.begin_output_tick();
    }
    let mut active = false;
    // Independent UAE counter trace: the start match is composed at 129,
    // while the stop match at 449 already selects border.
    for position in 128..=450 {
        let gates = window.output_gates(
            &mut active,
            position,
            true,
            denise.horizontal_diw_comparator_phase(),
        );
        assert_eq!(
            gates,
            [(129..449).contains(&position); 4],
            "counter {position}"
        );
    }
}

#[test]
fn lisa_strobe_uses_the_existing_normal_stage_once_per_cck() {
    let mut board = Denise::<DeniseAga>::new();
    board.service_timing_strobe(DeniseStrobe::Horizontal);
    for _ in 0..2 {
        board.prepare_output_timing_strobe();
        assert!(
            !board
                .ocs
                .diagnostic_snapshot()
                .vertical_blanking
                .previous_horizontal_strobe
        );
        board.begin_counter_output_tick();
        board.end_counter_output_tick();
    }
    board.prepare_output_timing_strobe();
    assert!(
        board
            .ocs
            .diagnostic_snapshot()
            .vertical_blanking
            .previous_horizontal_strobe
    );
    assert_eq!(
        board
            .ocs
            .diagnostic_snapshot()
            .vertical_blanking
            .pending_programmed,
        Some(false)
    );
    board.begin_counter_output_tick();
    board.end_counter_output_tick();
    // A stop comparator in the first half consumes the event. Delivering
    // the strobe again in the second half must not resurrect it.
    board.ocs.vertical_blanking_mut().edge(true, false);
    board.prepare_output_timing_strobe();
    assert_eq!(
        board
            .ocs
            .diagnostic_snapshot()
            .vertical_blanking
            .pending_programmed,
        None
    );
}
