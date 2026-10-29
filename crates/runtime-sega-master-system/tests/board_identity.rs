//! Envelope identity and embedded legacy execution state are validated atomically.
use emu198x_shell::MachineCore;
use runtime_sega_master_system_class::{SmsModel, SmsRuntime};
fn fixture(id: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../test-data/sega/board-split-v10/{id}.state")),
    )
    .expect("original state")
}
fn check<M: SmsModel>(target: M) {
    let mut r = SmsRuntime::blank(target);
    let initial = fixture(target.model_id());
    r.restore(&initial).expect("own save");
    assert_eq!(r.machine().expect("machine").hardware(), target.hardware());
    assert_eq!(r.snapshot().expect("same bytes"), initial);
    let sources = runtime_sega_master_system::Model::ALL
        .into_iter()
        .map(|m| (m.model_id(), m.variant()))
        .chain(std::iter::once((
            "sega-game-gear",
            runtime_sega_master_system_class::SmsVariant::GameGear,
        )));
    for (id, variant) in sources {
        if id == target.model_id() {
            continue;
        }
        let bytes = fixture(id);
        assert!(r.restore(&bytes).is_err(), "wrong envelope {id}");
        assert_eq!(r.snapshot().expect("unchanged"), initial);
        if variant != target.variant() {
            let ((version, time, _), tail) =
                postcard::take_from_bytes::<(u16, u64, String)>(&bytes).expect("legacy prefix");
            let mut forged =
                postcard::to_allocvec(&(version, time, target.model_id())).expect("prefix");
            forged.extend(tail);
            assert!(r.restore(&forged).is_err(), "wrong embedded variant {id}");
            assert_eq!(r.snapshot().expect("unchanged"), initial);
        }
    }
}
#[test]
fn all_nine_saved_models_bind_and_wrong_boards_reject_atomically() {
    for model in runtime_sega_master_system::Model::ALL {
        check(model);
    }
    check(runtime_sega_game_gear::Model::GameGear);
}
