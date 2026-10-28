//! These bytes were generated at 49956980d before the machine layout changed.
mod common;
use emu198x_shell::{HostIo, MachineCore, MachineTime};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
use serde::Deserialize;
#[derive(Deserialize)]
struct LegacyPrefix {
    version: u32,
    profile_id: String,
    time: MachineTime,
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/game-boy/snapshot-v3")
            .join(name),
    )
    .expect("committed v3 fixture")
}

#[test]
fn genuine_v3_states_load_for_every_existing_profile() {
    for model in Model::ALL {
        let old = fixture(&format!("{}.state", model.variant_id()));
        assert_eq!(old[0], 3);
        let mut runtime = GameBoyRuntime::blank(model);
        runtime.restore(&old).expect("v3 migration");
        let (prefix, _): (LegacyPrefix, _) =
            postcard::take_from_bytes(&old).expect("old envelope prefix");
        assert_eq!(prefix.version, 3);
        assert_eq!(prefix.profile_id, model.profile_id());
        assert!(prefix.time.get() >= 2048);
        assert_eq!(runtime.time(), prefix.time);
        assert!(runtime.machine().is_some());
        let current = runtime.snapshot().expect("v4 save");
        assert_eq!(current[0], 4);
        let mut fresh = GameBoyRuntime::blank(model);
        fresh.restore(&current).expect("migrated v4 reload");
        assert_eq!(fresh.snapshot().expect("same state"), current);
    }
}

#[test]
fn migrated_execution_agrees_with_independent_baseline_continuations() {
    for model in [Model::Dmg, Model::Cgb] {
        let mut runtime = GameBoyRuntime::blank(model);
        runtime
            .restore(&fixture(&format!("{}.state", model.variant_id())))
            .expect("v3 start");
        let (mut frame_sink, mut audio_sink, mut trace_sink) = common::null_host_buffers();
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let before = runtime.time();
        runtime
            .run_until(
                MachineTime::new(before.get() + model.frame_ticks()),
                &mut host,
            )
            .expect("continue migrated machine");
        assert!(runtime.time() > before, "continuation must advance");
        let mut baseline = GameBoyRuntime::blank(model);
        baseline
            .restore(&fixture(&format!("{}-next.state", model.variant_id())))
            .expect("baseline continuation");
        assert_eq!(
            runtime.snapshot().expect("actual"),
            baseline.snapshot().expect("expected"),
            "{} continuation diverged",
            model.variant_id()
        );
    }
}

#[test]
fn legacy_wrong_profile_and_truncation_leave_live_state_unchanged() {
    let mut runtime = GameBoyRuntime::blank(Model::Mgb);
    let before = runtime.snapshot().expect("current");
    assert!(runtime.restore(&fixture("dmg.state")).is_err());
    assert_eq!(runtime.snapshot().expect("unchanged"), before);
    let bytes = fixture("mgb.state");
    for length in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        assert!(runtime.restore(&bytes[..length]).is_err());
        assert_eq!(runtime.snapshot().expect("unchanged"), before);
    }
}
