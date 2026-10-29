//! Saved profile IDs bind opaque legacy state to its physical owner.
use common_nintendo_game_boy::GameBoyHardware;
use emu198x_shell::MachineCore;
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
fn fixture(model: Model) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../test-data/game-boy/board-split-v4/{}.state",
            model.variant_id()
        )),
    )
    .expect("original state")
}
#[test]
fn saved_profiles_select_their_physical_owner_without_guessing_from_registers() {
    for (model, hardware, machine) in [
        (Model::Dmg0, GameBoyHardware::Dmg, "nintendo-game-boy"),
        (Model::Dmg, GameBoyHardware::Dmg, "nintendo-game-boy"),
        (Model::Mgb, GameBoyHardware::Mgb, "nintendo-game-boy-pocket"),
        (Model::Sgb, GameBoyHardware::Sgb, "nintendo-super-game-boy"),
        (
            Model::Sgb2,
            GameBoyHardware::Sgb2,
            "nintendo-super-game-boy-2",
        ),
        (Model::Cgb, GameBoyHardware::Cgb, "nintendo-game-boy-color"),
        (
            Model::Agb,
            GameBoyHardware::Agb,
            "nintendo-game-boy-advance",
        ),
    ] {
        let mut runtime = GameBoyRuntime::blank(model);
        let old = fixture(model);
        runtime.restore(&old).expect("legacy profile");
        assert_eq!(runtime.machine().expect("machine").hardware(), hardware);
        assert_eq!(runtime.profile().machine_id.as_str(), machine);
        assert_eq!(runtime.snapshot().expect("same bytes"), old);
    }
}
#[test]
fn every_wrong_declared_profile_rejects_without_changing_an_occupied_receiver() {
    for target in Model::ALL {
        let mut receiver = GameBoyRuntime::blank(target);
        receiver
            .restore(&fixture(target))
            .expect("occupied receiver");
        let before = receiver.snapshot().expect("before");
        for source in Model::ALL.into_iter().filter(|&model| model != target) {
            assert!(
                receiver.restore(&fixture(source)).is_err(),
                "{source:?} must not load into {target:?}"
            );
            assert_eq!(receiver.snapshot().expect("unchanged"), before);
        }
    }
}
