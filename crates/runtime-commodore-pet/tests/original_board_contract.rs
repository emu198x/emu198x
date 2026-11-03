//! All current owners use the approved v6 contract and retain concrete identity.
use emu198x_shell::{MachineCore, MachineError};
use runtime_commodore_pet::{Model, PetRuntime};
fn runtime(model: Model) -> PetRuntime {
    let mut k = vec![0xea; 4096];
    k[..3].copy_from_slice(&[0x4c, 0, 0xf0]);
    k[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
    PetRuntime::new(
        model,
        k,
        vec![0; model.basic_size()],
        vec![0; 2048],
        vec![0; 2048],
    )
    .expect("matching board")
}
#[test]
fn four_board_profiles_and_current_saves_are_unambiguous() {
    assert_eq!(Model::ALL.len(), 4);
    let profiles = runtime_commodore_pet::profiles();
    assert_eq!(profiles.len(), 4);
    for model in Model::ALL {
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        let mut r = runtime(model);
        let before = r.snapshot().expect("current");
        assert_eq!(
            postcard::take_from_bytes::<u16>(&before).expect("header").0,
            6
        );
        for version in [5u8, 7] {
            assert!(
                matches!(r.restore(&[version]),Err(MachineError::InvalidSnapshot{reason}) if reason==format!("unsupported snapshot version {version}; expected 6"))
            );
            assert_eq!(r.snapshot().expect("atomic rejection"), before);
        }
        for source in Model::ALL.into_iter().filter(|&other| other != model) {
            let bytes = runtime(source).snapshot().expect("foreign board");
            assert!(r.restore(&bytes).is_err());
            assert_eq!(r.snapshot().expect("unchanged"), before);
            let ((version, time, _), payload) =
                postcard::take_from_bytes::<(u16, u64, String)>(&bytes).expect("envelope");
            let mut forged =
                postcard::to_allocvec(&(version, time, model.model_id())).expect("selected label");
            forged.extend(payload);
            assert!(
                r.restore(&forged).is_err(),
                "foreign physical state cannot be relabelled"
            );
            assert_eq!(r.snapshot().expect("unchanged"), before);
        }
    }
    let original = runtime(Model::Pet2001Original8k);
    assert_eq!(
        original.profile().machine_id.as_str(),
        "commodore-pet-2001-320008"
    );
    assert_eq!(
        original.profile().profile_id.as_str(),
        "commodore-pet-2001-320008-8k"
    );
}
