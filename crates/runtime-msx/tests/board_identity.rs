//! Original envelope IDs bind concrete owners; retired/foreign states reject atomically.
use common_msx::MsxHardware;
use emu198x_shell::MachineCore;
use runtime_msx::{Model, MsxRuntime};
fn fixture(id: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../test-data/msx/board-split-v10/{id}.state")),
    )
    .expect("original save")
}
#[test]
fn all_named_profiles_preserve_their_physical_owner_and_bytes() {
    for (model, hardware, id) in [
        (Model::SonyHb10p, MsxHardware::SonyHb10p, "sony-hb-10p"),
        (
            Model::ToshibaHx10,
            MsxHardware::ToshibaHx10,
            "toshiba-hx-10",
        ),
        (
            Model::MitsubishiMlF80,
            MsxHardware::MitsubishiMlF80,
            "mitsubishi-ml-f80",
        ),
        (Model::YamahaCx5m, MsxHardware::YamahaCx5m, "yamaha-cx5m"),
        (
            Model::MitsubishiMlFx1,
            MsxHardware::MitsubishiMlFx1,
            "mitsubishi-ml-fx1",
        ),
        (Model::SonyHb10, MsxHardware::SonyHb10, "sony-hb-10"),
        (
            Model::PhilipsVg8235,
            MsxHardware::PhilipsVg8235,
            "philips-vg-8235-00",
        ),
        (Model::SonyHbF1, MsxHardware::SonyHbF1, "sony-hb-f1"),
        (
            Model::PanasonicFsA1,
            MsxHardware::PanasonicFsA1,
            "panasonic-fs-a1",
        ),
    ] {
        let mut r = MsxRuntime::blank(model);
        let old = fixture(model.variant_id());
        r.restore(&old).expect("named legacy state");
        assert_eq!(r.machine().expect("machine").hardware(), hardware);
        assert_eq!(r.profile().machine_id.as_str(), id);
        assert_eq!(r.snapshot().expect("same bytes"), old);
    }
}
#[test]
fn wrong_named_envelopes_and_embedded_hardware_reject_atomically() {
    for target in Model::ALL {
        let initial = fixture(target.variant_id());
        let mut r = MsxRuntime::blank(target);
        r.restore(&initial).expect("occupied receiver");
        for source in Model::ALL.into_iter().filter(|&m| m != target) {
            let bytes = fixture(source.variant_id());
            assert!(r.restore(&bytes).is_err());
            assert_eq!(r.snapshot().expect("unchanged"), initial);
            let ((version, time, _), tail) =
                postcard::take_from_bytes::<(u16, u64, String)>(&bytes).expect("old prefix");
            let mut forged =
                postcard::to_allocvec(&(version, time, target.model_id())).expect("prefix");
            forged.extend(tail);
            assert!(r.restore(&forged).is_err(), "{source:?} into {target:?}");
            assert_eq!(r.snapshot().expect("unchanged"), initial);
        }
    }
}
#[test]
fn all_retired_generic_selections_and_saves_fail_explicitly() {
    assert_eq!(Model::ALL.len(), 9);
    for id in [
        "microsoft-msx1-ntsc",
        "microsoft-msx1-pal",
        "microsoft-msx2-ntsc",
        "microsoft-msx2-pal",
        "msx1-ntsc",
        "msx1-pal",
        "msx2-ntsc",
        "msx2-pal",
        "ntsc",
        "pal",
    ] {
        assert!(Model::is_retired_id(id));
        assert_eq!(Model::from_variant_id(id), None);
    }
    for target in Model::ALL {
        let initial = fixture(target.variant_id());
        let mut r = MsxRuntime::blank(target);
        r.restore(&initial).expect("named receiver");
        for id in [
            "microsoft-msx1-ntsc",
            "microsoft-msx1-pal",
            "microsoft-msx2-ntsc",
            "microsoft-msx2-pal",
        ] {
            let err = r.restore(&fixture(id)).expect_err("retired save");
            assert!(err.to_string().contains("retired"), "{err}");
            assert_eq!(r.snapshot().expect("unchanged"), initial);
        }
    }
}
