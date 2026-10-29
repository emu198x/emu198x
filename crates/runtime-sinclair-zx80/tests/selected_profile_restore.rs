//! Legacy family headers must not conceal a different selected board or RAM.
use emu198x_shell::{BootArtifacts, MachineCore, MachineError, boot_machine};
use runtime_sinclair_zx80::{Model, Zx80Runtime};
#[test]
fn every_profile_accepts_only_matching_hardware_without_mutating_on_rejection() {
    for saved in Model::ALL {
        let donor = Zx80Runtime::new(saved, vec![0; 4096]).expect("donor");
        let state = donor.snapshot().expect("snapshot");
        for selected in Model::ALL {
            let mut receiver = Zx80Runtime::new(selected, vec![0xFF; 4096]).expect("receiver");
            let before = receiver.snapshot().expect("before");
            let result = receiver.restore(&state);
            if saved == selected {
                result.expect("matching profile");
                assert_eq!(receiver.snapshot().expect("after"), state);
            } else {
                assert!(
                    matches!(result, Err(MachineError::InvalidSnapshot { .. })),
                    "saved {saved:?}, selected {selected:?}"
                );
                assert_eq!(receiver.snapshot().expect("rejected receiver"), before);
            }
            assert_eq!(receiver.model(), selected);
        }
    }
}
#[test]
fn snapshot_only_loader_requires_the_explicit_matching_profile() {
    for saved in Model::ALL {
        let donor = Zx80Runtime::new(saved, vec![0; 4096]).expect("donor");
        let state = donor.snapshot().expect("snapshot");
        let artifacts = BootArtifacts {
            snapshot: Some(&state),
            ..BootArtifacts::new()
        };
        for selected in Model::ALL {
            let result = boot_machine(
                &artifacts,
                |_| unreachable!("snapshot-only path"),
                || Zx80Runtime::blank(selected),
            );
            assert_eq!(
                result.is_ok(),
                saved == selected,
                "saved {saved:?}, selected {selected:?}"
            );
            if let Ok(runtime) = result {
                assert_eq!(runtime.model(), selected);
                assert_eq!(runtime.snapshot().expect("loaded"), state);
            }
        }
    }
}
#[test]
fn empty_legacy_saves_keep_the_selected_profile() {
    for saved in Model::ALL {
        let state = Zx80Runtime::blank(saved).snapshot().expect("empty state");
        for selected in Model::ALL {
            let mut receiver = Zx80Runtime::blank(selected);
            receiver.restore(&state).expect("no hardware to identify");
            assert_eq!(receiver.model(), selected);
            assert_eq!(receiver.ram_bytes(), selected.ram_bytes());
        }
    }
}
#[test]
fn explicit_ram_overrides_must_also_match() {
    let model = Model::ALL[0];
    let mut donor = Zx80Runtime::new(model, vec![0; 4096]).expect("donor");
    donor.set_ram_bytes(4096).expect("4K configuration");
    let state = donor.snapshot().expect("state");
    let mut receiver = Zx80Runtime::new(model, vec![0; 4096]).expect("receiver");
    assert!(receiver.restore(&state).is_err());
    receiver
        .set_ram_bytes(4096)
        .expect("matching explicit configuration");
    receiver.restore(&state).expect("matching board and RAM");
    assert_eq!(receiver.snapshot().expect("after"), state);
}
