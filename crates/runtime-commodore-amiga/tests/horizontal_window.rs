//! Display-window register delivery and partial-sample state must survive restore.
use common_commodore_amiga::{
    denise_chip::HorizontalDiwComparatorPhase, denise_window::DeniseWindow, driver::AmigaDriver,
};
use emu198x_shell::MachineCore;
use runtime_commodore_amiga::{AmigaA1200Runtime, AmigaLiveAccess, AmigaMachine, Model};

#[test]
fn window_restore_preserves_pending_writes_and_fractional_output_history() {
    let mut window = DeniseWindow::default();
    window.queue_write(0x08E, 0x81, true, true, true);
    window.queue_write(0x090, 0xA1, true, true, true);
    window.queue_write(0x1E4, 0x2010, true, true, true);
    let mut active = false;
    let mut saw_split = false;
    for position in 0x7E..0x86 {
        let bytes = postcard::to_allocvec(&window).expect("save window stage");
        let mut restored: DeniseWindow =
            postcard::from_bytes(&bytes).expect("restore window stage");
        let mut restored_active = active;
        window.begin_output_tick();
        restored.begin_output_tick();
        let expected = window.output_gates(
            &mut active,
            position,
            true,
            HorizontalDiwComparatorPhase::AfterOutput,
        );
        let actual = restored.output_gates(
            &mut restored_active,
            position,
            true,
            HorizontalDiwComparatorPhase::AfterOutput,
        );
        assert_eq!(actual, expected);
        assert_eq!(active, restored_active);
        assert_eq!(window, restored);
        saw_split |= actual == [false, false, true, true];
    }
    assert!(
        saw_split,
        "replay must exercise a fractional edge, not only blank output"
    );
}

#[test]
fn live_window_write_survives_both_half_cck_restore_phases() {
    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    for copper in [false, true] {
        for register in [0x08E, 0x090, 0x1E4] {
            if copper {
                AmigaDriver::dispatch_copper_write(original.machine_mut(), register, 0x2018);
            } else {
                AmigaDriver::dispatch_custom_write(original.machine_mut(), register, 0x2018);
            }
            let pending = serde_json::to_value(
                original
                    .machine()
                    .denise_board_pipeline_diagnostic_snapshot(),
            )
            .expect("diagnostic");
            assert!(
                !pending["horizontal_window"]["pending"]
                    .as_array()
                    .expect("queue")
                    .is_empty()
            );
            for _ in 0..8 {
                restored
                    .restore(&original.snapshot().expect("snapshot"))
                    .expect("restore");
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                assert_eq!(
                    original.snapshot().expect("original"),
                    restored.snapshot().expect("replay")
                );
            }
        }
    }
}

#[test]
fn malformed_window_stages_fail_validation() {
    for pending in [
        serde_json::json!({"register": 0x180, "value": 0, "ticks": 1}),
        serde_json::json!({"register": 0x08E, "value": 0, "ticks": 4}),
    ] {
        let mut value = serde_json::to_value(DeniseWindow::default()).expect("serialize");
        value["pending"] = serde_json::json!([pending]);
        let window: DeniseWindow = serde_json::from_value(value).expect("decode candidate");
        assert!(window.validate(true, true).is_err());
    }
}
